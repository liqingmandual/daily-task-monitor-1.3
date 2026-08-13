#![cfg(feature = "desktop")]

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::sync::{
    Arc, Mutex, MutexGuard, OnceLock,
    atomic::{AtomicUsize, Ordering},
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject};

use daily_task_monitor_core::ai::{AiExecutionMode, AiExecutionSnapshot, AiProviderConfig};
#[cfg(all(target_os = "windows", debug_assertions))]
use daily_task_monitor_core::ai_executor::force_next_job_assignment_failure_for_test;
use daily_task_monitor_core::ai_executor::{
    AiExecutionBackends, AiExecutionErrorKind, AiExecutionRequest, ApiProviderCredential,
    CodexExecutionConfig, CodexHealthStatus, codex_executable_candidates_with, execute_ai,
    probe_codex_health, resolve_codex_executable_from_candidates_with, test_codex_inference,
};

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
        .expect("test runtime")
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "daily-task-monitor-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("create test directory");
    path
}

fn fake_codex_path() -> &'static Path {
    static FAKE_CODEX: OnceLock<PathBuf> = OnceLock::new();
    FAKE_CODEX.get_or_init(|| {
        let directory = unique_temp_dir("fake-codex");
        let source = directory.join("fake_codex.rs");
        let executable = directory.join(if cfg!(windows) {
            "fake-codex.exe"
        } else {
            "fake-codex"
        });
        fs::write(
            &source,
        r##"
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::process::Command;
use std::thread;
use std::time::Duration;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if let Ok(marker) = env::var("FAKE_CODEX_COMMAND_LOG") {
        let mut log = fs::OpenOptions::new().create(true).append(true).open(marker).unwrap();
        writeln!(log, "{}", args.join(" ")).unwrap();
    }
    if args == ["--version"] {
        println!("codex-cli 9.8.7");
        return;
    }
    if args == ["login", "status"] {
        if env::var_os("FAKE_CODEX_LOGIN_FAILURE").is_some() {
            eprintln!("not logged in");
            std::process::exit(18);
        }
        println!("Logged in using ChatGPT");
        return;
    }

    let prompt = args.last().map(String::as_str).unwrap_or_default();
    if let Some(markers) = prompt.strip_prefix("descendant-timeout:") {
        let (pid_marker, completion_marker) = markers.split_once('|').unwrap();
        thread::sleep(Duration::from_millis(250));
        #[cfg(target_os = "windows")]
        let helper = Command::new("cmd.exe")
            .args([
                "/D",
                "/C",
                &format!(
                    "ping.exe -n 31 127.0.0.1 > nul && echo completed>\"{completion_marker}\""
                ),
            ])
            .spawn()
            .unwrap();
        #[cfg(not(target_os = "windows"))]
        let helper = Command::new(env::current_exe().unwrap())
            .arg(format!("pipe-holder:{completion_marker}"))
            .spawn()
            .unwrap();
        fs::write(pid_marker, helper.id().to_string()).unwrap();
        thread::sleep(Duration::from_secs(60));
        return;
    }
    if let Some(markers) = prompt.strip_prefix("assignment-failure:") {
        let mut markers = markers.split('|');
        let execution_marker = markers.next().unwrap();
        let pid_marker = markers.next().unwrap();
        let completion_marker = markers.next().unwrap();
        fs::write(execution_marker, "executed").unwrap();
        #[cfg(target_os = "windows")]
        let helper = Command::new("cmd.exe")
            .args([
                "/D",
                "/C",
                &format!(
                    "echo descendant>\"{completion_marker}\" && ping.exe -n 11 127.0.0.1 > nul"
                ),
            ])
            .spawn()
            .unwrap();
        #[cfg(not(target_os = "windows"))]
        let helper = Command::new(env::current_exe().unwrap())
            .arg(format!("pipe-holder:{completion_marker}"))
            .spawn()
            .unwrap();
        fs::write(pid_marker, helper.id().to_string()).unwrap();
        thread::sleep(Duration::from_secs(10));
        return;
    }
    if let Some(completion_marker) = prompt.strip_prefix("pipe-holder:") {
        thread::sleep(Duration::from_secs(10));
        fs::write(completion_marker, "completed").unwrap();
        return;
    }

    let mut stdin = String::new();
    io::stdin().read_to_string(&mut stdin).unwrap();
    if prompt.contains("health check") {
        match env::var("FAKE_CODEX_MANUAL_MODE").as_deref() {
            Ok("fail") => {
                eprintln!("manual inference failed");
                std::process::exit(19);
            }
            Ok("timeout") => {
                thread::sleep(Duration::from_secs(5));
                return;
            }
            _ => {
                println!("{}", r#"{"content":"{\"ok\":true}","model":"health-model"}"#);
                return;
            }
        }
    }
    match prompt {
        "success" => {
            let expected = [
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
                "--model",
                "snapshot-model",
                "success",
            ];
            if args.iter().map(String::as_str).collect::<Vec<_>>() != expected {
                eprintln!("unexpected args: {args:?}");
                std::process::exit(91);
            }
            if stdin != r#"{"bounded":"payload"}"# {
                eprintln!("unexpected stdin: {stdin}");
                std::process::exit(92);
            }
            println!("{}", r#"{"content":"{\"accepted\":true}","model":"actual-codex-model"}"#);
        }
        "default-model" => {
            if args.iter().any(|arg| arg == "--model") {
                eprintln!("empty model must omit --model");
                std::process::exit(93);
            }
            println!("{}", r#"{"accepted":true}"#);
        }
        "nonzero-sensitive" => {
            eprintln!("Authorization: Bearer sk-secret-token; api_key=private-key; prompt=nonzero-sensitive; payload={stdin}");
            std::process::exit(17);
        }
        "multiple-json" => println!("{{}}\n{{}}"),
        marker if marker.starts_with("overflow:") => {
            let markers = marker.trim_start_matches("overflow:");
            let (started_marker, completion_marker) = markers.split_once('|').unwrap();
            fs::write(started_marker, "started").unwrap();
            let chunk = vec![b'x'; 64 * 1024];
            let mut stdout = io::stdout().lock();
            for _ in 0..32 {
                stdout.write_all(&chunk).unwrap();
                stdout.flush().unwrap();
            }
            thread::sleep(Duration::from_secs(2));
            fs::write(completion_marker, "completed").unwrap();
        }
        marker if marker.starts_with("blocked-stdin:") => {
            let markers = marker.trim_start_matches("blocked-stdin:");
            let (started_marker, completion_marker) = markers.split_once('|').unwrap();
            fs::write(started_marker, "started").unwrap();
            thread::sleep(Duration::from_secs(30));
            fs::write(completion_marker, "completed").unwrap();
        }
        marker if marker.starts_with("timeout:") => {
            thread::sleep(Duration::from_secs(5));
            fs::write(marker.trim_start_matches("timeout:"), "completed").unwrap();
        }
        marker if marker.starts_with("marker:") => {
            fs::write(marker.trim_start_matches("marker:"), "invoked").unwrap();
            println!("{{}}");
        }
        other => {
            eprintln!("unknown fake prompt: {other}");
            std::process::exit(94);
        }
    }
}
"##,
        )
        .expect("write fake Codex source");
        let status = Command::new("rustc")
            .arg("--edition=2024")
            .arg(&source)
            .arg("-o")
            .arg(&executable)
            .status()
            .expect("rustc must be available for fake CLI tests");
        assert!(status.success(), "fake Codex CLI must compile");
        executable
    })
}

