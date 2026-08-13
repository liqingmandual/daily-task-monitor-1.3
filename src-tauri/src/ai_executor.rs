use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
#[cfg(all(target_os = "windows", debug_assertions))]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::ai::{AiExecutionMode, AiExecutionSnapshot, AiProviderConfig, ProviderRegistry};
use crate::db::sanitize_ai_diagnostic;

const MAX_STDOUT_BYTES: usize = 1024 * 1024;
const MAX_STDERR_BYTES: usize = 16 * 1024;
const MAX_DIAGNOSTIC_CHARS: usize = 1_000;
const FAILURE_IO_COMPLETION_TIMEOUT: Duration = Duration::from_millis(250);
const NORMAL_IO_COMPLETION_TIMEOUT: Duration = Duration::from_secs(5);
const PROCESS_TERMINATION_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiExecutionRequest {
    pub job_id: String,
    pub kind: String,
    pub snapshot: AiExecutionSnapshot,
    pub system_prompt: String,
    pub minimal_payload_json: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiExecutionOutput {
    pub content: String,
    pub executor_id: String,
    pub model: String,
    pub duration_ms: u64,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiExecutionErrorKind {
    Authentication,
    CliFailed,
    CliUnavailable,
    NotConfigured,
    InvalidJob,
    InvalidResponse,
    Network,
    PermissionDenied,
    Persistence,
    Provider,
    RateLimited,
    Timeout,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiExecutionError {
    pub kind: AiExecutionErrorKind,
    pub diagnostic: String,
    pub exit_code: Option<i32>,
    pub executor_id: Option<String>,
    pub model: Option<String>,
    pub duration_ms: Option<u64>,
}

impl AiExecutionError {
    pub fn new(
        kind: AiExecutionErrorKind,
        diagnostic: impl Into<String>,
        exit_code: Option<i32>,
    ) -> Self {
        Self {
            kind,
            diagnostic: bounded_text(&diagnostic.into(), MAX_DIAGNOSTIC_CHARS),
            exit_code,
            executor_id: None,
            model: None,
            duration_ms: None,
        }
    }

    pub fn with_execution_context(
        mut self,
        executor_id: impl Into<String>,
        model: impl Into<String>,
        duration_ms: u64,
    ) -> Self {
        self.executor_id = Some(executor_id.into());
        self.model = Some(model.into());
        self.duration_ms = Some(duration_ms);
        self
    }

    pub fn with_output_context(self, output: &AiExecutionOutput) -> Self {
        self.with_execution_context(
            output.executor_id.clone(),
            output.model.clone(),
            output.duration_ms,
        )
    }

    pub fn invalid_job(diagnostic: impl Into<String>) -> Self {
        Self::new(AiExecutionErrorKind::InvalidJob, diagnostic, None)
    }

    pub fn invalid_response(diagnostic: impl Into<String>) -> Self {
        Self::new(AiExecutionErrorKind::InvalidResponse, diagnostic, None)
    }

    pub fn persistence(diagnostic: impl Into<String>) -> Self {
        Self::new(AiExecutionErrorKind::Persistence, diagnostic, None)
    }
}

impl std::fmt::Display for AiExecutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.diagnostic)
    }
}

impl std::error::Error for AiExecutionError {}

impl From<String> for AiExecutionError {
    fn from(diagnostic: String) -> Self {
        Self::new(AiExecutionErrorKind::Unknown, diagnostic, None)
    }
}

