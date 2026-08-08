use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use url::{Host, Url};

pub fn validate_provider_endpoint(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    if url.scheme() == "https" {
        return url.host().is_some();
    }
    if url.scheme() != "http" {
        return false;
    }
    match url.host() {
        Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

pub(crate) fn sanitize_ai_diagnostic(value: &str) -> String {
    const MAX_LENGTH: usize = 240;

    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = normalized.to_ascii_lowercase();
    let contains_windows_path = normalized.as_bytes().windows(3).any(|window| {
        window[0].is_ascii_alphabetic() && window[1] == b':' && matches!(window[2], b'\\' | b'/')
    }) || normalized.contains(r#"\\"#);
    if lower.contains("http://")
        || lower.contains("https://")
        || lower.contains("bearer ")
        || lower.contains("authorization")
        || lower.contains("api_key")
        || lower.contains("api-key")
        || lower.contains("api key")
        || lower.contains("apikey")
        || lower.contains("sk-")
        || contains_windows_path
    {
        return "Diagnostic details redacted".into();
    }

    let mut chars = normalized.chars();
    let prefix = chars.by_ref().take(MAX_LENGTH - 3).collect::<String>();
    if chars.next().is_some() {
        format!("{prefix}...")
    } else {
        normalized
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub enabled: bool,
    pub auto_safe: bool,
    pub priority: i32,
    pub has_credential: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ProviderRegistry {
    providers: Vec<AiProviderConfig>,
}

impl ProviderRegistry {
    pub fn new(providers: Vec<AiProviderConfig>) -> Self {
        Self { providers }
    }

    pub fn automatic_candidates(&self) -> Vec<&AiProviderConfig> {
        let mut providers: Vec<_> = self
            .providers
            .iter()
            .filter(|provider| provider.enabled && provider.auto_safe && provider.has_credential)
            .collect();
        providers.sort_by_key(|provider| provider.priority);
        providers
    }
}

#[cfg(test)]
mod provider_endpoint_tests {
    use super::{sanitize_ai_diagnostic, validate_provider_endpoint};

    #[test]
    fn provider_endpoint_rejects_url_userinfo() {
        assert!(!validate_provider_endpoint(
            "https://user:pass@example.test/models?api_key=secret"
        ));
        assert!(!validate_provider_endpoint("https://user@example.test/v1"));
        assert!(validate_provider_endpoint("https://example.test/v1"));
    }

    #[test]
    fn diagnostic_sanitizer_removes_secrets_and_bounds_output() {
        let cases = [
            (
                "request https://user:pass@example.test/models?api_key=secret".to_string(),
                vec!["user", "pass", "api_key", "secret"],
            ),
            (
                "Authorization: Bearer bearer-secret".to_string(),
                vec!["bearer-secret"],
            ),
            ("API key: api-secret".to_string(), vec!["api-secret"]),
            (
                "credential sk-1234567890".to_string(),
                vec!["sk-1234567890"],
            ),
            (
                r#"failed to start C:\Users\alice\private\codex.exe"#.to_string(),
                vec![r#"C:\Users\alice\private\codex.exe"#],
            ),
            ("x".repeat(2_000), Vec::new()),
        ];

        for (malicious, secrets) in cases {
            let sanitized = sanitize_ai_diagnostic(&malicious);
            for secret in secrets {
                assert!(!sanitized.contains(secret), "exposed {secret}: {sanitized}");
            }
            assert!(sanitized.chars().count() <= 240);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiExecutionMode {
    ApiKey,
    Codex,
}

impl Default for AiExecutionMode {
    fn default() -> Self {
        Self::ApiKey
    }
}

impl AiExecutionMode {
    pub fn as_database(self) -> &'static str {
        match self {
            Self::ApiKey => "api-key",
            Self::Codex => "codex",
        }
    }

    pub fn from_database(value: &str) -> Self {
        match value {
            "codex" => Self::Codex,
            _ => Self::ApiKey,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiExecutionSnapshot {
    pub execution_mode: AiExecutionMode,
    pub executor_id: String,
    pub model: String,
    pub evidence_hash: String,
    pub created_at_ms: i64,
}

impl AiExecutionSnapshot {
    pub fn legacy(created_at_ms: i64) -> Self {
        Self {
            execution_mode: AiExecutionMode::ApiKey,
            executor_id: "legacy-provider-registry".into(),
            model: String::new(),
            evidence_hash: String::new(),
            created_at_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiExecutionErrorKind {
    Provider,
    Codex,
    InvalidJob,
    InvalidResponse,
    Persistence,
    Unknown,
}

impl AiExecutionErrorKind {
    pub fn as_database(self) -> &'static str {
        match self {
            Self::Provider => "provider",
            Self::Codex => "codex",
            Self::InvalidJob => "invalid-job",
            Self::InvalidResponse => "invalid-response",
            Self::Persistence => "persistence",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_database(value: &str) -> Self {
        match value {
            "provider" => Self::Provider,
            "codex" => Self::Codex,
            "invalid-job" => Self::InvalidJob,
            "invalid-response" => Self::InvalidResponse,
            "persistence" => Self::Persistence,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiJobStatus {
    Pending,
    Running,
    Complete,
    AwaitingReassignment,
}

impl AiJobStatus {
    pub(crate) fn from_database(value: &str) -> Self {
        match value {
            "running" => Self::Running,
            "complete" => Self::Complete,
            "awaiting-reassignment" => Self::AwaitingReassignment,
            _ => Self::Pending,
        }
    }

    pub(crate) fn as_database(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Complete => "complete",
            Self::AwaitingReassignment => "awaiting-reassignment",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiJob {
    pub id: String,
    pub generation: i64,
    pub kind: String,
    pub payload_json: String,
    pub status: AiJobStatus,
    pub attempts: u32,
    pub next_attempt_at_ms: i64,
    pub last_error: String,
    pub execution: AiExecutionSnapshot,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub duration_ms: Option<i64>,
    pub executor_id: Option<String>,
    pub model: Option<String>,
    pub exit_code: Option<i32>,
    pub error_kind: Option<AiExecutionErrorKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiQueueRecord {
    pub id: String,
    pub generation: i64,
    pub kind: String,
    pub status: AiJobStatus,
    pub attempts: u32,
    pub next_attempt_at_ms: i64,
    pub last_error: String,
    pub execution: AiExecutionSnapshot,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub duration_ms: Option<i64>,
    pub executor_id: Option<String>,
    pub model: Option<String>,
    pub exit_code: Option<i32>,
    pub error_kind: Option<AiExecutionErrorKind>,
}

impl From<AiJob> for AiQueueRecord {
    fn from(job: AiJob) -> Self {
        let mut execution = job.execution;
        if execution.execution_mode == AiExecutionMode::Codex {
            execution.executor_id = "codex".into();
        }
        let actual_executor_id = if execution.execution_mode == AiExecutionMode::Codex {
            job.executor_id.map(|_| "codex".to_string())
        } else {
            job.executor_id
        };
        Self {
            id: job.id,
            generation: job.generation,
            kind: job.kind,
            status: job.status,
            attempts: job.attempts,
            next_attempt_at_ms: job.next_attempt_at_ms,
            last_error: sanitize_ai_diagnostic(&job.last_error),
            execution,
            started_at_ms: job.started_at_ms,
            finished_at_ms: job.finished_at_ms,
            duration_ms: job.duration_ms,
            executor_id: actual_executor_id,
            model: job.model,
            exit_code: job.exit_code,
            error_kind: job.error_kind,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkLedgerAssignmentAiResult {
    pub task_id: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkLedgerDecisionKind {
    #[serde(rename = "match_existing_task", alias = "match_existing")]
    MatchExisting,
    #[serde(rename = "create_new")]
    CreateNew,
    DraftExistingWorkflow,
    DraftNewWorkflow,
    Unassigned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkLedgerReasonCode {
    TitleOverlap,
    ExpectedOutputOverlap,
    ApplicationHistory,
    DomainHistory,
    GoalContext,
    FocusContext,
    NewWorkCluster,
    SharedGoal,
    InsufficientEvidence,
}

impl WorkLedgerReasonCode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::TitleOverlap => "title_overlap",
            Self::ExpectedOutputOverlap => "expected_output_overlap",
            Self::ApplicationHistory => "application_history",
            Self::DomainHistory => "domain_history",
            Self::GoalContext => "goal_context",
            Self::FocusContext => "focus_context",
            Self::NewWorkCluster => "new_work_cluster",
            Self::SharedGoal => "shared_goal",
            Self::InsufficientEvidence => "insufficient_evidence",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkLedgerDraftTask {
    pub key: String,
    pub title: String,
    pub expected_output: String,
    pub cluster_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WorkLedgerStructuredDecision {
    pub decision: WorkLedgerDecisionKind,
    pub task_id: Option<String>,
    pub project_id: Option<String>,
    pub suggested_title: Option<String>,
    pub workflow_name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub tasks: Vec<WorkLedgerDraftTask>,
    pub confidence: f64,
    pub alternative_task_id: Option<String>,
    pub alternative_confidence: Option<f64>,
    pub reason_code: WorkLedgerReasonCode,
}

pub(crate) fn parse_work_ledger_decision_response(
    content: &str,
    allowed_task_ids: &[String],
    allowed_project_ids: &[String],
) -> Result<WorkLedgerStructuredDecision, String> {
    let trimmed = content.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return Err("Work ledger decision response must be one JSON object".into());
    }
    if let Ok(legacy) = parse_work_ledger_assignment_response(trimmed, allowed_task_ids) {
        return Ok(WorkLedgerStructuredDecision {
            decision: WorkLedgerDecisionKind::MatchExisting,
            task_id: Some(legacy.task_id),
            project_id: None,
            suggested_title: None,
            workflow_name: None,
            description: None,
            tasks: Vec::new(),
            confidence: legacy.confidence,
            alternative_task_id: None,
            alternative_confidence: None,
            reason_code: WorkLedgerReasonCode::TitleOverlap,
        });
    }
    let result: WorkLedgerStructuredDecision =
        serde_json::from_str(trimmed).map_err(|error| error.to_string())?;
    let bounded = |value: f64| value.is_finite() && (0.0..=1.0).contains(&value);
    if !bounded(result.confidence)
        || result
            .alternative_confidence
            .is_some_and(|value| !bounded(value))
        || result
            .alternative_task_id
            .as_ref()
            .is_some_and(|id| !allowed_task_ids.contains(id))
        || result
            .project_id
            .as_ref()
            .is_some_and(|id| !allowed_project_ids.contains(id))
    {
        return Err("Work ledger decision response fields are invalid".into());
    }
    match result.decision {
        WorkLedgerDecisionKind::MatchExisting => {
            let Some(task_id) = result.task_id.as_ref() else {
                return Err("match_existing requires taskId".into());
            };
            if !allowed_task_ids.contains(task_id)
                || result.suggested_title.is_some()
                || result.workflow_name.is_some()
                || result.description.is_some()
                || !result.tasks.is_empty()
                || result.alternative_task_id.as_ref() == Some(task_id)
            {
                return Err("match_existing fields are invalid".into());
            }
        }
        WorkLedgerDecisionKind::CreateNew => {
            if result.task_id.is_some()
                || result
                    .suggested_title
                    .as_deref()
                    .is_none_or(|title| title.trim().is_empty() || title.chars().count() > 80)
                || result.workflow_name.is_some()
                || result.description.is_some()
                || !result.tasks.is_empty()
            {
                return Err("create_new requires a bounded suggestedTitle".into());
            }
        }
        WorkLedgerDecisionKind::DraftExistingWorkflow
        | WorkLedgerDecisionKind::DraftNewWorkflow => {
            let workflow_name_valid = result
                .workflow_name
                .as_deref()
                .is_some_and(|name| !name.trim().is_empty() && name.chars().count() <= 80);
            let description_valid = result
                .description
                .as_deref()
                .is_some_and(|value| value.chars().count() <= 500);
            let tasks_valid = (1..=3).contains(&result.tasks.len())
                && result.tasks.iter().all(|task| {
                    !task.key.trim().is_empty()
                        && task.key.chars().count() <= 64
                        && !task.title.trim().is_empty()
                        && task.title.chars().count() <= 80
                        && task.expected_output.chars().count() <= 300
                        && !task.cluster_ids.is_empty()
                        && task.cluster_ids.len() <= 12
                });
            let mut task_keys = std::collections::HashSet::new();
            if result.task_id.is_some()
                || result.suggested_title.is_some()
                || result.alternative_task_id.is_some()
                || result.alternative_confidence.is_some()
                || !workflow_name_valid
                || !description_valid
                || !tasks_valid
                || result
                    .tasks
                    .iter()
                    .any(|task| !task_keys.insert(task.key.as_str()))
                || (result.decision == WorkLedgerDecisionKind::DraftExistingWorkflow
                    && result.project_id.is_none())
                || (result.decision == WorkLedgerDecisionKind::DraftNewWorkflow
                    && result.project_id.is_some())
            {
                return Err("Workflow draft fields are invalid".into());
            }
        }
        WorkLedgerDecisionKind::Unassigned => {
            if result.task_id.is_some()
                || result.project_id.is_some()
                || result.suggested_title.is_some()
                || result.workflow_name.is_some()
                || result.description.is_some()
                || !result.tasks.is_empty()
                || result.alternative_task_id.is_some()
                || result.alternative_confidence.is_some()
            {
                return Err("unassigned must not select a task or project".into());
            }
        }
    }
    Ok(result)
}

pub(crate) fn parse_work_ledger_assignment_response(
    content: &str,
    allowed_task_ids: &[String],
) -> Result<WorkLedgerAssignmentAiResult, String> {
    let trimmed = content.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return Err("Work ledger assignment response must be one JSON object".into());
    }
    let result: WorkLedgerAssignmentAiResult =
        serde_json::from_str(trimmed).map_err(|error| error.to_string())?;
    if !allowed_task_ids.contains(&result.task_id)
        || !result.confidence.is_finite()
        || !(0.0..=1.0).contains(&result.confidence)
    {
        return Err("Work ledger assignment response fields are invalid".into());
    }
    Ok(result)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TrendAnalysisAiResult {
    pub summary: String,
    pub observations: Vec<String>,
    pub suggestions: Vec<String>,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrendAnalysisAllowedCandidates {
    pub summaries: Vec<String>,
    pub observations: Vec<String>,
    pub suggestions: Vec<String>,
}

pub(crate) fn parse_trend_analysis_response(
    content: &str,
    allowed: &TrendAnalysisAllowedCandidates,
) -> Result<TrendAnalysisAiResult, String> {
    let trimmed = content.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return Err("Trend analysis response must be one JSON object".into());
    }
    let result: TrendAnalysisAiResult =
        serde_json::from_str(trimmed).map_err(|error| error.to_string())?;
    if !allowed.summaries.contains(&result.summary)
        || result.observations.is_empty()
        || result.observations.len() > 4
        || result.suggestions.is_empty()
        || result.suggestions.len() > 3
        || !result.confidence.is_finite()
        || !(0.0..=1.0).contains(&result.confidence)
    {
        return Err("Trend analysis response fields are invalid".into());
    }
    if result
        .observations
        .iter()
        .any(|item| !allowed.observations.contains(item))
        || has_duplicates(&result.observations)
    {
        return Err("Trend analysis observations must be unique allowed candidates".into());
    }
    if result
        .suggestions
        .iter()
        .any(|item| !allowed.suggestions.contains(item))
        || has_duplicates(&result.suggestions)
    {
        return Err("Trend analysis suggestions must be unique allowed candidates".into());
    }
    Ok(result)
}

fn has_duplicates(items: &[String]) -> bool {
    let mut unique = HashSet::with_capacity(items.len());
    items.iter().any(|item| !unique.insert(item))
}

#[cfg(test)]
mod trend_candidate_contract_tests {
    use super::{TrendAnalysisAllowedCandidates, parse_trend_analysis_response};

    fn allowed() -> TrendAnalysisAllowedCandidates {
        TrendAnalysisAllowedCandidates {
            summaries: vec![
                "本区间记录显示活动分布可供复核".into(),
                "本区间记录显示可用活动证据有限".into(),
            ],
            observations: vec![
                "当前区间存在有效活动记录".into(),
                "分类差异有所变化".into(),
                "记录覆盖情况可继续复核".into(),
            ],
            suggestions: vec![
                "建议尝试固定一段连续任务".into(),
                "下一周期可继续观察活动分布".into(),
            ],
        }
    }

    fn valid_json() -> &'static str {
        r#"{"summary":"本区间记录显示活动分布可供复核","observations":["当前区间存在有效活动记录","分类差异有所变化"],"suggestions":["建议尝试固定一段连续任务"],"confidence":0.8}"#
    }

    #[test]
    fn parser_accepts_only_exact_allowed_candidate_members() {
        let parsed = parse_trend_analysis_response(valid_json(), &allowed()).unwrap();
        assert_eq!(parsed.observations.len(), 2);

        for free_text in [
            "活动质量很差",
            "工作表现很差",
            "自律性不足",
            "学习时间翻倍",
            "学习时间减半",
            "活动时段出现新趋势",
        ] {
            let content = format!(
                r#"{{"summary":"本区间记录显示活动分布可供复核","observations":["{free_text}"],"suggestions":["建议尝试固定一段连续任务"],"confidence":0.8}}"#
            );
            assert!(
                parse_trend_analysis_response(&content, &allowed()).is_err(),
                "accepted free text {free_text}"
            );
        }

        let unlisted_summary = valid_json().replace(
            "本区间记录显示活动分布可供复核",
            "本区间记录显示学习分布可供复核",
        );
        assert!(parse_trend_analysis_response(&unlisted_summary, &allowed()).is_err());
        let unlisted_suggestion =
            valid_json().replace("建议尝试固定一段连续任务", "建议尝试完全不同的任务");
        assert!(parse_trend_analysis_response(&unlisted_suggestion, &allowed()).is_err());
    }

    #[test]
    fn parser_rejects_empty_duplicate_oversized_and_invalid_contract_results() {
        for content in [
            r#"{"summary":"本区间记录显示活动分布可供复核","observations":[],"suggestions":["建议尝试固定一段连续任务"],"confidence":0.8}"#,
            r#"{"summary":"本区间记录显示活动分布可供复核","observations":["当前区间存在有效活动记录"],"suggestions":[],"confidence":0.8}"#,
            r#"{"summary":"本区间记录显示活动分布可供复核","observations":["当前区间存在有效活动记录","当前区间存在有效活动记录"],"suggestions":["建议尝试固定一段连续任务"],"confidence":0.8}"#,
            r#"{"summary":"本区间记录显示活动分布可供复核","observations":["当前区间存在有效活动记录"],"suggestions":["建议尝试固定一段连续任务","建议尝试固定一段连续任务"],"confidence":0.8}"#,
            r#"{"summary":"本区间记录显示活动分布可供复核","observations":["当前区间存在有效活动记录","分类差异有所变化","记录覆盖情况可继续复核","当前区间存在有效活动记录","分类差异有所变化"],"suggestions":["建议尝试固定一段连续任务"],"confidence":0.8}"#,
            r#"{"summary":"本区间记录显示活动分布可供复核","observations":["当前区间存在有效活动记录"],"suggestions":["建议尝试固定一段连续任务","下一周期可继续观察活动分布","建议尝试固定一段连续任务","下一周期可继续观察活动分布"],"confidence":0.8}"#,
            r#"{"summary":"本区间记录显示活动分布可供复核","observations":["当前区间存在有效活动记录"],"suggestions":["建议尝试固定一段连续任务"],"confidence":1.1}"#,
            r#"{"summary":"本区间记录显示活动分布可供复核","observations":["当前区间存在有效活动记录"],"confidence":0.8}"#,
            r#"{"summary":"本区间记录显示活动分布可供复核","observations":["当前区间存在有效活动记录"],"suggestions":["建议尝试固定一段连续任务"],"confidence":0.8,"extra":true}"#,
        ] {
            assert!(parse_trend_analysis_response(content, &allowed()).is_err());
        }
    }
}

#[cfg(test)]
mod work_ledger_assignment_contract_tests {
    use super::{
        WorkLedgerDecisionKind, WorkLedgerReasonCode, parse_work_ledger_assignment_response,
        parse_work_ledger_decision_response,
    };

    #[test]
    fn work_ledger_assignment_parser_accepts_only_a_known_task_and_bounded_confidence() {
        let parsed = parse_work_ledger_assignment_response(
            r#"{"taskId":"task-1","confidence":0.72}"#,
            &["task-1".into(), "task-2".into()],
        )
        .unwrap();

        assert_eq!(parsed.task_id, "task-1");
        assert_eq!(parsed.confidence, 0.72);

        for invalid in [
            "not json",
            r#"{"taskId":"unknown","confidence":0.72}"#,
            r#"{"taskId":"task-1","confidence":1.01}"#,
            r#"{"taskId":"task-1","confidence":0.72,"reason":"free text"}"#,
        ] {
            assert!(parse_work_ledger_assignment_response(invalid, &["task-1".into()]).is_err());
        }
    }

    #[test]
    fn version_two_decision_parser_enforces_whitelisted_shapes_and_candidates() {
        let parsed = parse_work_ledger_decision_response(
            r#"{"decision":"match_existing","taskId":"task-1","projectId":null,"suggestedTitle":null,"confidence":0.91,"alternativeTaskId":"task-2","alternativeConfidence":0.54,"reasonCode":"application_history"}"#,
            &["task-1".into(), "task-2".into()],
            &["project-1".into()],
        )
        .unwrap();
        assert_eq!(parsed.decision, WorkLedgerDecisionKind::MatchExisting);
        assert_eq!(parsed.reason_code, WorkLedgerReasonCode::ApplicationHistory);

        let created = parse_work_ledger_decision_response(
            r#"{"decision":"create_new","taskId":null,"projectId":"project-1","suggestedTitle":"整理实验笔记","confidence":0.94,"alternativeTaskId":null,"alternativeConfidence":null,"reasonCode":"new_work_cluster"}"#,
            &[],
            &["project-1".into()],
        )
        .unwrap();
        assert_eq!(created.decision, WorkLedgerDecisionKind::CreateNew);

        for invalid in [
            r#"{"decision":"match_existing","taskId":"unknown","projectId":null,"suggestedTitle":null,"confidence":0.91,"alternativeTaskId":null,"alternativeConfidence":null,"reasonCode":"title_overlap"}"#,
            r#"{"decision":"create_new","taskId":null,"projectId":null,"suggestedTitle":"","confidence":0.94,"alternativeTaskId":null,"alternativeConfidence":null,"reasonCode":"new_work_cluster"}"#,
            r#"{"decision":"unassigned","taskId":null,"projectId":null,"suggestedTitle":null,"confidence":0.4,"alternativeTaskId":null,"alternativeConfidence":null,"reasonCode":"emotion_guess"}"#,
        ] {
            assert!(
                parse_work_ledger_decision_response(
                    invalid,
                    &["task-1".into()],
                    &["project-1".into()]
                )
                .is_err()
            );
        }
    }
}