fn codex_process_lock() -> MutexGuard<'static, ()> {
    static PROCESS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    PROCESS_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn snapshot(mode: AiExecutionMode, executor_id: String, model: &str) -> AiExecutionSnapshot {
    AiExecutionSnapshot {
        execution_mode: mode,
        executor_id,
        model: model.into(),
        evidence_hash: "evidence-hash".into(),
        created_at_ms: 100,
    }
}

fn request(
    mode: AiExecutionMode,
    executor_id: String,
    model: &str,
    prompt: &str,
) -> AiExecutionRequest {
    AiExecutionRequest {
        job_id: "job-1".into(),
        kind: "classification".into(),
        snapshot: snapshot(mode, executor_id, model),
        system_prompt: prompt.into(),
        minimal_payload_json: r#"{"bounded":"payload"}"#.into(),
        timeout_ms: 10_000,
    }
}

fn provider(id: &str, base_url: String, priority: i32) -> AiProviderConfig {
    AiProviderConfig {
        id: id.into(),
        name: id.into(),
        base_url,
        model: format!("{id}-configured-model"),
        enabled: true,
        auto_safe: true,
        priority,
        has_credential: true,
    }
}

fn backends(api_providers: Vec<ApiProviderCredential>) -> AiExecutionBackends {
    AiExecutionBackends {
        api_providers,
        configured_codex: Some(CodexExecutionConfig {
            executable: fake_codex_path().to_string_lossy().into_owned(),
            model: "changed-after-enqueue".into(),
        }),
    }
}

fn spawn_json_server(
    status: u16,
    response_json: &'static str,
) -> (String, Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback server");
    let address = listener.local_addr().expect("loopback address");
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept API request");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set read timeout");
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 4096];
        let header_end = loop {
            let read = stream.read(&mut chunk).expect("read request headers");
            assert!(read > 0, "request ended before headers");
            bytes.extend_from_slice(&chunk[..read]);
            if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                break index + 4;
            }
        };
        let headers = String::from_utf8_lossy(&bytes[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().expect("content length"))
                })
            })
            .unwrap_or_default();
        while bytes.len() - header_end < content_length {
            let read = stream.read(&mut chunk).expect("read request body");
            assert!(read > 0, "request ended before body");
            bytes.extend_from_slice(&chunk[..read]);
        }
        sender
            .send(
                String::from_utf8(bytes[header_end..header_end + content_length].to_vec())
                    .expect("UTF-8 body"),
            )
            .expect("send captured body");
        let reason = if status == 200 { "OK" } else { "ERROR" };
        write!(
            stream,
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_json}",
            response_json.len()
        )
        .expect("write API response");
    });
    (format!("http://{address}/v1"), receiver, handle)
}