impl From<&str> for AiExecutionError {
    fn from(diagnostic: &str) -> Self {
        Self::new(AiExecutionErrorKind::Unknown, diagnostic, None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiProviderCredential {
    pub provider: AiProviderConfig,
    pub api_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexExecutionConfig {
    pub executable: String,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCodexExecutable {
    pub path: PathBuf,
    pub version: String,
}

impl CodexExecutionConfig {
    pub fn model_label(&self) -> &str {
        if normalized_model_flag(&self.model).is_none() {
            "cli-default"
        } else {
            self.model.trim()
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AiExecutionBackends {
    pub api_providers: Vec<ApiProviderCredential>,
    pub configured_codex: Option<CodexExecutionConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CodexHealthStatus {
    Healthy,
    PermissionDenied,
    TimedOut,
    Unavailable,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexHealth {
    pub configured_path: String,
    pub detected_path: Option<String>,
    pub version: Option<String>,
    pub checked_at_ms: i64,
    pub status: CodexHealthStatus,
    pub diagnostic: Option<String>,
}

fn is_explicit_codex_path(value: &str) -> bool {
    let path = Path::new(value.trim());
    path.is_absolute() || path.components().count() > 1
}

fn is_windows_apps_candidate(path: &Path) -> bool {
    path.to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase()
        .contains("\\microsoft\\windowsapps\\")
}

fn push_unique_candidate(candidates: &mut Vec<PathBuf>, path: PathBuf) {
    if !candidates.iter().any(|candidate| candidate == &path) {
        candidates.push(path);
    }
}

pub fn codex_executable_candidates_with(
    configured: &str,
    local_app_data: Option<&Path>,
    search_path: Option<&OsStr>,
    path_ext: Option<&OsStr>,
) -> Vec<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() && is_explicit_codex_path(configured) {
        return vec![PathBuf::from(configured)];
    }

    let mut candidates = Vec::new();
    let automatic = configured.is_empty()
        || configured.eq_ignore_ascii_case("codex")
        || configured.eq_ignore_ascii_case("codex.exe");
    if automatic && let Some(local_app_data) = local_app_data {
        let app_bin = local_app_data.join("OpenAI").join("Codex").join("bin");
        if let Ok(entries) = std::fs::read_dir(app_bin) {
            let mut app_candidates = entries
                .flatten()
                .map(|entry| entry.path().join("codex.exe"))
                .filter(|path| path.is_file())
                .collect::<Vec<_>>();
            app_candidates.sort_by(|left, right| right.cmp(left));
            for path in app_candidates {
                push_unique_candidate(&mut candidates, path);
            }
        }
        let local_cli = local_app_data
            .join("DailyTaskMonitor")
            .join("codex-cli")
            .join("node_modules")
            .join("@openai")
            .join("codex-win32-x64")
            .join("vendor")
            .join("x86_64-pc-windows-msvc")
            .join("bin")
            .join("codex.exe");
        if local_cli.is_file() {
            push_unique_candidate(&mut candidates, local_cli);
        }
    }

    let bare_name = if configured.is_empty() {
        "codex"
    } else {
        configured
    };
    let mut names = vec![bare_name.to_string()];
    if Path::new(bare_name).extension().is_none() {
        let extensions = path_ext
            .and_then(OsStr::to_str)
            .unwrap_or(".COM;.EXE;.BAT;.CMD");
        names.extend(
            extensions
                .split(';')
                .filter(|extension| !extension.is_empty())
                .map(|extension| format!("{bare_name}{extension}")),
        );
    }
    if let Some(search_path) = search_path {
        for directory in std::env::split_paths(search_path) {
            for name in &names {
                let candidate = directory.join(name);
                if candidate.is_file() {
                    push_unique_candidate(&mut candidates, candidate);
                }
            }
        }
    }
    candidates
}

pub fn codex_executable_candidates(configured: &str) -> Vec<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let search_path = std::env::var_os("PATH");
    let path_ext = std::env::var_os("PATHEXT");
    codex_executable_candidates_with(
        configured,
        local_app_data.as_deref(),
        search_path.as_deref(),
        path_ext.as_deref(),
    )
}

pub fn resolve_codex_executable_from_candidates_with<F>(
    configured: &str,
    candidates: Vec<PathBuf>,
    mut probe: F,
) -> Result<ResolvedCodexExecutable, AiExecutionErrorKind>
where
    F: FnMut(&Path) -> Result<String, AiExecutionErrorKind>,
{
    let explicit = is_explicit_codex_path(configured);
    let mut last_error = AiExecutionErrorKind::CliUnavailable;
    for candidate in candidates {
        if !explicit && is_windows_apps_candidate(&candidate) {
            continue;
        }
        match probe(&candidate) {
            Ok(version) => {
                return Ok(ResolvedCodexExecutable {
                    path: candidate,
                    version,
                });
            }
            Err(error) => {
                last_error = error;
                if explicit {
                    break;
                }
            }
        }
    }
    Err(last_error)
}

pub fn resolve_codex_executable(
    configured: &str,
    timeout_ms: u64,
) -> Result<ResolvedCodexExecutable, AiExecutionError> {
    let candidates = codex_executable_candidates(configured);
    if candidates.is_empty() {
        return Err(AiExecutionError::new(
            AiExecutionErrorKind::CliUnavailable,
            "Codex CLI executable was not found",
            None,
        ));
    }
    let timeout = Duration::from_millis(timeout_ms.max(1));
    let result = resolve_codex_executable_from_candidates_with(configured, candidates, |path| {
        let executable = path.to_string_lossy();
        match run_process(&executable, &["--version".into()], None, timeout) {
            Ok(outcome) if outcome.status.success() && !outcome.stdout.trim().is_empty() => {
                Ok(bounded_text(outcome.stdout.trim(), 200))
            }
            Ok(_) => Err(AiExecutionErrorKind::CliFailed),
            Err(error) => Err(error.kind),
        }
    });
    result.map_err(|kind| {
        AiExecutionError::new(kind, "No usable Codex CLI executable was found", None)
    })
}

pub async fn execute_ai(
    request: &AiExecutionRequest,
    backends: &AiExecutionBackends,
) -> Result<AiExecutionOutput, AiExecutionError> {
    execute_ai_validated(request, backends, |_| Ok(())).await
}

pub async fn execute_ai_validated<F>(
    request: &AiExecutionRequest,
    backends: &AiExecutionBackends,
    validate: F,
) -> Result<AiExecutionOutput, AiExecutionError>
where
    F: Fn(&AiExecutionOutput) -> Result<(), String>,
{
    match request.snapshot.execution_mode {
        AiExecutionMode::ApiKey => execute_api(request, &backends.api_providers, &validate).await,
        AiExecutionMode::Codex => {
            let output = execute_codex(request, backends.configured_codex.as_ref())?;
            validate(&output).map_err(|diagnostic| {
                AiExecutionError::invalid_response(sanitize_diagnostic(&diagnostic, request))
                    .with_output_context(&output)
            })?;
            Ok(output)
        }
    }
}

async fn execute_api<F>(
    request: &AiExecutionRequest,
    credentials: &[ApiProviderCredential],
    validate: &F,
) -> Result<AiExecutionOutput, AiExecutionError>
where
    F: Fn(&AiExecutionOutput) -> Result<(), String>,
{
    let legacy_registry_snapshot = matches!(
        request.snapshot.executor_id.as_str(),
        "provider-registry" | "legacy-provider-registry"
    );
    let providers = credentials
        .iter()
        .map(|credential| credential.provider.clone())
        .collect::<Vec<_>>();
    let registry = ProviderRegistry::new(providers);
    let selected = if legacy_registry_snapshot {
        registry.automatic_candidates().into_iter().next()
    } else {
        credentials
            .iter()
            .map(|credential| &credential.provider)
            .find(|provider| {
                provider.id == request.snapshot.executor_id
                    && provider.enabled
                    && provider.has_credential
            })
    };
    let Some(selected) = selected else {
        return Err(AiExecutionError::new(
            AiExecutionErrorKind::Provider,
            "The queued AI provider is not configured",
            None,
        ));
    };
    let candidates = [selected];

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(request.timeout_ms.max(1)))
        .build()
        .map_err(|error| {
            AiExecutionError::new(
                AiExecutionErrorKind::Network,
                format!("Failed to create API client: {error}"),
                None,
            )
        })?;
    let started = Instant::now();
    let mut last_error = AiExecutionError::new(
        AiExecutionErrorKind::Provider,
        "No configured AI provider succeeded",
        None,
    );

    for provider in candidates {
        let Some(credential) = credentials
            .iter()
            .find(|credential| credential.provider.id == provider.id)
        else {
            continue;
        };
        let endpoint = format!(
            "{}/chat/completions",
            provider.base_url.trim_end_matches('/')
        );
        let requested_model =
            if !legacy_registry_snapshot && !request.snapshot.model.trim().is_empty() {
                request.snapshot.model.trim()
            } else {
                provider.model.as_str()
            };
        let mut body = serde_json::json!({
            "model": requested_model,
            "temperature": 0.1,
            "messages": [
                { "role": "system", "content": request.system_prompt },
                { "role": "user", "content": request.minimal_payload_json }
            ]
        });
        if provider.id == "modelscope" {
            body["enable_thinking"] = serde_json::Value::Bool(false);
        }
        let response = match client
            .post(endpoint)
            .bearer_auth(&credential.api_key)
            .json(&body)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                let kind = if error.is_timeout() {
                    AiExecutionErrorKind::Timeout
                } else {
                    AiExecutionErrorKind::Network
                };
                last_error = AiExecutionError::new(
                    kind,
                    format!("{} request failed: {error}", provider.name),
                    None,
                )
                .with_execution_context(
                    &provider.id,
                    requested_model,
                    elapsed_ms(started),
                );
                continue;
            }
        };

        let status = response.status();
        let response_text = match response.text().await {
            Ok(response_text) => response_text,
            Err(error) => {
                last_error = AiExecutionError::new(
                    AiExecutionErrorKind::Network,
                    format!("{} response could not be read: {error}", provider.name),
                    None,
                )
                .with_execution_context(
                    &provider.id,
                    requested_model,
                    elapsed_ms(started),
                );
                continue;
            }
        };
        if !status.is_success() {
            let kind = if matches!(status.as_u16(), 401 | 403) {
                AiExecutionErrorKind::Authentication
            } else if status.as_u16() == 429 {
                AiExecutionErrorKind::RateLimited
            } else {
                AiExecutionErrorKind::Provider
            };
            last_error = AiExecutionError::new(
                kind,
                format!(
                    "{} returned HTTP {}; response: {}",
                    provider.name,
                    status,
                    sanitize_diagnostic(&response_text, request)
                ),
                None,
            )
            .with_execution_context(&provider.id, requested_model, elapsed_ms(started));
            continue;
        }

        let response: serde_json::Value = match serde_json::from_str(&response_text) {
            Ok(response) => response,
            Err(error) => {
                last_error = AiExecutionError::invalid_response(format!(
                    "{} returned invalid JSON: {error}",
                    provider.name
                ))
                .with_execution_context(
                    &provider.id,
                    requested_model,
                    elapsed_ms(started),
                );
                continue;
            }
        };
        let Some(content) = response
            .pointer("/choices/0/message/content")
            .and_then(serde_json::Value::as_str)
        else {
            last_error = AiExecutionError::invalid_response(format!(
                "{} response did not include message content",
                provider.name
            ))
            .with_execution_context(
                &provider.id,
                requested_model,
                elapsed_ms(started),
            );
            continue;
        };
        let model = response
            .get("model")
            .and_then(serde_json::Value::as_str)
            .filter(|model| !model.trim().is_empty())
            .unwrap_or(requested_model)
            .to_string();
        let output = AiExecutionOutput {
            content: content.to_string(),
            executor_id: provider.id.clone(),
            model,
            duration_ms: elapsed_ms(started),
            exit_code: None,
        };
        if let Err(diagnostic) = validate(&output) {
            last_error =
                AiExecutionError::invalid_response(sanitize_diagnostic(&diagnostic, request))
                    .with_output_context(&output);
            continue;
        }
        return Ok(output);
    }

    Err(last_error)
}

fn execute_codex(
    request: &AiExecutionRequest,
    _configured: Option<&CodexExecutionConfig>,
) -> Result<AiExecutionOutput, AiExecutionError> {
    let executable = request.snapshot.executor_id.trim();
    let model = request.snapshot.model.as_str();
    let audit_model = model_label(model);
    if executable.is_empty() {
        return Err(AiExecutionError::new(
            AiExecutionErrorKind::NotConfigured,
            "Queued Codex executable is not configured",
            None,
        )
        .with_execution_context(executable, audit_model, 0));
    }
    let args = codex_exec_args(model, &request.system_prompt);
    let started = Instant::now();
    let outcome = run_process(
        executable,
        &args,
        Some(request.minimal_payload_json.as_bytes()),
        Duration::from_millis(request.timeout_ms.max(1)),
    )
    .map_err(|error| {
        sanitize_execution_error(error, request).with_execution_context(
            executable,
            audit_model,
            elapsed_ms(started),
        )
    })?;
    if !outcome.status.success() {
        let diagnostic = if outcome.stderr.trim().is_empty() {
            format!("Codex exited with status {}", outcome.status)
        } else {
            format!(
                "Codex exited with status {}; stderr: {}",
                outcome.status, outcome.stderr
            )
        };
        return Err(AiExecutionError::new(
            AiExecutionErrorKind::CliFailed,
            sanitize_diagnostic(&diagnostic, request),
            outcome.status.code(),
        )
        .with_execution_context(executable, audit_model, elapsed_ms(started)));
    }

    let value = parse_exactly_one_json_object(&outcome.stdout).map_err(|error| {
        error.with_execution_context(executable, audit_model, elapsed_ms(started))
    })?;
    let actual_model = value
        .get("model")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| model_label(model).to_string());
    let content = match value.get("content") {
        Some(serde_json::Value::String(content)) => {
            parse_exactly_one_json_object(content).map_err(|error| {
                error.with_execution_context(executable, &actual_model, elapsed_ms(started))
            })?;
            content.trim().to_string()
        }
        _ => serde_json::to_string(&value).map_err(|error| {
            AiExecutionError::invalid_response(format!(
                "Codex response could not be serialized: {error}"
            ))
            .with_execution_context(executable, &actual_model, elapsed_ms(started))
        })?,
    };
    Ok(AiExecutionOutput {
        content,
        executor_id: executable.to_string(),
        model: actual_model,
        duration_ms: elapsed_ms(started),
        exit_code: outcome.status.code(),
    })
}

pub(crate) fn codex_exec_args(model: &str, system_prompt: &str) -> Vec<String> {
    let mut args = [
        "exec",
        "--skip-git-repo-check",
        "--ephemeral",
        "--ignore-user-config",
        "--ignore-rules",
        "--sandbox",
        "read-only",
        "--color",
        "never",
        "-c",
        "model_provider=\"daily-task-monitor-http\"",
        "-c",
        "model_providers.daily-task-monitor-http.name=\"ChatGPT HTTP\"",
        "-c",
        "model_providers.daily-task-monitor-http.base_url=\"https://chatgpt.com/backend-api/codex\"",
        "-c",
        "model_providers.daily-task-monitor-http.wire_api=\"responses\"",
        "-c",
        "model_providers.daily-task-monitor-http.requires_openai_auth=true",
        "-c",
        "model_providers.daily-task-monitor-http.supports_websockets=false",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    if let Some(model) = normalized_model_flag(model) {
        args.push("--model".to_string());
        args.push(model.to_string());
    }
    args.push(system_prompt.to_string());
    args
}

fn normalized_model_flag(model: &str) -> Option<&str> {
    let model = model.trim();
    (!model.is_empty() && model != "cli-default").then_some(model)
}

fn model_label(model: &str) -> &str {
    normalized_model_flag(model).unwrap_or("cli-default")
}

fn parse_exactly_one_json_object(stdout: &str) -> Result<serde_json::Value, AiExecutionError> {
    let mut values = serde_json::Deserializer::from_str(stdout).into_iter::<serde_json::Value>();
    let value = values
        .next()
        .transpose()
        .map_err(|error| {
            AiExecutionError::invalid_response(format!("Codex returned invalid JSON: {error}"))
        })?
        .ok_or_else(|| AiExecutionError::invalid_response("Codex returned empty stdout"))?;
    if !value.is_object() {
        return Err(AiExecutionError::invalid_response(
            "Codex stdout must be one JSON object",
        ));
    }
    if values.next().is_some() {
        return Err(AiExecutionError::invalid_response(
            "Codex stdout must contain exactly one JSON object",
        ));
    }
    Ok(value)
}

pub fn probe_codex_health(
    configured_path: &str,
    _detected_path: Option<String>,
    timeout_ms: u64,
) -> CodexHealth {
    let checked_at_ms = unix_time_ms();
    let configured_path = configured_path.trim();
    let resolved = match resolve_codex_executable(configured_path, timeout_ms) {
        Ok(resolved) => resolved,
        Err(error) => {
            let status = match error.kind {
                AiExecutionErrorKind::CliUnavailable | AiExecutionErrorKind::NotConfigured => {
                    CodexHealthStatus::Unavailable
                }
                AiExecutionErrorKind::PermissionDenied => CodexHealthStatus::PermissionDenied,
                AiExecutionErrorKind::Timeout => CodexHealthStatus::TimedOut,
                _ => CodexHealthStatus::Error,
            };
            return CodexHealth {
                configured_path: configured_path.to_string(),
                detected_path: None,
                version: None,
                checked_at_ms,
                status,
                diagnostic: Some(sanitize_health_diagnostic(&error.diagnostic)),
            };
        }
    };
    let detected_path = resolved.path.to_string_lossy().into_owned();
    match run_process(
        &detected_path,
        &["login".into(), "status".into()],
        None,
        Duration::from_millis(timeout_ms.max(1)),
    ) {
        Ok(outcome) if outcome.status.success() => CodexHealth {
            configured_path: configured_path.to_string(),
            detected_path: Some(detected_path),
            version: Some(resolved.version),
            checked_at_ms,
            status: CodexHealthStatus::Healthy,
            diagnostic: None,
        },
        Ok(outcome) => CodexHealth {
            configured_path: configured_path.to_string(),
            detected_path: Some(detected_path),
            version: Some(resolved.version),
            checked_at_ms,
            status: CodexHealthStatus::PermissionDenied,
            diagnostic: Some(sanitize_health_diagnostic(&format!(
                "Codex login status failed with {}; stderr: {}",
                outcome.status, outcome.stderr
            ))),
        },
        Err(error) => {
            let status = match error.kind {
                AiExecutionErrorKind::PermissionDenied => CodexHealthStatus::PermissionDenied,
                AiExecutionErrorKind::Timeout => CodexHealthStatus::TimedOut,
                AiExecutionErrorKind::CliUnavailable => CodexHealthStatus::Unavailable,
                _ => CodexHealthStatus::Error,
            };
            CodexHealth {
                configured_path: configured_path.to_string(),
                detected_path: Some(detected_path),
                version: Some(resolved.version),
                checked_at_ms,
                status,
                diagnostic: Some(sanitize_health_diagnostic(&error.diagnostic)),
            }
        }
    }
}

pub async fn test_codex_inference(
    configured_path: &str,
    model: &str,
    timeout_ms: u64,
) -> CodexHealth {
    let checked_at_ms = unix_time_ms();
    let resolved = match resolve_codex_executable(configured_path, timeout_ms) {
        Ok(resolved) => resolved,
        Err(error) => {
            let status = match error.kind {
                AiExecutionErrorKind::PermissionDenied => CodexHealthStatus::PermissionDenied,
                AiExecutionErrorKind::Timeout => CodexHealthStatus::TimedOut,
                AiExecutionErrorKind::CliUnavailable | AiExecutionErrorKind::NotConfigured => {
                    CodexHealthStatus::Unavailable
                }
                _ => CodexHealthStatus::Error,
            };
            return CodexHealth {
                configured_path: configured_path.trim().to_string(),
                detected_path: None,
                version: None,
                checked_at_ms,
                status,
                diagnostic: Some(sanitize_health_diagnostic(&error.diagnostic)),
            };
        }
    };
    let executable = resolved.path.to_string_lossy().into_owned();
    let request = AiExecutionRequest {
        job_id: "codex-health-check".into(),
        kind: "health_check".into(),
        snapshot: AiExecutionSnapshot {
            execution_mode: AiExecutionMode::Codex,
            executor_id: executable.clone(),
            model: model_label(model).into(),
            evidence_hash: "health-check".into(),
            created_at_ms: checked_at_ms,
        },
        system_prompt: "health check: return exactly one JSON object with {\"ok\":true}".into(),
        minimal_payload_json: "{}".into(),
        timeout_ms,
    };
    match execute_ai(&request, &AiExecutionBackends::default()).await {
        Ok(_output) => CodexHealth {
            configured_path: configured_path.trim().to_string(),
            detected_path: Some(executable),
            version: Some(resolved.version),
            checked_at_ms,
            status: CodexHealthStatus::Healthy,
            diagnostic: None,
        },
        Err(error) => {
            let status = match error.kind {
                AiExecutionErrorKind::PermissionDenied | AiExecutionErrorKind::Authentication => {
                    CodexHealthStatus::PermissionDenied
                }
                AiExecutionErrorKind::Timeout => CodexHealthStatus::TimedOut,
                AiExecutionErrorKind::CliUnavailable => CodexHealthStatus::Unavailable,
                _ => CodexHealthStatus::Error,
            };
            CodexHealth {
                configured_path: configured_path.trim().to_string(),
                detected_path: Some(executable),
                version: Some(resolved.version),
                checked_at_ms,
                status,
                diagnostic: Some(sanitize_health_diagnostic(&error.diagnostic)),
            }
        }
    }
}

struct ProcessOutcome {
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

#[derive(Clone, Copy)]
enum CapturedStream {
    Stdout,
    Stderr,
}

enum CaptureEvent {
    Overflow(CapturedStream),
    ReadFailed(CapturedStream, String),
}

struct BoundedCapture {
    bytes: Vec<u8>,
}

fn run_process(
    executable: &str,
    args: &[String],
    stdin: Option<&[u8]>,
    timeout: Duration,
) -> Result<ProcessOutcome, AiExecutionError> {
    let working_dir = codex_working_dir();
    let _ = std::fs::create_dir_all(&working_dir);
    let process_tree = ProcessTree::new().map_err(process_job_error)?;
    let mut command = Command::new(executable);
    command
        .args(args)
        .current_dir(working_dir)
        .env("NO_COLOR", "1")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        const CREATE_SUSPENDED: u32 = 0x00000004;
        command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
    }

    let started = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|error| process_spawn_error(executable, error))?;
    #[cfg(target_os = "windows")]
    let suspended_thread = match WindowsSuspendedThread::open(child.id()) {
        Ok(thread) => thread,
        Err(error) => {
            let _ = terminate_child_direct(&mut child);
            return Err(AiExecutionError::new(
                AiExecutionErrorKind::CliFailed,
                format!("Failed to access suspended Codex CLI main thread: {error}"),
                None,
            ));
        }
    };
    if let Err(error) = process_tree.assign(&child) {
        let _ = terminate_child_direct(&mut child);
        return Err(AiExecutionError::new(
            AiExecutionErrorKind::CliFailed,
            format!("Failed to isolate Codex CLI process: {error}"),
            None,
        ));
    }
    #[cfg(target_os = "windows")]
    if let Err(error) = suspended_thread.resume() {
        let _ = process_tree.terminate(&mut child);
        return Err(AiExecutionError::new(
            AiExecutionErrorKind::CliFailed,
            format!("Failed to resume isolated Codex CLI process: {error}"),
            None,
        ));
    }
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = process_tree.terminate(&mut child);
            return Err(AiExecutionError::new(
                AiExecutionErrorKind::Unknown,
                "Codex stdout pipe is unavailable",
                None,
            ));
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            let _ = process_tree.terminate(&mut child);
            return Err(AiExecutionError::new(
                AiExecutionErrorKind::Unknown,
                "Codex stderr pipe is unavailable",
                None,
            ));
        }
    };
    let (event_sender, event_receiver) = mpsc::channel();
    let stdout_reader = spawn_bounded_reader(
        stdout,
        MAX_STDOUT_BYTES,
        CapturedStream::Stdout,
        event_sender.clone(),
    );
    let stderr_reader = spawn_bounded_reader(
        stderr,
        MAX_STDERR_BYTES,
        CapturedStream::Stderr,
        event_sender,
    );
    let stdin_writer = match stdin {
        Some(input) => {
            let mut pipe = match child.stdin.take() {
                Some(pipe) => pipe,
                None => {
                    let _ = process_tree.terminate(&mut child);
                    return Err(AiExecutionError::new(
                        AiExecutionErrorKind::Unknown,
                        "Codex stdin pipe is unavailable",
                        None,
                    ));
                }
            };
            let input = input.to_vec();
            let (sender, receiver) = mpsc::channel();
            thread::spawn(move || {
                let _ = sender.send(pipe.write_all(&input));
            });
            Some(receiver)
        }
        None => None,
    };

    let mut terminal_error = None;
    let status = loop {
        match event_receiver.try_recv() {
            Ok(CaptureEvent::Overflow(CapturedStream::Stdout)) => {
                terminal_error = Some(AiExecutionError::invalid_response(
                    "Codex stdout exceeded the maximum allowed size",
                ));
                break None;
            }
            Ok(CaptureEvent::Overflow(CapturedStream::Stderr)) => {
                terminal_error = Some(AiExecutionError::new(
                    AiExecutionErrorKind::CliFailed,
                    "Codex stderr exceeded the maximum allowed size",
                    None,
                ));
                break None;
            }
            Ok(CaptureEvent::ReadFailed(stream, error)) => {
                terminal_error = Some(AiExecutionError::new(
                    AiExecutionErrorKind::Unknown,
                    format!("Failed to read Codex {}: {error}", stream.label()),
                    None,
                ));
                break None;
            }
            Err(mpsc::TryRecvError::Disconnected | mpsc::TryRecvError::Empty) => {}
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                terminal_error = Some(AiExecutionError::new(
                    AiExecutionErrorKind::Timeout,
                    format!("Codex CLI timed out after {} ms", timeout.as_millis()),
                    None,
                ));
                break None;
            }
            Err(error) => {
                terminal_error = Some(AiExecutionError::new(
                    AiExecutionErrorKind::Unknown,
                    format!("Failed while waiting for Codex CLI: {error}"),
                    None,
                ));
                break None;
            }
        }
    };

    if let Some(error) = terminal_error {
        process_tree.terminate(&mut child)?;
        drop(process_tree);
        let deadline = Instant::now() + FAILURE_IO_COMPLETION_TIMEOUT;
        let _ = receive_stdin_writer(stdin_writer, deadline);
        let _ = receive_bounded_reader(stdout_reader, "stdout", deadline);
        let _ = receive_bounded_reader(stderr_reader, "stderr", deadline);
        return Err(error);
    }
    drop(process_tree);
    let deadline = Instant::now() + NORMAL_IO_COMPLETION_TIMEOUT;
    let stdin_result = receive_stdin_writer(stdin_writer, deadline)?;
    let stdout = receive_bounded_reader(stdout_reader, "stdout", deadline)?;
    let stderr = receive_bounded_reader(stderr_reader, "stderr", deadline)?;
    stdin_result.map_err(|error| {
        AiExecutionError::new(
            AiExecutionErrorKind::CliFailed,
            format!("Failed to write Codex stdin: {error}"),
            None,
        )
    })?;
    Ok(ProcessOutcome {
        status: status.expect("completed process has an exit status"),
        stdout: String::from_utf8_lossy(&stdout.bytes).into_owned(),
        stderr: String::from_utf8_lossy(&stderr.bytes).into_owned(),
    })
}

impl CapturedStream {
    fn label(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

fn spawn_bounded_reader<R: Read + Send + 'static>(
    mut reader: R,
    limit: usize,
    stream: CapturedStream,
    events: mpsc::Sender<CaptureEvent>,
) -> mpsc::Receiver<BoundedCapture> {
    let (completion_sender, completion_receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::with_capacity(limit.min(8 * 1024));
        let mut overflow_reported = false;
        let mut chunk = [0_u8; 8 * 1024];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    let remaining = limit.saturating_sub(bytes.len());
                    bytes.extend_from_slice(&chunk[..read.min(remaining)]);
                    if read > remaining && !overflow_reported {
                        overflow_reported = true;
                        let _ = events.send(CaptureEvent::Overflow(stream));
                    }
                }
                Err(error) => {
                    let _ = events.send(CaptureEvent::ReadFailed(stream, error.to_string()));
                    break;
                }
            }
        }
        let _ = completion_sender.send(BoundedCapture { bytes });
    });
    completion_receiver
}