#[test]
fn codex_health_reports_version_and_explicit_failures() {
    let _process_guard = codex_process_lock();
    let executable = fake_codex_path().to_string_lossy().into_owned();
    let healthy = probe_codex_health(&executable, None, 10_000);
    assert_eq!(healthy.status, CodexHealthStatus::Healthy);
    assert_eq!(healthy.version.as_deref(), Some("codex-cli 9.8.7"));

    let missing = probe_codex_health("missing-codex-executable", None, 100);
    assert_eq!(missing.status, CodexHealthStatus::Unavailable);
    assert!(missing.version.is_none());

    let directory = unique_temp_dir("permission-denied");
    let denied = probe_codex_health(&directory.to_string_lossy(), None, 100);
    assert_eq!(denied.status, CodexHealthStatus::PermissionDenied);
}

#[test]
fn codex_health_serializes_the_exact_structured_contract() {
    let _process_guard = codex_process_lock();
    let executable = fake_codex_path().to_string_lossy().into_owned();
    let health = probe_codex_health(&executable, None, 10_000);
    let value = serde_json::to_value(&health).expect("serialize health DTO");
    let object = value.as_object().expect("health DTO object");

    let mut keys = object.keys().map(String::as_str).collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "checkedAtMs",
            "configuredPath",
            "detectedPath",
            "diagnostic",
            "status",
            "version",
        ]
    );
    assert_eq!(value["configuredPath"], executable);
    assert_eq!(value["detectedPath"], executable);
    assert!(value["checkedAtMs"].as_i64().unwrap_or_default() > 0);
    assert_eq!(value["status"], "healthy");
}

#[test]
fn explicit_codex_path_never_falls_back_to_a_detected_path() {
    let configured = unique_temp_dir("explicit-missing").join("missing-codex.exe");
    let health = probe_codex_health(&configured.to_string_lossy(), None, 10_000);

    assert_eq!(health.status, CodexHealthStatus::Unavailable);
    assert!(health.version.is_none());
}

#[test]
fn codex_candidates_prefer_app_then_local_cli_then_every_path_match() {
    let root = unique_temp_dir("candidate-order");
    let local = root.join("LocalAppData");
    let app_old = local.join("OpenAI/Codex/bin/100/codex.exe");
    let app_new = local.join("OpenAI/Codex/bin/200/codex.exe");
    let local_cli = local.join("DailyTaskMonitor/codex-cli/node_modules/@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin/codex.exe");
    let path_one = root.join("path-one");
    let path_two = root.join("path-two");
    for path in [
        &app_old,
        &app_new,
        &local_cli,
        &path_one.join("codex.exe"),
        &path_two.join("codex.exe"),
    ] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "candidate").unwrap();
    }
    let search_path = std::env::join_paths([&path_one, &path_two]).unwrap();

    let candidates = codex_executable_candidates_with(
        "codex",
        Some(&local),
        None,
        None,
        Some(search_path.as_os_str()),
        Some(std::ffi::OsStr::new(".EXE")),
    );

    let normalize = |path: &Path| {
        path.to_string_lossy()
            .replace('/', "\\")
            .to_ascii_lowercase()
    };
    assert_eq!(
        candidates
            .iter()
            .map(|path| normalize(path))
            .collect::<Vec<_>>(),
        [
            app_new,
            app_old,
            local_cli,
            path_one.join("codex.exe"),
            path_two.join("codex.exe")
        ]
        .iter()
        .map(|path| normalize(path))
        .collect::<Vec<_>>()
    );
}

#[test]
fn resolver_skips_windowsapps_and_unlaunchable_candidates() {
    let candidates = vec![
        PathBuf::from(r"C:\Users\me\AppData\Local\Microsoft\WindowsApps\codex.exe"),
        PathBuf::from(r"C:\broken\codex.exe"),
        PathBuf::from(r"C:\working\codex.exe"),
    ];
    let attempted = Arc::new(Mutex::new(Vec::new()));
    let captured = attempted.clone();

    let resolved =
        resolve_codex_executable_from_candidates_with("codex", candidates, move |path| {
            captured.lock().unwrap().push(path.to_path_buf());
            if path == Path::new(r"C:\working\codex.exe") {
                Ok("codex-cli 1.0".to_string())
            } else {
                Err(AiExecutionErrorKind::CliUnavailable)
            }
        })
        .unwrap();

    assert_eq!(resolved.path, PathBuf::from(r"C:\working\codex.exe"));
    assert_eq!(
        *attempted.lock().unwrap(),
        vec![
            PathBuf::from(r"C:\broken\codex.exe"),
            PathBuf::from(r"C:\working\codex.exe")
        ]
    );
}