fn terminate_child_direct(child: &mut Child) -> Result<(), AiExecutionError> {
    match child.try_wait() {
        Ok(Some(_)) => return Ok(()),
        Ok(None) => {}
        Err(error) => {
            return Err(AiExecutionError::new(
                AiExecutionErrorKind::Unknown,
                format!("Failed while checking Codex CLI before termination: {error}"),
                None,
            ));
        }
    }
    child.kill().map_err(|error| {
        AiExecutionError::new(
            AiExecutionErrorKind::Unknown,
            format!("Failed to terminate Codex CLI: {error}"),
            None,
        )
    })?;
    wait_for_child_exit(child, PROCESS_TERMINATION_TIMEOUT)
}

fn wait_for_child_exit(child: &mut Child, timeout: Duration) -> Result<(), AiExecutionError> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(()),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            Ok(None) => {
                return Err(AiExecutionError::new(
                    AiExecutionErrorKind::Unknown,
                    "Timed out while reaping terminated Codex CLI",
                    None,
                ));
            }
            Err(error) => {
                return Err(AiExecutionError::new(
                    AiExecutionErrorKind::Unknown,
                    format!("Failed to reap terminated Codex CLI: {error}"),
                    None,
                ));
            }
        }
    }
}

fn receive_stdin_writer(
    writer: Option<mpsc::Receiver<std::io::Result<()>>>,
    deadline: Instant,
) -> Result<std::io::Result<()>, AiExecutionError> {
    match writer {
        Some(writer) => receive_worker(writer, "stdin writer", deadline),
        None => Ok(Ok(())),
    }
}