#[test]
fn explicit_absolute_candidate_never_falls_back() {
    let explicit = PathBuf::from(r"C:\fixed\codex.exe");
    let fallback = PathBuf::from(r"C:\fallback\codex.exe");
    let candidates = codex_executable_candidates_with(
        explicit.to_str().unwrap(),
        None,
        None,
        None,
        Some(
            std::env::join_paths([fallback.parent().unwrap()])
                .unwrap()
                .as_os_str(),
        ),
        Some(std::ffi::OsStr::new(".EXE")),
    );
    assert_eq!(candidates, vec![explicit]);
}

#[test]
fn codex_candidates_include_gui_safe_macos_locations() {
    let root = unique_temp_dir("macos-codex-candidates");
    let home = root.join("home");
    let applications = root.join("Applications");
    let local_cli = home.join(".local/bin/codex");
    let nvm_old = home.join(".nvm/versions/node/v20.0.0/bin/codex");
    let nvm_new = home.join(".nvm/versions/node/v22.0.0/bin/codex");
    let bundled_cli = applications.join("ChatGPT.app/Contents/Resources/codex");
    for path in [&local_cli, &nvm_old, &nvm_new, &bundled_cli] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "candidate").unwrap();
    }

    let candidates = codex_executable_candidates_with(
        "codex",
        None,
        Some(&home),
        Some(&applications),
        None,
        None,
    );

    assert_eq!(candidates, vec![local_cli, nvm_new, nvm_old, bundled_cli]);
}

#[test]
fn background_health_requires_login_and_never_runs_inference() {
    let _process_guard = codex_process_lock();
    let marker = unique_temp_dir("background-command-log").join("commands.txt");
    unsafe { std::env::set_var("FAKE_CODEX_COMMAND_LOG", &marker) };
    let executable = fake_codex_path().to_string_lossy().into_owned();

    let health = probe_codex_health(&executable, None, 10_000);

    unsafe { std::env::remove_var("FAKE_CODEX_COMMAND_LOG") };
    assert_eq!(health.status, CodexHealthStatus::Healthy);
    let commands = fs::read_to_string(marker).unwrap();
    assert!(commands.contains("--version"));
    assert!(commands.contains("login status"));
    assert!(!commands.contains("exec"));
}

#[test]
fn version_success_but_login_failure_is_not_healthy() {
    let _process_guard = codex_process_lock();
    unsafe { std::env::set_var("FAKE_CODEX_LOGIN_FAILURE", "1") };
    let health = probe_codex_health(&fake_codex_path().to_string_lossy(), None, 10_000);
    unsafe { std::env::remove_var("FAKE_CODEX_LOGIN_FAILURE") };

    assert_ne!(health.status, CodexHealthStatus::Healthy);
    assert_eq!(health.version.as_deref(), Some("codex-cli 9.8.7"));
}

#[test]
fn manual_codex_inference_reports_success_failure_and_timeout() {
    let _process_guard = codex_process_lock();
    let executable = fake_codex_path().to_string_lossy().into_owned();

    let success = runtime().block_on(test_codex_inference(&executable, "", 10_000));
    assert_eq!(success.status, CodexHealthStatus::Healthy);
    assert_eq!(success.version.as_deref(), Some("codex-cli 9.8.7"));

    unsafe { std::env::set_var("FAKE_CODEX_MANUAL_MODE", "fail") };
    let failure = runtime().block_on(test_codex_inference(&executable, "", 10_000));
    unsafe { std::env::set_var("FAKE_CODEX_MANUAL_MODE", "timeout") };
    let timeout = runtime().block_on(test_codex_inference(&executable, "", 50));
    unsafe { std::env::remove_var("FAKE_CODEX_MANUAL_MODE") };

    assert_eq!(failure.status, CodexHealthStatus::Error);
    assert_eq!(timeout.status, CodexHealthStatus::TimedOut);
}

#[test]
fn codex_uses_queued_path_and_model_with_minimal_payload_on_stdin() {
    let _process_guard = codex_process_lock();
    let queued_path = fake_codex_path().to_string_lossy().into_owned();
    let output = runtime()
        .block_on(execute_ai(
            &request(
                AiExecutionMode::Codex,
                queued_path.clone(),
                "snapshot-model",
                "success",
            ),
            &backends(Vec::new()),
        ))
        .expect("fake Codex succeeds");

    assert_eq!(output.content, r#"{"accepted":true}"#);
    assert_eq!(output.executor_id, queued_path);
    assert_eq!(output.model, "actual-codex-model");
    assert_eq!(output.exit_code, Some(0));
}

#[test]
fn codex_omits_empty_model_and_rejects_more_than_one_stdout_object() {
    let _process_guard = codex_process_lock();
    let executable = fake_codex_path().to_string_lossy().into_owned();
    let output = runtime()
        .block_on(execute_ai(
            &request(
                AiExecutionMode::Codex,
                executable.clone(),
                "",
                "default-model",
            ),
            &backends(Vec::new()),
        ))
        .expect("default model invocation succeeds");
    assert_eq!(output.model, "cli-default");

    let error = runtime()
        .block_on(execute_ai(
            &request(
                AiExecutionMode::Codex,
                executable,
                "cli-default",
                "multiple-json",
            ),
            &backends(Vec::new()),
        ))
        .expect_err("multiple stdout objects must fail");
    assert_eq!(error.kind, AiExecutionErrorKind::InvalidResponse);
}

#[test]
fn empty_queued_codex_executable_is_not_configured_and_never_uses_fallback() {
    let error = runtime()
        .block_on(execute_ai(
            &request(AiExecutionMode::Codex, String::new(), "", "default-model"),
            &backends(Vec::new()),
        ))
        .expect_err("empty queued executable must not run configured or PATH fallback");

    assert_eq!(error.kind, AiExecutionErrorKind::NotConfigured);
    assert_eq!(error.executor_id.as_deref(), Some(""));
    assert_eq!(error.model.as_deref(), Some("cli-default"));
}

#[test]
fn codex_nonzero_exit_is_auditable_and_sensitive_diagnostics_are_bounded() {
    let _process_guard = codex_process_lock();
    let executable = fake_codex_path().to_string_lossy().into_owned();
    let error = runtime()
        .block_on(execute_ai(
            &request(
                AiExecutionMode::Codex,
                executable,
                "snapshot-model",
                "nonzero-sensitive",
            ),
            &backends(Vec::new()),
        ))
        .expect_err("nonzero exit must fail");

    assert_eq!(error.exit_code, Some(17));
    assert!(error.diagnostic.len() <= 1_000);
    assert!(!error.diagnostic.contains("sk-secret-token"));
    assert!(!error.diagnostic.contains("private-key"));
    assert!(!error.diagnostic.contains("nonzero-sensitive"));
    assert!(!error.diagnostic.contains(r#"{"bounded":"payload"}"#));
}

#[test]
fn codex_timeout_kills_the_process_and_never_falls_back_to_api() {
    let _process_guard = codex_process_lock();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind unused API endpoint");
    let address = listener.local_addr().expect("unused API address");
    listener
        .set_nonblocking(true)
        .expect("nonblocking listener");
    let api_hits = Arc::new(AtomicUsize::new(0));
    let observed_hits = Arc::clone(&api_hits);
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            match listener.accept() {
                Ok(_) => {
                    observed_hits.fetch_add(1, Ordering::SeqCst);
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("API listener failed: {error}"),
            }
        }
    });
    let providers = vec![ApiProviderCredential {
        provider: provider("must-not-run", format!("http://{address}/v1"), 0),
        api_key: "unused".into(),
    }];
    let executable = fake_codex_path().to_string_lossy().into_owned();
    let completion_marker = unique_temp_dir("timeout-kill").join("completed");
    let mut timed = request(
        AiExecutionMode::Codex,
        executable,
        "snapshot-model",
        &format!("timeout:{}", completion_marker.display()),
    );
    timed.timeout_ms = 75;
    let runtime = runtime();
    let error = runtime
        .block_on(execute_ai(&timed, &backends(providers)))
        .expect_err("timeout must fail");

    assert_eq!(error.kind, AiExecutionErrorKind::Timeout);
    thread::sleep(Duration::from_millis(250));
    assert!(!completion_marker.exists(), "timed-out CLI was not killed");
    server.join().expect("join API observer");
    assert_eq!(api_hits.load(Ordering::SeqCst), 0);
}

#[cfg(target_os = "windows")]
#[test]
fn codex_timeout_terminates_pipe_holding_descendant_without_waiting_for_io_threads() {
    let _process_guard = codex_process_lock();
    let executable = fake_codex_path().to_string_lossy().into_owned();
    let warmup = request(
        AiExecutionMode::Codex,
        executable.clone(),
        "snapshot-model",
        "success",
    );
    let warmup_started = Instant::now();
    runtime()
        .block_on(execute_ai(&warmup, &backends(Vec::new())))
        .expect("measure isolated fake Codex startup");
    let timeout = warmup_started.elapsed() + Duration::from_secs(10);
    let marker_dir = unique_temp_dir("descendant-timeout");
    let pid_marker = marker_dir.join("helper-pid");
    let completion_marker = marker_dir.join("helper-completed");
    let mut timed = request(
        AiExecutionMode::Codex,
        executable,
        "snapshot-model",
        &format!(
            "descendant-timeout:{}|{}",
            pid_marker.display(),
            completion_marker.display()
        ),
    );
    timed.timeout_ms = timeout.as_millis().min(u64::MAX as u128) as u64;

    let started = Instant::now();
    let error = runtime()
        .block_on(execute_ai(&timed, &backends(Vec::new())))
        .expect_err("descendant-holding timeout must fail");
    let elapsed = started.elapsed();

    assert_eq!(error.kind, AiExecutionErrorKind::Timeout);
    let helper_pid = wait_for_pid_marker(&pid_marker);
    assert!(
        elapsed < timeout + Duration::from_secs(2),
        "descendant-held pipes delayed timeout return: {elapsed:?}"
    );
    assert!(
        wait_for_process_exit(helper_pid, Duration::from_secs(1)),
        "pipe-holding helper process {helper_pid} survived timeout"
    );
    assert!(
        !completion_marker.exists(),
        "pipe-holding helper ran after timeout"
    );
}

#[cfg(all(target_os = "windows", debug_assertions))]
#[test]
fn codex_assignment_failure_never_runs_the_suspended_process_or_descendant() {
    let _process_guard = codex_process_lock();
    let executable = fake_codex_path().to_string_lossy().into_owned();
    let warmup = Command::new(&executable)
        .arg("--version")
        .output()
        .expect("warm fake Codex executable");
    assert!(warmup.status.success(), "fake Codex warmup must succeed");
    let marker_dir = unique_temp_dir("assignment-failure");
    let execution_marker = marker_dir.join("process-executed");
    let pid_marker = marker_dir.join("descendant-pid");
    let completion_marker = marker_dir.join("descendant-executed");
    let request = request(
        AiExecutionMode::Codex,
        executable.clone(),
        "snapshot-model",
        &format!(
            "assignment-failure:{}|{}|{}",
            execution_marker.display(),
            pid_marker.display(),
            completion_marker.display()
        ),
    );
    force_next_job_assignment_failure_for_test();

    let started = Instant::now();
    let error = runtime()
        .block_on(execute_ai(&request, &backends(Vec::new())))
        .expect_err("forced Job Object assignment failure must fail");
    let elapsed = started.elapsed();

    assert_eq!(error.kind, AiExecutionErrorKind::CliFailed);
    assert_eq!(error.exit_code, None);
    assert_eq!(error.executor_id.as_deref(), Some(executable.as_str()));
    assert_eq!(error.model.as_deref(), Some("snapshot-model"));
    assert!(
        error
            .diagnostic
            .starts_with("Failed to isolate Codex CLI process:"),
        "unexpected normalized diagnostic: {}",
        error.diagnostic
    );
    assert!(
        elapsed < Duration::from_secs(7),
        "assignment failure did not return promptly: {elapsed:?}"
    );
    thread::sleep(Duration::from_millis(250));
    assert!(!execution_marker.exists(), "suspended process executed");
    assert!(
        !pid_marker.exists(),
        "suspended process created a descendant"
    );
    assert!(
        !completion_marker.exists(),
        "descendant from suspended process executed"
    );
}

#[cfg(target_os = "windows")]
fn wait_for_pid_marker(marker: &Path) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if let Ok(pid) = fs::read_to_string(marker) {
            return pid.parse().expect("helper PID marker must be numeric");
        }
        assert!(
            Instant::now() < deadline,
            "helper PID marker was not written"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(target_os = "windows")]
fn wait_for_process_exit(pid: u32, timeout: Duration) -> bool {
    let process = match unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) } {
        Ok(process) => process,
        Err(_) => return true,
    };
    let wait_ms = timeout.as_millis().min(u32::MAX as u128) as u32;
    let result = unsafe { WaitForSingleObject(process, wait_ms) };
    unsafe {
        let _ = CloseHandle(process);
    }
    result == WAIT_OBJECT_0
}