fn receive_bounded_reader(
    reader: mpsc::Receiver<BoundedCapture>,
    label: &str,
    deadline: Instant,
) -> Result<BoundedCapture, AiExecutionError> {
    receive_worker(reader, &format!("{label} reader"), deadline)
}

fn receive_worker<T>(
    receiver: mpsc::Receiver<T>,
    label: &str,
    deadline: Instant,
) -> Result<T, AiExecutionError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    receiver.recv_timeout(remaining).map_err(|error| {
        let diagnostic = match error {
            mpsc::RecvTimeoutError::Timeout => {
                format!("Timed out waiting for Codex {label} completion")
            }
            mpsc::RecvTimeoutError::Disconnected => {
                format!("Codex {label} stopped without reporting completion")
            }
        };
        AiExecutionError::new(AiExecutionErrorKind::Unknown, diagnostic, None)
    })
}

struct ProcessTree {
    #[cfg(target_os = "windows")]
    job: WindowsJob,
}

impl ProcessTree {
    fn new() -> std::io::Result<Self> {
        Ok(Self {
            #[cfg(target_os = "windows")]
            job: WindowsJob::new()?,
        })
    }

    fn assign(&self, child: &Child) -> std::io::Result<()> {
        #[cfg(target_os = "windows")]
        {
            #[cfg(debug_assertions)]
            if FORCE_NEXT_JOB_ASSIGNMENT_FAILURE.swap(false, Ordering::SeqCst) {
                return Err(std::io::Error::other(
                    "injected Job Object assignment failure",
                ));
            }
            self.job.assign(child)?;
        }
        #[cfg(not(target_os = "windows"))]
        let _ = child;
        Ok(())
    }

    fn terminate(&self, child: &mut Child) -> Result<(), AiExecutionError> {
        #[cfg(target_os = "windows")]
        {
            if let Err(error) = self.job.terminate() {
                let _ = terminate_child_direct(child);
                return Err(AiExecutionError::new(
                    AiExecutionErrorKind::Unknown,
                    format!("Failed to terminate Codex CLI process tree: {error}"),
                    None,
                ));
            }
            wait_for_child_exit(child, PROCESS_TERMINATION_TIMEOUT)
        }
        #[cfg(not(target_os = "windows"))]
        terminate_child_direct(child)
    }
}

#[cfg(all(target_os = "windows", debug_assertions))]
static FORCE_NEXT_JOB_ASSIGNMENT_FAILURE: AtomicBool = AtomicBool::new(false);

#[doc(hidden)]
#[cfg(all(target_os = "windows", debug_assertions))]
pub fn force_next_job_assignment_failure_for_test() {
    FORCE_NEXT_JOB_ASSIGNMENT_FAILURE.store(true, Ordering::SeqCst);
}

fn process_job_error(error: std::io::Error) -> AiExecutionError {
    AiExecutionError::new(
        AiExecutionErrorKind::CliFailed,
        format!("Failed to prepare Codex CLI process isolation: {error}"),
        None,
    )
}