#[test]
fn codex_stdout_overflow_is_detected_live_and_kills_the_child() {
    let _process_guard = codex_process_lock();
    let executable = fake_codex_path().to_string_lossy().into_owned();
    let marker_dir = unique_temp_dir("stdout-overflow");
    let started_marker = marker_dir.join("started");
    let completion_marker = marker_dir.join("completed");
    let mut overflowing = request(
        AiExecutionMode::Codex,
        executable,
        "snapshot-model",
        &format!(
            "overflow:{}|{}",
            started_marker.display(),
            completion_marker.display()
        ),
    );
    overflowing.timeout_ms = 10_000;

    let started = Instant::now();
    let error = runtime()
        .block_on(execute_ai(&overflowing, &backends(Vec::new())))
        .expect_err("stdout overflow must fail");

    assert_eq!(error.kind, AiExecutionErrorKind::InvalidResponse);
    let overflow_started_at = fs::metadata(&started_marker)
        .and_then(|metadata| metadata.modified())
        .expect("overflow fixture must report when stdout begins");
    let detection_latency = SystemTime::now()
        .duration_since(overflow_started_at)
        .expect("overflow start marker must not be in the future");
    assert!(
        detection_latency < Duration::from_secs(5),
        "overflow was not reported promptly after stdout began: {detection_latency:?}; total elapsed: {:?}",
        started.elapsed()
    );
    thread::sleep(Duration::from_millis(150));
    assert!(
        !completion_marker.exists(),
        "overflowing CLI was not killed"
    );
}