#[cfg(target_os = "windows")]
struct WindowsJob {
    handle: windows::Win32::Foundation::HANDLE,
}

#[cfg(target_os = "windows")]
struct WindowsSuspendedThread {
    handle: windows::Win32::Foundation::HANDLE,
}

#[cfg(target_os = "windows")]
impl WindowsSuspendedThread {
    fn open(process_id: u32) -> std::io::Result<Self> {
        use windows::Win32::System::Threading::{OpenThread, THREAD_SUSPEND_RESUME};

        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot.is_invalid() {
            return Err(std::io::Error::last_os_error());
        }
        let snapshot = WindowsSnapshot { handle: snapshot };
        let mut entry = ThreadEntry32 {
            size: std::mem::size_of::<ThreadEntry32>() as u32,
            ..ThreadEntry32::default()
        };
        let mut available = unsafe { Thread32First(snapshot.handle, &raw mut entry) } != 0;
        while available {
            if entry.owner_process_id == process_id {
                let handle = unsafe { OpenThread(THREAD_SUSPEND_RESUME, false, entry.thread_id) }
                    .map_err(std::io::Error::other)?;
                return Ok(Self { handle });
            }
            entry.size = std::mem::size_of::<ThreadEntry32>() as u32;
            available = unsafe { Thread32Next(snapshot.handle, &raw mut entry) } != 0;
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no thread found for suspended process {process_id}"),
        ))
    }

    fn resume(self) -> std::io::Result<()> {
        use windows::Win32::System::Threading::ResumeThread;

        let previous_suspend_count = unsafe { ResumeThread(self.handle) };
        match previous_suspend_count {
            1 => Ok(()),
            u32::MAX => Err(std::io::Error::last_os_error()),
            count => Err(std::io::Error::other(format!(
                "unexpected main-thread suspend count {count}"
            ))),
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for WindowsSuspendedThread {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(target_os = "windows")]
struct WindowsSnapshot {
    handle: windows::Win32::Foundation::HANDLE,
}

#[cfg(target_os = "windows")]
impl Drop for WindowsSnapshot {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(target_os = "windows")]
impl WindowsJob {
    fn new() -> std::io::Result<Self> {
        use std::ffi::c_void;

        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_invalid() {
            return Err(std::io::Error::last_os_error());
        }
        let job = Self { handle };
        let mut information = JobObjectExtendedLimitInformation::default();
        information.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                job.handle,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
                (&raw const information).cast::<c_void>(),
                std::mem::size_of::<JobObjectExtendedLimitInformation>() as u32,
            )
        };
        if configured == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(job)
    }

    fn assign(&self, child: &Child) -> std::io::Result<()> {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::HANDLE;

        let process = HANDLE(child.as_raw_handle());
        let assigned = unsafe { AssignProcessToJobObject(self.handle, process) };
        if assigned == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn terminate(&self) -> std::io::Result<()> {
        let terminated = unsafe { TerminateJobObject(self.handle, 1) };
        if terminated == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(target_os = "windows")]
const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
#[cfg(target_os = "windows")]
const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: i32 = 9;
#[cfg(target_os = "windows")]
const TH32CS_SNAPTHREAD: u32 = 0x0000_0004;

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Default)]
struct ThreadEntry32 {
    size: u32,
    usage_count: u32,
    thread_id: u32,
    owner_process_id: u32,
    base_priority: i32,
    priority_delta: i32,
    flags: u32,
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Default)]
struct JobObjectBasicLimitInformation {
    per_process_user_time_limit: i64,
    per_job_user_time_limit: i64,
    limit_flags: u32,
    minimum_working_set_size: usize,
    maximum_working_set_size: usize,
    active_process_limit: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Default)]