#[test]
fn codex_timeout_starts_before_blocked_stdin_write_and_kills_the_child() {
    let _process_guard = codex_process_lock();
    let executable = fake_codex_path().to_string_lossy().into_owned();
    let warmup = request(
        AiExecutionMode::Codex,
        executable.clone(),
        "snapshot-model",
        "success",
    );
    let warmup_started = Instant::now();
    runtime()
        .block_on(execute_ai(&warmup, &backends(Vec::new())))
        .expect("measure isolated fake Codex startup");
    let timeout = warmup_started.elapsed() + Duration::from_secs(10);
    let marker_dir = unique_temp_dir("blocked-stdin");
    let started_marker = marker_dir.join("started");
    let completion_marker = marker_dir.join("completed");
    let mut blocked = request(
        AiExecutionMode::Codex,
        executable,
        "snapshot-model",
        &format!(
            "blocked-stdin:{}|{}",
            started_marker.display(),
            completion_marker.display()
        ),
    );
    blocked.minimal_payload_json = "x".repeat(8 * 1024 * 1024);
    blocked.timeout_ms = timeout.as_millis().min(u64::MAX as u128) as u64;

    let started = Instant::now();
    let error = runtime()
        .block_on(execute_ai(&blocked, &backends(Vec::new())))
        .expect_err("blocked stdin must time out");

    assert_eq!(error.kind, AiExecutionErrorKind::Timeout);
    assert!(
        started_marker.exists(),
        "blocked-stdin fixture did not begin before the timeout"
    );
    assert!(
        started.elapsed() < timeout + Duration::from_secs(2),
        "blocked stdin ignored the timeout: {:?}",
        started.elapsed()
    );
    thread::sleep(Duration::from_millis(150));
    assert!(
        !completion_marker.exists(),
        "blocked-stdin CLI was not killed"
    );
}

#[test]
fn unavailable_and_permission_denied_map_to_normalized_execution_errors() {
    let missing = runtime()
        .block_on(execute_ai(
            &request(
                AiExecutionMode::Codex,
                "missing-codex-executable".into(),
                "cli-default",
                "success",
            ),
            &backends(Vec::new()),
        ))
        .expect_err("missing executable must fail");
    assert_eq!(missing.kind, AiExecutionErrorKind::CliUnavailable);

    let directory = unique_temp_dir("execute-permission-denied");
    let denied = runtime()
        .block_on(execute_ai(
            &request(
                AiExecutionMode::Codex,
                directory.to_string_lossy().into_owned(),
                "cli-default",
                "success",
            ),
            &backends(Vec::new()),
        ))
        .expect_err("directory cannot be executed");
    assert_eq!(denied.kind, AiExecutionErrorKind::PermissionDenied);
}

#[test]
fn api_errors_sanitize_labeled_secrets_and_malformed_urls() {
    let response = r#"{"error":"Bearer api-secret; API_KEY=key-secret; PrOmPt=prompt-secret; EVIDENCE=evidence-secret; PaYlOaD=payload-secret; HtTpS:/user:pass@example/private?q=query#fragment"}"#;
    let (api_url, body, server) = spawn_json_server(500, response);
    let error = runtime()
        .block_on(execute_ai(
            &request(
                AiExecutionMode::ApiKey,
                "unsafe-api".into(),
                "queued-model",
                "bounded prompt",
            ),
            &backends(vec![ApiProviderCredential {
                provider: provider("unsafe-api", api_url, 0),
                api_key: "request-key".into(),
            }]),
        ))
        .expect_err("provider error must fail");

    assert_eq!(error.kind, AiExecutionErrorKind::Provider);
    assert_eq!(error.executor_id.as_deref(), Some("unsafe-api"));
    assert_eq!(error.model.as_deref(), Some("queued-model"));
    assert!(error.duration_ms.is_some());
    assert!(error.diagnostic.len() <= 1_000);
    for secret in [
        "api-secret",
        "key-secret",
        "prompt-secret",
        "evidence-secret",
        "payload-secret",
        "user:pass",
        "example/private",
        "query#fragment",
    ] {
        assert!(
            !error.diagnostic.contains(secret),
            "API diagnostic leaked {secret}: {}",
            error.diagnostic
        );
    }
    body.recv_timeout(Duration::from_secs(1))
        .expect("API request received");
    server.join().expect("join API server");
}

#[test]
fn codex_health_sanitizes_malformed_urls_in_diagnostics() {
    let configured = "HtTpS:/health-user:health-pass@health.example/private?api_key=health-query#health-fragment";
    let health = probe_codex_health(configured, None, 100);
    let diagnostic = health.diagnostic.expect("missing executable diagnostic");

    assert!(diagnostic.len() <= 1_000);
    for secret in [
        "health-user",
        "health-pass",
        "health.example",
        "private",
        "health-query",
        "health-fragment",
    ] {
        assert!(
            !diagnostic.contains(secret),
            "health diagnostic leaked {secret}: {diagnostic}"
        );
    }
}