struct JobObjectExtendedLimitInformation {
    basic_limit_information: JobObjectBasicLimitInformation,
    io_info: windows::Win32::System::Threading::IO_COUNTERS,
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_used: usize,
    peak_job_memory_used: usize,
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateJobObjectW(
        job_attributes: *const std::ffi::c_void,
        name: *const u16,
    ) -> windows::Win32::Foundation::HANDLE;
    fn SetInformationJobObject(
        job: windows::Win32::Foundation::HANDLE,
        information_class: i32,
        information: *const std::ffi::c_void,
        information_length: u32,
    ) -> i32;
    fn AssignProcessToJobObject(
        job: windows::Win32::Foundation::HANDLE,
        process: windows::Win32::Foundation::HANDLE,
    ) -> i32;
    fn TerminateJobObject(job: windows::Win32::Foundation::HANDLE, exit_code: u32) -> i32;
    fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> windows::Win32::Foundation::HANDLE;
    fn Thread32First(
        snapshot: windows::Win32::Foundation::HANDLE,
        entry: *mut ThreadEntry32,
    ) -> i32;
    fn Thread32Next(snapshot: windows::Win32::Foundation::HANDLE, entry: *mut ThreadEntry32)
    -> i32;
}

fn process_spawn_error(executable: &str, error: std::io::Error) -> AiExecutionError {
    let kind = if error.kind() == std::io::ErrorKind::NotFound {
        AiExecutionErrorKind::CliUnavailable
    } else if error.kind() == std::io::ErrorKind::PermissionDenied
        || error.raw_os_error() == Some(5)
    {
        AiExecutionErrorKind::PermissionDenied
    } else {
        AiExecutionErrorKind::CliFailed
    };
    AiExecutionError::new(
        kind,
        format!("Failed to start configured Codex executable {executable:?}: {error}"),
        None,
    )
}

fn sanitize_execution_error(
    mut error: AiExecutionError,
    request: &AiExecutionRequest,
) -> AiExecutionError {
    error.diagnostic = sanitize_diagnostic(&error.diagnostic, request);
    error
}

fn sanitize_diagnostic(value: &str, request: &AiExecutionRequest) -> String {
    sanitize_ai_diagnostic(
        value,
        &[
            (&request.minimal_payload_json, "[payload redacted]"),
            (&request.system_prompt, "[prompt redacted]"),
        ],
    )
}

fn sanitize_health_diagnostic(value: &str) -> String {
    sanitize_ai_diagnostic(value, &[])
}

fn bounded_text(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn codex_working_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data"))
        .join("DailyTaskMonitor")
        .join("data")
}