#[test]
fn api_snapshot_failure_sends_minimal_payload_and_never_calls_the_next_provider() {
    let (first_url, first_body, first_server) = spawn_json_server(500, r#"{"error":"temporary"}"#);
    let (second_url, second_body, second_server) = spawn_json_server(
        200,
        r#"{"model":"api-actual-model","choices":[{"message":{"content":"{\"accepted\":true}"}}]}"#,
    );
    let providers = vec![
        ApiProviderCredential {
            provider: provider("first", first_url, 0),
            api_key: "first-key".into(),
        },
        ApiProviderCredential {
            provider: provider("second", second_url, 1),
            api_key: "second-key".into(),
        },
    ];
    let error = runtime()
        .block_on(execute_ai(
            &request(
                AiExecutionMode::ApiKey,
                "first".into(),
                "queued-api-model",
                "bounded system prompt",
            ),
            &backends(providers),
        ))
        .expect_err("the selected provider failure must not fall back");

    assert_eq!(error.kind, AiExecutionErrorKind::Provider);
    assert_eq!(error.executor_id.as_deref(), Some("first"));
    assert_eq!(error.model.as_deref(), Some("queued-api-model"));
    assert!(error.diagnostic.contains("HTTP 500"));
    let first = first_body
        .recv_timeout(Duration::from_secs(1))
        .expect("first API body");
    let body: serde_json::Value = serde_json::from_str(&first).expect("JSON request");
    assert_eq!(body["messages"][1]["content"], r#"{"bounded":"payload"}"#);
    let serialized = body.to_string();
    assert!(!serialized.contains("timeline"));
    assert!(!serialized.contains("windowTitle"));
    assert!(!serialized.contains("browserVisits"));
    assert!(
        second_body
            .recv_timeout(Duration::from_millis(100))
            .is_err()
    );
    first_server.join().expect("join first API server");
    drop(second_server);
}

#[test]
fn api_uses_the_queued_primary_and_model_before_current_registry_priority() {
    let (changed_url, _changed_body, _changed_server) = spawn_json_server(
        200,
        r#"{"model":"changed-model","choices":[{"message":{"content":"{\"wrong\":true}"}}]}"#,
    );
    let (queued_url, queued_body, queued_server) = spawn_json_server(
        200,
        r#"{"choices":[{"message":{"content":"{\"accepted\":true}"}}]}"#,
    );
    let providers = vec![
        ApiProviderCredential {
            provider: provider("changed-primary", changed_url, 0),
            api_key: "changed-key".into(),
        },
        ApiProviderCredential {
            provider: provider("queued-primary", queued_url, 99),
            api_key: "queued-key".into(),
        },
    ];

    let output = runtime()
        .block_on(execute_ai(
            &request(
                AiExecutionMode::ApiKey,
                "queued-primary".into(),
                "queued-api-model",
                "bounded system prompt",
            ),
            &backends(providers),
        ))
        .expect("queued primary succeeds");

    assert_eq!(output.executor_id, "queued-primary");
    assert_eq!(output.model, "queued-api-model");
    assert_eq!(output.content, r#"{"accepted":true}"#);
    let body = queued_body
        .recv_timeout(Duration::from_secs(1))
        .expect("queued primary request body");
    let body: serde_json::Value = serde_json::from_str(&body).expect("JSON request");
    assert_eq!(body["model"], "queued-api-model");
    queued_server.join().expect("join queued primary server");
}

#[test]
fn api_failure_never_launches_the_configured_codex_backend() {
    let (api_url, body, server) = spawn_json_server(401, r#"{"error":"unauthorized"}"#);
    let marker_dir = unique_temp_dir("api-no-codex-fallback");
    let marker = marker_dir.join("codex-invoked");
    let providers = vec![ApiProviderCredential {
        provider: provider("api-only", api_url, 0),
        api_key: "bad-key".into(),
    }];
    let context = AiExecutionBackends {
        api_providers: providers,
        configured_codex: Some(CodexExecutionConfig {
            executable: fake_codex_path().to_string_lossy().into_owned(),
            model: format!("marker:{}", marker.display()),
        }),
    };
    let error = runtime()
        .block_on(execute_ai(
            &request(
                AiExecutionMode::ApiKey,
                "api-only".into(),
                "queued-api-model",
                "api request",
            ),
            &context,
        ))
        .expect_err("authentication failure must remain in API mode");

    assert_eq!(error.kind, AiExecutionErrorKind::Authentication);
    assert!(!marker.exists());
    body.recv_timeout(Duration::from_secs(1))
        .expect("API request received");
    server.join().expect("join API server");
}

#[test]
fn modelscope_api_execution_preserves_structured_json_options() {
    let (api_url, body, server) = spawn_json_server(
        200,
        r#"{"choices":[{"message":{"content":"{\"accepted\":true}"}}]}"#,
    );
    let providers = vec![ApiProviderCredential {
        provider: provider("modelscope", api_url, 0),
        api_key: "modelscope-key".into(),
    }];

    runtime()
        .block_on(execute_ai(
            &request(
                AiExecutionMode::ApiKey,
                "modelscope".into(),
                "queued-modelscope-model",
                "bounded prompt",
            ),
            &backends(providers),
        ))
        .expect("ModelScope request succeeds");

    let body = body
        .recv_timeout(Duration::from_secs(1))
        .expect("ModelScope request body");
    let body: serde_json::Value = serde_json::from_str(&body).expect("JSON request");
    assert_eq!(body["enable_thinking"], false);
    server.join().expect("join ModelScope server");
}
