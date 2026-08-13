use std::collections::{BTreeMap, BTreeSet, HashSet};

use chrono::{Local, LocalResult, NaiveDate, TimeZone};
use rusqlite::{
    Connection, OptionalExtension, Result, Row, Transaction, TransactionBehavior, params,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ai::{
    AiExecutionErrorKind, AiExecutionMode, AiExecutionSnapshot, AiJob, AiJobStatus, AiQueueRecord,
};
use crate::ai_review::{
    AiExecutionAuditView, AiReviewAction, AiReviewDraft, AiReviewEventKind, AiReviewFilter,
    AiReviewKind, AiReviewRecord, AiReviewResolution, AiReviewState,
};
use crate::browser::BrowserVisit;
use crate::browser_watcher::BrowserActivitySlice;
use crate::classifier::{AiDisposition, ManualRule, decide_ai_disposition};
use crate::context::{ExternalContextImport, ExternalContextItem, ExternalContextKind};
use crate::domain::{
    ActivityCategory, ActivityScope, AiClassificationReviewValue, AiWorkflowAssignmentReviewValue,
    ClassificationSource, InactivityReason, VideoPurpose,
};
use crate::segment_overlap::canonicalize_activity_segments;
use crate::sync::{
    SyncEvent, SyncStatus, build_sync_event, latest_entity_events, merge_sync_events,
    random_device_id,
};
use crate::trend_analysis::{ResearchStatus, TrendResearchAnalysis, TrendResearchFinding};
use crate::work_ledger::{
    AiTaskCancellation, AiWorkLedgerSuggestion, ConfirmedDailyGoalTask, DailyGoalTaskLink,
    EvidenceLink, EvidenceProvenance, NewProgressEntry, NewProject, NewTask, ProgressEntry,
    ProgressOriginKind, Project, ProjectDailyPoint, ProjectDraftProposal, ProjectRangeRollup,
    ProjectStatus, ProjectTaskContribution, ProjectTimeInsight, ProjectTimeSummary, ProjectUpdate,
    Task, TaskDailyPoint, TaskEfficiencyAssessment, TaskEfficiencyDimension, TaskOriginKind,
    TaskPriority, TaskRangeRollup, TaskReviewState, TaskStatus, TaskTimeInsight, TaskTimeSummary,
    TaskUpdate, WorkLedgerEvidence, WorkLedgerRangeRollup, parse_work_ledger_assignment_job,
    work_ledger_evidence_from_activity, work_ledger_evidence_from_browser,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySegmentRecord {
    pub id: String,
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
    pub app: String,
    #[serde(default)]
    pub app_path: String,
    pub title: String,
    pub category: ActivityCategory,
    pub video_purpose: VideoPurpose,
    pub confidence: f32,
    pub source: ClassificationSource,
    pub reason: String,
    pub model_version: String,
    pub needs_review: bool,
    #[serde(default)]
    pub inactivity_reason: Option<InactivityReason>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySampleWrite {
    pub id: String,
    pub sampled_at_ms: i64,
    pub app: String,
    #[serde(default)]
    pub app_path: String,
    pub title: String,
    pub idle_seconds: i64,
    pub key_presses: u32,
    pub mouse_events: u32,
    pub media_playing: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringContinuityCheckpoint {
    pub expected_tracking: bool,
    pub last_observed_at_ms: i64,
    pub last_boot_started_at_ms: i64,
    pub last_uptime_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringGapRepairResult {
    pub repaired_gap_count: i64,
    pub repaired_seconds: i64,
}

#[derive(Debug)]
struct RepairableActivitySegment {
    id: String,
    started_at_ms: i64,
    ended_at_ms: i64,
    app: String,
    app_path: String,
    title: String,
    category: String,
    video_purpose: String,
    confidence: f64,
    classification_source: String,
    reason: String,
    model_version: String,
    needs_review: bool,
    inactivity_reason: Option<String>,
    origin: String,
}

#[derive(Debug, Clone)]
pub struct TrendRangeFacts {
    pub segments: Vec<ActivitySegmentRecord>,
    pub completed_task_timestamps: Vec<i64>,
}

#[derive(Debug, Clone)]
pub struct WorkLedgerActivityRangeFact {
    pub segment: ActivitySegmentRecord,
    pub task_id: String,
    pub task_title: String,
    pub task_status: TaskStatus,
    pub project_id: String,
    pub project_name: String,
    pub project_status: ProjectStatus,
    pub provenance: EvidenceProvenance,
    pub assignment_confidence: f64,
    pub assignment_reason: String,
    pub assigned_at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct WorkLedgerFocusRangeFact {
    pub session_id: String,
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
    pub task_id: String,
    pub task_title: String,
    pub task_status: TaskStatus,
    pub project_id: String,
    pub project_name: String,
    pub project_status: ProjectStatus,
}

#[derive(Debug, Clone)]
pub struct WorkLedgerCompletedTaskRangeFact {
    pub task_id: String,
    pub task_title: String,
    pub project_id: String,
    pub project_name: String,
    pub completed_at_ms: i64,
}

#[derive(Debug, Clone, Default)]
pub struct WorkLedgerRangeFacts {
    pub activities: Vec<WorkLedgerActivityRangeFact>,
    pub focus_sessions: Vec<WorkLedgerFocusRangeFact>,
    pub completed_tasks: Vec<WorkLedgerCompletedTaskRangeFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserVisitRecord {
    pub id: String,
    pub browser: String,
    pub profile: String,
    pub visited_at_ms: i64,
    pub url: String,
    pub domain: String,
    pub title: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardTotals {
    pub monitored_seconds: i64,
    pub active_seconds: i64,
    pub idle_seconds: i64,
    pub learning_seconds: i64,
    pub category_seconds: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyGoalRecord {
    pub date: String,
    pub goals: String,
    pub expected_output: String,
    pub actual_output: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusSessionRecord {
    pub id: String,
    pub goal_date: String,
    pub goal_text: String,
    pub planned_minutes: u32,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub outcome: String,
    pub task_id: Option<String>,
    pub paused_at_ms: Option<i64>,
    pub paused_total_ms: i64,
    pub notified_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyAnalysisRecord {
    pub date: String,
    pub evidence_hash: String,
    pub portrait: String,
    pub recommendation: String,
    #[serde(default)]
    pub findings_json: String,
    #[serde(default = "legacy_analysis_protocol_version")]
    pub protocol_version: i64,
    pub source: String,
    pub generated_at_ms: i64,
}

fn legacy_analysis_protocol_version() -> i64 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendAnalysisRecord {
    pub range_start: String,
    pub range_end: String,
    pub evidence_hash: String,
    pub summary: String,
    pub observations: Vec<String>,
    pub suggestions: Vec<String>,
    pub source: String,
    pub model: String,
    pub confidence: f64,
    pub generated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendResearchAnalysisRecord {
    pub range_start: String,
    pub range_end: String,
    pub analysis: TrendResearchAnalysis,
}

pub struct Database {
    connection: Connection,
}

const TREND_RESEARCH_STORAGE_NAMESPACE: &str = "research:v2:";

const AI_JOB_COLUMNS: &str = "id, generation, kind, payload_json, status, attempts,
    next_attempt_at_ms, last_error, execution_mode, executor_id, model, evidence_hash,
    execution_created_at_ms, started_at_ms, finished_at_ms, duration_ms, actual_executor_id,
    actual_model, exit_code, error_kind";

fn ai_job_from_row(row: &Row<'_>) -> Result<AiJob> {
    let error_kind = row
        .get::<_, Option<String>>(19)?
        .map(|value| AiExecutionErrorKind::from_database(&value));
    Ok(AiJob {
        id: row.get(0)?,
        generation: row.get(1)?,
        kind: row.get(2)?,
        payload_json: row.get(3)?,
        status: AiJobStatus::from_database(&row.get::<_, String>(4)?),
        attempts: row.get(5)?,
        next_attempt_at_ms: row.get(6)?,
        last_error: row.get(7)?,
        execution: AiExecutionSnapshot {
            execution_mode: AiExecutionMode::from_database(&row.get::<_, String>(8)?),
            executor_id: row.get(9)?,
            model: row.get(10)?,
            evidence_hash: row.get(11)?,
            created_at_ms: row.get(12)?,
        },
        started_at_ms: row.get(13)?,
        finished_at_ms: row.get(14)?,
        duration_ms: row.get(15)?,
        executor_id: row.get(16)?,
        model: row.get(17)?,
        exit_code: row.get(18)?,
        error_kind,
    })
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn redact_bearer_tokens(value: &mut String) {
    let mut search_from = 0;
    loop {
        let lower = value.to_ascii_lowercase();
        let Some(relative_index) = lower[search_from..].find("bearer ") else {
            break;
        };
        let index = search_from + relative_index;
        let start = index + "bearer ".len();
        let end = value[start..]
            .find(|character: char| character.is_whitespace() || ",;\"'".contains(character))
            .map(|offset| start + offset)
            .unwrap_or(value.len());
        value.replace_range(start..end, "[redacted]");
        search_from = start + "[redacted]".len();
    }
}

fn redact_labeled_values(value: &mut String, label: &str) {
    let mut search_from = 0;
    loop {
        let lower = value.to_ascii_lowercase();
        let Some(relative_index) = lower[search_from..].find(label) else {
            break;
        };
        let index = search_from + relative_index;
        let mut separator = index + label.len();
        while value
            .as_bytes()
            .get(separator)
            .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'\'' || *byte == b'"')
        {
            separator += 1;
        }
        if !value
            .as_bytes()
            .get(separator)
            .is_some_and(|byte| *byte == b':' || *byte == b'=')
        {
            search_from = index + label.len();
            continue;
        }
        let mut start = separator + 1;
        while value
            .as_bytes()
            .get(start)
            .is_some_and(u8::is_ascii_whitespace)
        {
            start += 1;
        }
        let Some(first) = value.as_bytes().get(start).copied() else {
            break;
        };
        let end = if first == b'"' || first == b'\'' {
            find_quoted_value_end(value.as_bytes(), start, first)
        } else if first == b'{' || first == b'[' {
            find_balanced_value_end(value.as_bytes(), start, first)
        } else {
            find_unquoted_sensitive_value_end(value, start)
        };
        value.replace_range(start..end, "[redacted]");
        search_from = start + "[redacted]".len();
    }
}

fn find_quoted_value_end(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut escaped = false;
    for (offset, byte) in bytes[start + 1..].iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if *byte == b'\\' {
            escaped = true;
            continue;
        }
        if *byte == quote {
            return start + 1 + offset + 1;
        }
    }
    bytes.len()
}

fn find_balanced_value_end(bytes: &[u8], start: usize, open: u8) -> usize {
    let close = if open == b'{' { b'}' } else { b']' };
    let mut depth = 0_i32;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for (offset, byte) in bytes[start..].iter().enumerate() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == active_quote {
                quote = None;
            }
            continue;
        }
        if *byte == b'"' || *byte == b'\'' {
            quote = Some(*byte);
        } else if *byte == open {
            depth += 1;
        } else if *byte == close {
            depth -= 1;
            if depth == 0 {
                return start + offset + 1;
            }
        }
    }
    bytes.len()
}

fn find_unquoted_sensitive_value_end(value: &str, start: usize) -> usize {
    let bytes = value.as_bytes();
    let punctuation_end = bytes[start..]
        .iter()
        .position(|byte| b";".contains(byte))
        .map(|offset| start + offset)
        .unwrap_or(value.len());
    let stderr_end = value[start..]
        .to_ascii_lowercase()
        .find(" stderr:")
        .map(|offset| start + offset)
        .unwrap_or(value.len());
    [
        punctuation_end,
        stderr_end,
        next_sensitive_label_index(value, start),
    ]
    .into_iter()
    .min()
    .unwrap_or(value.len())
}

fn next_sensitive_label_index(value: &str, start: usize) -> usize {
    let lower = value.to_ascii_lowercase();
    let labels = [
        "authorization",
        "x-api-key",
        "api-key",
        "api_key",
        "apikey",
        "access_token",
        "refresh_token",
        "token",
        "password",
        "secret",
        "prompt",
        "evidence",
        "payload",
    ];
    let mut best = value.len();
    for label in labels {
        let mut offset = start;
        while let Some(relative) = lower[offset..].find(label) {
            let index = offset + relative;
            let before_ok = index == 0
                || value.as_bytes()[index - 1].is_ascii_whitespace()
                || b"{[,;".contains(&value.as_bytes()[index - 1]);
            let mut separator = index + label.len();
            while value
                .as_bytes()
                .get(separator)
                .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'\'' || *byte == b'"')
            {
                separator += 1;
            }
            let after_ok = value
                .as_bytes()
                .get(separator)
                .is_some_and(|byte| *byte == b':' || *byte == b'=');
            if before_ok && after_ok {
                best = best.min(index);
                break;
            }
            offset = index + label.len();
        }
    }
    best
}

fn redact_urls(value: &mut String) {
    let mut search_from = 0;
    loop {
        let lower = value.to_ascii_lowercase();
        let tail = &lower[search_from..];
        let relative = [tail.find("https:"), tail.find("http:")]
            .into_iter()
            .flatten()
            .min();
        let Some(relative_start) = relative else {
            break;
        };
        let start = search_from + relative_start;
        let scheme_len = if lower[start..].starts_with("https:") {
            "https:".len()
        } else {
            "http:".len()
        };
        let boundary_ok = start == 0
            || value
                .as_bytes()
                .get(start - 1)
                .is_some_and(|byte| !byte.is_ascii_alphanumeric() && !b"+-._".contains(byte));
        let token_start = start + scheme_len;
        let has_token = value[token_start..]
            .chars()
            .next()
            .is_some_and(|character| !character.is_whitespace() && !"\"'<>)]}".contains(character));
        if !boundary_ok || !has_token {
            search_from = token_start;
            continue;
        }
        let end = value[start..]
            .find(|character: char| character.is_whitespace() || "\"'<>)]}".contains(character))
            .map(|offset| start + offset)
            .unwrap_or(value.len());
        let replacement = "[redacted-url]";
        value.replace_range(start..end, &replacement);
        search_from = start + replacement.len();
    }
}

pub(crate) fn sanitize_ai_diagnostic(error: &str, sensitive_values: &[(&str, &str)]) -> String {
    let mut cleaned = error.replace('\r', " ").replace('\n', " ");
    for (sensitive, replacement) in sensitive_values {
        if !sensitive.is_empty() {
            cleaned = cleaned.replace(sensitive, replacement);
        }
    }
    redact_bearer_tokens(&mut cleaned);
    redact_urls(&mut cleaned);
    for label in [
        "authorization",
        "x-api-key",
        "api-key",
        "api_key",
        "apikey",
        "access_token",
        "refresh_token",
        "token",
        "password",
        "secret",
        "prompt",
        "evidence",
        "payload",
    ] {
        redact_labeled_values(&mut cleaned, label);
    }
    if let Some(index) = cleaned.to_ascii_lowercase().find("stderr:") {
        cleaned.replace_range(index.., "stderr: [redacted]");
    }
    let trimmed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    trimmed.chars().take(1_000).collect()
}

fn sanitize_ai_error(error: &str) -> String {
    sanitize_ai_diagnostic(error, &[])
}

fn ai_job_owns_lease(connection: &Connection, id: &str, generation: i64) -> Result<bool> {
    connection.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM ai_jobs current
            WHERE current.id=?1 AND current.generation=?2 AND current.status='running'
              AND NOT EXISTS(
                SELECT 1 FROM ai_jobs newer
                WHERE newer.kind=current.kind
                  AND newer.subject_key=current.subject_key
                  AND current.subject_key != ''
                  AND newer.generation > current.generation
              )
         )",
        params![id, generation],
        |row| row.get(0),
    )
}

fn ai_job_owns_kind_lease(
    connection: &Connection,
    id: &str,
    generation: i64,
    kind: &str,
) -> Result<bool> {
    Ok(ai_job_owns_lease(connection, id, generation)?
        && connection.query_row(
            "SELECT kind=?3 FROM ai_jobs WHERE id=?1 AND generation=?2",
            params![id, generation, kind],
            |row| row.get(0),
        )?)
}

#[derive(Clone, Copy)]
struct AiJobCompletionAudit<'a> {
    finished_at_ms: i64,
    duration_ms: Option<i64>,
    executor_id: Option<&'a str>,
    model: Option<&'a str>,
    exit_code: Option<i32>,
}

fn ai_job_completion_transaction(connection: &Connection) -> Result<Transaction<'_>> {
    Transaction::new_unchecked(connection, TransactionBehavior::Immediate)
}

fn retire_superseded_ai_job_generation_on(
    connection: &Connection,
    id: &str,
    generation: i64,
    audit: AiJobCompletionAudit<'_>,
) -> Result<bool> {
    Ok(connection.execute(
        "UPDATE ai_jobs
         SET status='complete',
             last_error='Superseded by newer AI job generation',
             finished_at_ms=?3,
             duration_ms=COALESCE(?4, MAX(0, ?3-COALESCE(started_at_ms, ?3))),
             actual_executor_id=COALESCE(?5, actual_executor_id),
             actual_model=COALESCE(?6, actual_model),
             exit_code=COALESCE(?7, exit_code),
             error_kind=NULL
         WHERE id=?1 AND generation=?2 AND status='running'
           AND EXISTS(
             SELECT 1 FROM ai_jobs newer
             WHERE newer.kind=ai_jobs.kind
               AND newer.subject_key=ai_jobs.subject_key
               AND ai_jobs.subject_key != ''
               AND newer.generation > ai_jobs.generation
           )",
        params![
            id,
            generation,
            audit.finished_at_ms,
            audit.duration_ms.map(|value| value.max(0)),
            audit.executor_id,
            audit.model,
            audit.exit_code,
        ],
    )? == 1)
}

fn ai_job_generation_is_current_or_retire_on(
    connection: &Connection,
    id: &str,
    generation: i64,
    expected_kind: Option<&str>,
    audit: AiJobCompletionAudit<'_>,
) -> Result<bool> {
    let is_current = match expected_kind {
        Some(kind) => ai_job_owns_kind_lease(connection, id, generation, kind)?,
        None => ai_job_owns_lease(connection, id, generation)?,
    };
    if is_current {
        return Ok(true);
    }
    retire_superseded_ai_job_generation_on(connection, id, generation, audit)?;
    Ok(false)
}

#[allow(clippy::too_many_arguments)]
fn complete_ai_job_generation_on(
    connection: &Connection,
    id: &str,
    generation: i64,
    audit: AiJobCompletionAudit<'_>,
    last_error: &str,
    error_kind: Option<&str>,
) -> Result<bool> {
    let completed = connection.execute(
        "UPDATE ai_jobs
         SET status='complete',
             last_error=?3,
             finished_at_ms=?4,
             duration_ms=COALESCE(?5, MAX(0, ?4-COALESCE(started_at_ms, ?4))),
             actual_executor_id=COALESCE(?6, actual_executor_id),
             actual_model=COALESCE(?7, actual_model),
             exit_code=COALESCE(?8, exit_code),
             error_kind=?9
         WHERE id=?1 AND generation=?2 AND status='running'
           AND NOT EXISTS(
             SELECT 1 FROM ai_jobs newer
             WHERE newer.kind=ai_jobs.kind
               AND newer.subject_key=ai_jobs.subject_key
               AND ai_jobs.subject_key != ''
               AND newer.generation > ai_jobs.generation
           )",
        params![
            id,
            generation,
            last_error,
            audit.finished_at_ms,
            audit.duration_ms.map(|value| value.max(0)),
            audit.executor_id,
            audit.model,
            audit.exit_code,
            error_kind,
        ],
    )? == 1;
    if !completed {
        retire_superseded_ai_job_generation_on(connection, id, generation, audit)?;
    }
    Ok(completed)
}

impl Database {
    pub fn classification_evidence_hash(&self, segment_id: &str) -> Result<Option<String>> {
        classification_evidence_hash_on(&self.connection, segment_id)
    }

    pub fn browser_visit_exists(&self, visit_id: &str) -> Result<bool> {
        self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM browser_visits WHERE id=?1)",
            [visit_id],
            |row| row.get(0),
        )
    }

    pub fn page_classification_payload_is_current(&self, payload_json: &str) -> Result<bool> {
        let payload: serde_json::Value = match serde_json::from_str(payload_json) {
            Ok(payload) => payload,
            Err(_) => return Ok(false),
        };
        let Some(visit_id) = payload.get("visitId").and_then(serde_json::Value::as_str) else {
            return Ok(false);
        };
        let Some(domain) = payload.get("domain").and_then(serde_json::Value::as_str) else {
            return Ok(false);
        };
        let Some(title) = payload.get("title").and_then(serde_json::Value::as_str) else {
            return Ok(false);
        };
        let Some(summary) = payload.get("summary").and_then(serde_json::Value::as_str) else {
            return Ok(false);
        };
        let Some(text_snippet) = payload
            .get("textSnippet")
            .and_then(serde_json::Value::as_str)
        else {
            return Ok(false);
        };
        let current = self
            .connection
            .query_row(
                "SELECT browser_visits.domain, browser_visits.title,
                        COALESCE(page_snapshots.summary, ''),
                        COALESCE(page_snapshots.text_snippet, '')
                 FROM browser_visits
                 LEFT JOIN page_snapshots ON page_snapshots.visit_id=browser_visits.id
                 WHERE browser_visits.id=?1",
                [visit_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((current_domain, current_title, current_summary, current_text)) = current else {
            return Ok(false);
        };
        Ok(current_domain == domain
            && current_title == title
            && current_summary.starts_with(summary)
            && current_text.starts_with(text_snippet))
    }

    pub fn work_ledger_assignment_evidence_is_current(&self, payload_json: &str) -> Result<bool> {
        let queued = parse_work_ledger_assignment_job(payload_json).map_err(invalid_review)?;
        Ok(current_work_ledger_evidence_hash_on(
            &self.connection,
            &queued.evidence.kind,
            &queued.evidence.id,
            queued.start_ms,
            queued.end_ms,
        )?
        .as_deref()
            == Some(queued.evidence_hash.as_str())
            && queued.evidence.evidence_hash == queued.evidence_hash)
    }

    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let connection = Connection::open(path)?;
        let database = Self { connection };
        database.migrate()?;
        Ok(database)
    }

    pub fn open_in_memory() -> Result<Self> {
        let connection = Connection::open_in_memory()?;
        let database = Self { connection };
        database.migrate()?;
        Ok(database)
    }

    fn migrate(&self) -> Result<()> {
        self.connection.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS activity_samples (
                id TEXT PRIMARY KEY,
                sampled_at_ms INTEGER NOT NULL,
                app TEXT NOT NULL,
                app_path TEXT NOT NULL DEFAULT '',
                title TEXT NOT NULL,
                idle_seconds INTEGER NOT NULL DEFAULT 0,
                key_presses INTEGER NOT NULL DEFAULT 0,
                mouse_events INTEGER NOT NULL DEFAULT 0,
                media_playing INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS activity_segments (
                id TEXT PRIMARY KEY,
                started_at_ms INTEGER NOT NULL,
                ended_at_ms INTEGER NOT NULL,
                app TEXT NOT NULL,
                app_path TEXT NOT NULL DEFAULT '',
                title TEXT NOT NULL,
                category TEXT NOT NULL,
                video_purpose TEXT NOT NULL,
                confidence REAL NOT NULL,
                classification_source TEXT NOT NULL,
                reason TEXT NOT NULL,
                model_version TEXT NOT NULL,
                needs_review INTEGER NOT NULL DEFAULT 0,
                inactivity_reason TEXT CHECK(inactivity_reason IN ('input_idle', 'continuity_gap', 'legacy_gap_repair') OR inactivity_reason IS NULL),
                origin TEXT NOT NULL DEFAULT 'native'
            );
            CREATE INDEX IF NOT EXISTS idx_segments_time
                ON activity_segments(started_at_ms, ended_at_ms);
            CREATE TABLE IF NOT EXISTS browser_visits (
                id TEXT PRIMARY KEY,
                browser TEXT NOT NULL,
                profile TEXT NOT NULL,
                visited_at_ms INTEGER NOT NULL,
                url TEXT NOT NULL,
                domain TEXT NOT NULL,
                title TEXT NOT NULL,
                UNIQUE(browser, profile, visited_at_ms, url)
            );
            CREATE TABLE IF NOT EXISTS page_snapshots (
                visit_id TEXT PRIMARY KEY,
                summary TEXT NOT NULL DEFAULT '',
                text_snippet TEXT NOT NULL DEFAULT '',
                fetch_status TEXT NOT NULL,
                FOREIGN KEY(visit_id) REFERENCES browser_visits(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS classifications (
                segment_id TEXT PRIMARY KEY,
                category TEXT NOT NULL,
                video_purpose TEXT NOT NULL,
                confidence REAL NOT NULL,
                source TEXT NOT NULL,
                reason TEXT NOT NULL,
                model_version TEXT NOT NULL,
                needs_review INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY(segment_id) REFERENCES activity_segments(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS classification_rules (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                pattern TEXT NOT NULL,
                category TEXT NOT NULL,
                video_purpose TEXT NOT NULL DEFAULT 'unknown',
                created_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS daily_goals (
                date TEXT PRIMARY KEY,
                goals TEXT NOT NULL DEFAULT '',
                expected_output TEXT NOT NULL DEFAULT '',
                actual_output TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS daily_analyses (
                date TEXT PRIMARY KEY,
                evidence_hash TEXT NOT NULL,
                portrait TEXT NOT NULL,
                recommendation TEXT NOT NULL,
                findings_json TEXT NOT NULL DEFAULT '[]',
                protocol_version INTEGER NOT NULL DEFAULT 1,
                source TEXT NOT NULL,
                generated_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS daily_analysis_scopes (
                date TEXT NOT NULL,
                activity_scope TEXT NOT NULL CHECK(activity_scope IN ('all', 'active', 'meaningful')),
                evidence_hash TEXT NOT NULL,
                portrait TEXT NOT NULL,
                recommendation TEXT NOT NULL,
                findings_json TEXT NOT NULL DEFAULT '[]',
                protocol_version INTEGER NOT NULL DEFAULT 1,
                source TEXT NOT NULL,
                generated_at_ms INTEGER NOT NULL,
                PRIMARY KEY(date, activity_scope)
            );
            CREATE TABLE IF NOT EXISTS trend_analyses (
                range_start TEXT NOT NULL,
                range_end TEXT NOT NULL,
                evidence_hash TEXT NOT NULL,
                summary TEXT NOT NULL,
                observations_json TEXT NOT NULL,
                suggestions_json TEXT NOT NULL,
                source TEXT NOT NULL,
                model TEXT NOT NULL,
                confidence REAL NOT NULL,
                generated_at_ms INTEGER NOT NULL,
                PRIMARY KEY(range_start, range_end, evidence_hash)
            );
            CREATE TABLE IF NOT EXISTS focus_sessions (
                id TEXT PRIMARY KEY,
                goal_date TEXT NOT NULL,
                goal_text TEXT NOT NULL,
                planned_minutes INTEGER NOT NULL,
                started_at_ms INTEGER NOT NULL,
                ended_at_ms INTEGER,
                outcome TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS ai_jobs (
                id TEXT PRIMARY KEY,
                content_hash TEXT NOT NULL UNIQUE,
                subject_key TEXT NOT NULL DEFAULT '',
                kind TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                status TEXT NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0,
                next_attempt_at_ms INTEGER NOT NULL,
                last_error TEXT NOT NULL DEFAULT '',
                generation INTEGER NOT NULL DEFAULT 0,
                execution_mode TEXT NOT NULL DEFAULT 'api-key',
                executor_id TEXT NOT NULL DEFAULT 'legacy-provider-registry',
                model TEXT NOT NULL DEFAULT '',
                evidence_hash TEXT NOT NULL DEFAULT '',
                execution_created_at_ms INTEGER NOT NULL DEFAULT 0,
                started_at_ms INTEGER,
                finished_at_ms INTEGER,
                duration_ms INTEGER,
                actual_executor_id TEXT,
                actual_model TEXT,
                exit_code INTEGER,
                error_kind TEXT
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS monitoring_continuity_checkpoint (
                singleton_id INTEGER PRIMARY KEY CHECK(singleton_id=1),
                expected_tracking INTEGER NOT NULL DEFAULT 0,
                last_observed_at_ms INTEGER NOT NULL DEFAULT 0,
                last_boot_started_at_ms INTEGER NOT NULL DEFAULT 0,
                last_uptime_ms INTEGER NOT NULL DEFAULT 0,
                updated_at_ms INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS collector_lease (
                singleton_id INTEGER PRIMARY KEY CHECK(singleton_id=1),
                owner_id TEXT NOT NULL,
                expires_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS monitoring_gap_repairs (
                gap_key TEXT PRIMARY KEY,
                started_at_ms INTEGER NOT NULL,
                ended_at_ms INTEGER NOT NULL,
                reason TEXT NOT NULL CHECK(reason IN ('continuity_gap', 'legacy_gap_repair')),
                repaired_at_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS workflow_evidence_cleanup_audit (
                id TEXT PRIMARY KEY,
                evidence_kind TEXT NOT NULL CHECK(evidence_kind IN ('activity', 'browser')),
                evidence_id TEXT NOT NULL,
                task_id TEXT NOT NULL,
                provenance TEXT NOT NULL,
                reason TEXT NOT NULL,
                removed_at_ms INTEGER NOT NULL
            );
            ",
        )?;
        self.ensure_text_column("activity_samples", "app_path")?;
        self.ensure_text_column("activity_segments", "app_path")?;
        self.ensure_nullable_text_column("activity_segments", "inactivity_reason")?;
        self.ensure_text_column("daily_analyses", "findings_json")?;
        self.connection.execute(
            "UPDATE daily_analyses SET findings_json='[]' WHERE findings_json=''",
            [],
        )?;
        self.ensure_integer_column("daily_analyses", "protocol_version", 1)?;
        self.ensure_integer_column("ai_jobs", "generation", 0)?;
        self.ensure_text_column("ai_jobs", "subject_key")?;
        self.ensure_text_column("ai_jobs", "execution_mode")?;
        self.connection.execute(
            "UPDATE ai_jobs SET execution_mode='api-key' WHERE execution_mode=''",
            [],
        )?;
        self.ensure_text_column("ai_jobs", "executor_id")?;
        self.connection.execute(
            "UPDATE ai_jobs SET executor_id='legacy-provider-registry' WHERE executor_id=''",
            [],
        )?;
        self.ensure_text_column("ai_jobs", "model")?;
        self.ensure_text_column("ai_jobs", "evidence_hash")?;
        self.ensure_integer_column("ai_jobs", "execution_created_at_ms", 0)?;
        self.connection.execute(
            "UPDATE ai_jobs
             SET execution_created_at_ms=next_attempt_at_ms
             WHERE execution_created_at_ms=0",
            [],
        )?;
        self.ensure_nullable_integer_column("ai_jobs", "started_at_ms")?;
        self.ensure_nullable_integer_column("ai_jobs", "finished_at_ms")?;
        self.ensure_nullable_integer_column("ai_jobs", "duration_ms")?;
        self.ensure_nullable_text_column("ai_jobs", "actual_executor_id")?;
        self.ensure_nullable_text_column("ai_jobs", "actual_model")?;
        self.ensure_nullable_integer_column("ai_jobs", "exit_code")?;
        self.ensure_nullable_text_column("ai_jobs", "error_kind")?;
        self.connection.execute(
            "UPDATE ai_jobs
             SET status='pending'
             WHERE status='running'
               AND NOT EXISTS(
                 SELECT 1 FROM ai_jobs newer
                 WHERE newer.kind=ai_jobs.kind
                   AND newer.subject_key=ai_jobs.subject_key
                   AND ai_jobs.subject_key != ''
                   AND newer.generation > ai_jobs.generation
               )",
            [],
        )?;
        self.migrate_work_ledger()?;
        Ok(())
    }

    fn migrate_work_ledger(&self) -> Result<()> {
        let version = self
            .connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;
        if version < 1 {
            self.connection.execute_batch("PRAGMA user_version = 1;")?;
        }
        if version < 2 {
            self.connection.execute_batch(
                "
                BEGIN IMMEDIATE;
                CREATE TABLE IF NOT EXISTS projects (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    color TEXT NOT NULL DEFAULT '',
                    status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active', 'archived')),
                    description TEXT NOT NULL DEFAULT '',
                    created_at_ms INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL,
                    archived_at_ms INTEGER
                );
                CREATE TABLE IF NOT EXISTS tasks (
                    id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL,
                    title TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'todo' CHECK(status IN ('todo', 'in_progress', 'blocked', 'completed', 'cancelled')),
                    priority TEXT NOT NULL DEFAULT 'medium' CHECK(priority IN ('low', 'medium', 'high', 'urgent')),
                    expected_output TEXT NOT NULL DEFAULT '',
                    due_date TEXT,
                    created_at_ms INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL,
                    completed_at_ms INTEGER,
                    FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_tasks_project ON tasks(project_id, status);
                CREATE TABLE IF NOT EXISTS task_activity_links (
                    task_id TEXT NOT NULL,
                    activity_segment_id TEXT NOT NULL,
                    provenance TEXT NOT NULL CHECK(provenance IN ('manual', 'rule', 'ai')),
                    confidence REAL NOT NULL CHECK(confidence >= 0 AND confidence <= 1),
                    reason TEXT NOT NULL DEFAULT '',
                    created_at_ms INTEGER NOT NULL,
                    PRIMARY KEY(task_id, activity_segment_id),
                    FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_task_activity_links_segment
                    ON task_activity_links(activity_segment_id);
                CREATE TABLE IF NOT EXISTS task_browser_links (
                    task_id TEXT NOT NULL,
                    browser_visit_id TEXT NOT NULL,
                    provenance TEXT NOT NULL CHECK(provenance IN ('manual', 'rule', 'ai')),
                    confidence REAL NOT NULL CHECK(confidence >= 0 AND confidence <= 1),
                    reason TEXT NOT NULL DEFAULT '',
                    created_at_ms INTEGER NOT NULL,
                    PRIMARY KEY(task_id, browser_visit_id),
                    FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_task_browser_links_visit
                    ON task_browser_links(browser_visit_id);
                CREATE TABLE IF NOT EXISTS task_progress_entries (
                    id TEXT PRIMARY KEY,
                    task_id TEXT NOT NULL,
                    note TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL,
                    FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_task_progress_entries_task
                    ON task_progress_entries(task_id, created_at_ms);
                PRAGMA user_version = 2;
                COMMIT;
                ",
            )?;
        }
        if version < 3 {
            self.connection.execute_batch(
                "
                BEGIN IMMEDIATE;
                DELETE FROM task_activity_links AS discarded
                WHERE EXISTS(
                    SELECT 1 FROM task_activity_links AS preferred
                    WHERE preferred.activity_segment_id=discarded.activity_segment_id
                      AND (
                        CASE preferred.provenance WHEN 'manual' THEN 3 WHEN 'rule' THEN 2 ELSE 1 END
                          > CASE discarded.provenance WHEN 'manual' THEN 3 WHEN 'rule' THEN 2 ELSE 1 END
                        OR (
                            CASE preferred.provenance WHEN 'manual' THEN 3 WHEN 'rule' THEN 2 ELSE 1 END
                              = CASE discarded.provenance WHEN 'manual' THEN 3 WHEN 'rule' THEN 2 ELSE 1 END
                            AND (
                                preferred.created_at_ms > discarded.created_at_ms
                                OR (preferred.created_at_ms=discarded.created_at_ms AND preferred.task_id < discarded.task_id)
                            )
                        )
                      )
                );
                DELETE FROM task_browser_links AS discarded
                WHERE EXISTS(
                    SELECT 1 FROM task_browser_links AS preferred
                    WHERE preferred.browser_visit_id=discarded.browser_visit_id
                      AND (
                        CASE preferred.provenance WHEN 'manual' THEN 3 WHEN 'rule' THEN 2 ELSE 1 END
                          > CASE discarded.provenance WHEN 'manual' THEN 3 WHEN 'rule' THEN 2 ELSE 1 END
                        OR (
                            CASE preferred.provenance WHEN 'manual' THEN 3 WHEN 'rule' THEN 2 ELSE 1 END
                              = CASE discarded.provenance WHEN 'manual' THEN 3 WHEN 'rule' THEN 2 ELSE 1 END
                            AND (
                                preferred.created_at_ms > discarded.created_at_ms
                                OR (preferred.created_at_ms=discarded.created_at_ms AND preferred.task_id < discarded.task_id)
                            )
                        )
                      )
                );
                CREATE UNIQUE INDEX IF NOT EXISTS idx_task_activity_links_unique_segment
                    ON task_activity_links(activity_segment_id);
                CREATE UNIQUE INDEX IF NOT EXISTS idx_task_browser_links_unique_visit
                    ON task_browser_links(browser_visit_id);
                PRAGMA user_version = 3;
                COMMIT;
                ",
            )?;
        }
        if version < 4 {
            self.connection.execute_batch(
                "
                BEGIN IMMEDIATE;
                CREATE TABLE IF NOT EXISTS work_ledger_ai_suggestions (
                    evidence_kind TEXT NOT NULL CHECK(evidence_kind IN ('activity', 'browser')),
                    evidence_id TEXT NOT NULL,
                    evidence_hash TEXT NOT NULL,
                    task_id TEXT NOT NULL,
                    confidence REAL NOT NULL CHECK(confidence >= 0 AND confidence <= 1),
                    reason TEXT NOT NULL,
                    provider_id TEXT NOT NULL,
                    model TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL,
                    PRIMARY KEY(evidence_kind, evidence_id),
                    FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_work_ledger_ai_suggestions_task
                    ON work_ledger_ai_suggestions(task_id, created_at_ms);
                PRAGMA user_version = 4;
                COMMIT;
                ",
            )?;
        }
        if version < 5 {
            let transaction = self.connection.unchecked_transaction()?;
            if !Self::table_has_column(&transaction, "focus_sessions", "task_id")? {
                transaction.execute(
                    "ALTER TABLE focus_sessions ADD COLUMN task_id TEXT
                        REFERENCES tasks(id) ON DELETE SET NULL",
                    [],
                )?;
            }
            if !Self::table_has_column(&transaction, "task_progress_entries", "origin_kind")? {
                transaction.execute(
                    "ALTER TABLE task_progress_entries ADD COLUMN origin_kind TEXT NOT NULL
                        DEFAULT 'manual'
                        CHECK(origin_kind IN ('manual', 'focus_outcome', 'daily_actual_output'))",
                    [],
                )?;
            }
            if !Self::table_has_column(&transaction, "task_progress_entries", "source_id")? {
                transaction.execute(
                    "ALTER TABLE task_progress_entries ADD COLUMN source_id TEXT",
                    [],
                )?;
            }
            if !Self::table_has_column(&transaction, "task_progress_entries", "source_date")? {
                transaction.execute(
                    "ALTER TABLE task_progress_entries ADD COLUMN source_date TEXT",
                    [],
                )?;
            }
            transaction.execute_batch(
                "
                CREATE UNIQUE INDEX IF NOT EXISTS idx_task_progress_entries_provenance
                    ON task_progress_entries(task_id, origin_kind, source_id)
                    WHERE source_id IS NOT NULL;
                CREATE TABLE IF NOT EXISTS daily_goal_task_links (
                    goal_row_id TEXT PRIMARY KEY,
                    goal_date TEXT NOT NULL,
                    goal_text TEXT NOT NULL,
                    task_id TEXT NOT NULL,
                    confirmed_at_ms INTEGER NOT NULL,
                    FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_daily_goal_task_links_task
                    ON daily_goal_task_links(task_id, confirmed_at_ms);
                PRAGMA user_version = 5;
                ",
            )?;
            transaction.commit()?;
        }
        if version < 6 {
            let transaction = ai_job_completion_transaction(&self.connection)?;
            transaction.execute_batch(
                "
                CREATE TABLE IF NOT EXISTS ai_review_records (
                    id TEXT PRIMARY KEY,
                    job_id TEXT,
                    kind TEXT NOT NULL CHECK(kind IN ('classification', 'workflow_assignment', 'project_draft')),
                    state TEXT NOT NULL CHECK(state IN ('pending', 'auto_applied', 'manual_override', 'execution_error', 'dismissed', 'reverted')),
                    subject_id TEXT NOT NULL,
                    before_json TEXT NOT NULL,
                    proposed_json TEXT NOT NULL,
                    applied_json TEXT,
                    confidence REAL,
                    evidence_summary TEXT NOT NULL,
                    evidence_hash TEXT NOT NULL,
                    execution_mode TEXT CHECK(execution_mode IN ('api-key', 'codex') OR execution_mode IS NULL),
                    execution_executor_id TEXT,
                    execution_model TEXT,
                    execution_evidence_hash TEXT NOT NULL,
                    generation INTEGER NOT NULL,
                    execution_created_at_ms INTEGER NOT NULL,
                    started_at_ms INTEGER,
                    finished_at_ms INTEGER,
                    duration_ms INTEGER,
                    exit_code INTEGER,
                    error_kind TEXT,
                    diagnostic TEXT NOT NULL DEFAULT '',
                    created_at_ms INTEGER NOT NULL,
                    resolved_at_ms INTEGER,
                    FOREIGN KEY(job_id) REFERENCES ai_jobs(id)
                );
                CREATE INDEX IF NOT EXISTS idx_ai_review_filter
                    ON ai_review_records(state, kind, created_at_ms DESC);
                CREATE INDEX IF NOT EXISTS idx_ai_review_subject
                    ON ai_review_records(kind, subject_id, created_at_ms DESC);

                DROP TABLE IF EXISTS work_ledger_ai_suggestions;
                CREATE TABLE work_ledger_ai_suggestions (
                    review_id TEXT NOT NULL UNIQUE,
                    evidence_kind TEXT NOT NULL CHECK(evidence_kind IN ('activity', 'browser')),
                    evidence_id TEXT NOT NULL,
                    evidence_hash TEXT NOT NULL,
                    task_id TEXT NOT NULL,
                    confidence REAL NOT NULL CHECK(confidence >= 0 AND confidence <= 1),
                    reason TEXT NOT NULL,
                    provider_id TEXT NOT NULL,
                    model TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL,
                    PRIMARY KEY(evidence_kind, evidence_id),
                    FOREIGN KEY(review_id) REFERENCES ai_review_records(id) ON DELETE CASCADE,
                    FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE
                );
                CREATE INDEX idx_work_ledger_ai_suggestions_task
                    ON work_ledger_ai_suggestions(task_id, created_at_ms);

                CREATE TABLE IF NOT EXISTS ai_review_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    review_id TEXT NOT NULL,
                    event_kind TEXT NOT NULL CHECK(event_kind IN ('generated', 'auto_applied', 'accepted', 'changed', 'ignored', 'reverted', 'retried', 'failed')),
                    event_json TEXT NOT NULL DEFAULT '{}',
                    created_at_ms INTEGER NOT NULL,
                    FOREIGN KEY(review_id) REFERENCES ai_review_records(id)
                );
                CREATE INDEX IF NOT EXISTS idx_ai_review_events_review
                    ON ai_review_events(review_id, id);
                CREATE TRIGGER IF NOT EXISTS ai_review_events_no_update
                    BEFORE UPDATE ON ai_review_events BEGIN
                        SELECT RAISE(ABORT, 'ai_review_events are immutable');
                    END;
                CREATE TRIGGER IF NOT EXISTS ai_review_events_no_delete
                    BEFORE DELETE ON ai_review_events BEGIN
                        SELECT RAISE(ABORT, 'ai_review_events are immutable');
                    END;

                CREATE TABLE IF NOT EXISTS manual_field_ownership (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    kind TEXT NOT NULL CHECK(kind IN ('classification', 'workflow_assignment', 'project_draft')),
                    subject_id TEXT NOT NULL,
                    field_name TEXT NOT NULL,
                    owner TEXT NOT NULL,
                    claimed_at_ms INTEGER NOT NULL,
                    released_at_ms INTEGER
                );
                CREATE UNIQUE INDEX IF NOT EXISTS idx_manual_field_ownership_current
                    ON manual_field_ownership(kind, subject_id, field_name)
                    WHERE released_at_ms IS NULL;
                PRAGMA user_version = 6;
                ",
            )?;
            transaction.commit()?;
        }
        if version < 7 {
            let transaction = ai_job_completion_transaction(&self.connection)?;
            transaction.execute(
                "UPDATE ai_jobs
                 SET status='awaiting-reassignment'
                 WHERE status='pending' AND executor_id='legacy-provider-registry'",
                [],
            )?;
            transaction.execute_batch("PRAGMA user_version = 7;")?;
            transaction.commit()?;
        }
        if version < 8 {
            let transaction = self.connection.unchecked_transaction()?;
            let tasks_exist = Self::table_exists(&transaction, "tasks")?;
            if tasks_exist && !Self::table_has_column(&transaction, "tasks", "origin_kind")? {
                transaction.execute(
                    "ALTER TABLE tasks ADD COLUMN origin_kind TEXT NOT NULL DEFAULT 'manual'
                     CHECK(origin_kind IN ('manual', 'ai'))",
                    [],
                )?;
            }
            if tasks_exist && !Self::table_has_column(&transaction, "tasks", "origin_key")? {
                transaction.execute("ALTER TABLE tasks ADD COLUMN origin_key TEXT", [])?;
            }
            if tasks_exist && !Self::table_has_column(&transaction, "tasks", "origin_confidence")? {
                transaction.execute(
                    "ALTER TABLE tasks ADD COLUMN origin_confidence REAL
                     CHECK(origin_confidence IS NULL OR (origin_confidence >= 0 AND origin_confidence <= 1))",
                    [],
                )?;
            }
            if tasks_exist && !Self::table_has_column(&transaction, "tasks", "review_state")? {
                transaction.execute(
                    "ALTER TABLE tasks ADD COLUMN review_state TEXT NOT NULL DEFAULT 'confirmed'
                     CHECK(review_state IN ('confirmed', 'provisional', 'pending'))",
                    [],
                )?;
            }
            if tasks_exist {
                transaction.execute_batch(
                    "CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_origin_key
                         ON tasks(origin_key) WHERE origin_key IS NOT NULL;",
                )?;
            }
            transaction.execute_batch(
                "
                CREATE TABLE IF NOT EXISTS work_episode_clusters (
                    cluster_key TEXT PRIMARY KEY,
                    signature_hash TEXT NOT NULL,
                    accumulated_seconds INTEGER NOT NULL DEFAULT 0,
                    episode_count INTEGER NOT NULL DEFAULT 0,
                    status TEXT NOT NULL DEFAULT 'collecting'
                        CHECK(status IN ('collecting', 'created', 'dismissed', 'merged', 'archived')),
                    created_task_id TEXT,
                    first_seen_at_ms INTEGER NOT NULL,
                    last_seen_at_ms INTEGER NOT NULL,
                    FOREIGN KEY(created_task_id) REFERENCES tasks(id) ON DELETE SET NULL
                );
                CREATE TABLE IF NOT EXISTS task_match_profiles (
                    profile_key TEXT NOT NULL,
                    task_id TEXT NOT NULL,
                    weight REAL NOT NULL DEFAULT 1,
                    updated_at_ms INTEGER NOT NULL,
                    PRIMARY KEY(profile_key, task_id),
                    FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_task_match_profiles_task
                    ON task_match_profiles(task_id, updated_at_ms DESC);
                PRAGMA user_version = 8;
                ",
            )?;
            transaction.commit()?;
        }
        if version < 9 {
            self.connection.execute_batch("PRAGMA foreign_keys=OFF;")?;
            let migration_result = (|| -> Result<()> {
                let transaction = self.connection.unchecked_transaction()?;
                transaction.execute_batch(
                    "
                    CREATE TABLE ai_review_records_v9 (
                        id TEXT PRIMARY KEY,
                        job_id TEXT,
                        kind TEXT NOT NULL CHECK(kind IN ('classification', 'workflow_assignment', 'project_draft')),
                        state TEXT NOT NULL CHECK(state IN ('pending', 'auto_applied', 'manual_override', 'execution_error', 'dismissed', 'reverted')),
                        subject_id TEXT NOT NULL,
                        before_json TEXT NOT NULL,
                        proposed_json TEXT NOT NULL,
                        applied_json TEXT,
                        confidence REAL,
                        evidence_summary TEXT NOT NULL,
                        evidence_hash TEXT NOT NULL,
                        execution_mode TEXT CHECK(execution_mode IN ('api-key', 'codex') OR execution_mode IS NULL),
                        execution_executor_id TEXT,
                        execution_model TEXT,
                        execution_evidence_hash TEXT NOT NULL,
                        generation INTEGER NOT NULL,
                        execution_created_at_ms INTEGER NOT NULL,
                        started_at_ms INTEGER,
                        finished_at_ms INTEGER,
                        duration_ms INTEGER,
                        exit_code INTEGER,
                        error_kind TEXT,
                        diagnostic TEXT NOT NULL DEFAULT '',
                        created_at_ms INTEGER NOT NULL,
                        resolved_at_ms INTEGER,
                        FOREIGN KEY(job_id) REFERENCES ai_jobs(id)
                    );
                    INSERT INTO ai_review_records_v9 SELECT * FROM ai_review_records;
                    DROP TABLE ai_review_records;
                    ALTER TABLE ai_review_records_v9 RENAME TO ai_review_records;
                    CREATE INDEX idx_ai_review_filter
                        ON ai_review_records(state, kind, created_at_ms DESC);
                    CREATE INDEX idx_ai_review_subject
                        ON ai_review_records(kind, subject_id, created_at_ms DESC);

                    CREATE TABLE manual_field_ownership_v9 (
                        id INTEGER PRIMARY KEY AUTOINCREMENT,
                        kind TEXT NOT NULL CHECK(kind IN ('classification', 'workflow_assignment', 'project_draft')),
                        subject_id TEXT NOT NULL,
                        field_name TEXT NOT NULL,
                        owner TEXT NOT NULL,
                        claimed_at_ms INTEGER NOT NULL,
                        released_at_ms INTEGER
                    );
                    INSERT INTO manual_field_ownership_v9 SELECT * FROM manual_field_ownership;
                    DROP TABLE manual_field_ownership;
                    ALTER TABLE manual_field_ownership_v9 RENAME TO manual_field_ownership;
                    CREATE UNIQUE INDEX idx_manual_field_ownership_current
                        ON manual_field_ownership(kind, subject_id, field_name)
                        WHERE released_at_ms IS NULL;

                    CREATE TABLE IF NOT EXISTS work_episode_members (
                        episode_key TEXT PRIMARY KEY,
                        cluster_key TEXT NOT NULL,
                        signature_hash TEXT NOT NULL,
                        duration_seconds INTEGER NOT NULL,
                        started_at_ms INTEGER NOT NULL,
                        ended_at_ms INTEGER NOT NULL,
                        created_at_ms INTEGER NOT NULL
                    );
                    CREATE INDEX IF NOT EXISTS idx_work_episode_members_cluster
                        ON work_episode_members(cluster_key, started_at_ms);
                    CREATE TABLE IF NOT EXISTS work_episode_evidence (
                        episode_key TEXT NOT NULL,
                        evidence_kind TEXT NOT NULL CHECK(evidence_kind IN ('activity', 'browser')),
                        evidence_id TEXT NOT NULL,
                        evidence_hash TEXT NOT NULL,
                        occurred_at_ms INTEGER NOT NULL,
                        duration_seconds INTEGER NOT NULL,
                        PRIMARY KEY(episode_key, evidence_kind, evidence_id),
                        FOREIGN KEY(episode_key) REFERENCES work_episode_members(episode_key)
                            ON DELETE CASCADE
                    );
                    CREATE INDEX IF NOT EXISTS idx_work_episode_evidence_hash
                        ON work_episode_evidence(evidence_hash);
                    CREATE TABLE IF NOT EXISTS project_draft_bindings (
                        cluster_id TEXT PRIMARY KEY,
                        review_id TEXT NOT NULL,
                        episode_key TEXT NOT NULL,
                        created_at_ms INTEGER NOT NULL,
                        FOREIGN KEY(review_id) REFERENCES ai_review_records(id) ON DELETE CASCADE
                    );
                    CREATE INDEX IF NOT EXISTS idx_project_draft_bindings_review
                        ON project_draft_bindings(review_id);
                    PRAGMA user_version = 9;
                    ",
                )?;
                transaction.commit()
            })();
            self.connection.execute_batch("PRAGMA foreign_keys=ON;")?;
            migration_result?;
        }
        if version < 10 {
            let transaction = ai_job_completion_transaction(&self.connection)?;
            let resolved_at_ms = now_millis();
            transaction.execute(
                "INSERT INTO ai_review_events(review_id, event_kind, event_json, created_at_ms)
                 SELECT id, 'ignored', '{\"reason\":\"automatic-workflow-mode\"}', ?1
                 FROM ai_review_records
                 WHERE kind='project_draft' AND state='pending'",
                [resolved_at_ms],
            )?;
            transaction.execute(
                "UPDATE ai_review_records
                 SET state='dismissed', resolved_at_ms=?1
                 WHERE kind='project_draft' AND state='pending'",
                [resolved_at_ms],
            )?;
            transaction.execute(
                "DELETE FROM project_draft_bindings
                 WHERE review_id IN (
                    SELECT id FROM ai_review_records
                    WHERE kind='project_draft' AND state='dismissed'
                 )",
                [],
            )?;
            transaction.execute_batch("PRAGMA user_version = 10;")?;
            transaction.commit()?;
        }
        if version < 11 {
            self.connection.execute_batch(
                "
                BEGIN IMMEDIATE;
                CREATE TABLE IF NOT EXISTS browser_activity_segments (
                    id TEXT PRIMARY KEY,
                    source_id TEXT NOT NULL,
                    browser TEXT NOT NULL,
                    profile TEXT NOT NULL DEFAULT '',
                    started_at_ms INTEGER NOT NULL,
                    ended_at_ms INTEGER NOT NULL,
                    url TEXT NOT NULL,
                    domain TEXT NOT NULL,
                    title TEXT NOT NULL DEFAULT '',
                    provenance TEXT NOT NULL
                        CHECK(provenance IN ('watcher-heartbeat-v1')),
                    CHECK(ended_at_ms >= started_at_ms)
                );
                CREATE INDEX IF NOT EXISTS idx_browser_activity_time
                    ON browser_activity_segments(started_at_ms, ended_at_ms);
                CREATE INDEX IF NOT EXISTS idx_browser_activity_source
                    ON browser_activity_segments(source_id, ended_at_ms);
                PRAGMA user_version = 11;
                COMMIT;
                ",
            )?;
        }
        if version < 12 {
            let transaction = self.connection.unchecked_transaction()?;
            if !Self::table_has_column(&transaction, "focus_sessions", "paused_at_ms")? {
                transaction.execute(
                    "ALTER TABLE focus_sessions ADD COLUMN paused_at_ms INTEGER",
                    [],
                )?;
            }
            if !Self::table_has_column(&transaction, "focus_sessions", "paused_total_ms")? {
                transaction.execute(
                    "ALTER TABLE focus_sessions ADD COLUMN paused_total_ms INTEGER NOT NULL DEFAULT 0",
                    [],
                )?;
            }
            if !Self::table_has_column(&transaction, "focus_sessions", "notified_at_ms")? {
                transaction.execute(
                    "ALTER TABLE focus_sessions ADD COLUMN notified_at_ms INTEGER",
                    [],
                )?;
            }
            transaction.execute_batch("PRAGMA user_version = 12;")?;
            transaction.commit()?;
        }
        if version < 13 {
            let transaction = self.connection.unchecked_transaction()?;
            let migrated_at_ms = now_millis();
            transaction.execute(
                "UPDATE focus_sessions
                 SET ended_at_ms=MAX(
                        started_at_ms,
                        MIN(
                            ?1,
                            started_at_ms + planned_minutes * 60000 + paused_total_ms
                        )
                     ),
                     paused_at_ms=NULL
                 WHERE ended_at_ms IS NULL
                   AND id NOT IN (
                        SELECT id FROM focus_sessions
                        WHERE ended_at_ms IS NULL
                        ORDER BY started_at_ms DESC, id DESC
                        LIMIT 1
                   )",
                [migrated_at_ms],
            )?;
            transaction.execute_batch(
                "CREATE UNIQUE INDEX IF NOT EXISTS idx_focus_sessions_single_active
                    ON focus_sessions((1)) WHERE ended_at_ms IS NULL;
                 PRAGMA user_version = 13;",
            )?;
            transaction.commit()?;
        }
        if version < 14 {
            self.connection.execute_batch(
                "
                BEGIN IMMEDIATE;
                CREATE TABLE IF NOT EXISTS sync_devices (
                    device_id TEXT PRIMARY KEY,
                    display_name TEXT NOT NULL DEFAULT '',
                    created_at_ms INTEGER NOT NULL,
                    last_seen_at_ms INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS sync_events (
                    event_id TEXT PRIMARY KEY,
                    device_id TEXT NOT NULL,
                    sequence INTEGER NOT NULL CHECK(sequence >= 1),
                    occurred_at_ms INTEGER NOT NULL,
                    entity_kind TEXT NOT NULL,
                    entity_id TEXT NOT NULL,
                    operation TEXT NOT NULL,
                    payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
                    payload_hash TEXT NOT NULL,
                    imported_at_ms INTEGER NOT NULL,
                    UNIQUE(device_id, sequence)
                );
                CREATE INDEX IF NOT EXISTS idx_sync_events_order
                    ON sync_events(occurred_at_ms, device_id, sequence, event_id);
                CREATE INDEX IF NOT EXISTS idx_sync_events_entity
                    ON sync_events(entity_kind, entity_id, occurred_at_ms);
                PRAGMA user_version = 14;
                COMMIT;
                ",
            )?;
        }
        if version < 15 {
            self.connection.execute_batch(
                "
                BEGIN IMMEDIATE;
                CREATE TABLE IF NOT EXISTS external_context_sources (
                    source_id TEXT PRIMARY KEY,
                    source_name TEXT NOT NULL,
                    source_kind TEXT NOT NULL,
                    imported_at_ms INTEGER NOT NULL
                );
                CREATE TABLE IF NOT EXISTS external_context_items (
                    id TEXT PRIMARY KEY,
                    source_id TEXT NOT NULL,
                    source_name TEXT NOT NULL,
                    source_kind TEXT NOT NULL,
                    external_id TEXT NOT NULL,
                    kind TEXT NOT NULL CHECK(kind IN ('calendar_event', 'project', 'task')),
                    title TEXT NOT NULL,
                    start_at_ms INTEGER,
                    end_at_ms INTEGER,
                    project_name TEXT NOT NULL DEFAULT '',
                    status TEXT NOT NULL DEFAULT '',
                    imported_at_ms INTEGER NOT NULL,
                    UNIQUE(source_id, external_id, kind),
                    FOREIGN KEY(source_id) REFERENCES external_context_sources(source_id)
                        ON DELETE CASCADE
                );
                CREATE INDEX IF NOT EXISTS idx_external_context_time
                    ON external_context_items(kind, start_at_ms, end_at_ms);
                PRAGMA user_version = 15;
                COMMIT;
                ",
            )?;
        }
        if version < 16 {
            self.connection.execute_batch(
                "
                BEGIN IMMEDIATE;
                DELETE FROM ai_jobs
                WHERE status='pending'
                  AND attempts=0
                  AND kind IN (
                    'classify_segment',
                    'classify_page',
                    'daily_analysis',
                    'work_ledger_assignment'
                  );
                PRAGMA user_version = 16;
                COMMIT;
                ",
            )?;
        }
        if version < 17 {
            // The v16 cleanup can release several megabytes. VACUUM is best-effort because a
            // concurrent read-only UI may briefly hold a lock; even without it SQLite will reuse
            // the freed pages for future writes.
            let _ = self.connection.execute_batch("VACUUM;");
            self.connection.execute_batch("PRAGMA user_version = 17;")?;
        }
        if version < 18 {
            self.connection.execute_batch(
                "
                BEGIN IMMEDIATE;
                ALTER TABLE daily_analysis_scopes RENAME TO daily_analysis_scopes_v17;
                CREATE TABLE daily_analysis_scopes (
                    date TEXT NOT NULL,
                    activity_scope TEXT NOT NULL CHECK(activity_scope IN ('all', 'active', 'meaningful')),
                    evidence_hash TEXT NOT NULL,
                    portrait TEXT NOT NULL,
                    recommendation TEXT NOT NULL,
                    findings_json TEXT NOT NULL DEFAULT '[]',
                    protocol_version INTEGER NOT NULL DEFAULT 1,
                    source TEXT NOT NULL,
                    generated_at_ms INTEGER NOT NULL,
                    PRIMARY KEY(date, activity_scope)
                );
                INSERT INTO daily_analysis_scopes(
                    date, activity_scope, evidence_hash, portrait, recommendation,
                    findings_json, protocol_version, source, generated_at_ms
                )
                SELECT date, activity_scope, evidence_hash, portrait, recommendation,
                       findings_json, protocol_version, source, generated_at_ms
                FROM daily_analysis_scopes_v17;
                DROP TABLE daily_analysis_scopes_v17;
                PRAGMA user_version = 18;
                COMMIT;
                ",
            )?;
        }
        Ok(())
    }

    fn table_has_column(connection: &Connection, table: &str, column: &str) -> Result<bool> {
        let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
        let names = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>>>()?;
        Ok(names.iter().any(|name| name == column))
    }

    fn table_exists(connection: &Connection, table: &str) -> Result<bool> {
        connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1
            )",
            [table],
            |row| row.get(0),
        )
    }

    fn ensure_text_column(&self, table: &str, column: &str) -> Result<()> {
        let mut statement = self
            .connection
            .prepare(&format!("PRAGMA table_info({table})"))?;
        let names = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>>>()?;
        if !names.iter().any(|name| name == column) {
            self.connection.execute(
                &format!("ALTER TABLE {table} ADD COLUMN {column} TEXT NOT NULL DEFAULT ''"),
                [],
            )?;
        }
        Ok(())
    }

    fn ensure_integer_column(&self, table: &str, column: &str, default: i64) -> Result<()> {
        let mut statement = self
            .connection
            .prepare(&format!("PRAGMA table_info({table})"))?;
        let names = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>>>()?;
        if !names.iter().any(|name| name == column) {
            self.connection.execute(
                &format!(
                    "ALTER TABLE {table} ADD COLUMN {column} INTEGER NOT NULL DEFAULT {default}"
                ),
                [],
            )?;
        }
        Ok(())
    }

    fn ensure_nullable_text_column(&self, table: &str, column: &str) -> Result<()> {
        let mut statement = self
            .connection
            .prepare(&format!("PRAGMA table_info({table})"))?;
        let names = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>>>()?;
        if !names.iter().any(|name| name == column) {
            self.connection
                .execute(&format!("ALTER TABLE {table} ADD COLUMN {column} TEXT"), [])?;
        }
        Ok(())
    }

    fn ensure_nullable_integer_column(&self, table: &str, column: &str) -> Result<()> {
        let mut statement = self
            .connection
            .prepare(&format!("PRAGMA table_info({table})"))?;
        let names = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>>>()?;
        if !names.iter().any(|name| name == column) {
            self.connection.execute(
                &format!("ALTER TABLE {table} ADD COLUMN {column} INTEGER"),
                [],
            )?;
        }
        Ok(())
    }

    pub fn has_table(&self, table: &str) -> Result<bool> {
        let found = self
            .connection
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |_| Ok(true),
            )
            .optional()?;
        Ok(found.unwrap_or(false))
    }

    pub fn insert_segment(&self, segment: &ActivitySegmentRecord) -> Result<()> {
        self.insert_segment_with_origin(segment, "native")
            .map(|_| ())
    }

    pub fn insert_segment_with_origin(
        &self,
        segment: &ActivitySegmentRecord,
        origin: &str,
    ) -> Result<bool> {
        let changed = self.connection.execute(
            "INSERT OR IGNORE INTO activity_segments (
                id, started_at_ms, ended_at_ms, app, app_path, title, category, video_purpose,
                confidence, classification_source, reason, model_version, needs_review,
                inactivity_reason, origin
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                segment.id,
                segment.started_at_ms,
                segment.ended_at_ms.max(segment.started_at_ms),
                segment.app,
                segment.app_path,
                segment.title,
                category_key(segment.category),
                video_purpose_key(segment.video_purpose),
                segment.confidence.clamp(0.0, 1.0),
                source_key(segment.source),
                segment.reason,
                segment.model_version,
                segment.needs_review,
                segment.inactivity_reason.map(inactivity_reason_key),
                origin,
            ],
        )?;
        Ok(changed > 0)
    }

    pub fn upsert_native_segment(&self, segment: &ActivitySegmentRecord) -> Result<()> {
        upsert_native_segment_on(&self.connection, segment)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_activity_sample(
        &self,
        id: &str,
        sampled_at_ms: i64,
        app: &str,
        app_path: &str,
        title: &str,
        idle_seconds: i64,
        key_presses: u32,
        mouse_events: u32,
        media_playing: bool,
    ) -> Result<()> {
        self.connection.execute(
            "INSERT OR IGNORE INTO activity_samples(
                id, sampled_at_ms, app, app_path, title, idle_seconds, key_presses, mouse_events, media_playing
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, sampled_at_ms, app, app_path, title, idle_seconds.max(0), key_presses, mouse_events, media_playing],
        )?;
        Ok(())
    }

    pub fn record_monitoring_tick(
        &self,
        sample: &ActivitySampleWrite,
        segments: &[ActivitySegmentRecord],
        checkpoint: &MonitoringContinuityCheckpoint,
    ) -> Result<()> {
        self.record_monitoring_tick_with_optional_sample(Some(sample), segments, checkpoint)
    }

    pub fn record_monitoring_tick_with_optional_sample(
        &self,
        sample: Option<&ActivitySampleWrite>,
        segments: &[ActivitySegmentRecord],
        checkpoint: &MonitoringContinuityCheckpoint,
    ) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        if let Some(sample) = sample {
            transaction.execute(
                "INSERT OR IGNORE INTO activity_samples(
                    id, sampled_at_ms, app, app_path, title, idle_seconds, key_presses,
                    mouse_events, media_playing
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    sample.id,
                    sample.sampled_at_ms,
                    sample.app,
                    sample.app_path,
                    sample.title,
                    sample.idle_seconds.max(0),
                    sample.key_presses,
                    sample.mouse_events,
                    sample.media_playing,
                ],
            )?;
        }
        for segment in segments {
            upsert_native_segment_on(&transaction, segment)?;
        }
        save_monitoring_continuity_checkpoint_on(&transaction, checkpoint)?;
        transaction.commit()
    }

    pub fn record_monitoring_pause(
        &self,
        segment: Option<&ActivitySegmentRecord>,
        checkpoint: &MonitoringContinuityCheckpoint,
    ) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        if let Some(segment) = segment {
            upsert_native_segment_on(&transaction, segment)?;
        }
        save_monitoring_continuity_checkpoint_on(&transaction, checkpoint)?;
        transaction.commit()
    }

    pub fn load_monitoring_continuity_checkpoint(
        &self,
    ) -> Result<Option<MonitoringContinuityCheckpoint>> {
        self.connection
            .query_row(
                "SELECT expected_tracking, last_observed_at_ms, last_boot_started_at_ms,
                        last_uptime_ms, updated_at_ms
                 FROM monitoring_continuity_checkpoint WHERE singleton_id=1",
                [],
                |row| {
                    Ok(MonitoringContinuityCheckpoint {
                        expected_tracking: row.get(0)?,
                        last_observed_at_ms: row.get(1)?,
                        last_boot_started_at_ms: row.get(2)?,
                        last_uptime_ms: row.get(3)?,
                        updated_at_ms: row.get(4)?,
                    })
                },
            )
            .optional()
    }

    pub fn try_acquire_collector_lease(
        &self,
        owner_id: &str,
        now_ms: i64,
        ttl_ms: i64,
    ) -> Result<bool> {
        if owner_id.trim().is_empty() || ttl_ms <= 0 {
            return Err(rusqlite::Error::InvalidParameterName(
                "collector lease requires a non-empty owner and positive ttl".into(),
            ));
        }
        let expires_at_ms = now_ms.saturating_add(ttl_ms);
        let changed = self.connection.execute(
            "INSERT INTO collector_lease(singleton_id, owner_id, expires_at_ms, updated_at_ms)
             VALUES(1, ?1, ?2, ?3)
             ON CONFLICT(singleton_id) DO UPDATE SET
                owner_id=excluded.owner_id,
                expires_at_ms=excluded.expires_at_ms,
                updated_at_ms=excluded.updated_at_ms
             WHERE collector_lease.owner_id=excluded.owner_id
                OR collector_lease.expires_at_ms <= ?3",
            params![owner_id, expires_at_ms, now_ms],
        )?;
        Ok(changed > 0)
    }

    pub fn release_collector_lease(&self, owner_id: &str) -> Result<bool> {
        let changed = self.connection.execute(
            "DELETE FROM collector_lease WHERE singleton_id=1 AND owner_id=?1",
            [owner_id],
        )?;
        Ok(changed > 0)
    }

    pub fn save_monitoring_continuity_checkpoint(
        &self,
        checkpoint: &MonitoringContinuityCheckpoint,
    ) -> Result<()> {
        save_monitoring_continuity_checkpoint_on(&self.connection, checkpoint)
    }

    pub fn repair_recent_monitoring_gaps(
        &self,
        now_ms: i64,
        lookback_ms: i64,
        gap_threshold_ms: i64,
    ) -> Result<MonitoringGapRepairResult> {
        let range_start_ms = now_ms.saturating_sub(lookback_ms.max(0));
        let sampled_at = {
            let mut statement = self.connection.prepare(
                "SELECT sampled_at_ms
                 FROM activity_samples
                 WHERE sampled_at_ms >= ?1 AND sampled_at_ms <= ?2
                 ORDER BY sampled_at_ms ASC, id ASC",
            )?;
            statement
                .query_map(params![range_start_ms, now_ms], |row| row.get::<_, i64>(0))?
                .collect::<Result<Vec<_>>>()?
        };
        let gaps = sampled_at
            .windows(2)
            .filter_map(|pair| {
                let started_at_ms = pair[0];
                let ended_at_ms = pair[1];
                (ended_at_ms.saturating_sub(started_at_ms) > gap_threshold_ms.max(0))
                    .then_some((started_at_ms, ended_at_ms))
            })
            .collect::<Vec<_>>();
        if gaps.is_empty() {
            return Ok(MonitoringGapRepairResult::default());
        }

        let transaction = self.connection.unchecked_transaction()?;
        let mut result = MonitoringGapRepairResult::default();
        for (gap_start_ms, gap_end_ms) in gaps {
            let gap_key = monitoring_gap_key("legacy_gap_repair", gap_start_ms, gap_end_ms);
            let already_repaired = transaction
                .query_row(
                    "SELECT 1 FROM monitoring_gap_repairs WHERE gap_key=?1",
                    [&gap_key],
                    |_| Ok(true),
                )
                .optional()?
                .unwrap_or(false);
            if already_repaired {
                continue;
            }
            repair_activity_segments_for_gap(&transaction, gap_start_ms, gap_end_ms)?;
            let idle_id = format!("idle-gap-{}", &gap_key[..24]);
            transaction.execute(
                "INSERT OR IGNORE INTO activity_segments(
                    id, started_at_ms, ended_at_ms, app, app_path, title, category,
                    video_purpose, confidence, classification_source, reason, model_version,
                    needs_review, inactivity_reason, origin
                 ) VALUES (?1, ?2, ?3, 'Idle', '', 'Monitoring gap', 'idle', 'unknown',
                    1.0, 'idle', 'Historical monitoring gap inferred from adjacent samples',
                    'legacy-gap-repair-v1', 0, 'legacy_gap_repair', 'native')",
                params![idle_id, gap_start_ms, gap_end_ms],
            )?;
            transaction.execute(
                "INSERT INTO monitoring_gap_repairs(
                    gap_key, started_at_ms, ended_at_ms, reason, repaired_at_ms
                 ) VALUES (?1, ?2, ?3, 'legacy_gap_repair', ?4)",
                params![gap_key, gap_start_ms, gap_end_ms, now_ms],
            )?;
            result.repaired_gap_count += 1;
            result.repaired_seconds += gap_end_ms.saturating_sub(gap_start_ms) / 1_000;
        }
        transaction.commit()?;
        Ok(result)
    }

    pub fn insert_browser_visit(
        &self,
        id: &str,
        browser: &str,
        profile: &str,
        visit: &BrowserVisit,
        domain: &str,
    ) -> Result<bool> {
        let changed = self.connection.execute(
            "INSERT OR IGNORE INTO browser_visits(
                id, browser, profile, visited_at_ms, url, domain, title
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                browser,
                profile,
                visit.visited_at_ms,
                visit.url,
                domain,
                visit.title
            ],
        )?;
        Ok(changed > 0)
    }

    pub fn insert_browser_activity_slice(&self, slice: &BrowserActivitySlice) -> Result<bool> {
        let changed = self.connection.execute(
            "INSERT OR IGNORE INTO browser_activity_segments(
                id, source_id, browser, profile, started_at_ms, ended_at_ms,
                url, domain, title, provenance
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                slice.id,
                slice.source_id,
                slice.browser,
                slice.profile,
                slice.started_at_ms,
                slice.ended_at_ms,
                slice.url,
                slice.domain,
                slice.title,
                slice.provenance,
            ],
        )?;
        Ok(changed > 0)
    }

    pub fn browser_activity_summary(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<(i64, i64, Option<i64>)> {
        self.connection.query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(MAX(0, MIN(ended_at_ms, ?2) - MAX(started_at_ms, ?1))), 0),
                    MAX(ended_at_ms)
             FROM browser_activity_segments
             WHERE ended_at_ms > ?1 AND started_at_ms < ?2",
            params![start_ms, end_ms],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
    }

    pub fn latest_browser_context(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Option<(String, String)>> {
        self.connection
            .query_row(
                "SELECT domain, title FROM browser_visits
                 WHERE visited_at_ms BETWEEN ?1 AND ?2
                 ORDER BY visited_at_ms DESC LIMIT 1",
                params![start_ms, end_ms],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
    }

    pub fn list_browser_visits(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<BrowserVisitRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, browser, profile, visited_at_ms, url, domain, title
             FROM browser_visits
             WHERE visited_at_ms >= ?1 AND visited_at_ms < ?2
             ORDER BY visited_at_ms ASC",
        )?;
        statement
            .query_map(params![start_ms, end_ms], |row| {
                Ok(BrowserVisitRecord {
                    id: row.get(0)?,
                    browser: row.get(1)?,
                    profile: row.get(2)?,
                    visited_at_ms: row.get(3)?,
                    url: row.get(4)?,
                    domain: row.get(5)?,
                    title: row.get(6)?,
                })
            })?
            .collect()
    }

    pub fn segment_count(&self) -> Result<i64> {
        self.connection
            .query_row("SELECT COUNT(*) FROM activity_segments", [], |row| {
                row.get(0)
            })
    }

    pub fn segment_origin(&self, id: &str) -> Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT origin FROM activity_segments WHERE id=?1",
                [id],
                |row| row.get(0),
            )
            .optional()
    }

    pub fn dashboard_totals(&self, start_ms: i64, end_ms: i64) -> Result<DashboardTotals> {
        if end_ms <= start_ms {
            return Ok(DashboardTotals::default());
        }
        let segments = self.list_clipped_segments(start_ms, end_ms)?;
        let mut duration_ms = BTreeMap::<(String, String), i64>::new();
        for segment in canonicalize_activity_segments(&segments, start_ms, end_ms) {
            let key = (
                category_key(segment.category).to_string(),
                video_purpose_key(segment.video_purpose).to_string(),
            );
            *duration_ms.entry(key).or_default() +=
                segment.ended_at_ms.saturating_sub(segment.started_at_ms);
        }

        let mut totals = DashboardTotals::default();
        for ((category, video_purpose), milliseconds) in duration_ms {
            let seconds = milliseconds.max(0) / 1_000;
            totals.monitored_seconds += seconds;
            *totals.category_seconds.entry(category.clone()).or_default() += seconds;
            if category == "idle" {
                totals.idle_seconds += seconds;
            } else {
                totals.active_seconds += seconds;
            }
            if matches!(
                category.as_str(),
                "research" | "text_input" | "creation_development"
            ) || (category == "video_input" && video_purpose == "learning")
            {
                totals.learning_seconds += seconds;
            }
        }
        Ok(totals)
    }

    pub fn list_segments(&self, start_ms: i64, end_ms: i64) -> Result<Vec<ActivitySegmentRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, started_at_ms, ended_at_ms, app, app_path, title, category, video_purpose,
                    confidence, classification_source, reason, model_version, needs_review,
                    inactivity_reason
             FROM activity_segments
             WHERE ended_at_ms > ?1 AND started_at_ms < ?2
             ORDER BY started_at_ms DESC",
        )?;
        statement
            .query_map(params![start_ms, end_ms], |row| {
                Ok(ActivitySegmentRecord {
                    id: row.get(0)?,
                    started_at_ms: row.get(1)?,
                    ended_at_ms: row.get(2)?,
                    app: row.get(3)?,
                    app_path: row.get(4)?,
                    title: row.get(5)?,
                    category: category_from_key(&row.get::<_, String>(6)?),
                    video_purpose: video_purpose_from_key(&row.get::<_, String>(7)?),
                    confidence: row.get(8)?,
                    source: source_from_key(&row.get::<_, String>(9)?),
                    reason: row.get(10)?,
                    model_version: row.get(11)?,
                    needs_review: row.get(12)?,
                    inactivity_reason: inactivity_reason_from_key(
                        row.get::<_, Option<String>>(13)?.as_deref(),
                    ),
                })
            })?
            .collect()
    }

    pub fn get_segment(&self, segment_id: &str) -> Result<Option<ActivitySegmentRecord>> {
        self.connection
            .query_row(
                "SELECT id, started_at_ms, ended_at_ms, app, app_path, title, category,
                        video_purpose, confidence, classification_source, reason, model_version,
                        needs_review, inactivity_reason
                 FROM activity_segments WHERE id=?1",
                [segment_id],
                |row| {
                    Ok(ActivitySegmentRecord {
                        id: row.get(0)?,
                        started_at_ms: row.get(1)?,
                        ended_at_ms: row.get(2)?,
                        app: row.get(3)?,
                        app_path: row.get(4)?,
                        title: row.get(5)?,
                        category: category_from_key(&row.get::<_, String>(6)?),
                        video_purpose: video_purpose_from_key(&row.get::<_, String>(7)?),
                        confidence: row.get(8)?,
                        source: source_from_key(&row.get::<_, String>(9)?),
                        reason: row.get(10)?,
                        model_version: row.get(11)?,
                        needs_review: row.get(12)?,
                        inactivity_reason: inactivity_reason_from_key(
                            row.get::<_, Option<String>>(13)?.as_deref(),
                        ),
                    })
                },
            )
            .optional()
    }

    pub fn list_clipped_segments(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<ActivitySegmentRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, MAX(started_at_ms, ?1), MIN(ended_at_ms, ?2), app, app_path, title,
                    category, video_purpose, confidence, classification_source, reason,
                    model_version, needs_review, inactivity_reason
             FROM activity_segments
             WHERE ended_at_ms > ?1 AND started_at_ms < ?2
             ORDER BY started_at_ms ASC, id ASC",
        )?;
        statement
            .query_map(params![start_ms, end_ms], |row| {
                Ok(ActivitySegmentRecord {
                    id: row.get(0)?,
                    started_at_ms: row.get(1)?,
                    ended_at_ms: row.get(2)?,
                    app: row.get(3)?,
                    app_path: row.get(4)?,
                    title: row.get(5)?,
                    category: category_from_key(&row.get::<_, String>(6)?),
                    video_purpose: video_purpose_from_key(&row.get::<_, String>(7)?),
                    confidence: row.get(8)?,
                    source: source_from_key(&row.get::<_, String>(9)?),
                    reason: row.get(10)?,
                    model_version: row.get(11)?,
                    needs_review: row.get(12)?,
                    inactivity_reason: inactivity_reason_from_key(
                        row.get::<_, Option<String>>(13)?.as_deref(),
                    ),
                })
            })?
            .collect()
    }

    pub fn save_manual_classification(
        &self,
        segment_id: &str,
        category: ActivityCategory,
        video_purpose: VideoPurpose,
        reason: &str,
    ) -> Result<bool> {
        let transaction = ai_job_completion_transaction(&self.connection)?;
        let Some((before_json, evidence_summary, _)) =
            classification_review_value_on(&transaction, segment_id)?
        else {
            transaction.rollback()?;
            return Ok(false);
        };
        let applied_json = serde_json::to_string(&AiClassificationReviewValue {
            category,
            video_purpose,
            confidence: 1.0,
            reason: reason.into(),
            model_version: "manual-v1".into(),
        })
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        if !apply_review_value_on(
            &transaction,
            AiReviewKind::Classification,
            segment_id,
            &applied_json,
            true,
        )? {
            transaction.rollback()?;
            return Ok(false);
        }
        insert_manual_override_review_on(
            &transaction,
            AiReviewKind::Classification,
            segment_id,
            &before_json,
            &applied_json,
            &evidence_summary,
            reason,
            now_millis(),
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn save_manual_rule_for_segment(
        &self,
        segment_id: &str,
        category: ActivityCategory,
        video_purpose: VideoPurpose,
        created_at_ms: i64,
    ) -> Result<bool> {
        let Some((app, title)) = self
            .connection
            .query_row(
                "SELECT app, title FROM activity_segments WHERE id=?1",
                [segment_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
        else {
            return Ok(false);
        };
        let pattern = format!("{}\n{}", app.trim(), title.trim());
        let hash = format!(
            "{:x}",
            Sha256::digest(format!("app_title\n{pattern}").as_bytes())
        );
        let id = format!("rule-{}", &hash[..24]);
        self.connection.execute(
            "INSERT INTO classification_rules(
                id, kind, pattern, category, video_purpose, created_at_ms
             ) VALUES (?1, 'app_title', ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET category=excluded.category,
                video_purpose=excluded.video_purpose, created_at_ms=excluded.created_at_ms",
            params![
                id,
                pattern,
                category_key(category),
                video_purpose_key(video_purpose),
                created_at_ms,
            ],
        )?;
        Ok(true)
    }

    pub fn classification_rule_context_for_segment(
        &self,
        segment_id: &str,
    ) -> Result<Option<(String, String, i64)>> {
        let Some((app, title)) = self
            .connection
            .query_row(
                "SELECT app, title FROM activity_segments WHERE id=?1",
                [segment_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
        else {
            return Ok(None);
        };
        let historical_match_count = self.connection.query_row(
            "SELECT COUNT(*) FROM activity_segments WHERE app=?1 AND title=?2",
            params![app, title],
            |row| row.get(0),
        )?;
        Ok(Some((app, title, historical_match_count)))
    }

    pub fn list_manual_rules(&self) -> Result<Vec<ManualRule>> {
        let mut statement = self.connection.prepare(
            "SELECT kind, pattern, category, video_purpose
             FROM classification_rules ORDER BY created_at_ms DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                category_from_key(&row.get::<_, String>(2)?),
                video_purpose_from_key(&row.get::<_, String>(3)?),
            ))
        })?;
        let mut rules = Vec::new();
        for row in rows {
            let (kind, pattern, category, video_purpose) = row?;
            let rule = match kind.as_str() {
                "domain" => ManualRule::domain(pattern, category),
                "app_title" => {
                    let (app, title) = pattern.split_once('\n').unwrap_or((&pattern, ""));
                    ManualRule::app_title_with_video_purpose(app, title, category, video_purpose)
                }
                _ => ManualRule::app(pattern, category),
            };
            rules.push(rule);
        }
        Ok(rules)
    }

    pub fn apply_ai_classification(
        &self,
        segment_id: &str,
        category: ActivityCategory,
        video_purpose: VideoPurpose,
        confidence: f32,
        reason: &str,
        model_version: &str,
    ) -> Result<bool> {
        Self::apply_ai_classification_on(
            &self.connection,
            segment_id,
            category,
            video_purpose,
            confidence,
            reason,
            model_version,
        )
    }

    fn apply_ai_classification_on(
        connection: &Connection,
        segment_id: &str,
        category: ActivityCategory,
        video_purpose: VideoPurpose,
        confidence: f32,
        reason: &str,
        model_version: &str,
    ) -> Result<bool> {
        let confidence = confidence.clamp(0.0, 1.0);
        let manually_owned = connection
            .query_row(
                "SELECT classification_source='manual' FROM activity_segments WHERE id=?1",
                [segment_id],
                |row| row.get::<_, bool>(0),
            )
            .optional()?;
        let Some(manually_owned) = manually_owned else {
            return Ok(false);
        };
        let disposition = decide_ai_disposition(confidence.into(), manually_owned);
        if disposition == AiDisposition::ManualLock {
            return Ok(false);
        }
        let changed = connection.execute(
            "UPDATE activity_segments
             SET category=?2, video_purpose=?3, confidence=?4,
                 classification_source='ai', reason=?5, model_version=?6,
                 needs_review=?7
             WHERE id=?1 AND classification_source != 'manual'
               AND (?2 != 'idle' OR category='idle')",
            params![
                segment_id,
                category_key(category),
                video_purpose_key(video_purpose),
                confidence,
                reason,
                model_version,
                disposition == AiDisposition::Review,
            ],
        )?;
        Ok(changed > 0)
    }

    pub fn save_page_snapshot(
        &self,
        visit_id: &str,
        summary: &str,
        text_snippet: &str,
        fetch_status: &str,
    ) -> Result<()> {
        self.connection.execute(
            "INSERT INTO page_snapshots(visit_id, summary, text_snippet, fetch_status)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(visit_id) DO UPDATE SET summary=excluded.summary,
                text_snippet=excluded.text_snippet, fetch_status=excluded.fetch_status",
            params![visit_id, summary, text_snippet, fetch_status],
        )?;
        Ok(())
    }

    pub fn save_page_classification(
        &self,
        visit_id: &str,
        classification_json: &str,
    ) -> Result<()> {
        Self::save_page_classification_on(&self.connection, visit_id, classification_json)
    }

    fn save_page_classification_on(
        connection: &Connection,
        visit_id: &str,
        classification_json: &str,
    ) -> Result<()> {
        connection.execute(
            "INSERT INTO page_snapshots(visit_id, summary, text_snippet, fetch_status)
             VALUES (?1, ?2, '', 'ai_classified')
             ON CONFLICT(visit_id) DO UPDATE SET
                summary=CASE WHEN page_snapshots.summary=''
                    THEN excluded.summary
                    ELSE page_snapshots.summary || char(10) || 'AI: ' || excluded.summary END,
                fetch_status=CASE WHEN instr(page_snapshots.fetch_status, 'ai_classified') > 0
                    THEN page_snapshots.fetch_status
                    ELSE page_snapshots.fetch_status || '+ai_classified' END",
            params![visit_id, classification_json],
        )?;
        Ok(())
    }

    pub fn get_setting_json(&self, key: &str) -> Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT value_json FROM settings WHERE key=?1",
                [key],
                |row| row.get(0),
            )
            .optional()
    }

    pub fn set_setting_json(&self, key: &str, value_json: &str) -> Result<()> {
        self.connection.execute(
            "INSERT INTO settings(key, value_json) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",
            params![key, value_json],
        )?;
        Ok(())
    }

    pub fn local_sync_device_id(&self, now_ms: i64) -> Result<String> {
        let proposed = random_device_id().map_err(invalid_review)?;
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let existing = transaction
            .query_row(
                "SELECT value_json FROM settings WHERE key='sync_device_id'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let device_id = existing
            .as_deref()
            .and_then(|value| serde_json::from_str::<String>(value).ok())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(proposed);
        let device_json =
            serde_json::to_string(&device_id).map_err(|error| invalid_review(error.to_string()))?;
        transaction.execute(
            "INSERT INTO settings(key, value_json) VALUES ('sync_device_id', ?1)
             ON CONFLICT(key) DO NOTHING",
            [device_json],
        )?;
        transaction.execute(
            "INSERT INTO sync_devices(device_id, created_at_ms, last_seen_at_ms)
             VALUES (?1, ?2, ?2)
             ON CONFLICT(device_id) DO UPDATE SET last_seen_at_ms=MAX(last_seen_at_ms, excluded.last_seen_at_ms)",
            params![device_id, now_ms],
        )?;
        transaction.commit()?;
        Ok(device_id)
    }

    pub fn append_local_sync_event(
        &self,
        occurred_at_ms: i64,
        entity_kind: &str,
        entity_id: &str,
        operation: &str,
        payload_json: &str,
    ) -> Result<SyncEvent> {
        let device_id = self.local_sync_device_id(occurred_at_ms)?;
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let sequence = transaction.query_row(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM sync_events WHERE device_id=?1",
            [&device_id],
            |row| row.get::<_, i64>(0),
        )?;
        let event = build_sync_event(
            &device_id,
            sequence,
            occurred_at_ms,
            entity_kind,
            entity_id,
            operation,
            payload_json,
        )
        .map_err(invalid_review)?;
        insert_sync_event_on(&transaction, &event, occurred_at_ms)?;
        transaction.execute(
            "UPDATE sync_devices SET last_seen_at_ms=MAX(last_seen_at_ms, ?2) WHERE device_id=?1",
            params![device_id, occurred_at_ms],
        )?;
        transaction.commit()?;
        Ok(event)
    }

    pub fn list_sync_events(&self) -> Result<Vec<SyncEvent>> {
        let mut statement = self.connection.prepare(
            "SELECT event_id, device_id, sequence, occurred_at_ms, entity_kind, entity_id,
                    operation, payload_json, payload_hash
             FROM sync_events
             ORDER BY occurred_at_ms, device_id, sequence, event_id",
        )?;
        statement.query_map([], sync_event_from_row)?.collect()
    }

    pub fn ensure_organization_sync_snapshot(&self) -> Result<usize> {
        let projects = self.list_work_ledger_projects(true)?;
        let mut entities = Vec::<(String, String, i64, String)>::new();
        for project in projects {
            let tasks = self.list_work_ledger_tasks(&project.id)?;
            entities.push((
                "project".to_string(),
                project.id.clone(),
                project.updated_at_ms,
                serde_json::to_string(&project)
                    .map_err(|error| invalid_review(error.to_string()))?,
            ));
            for task in tasks {
                entities.push((
                    "task".to_string(),
                    task.id.clone(),
                    task.updated_at_ms,
                    serde_json::to_string(&task)
                        .map_err(|error| invalid_review(error.to_string()))?,
                ));
            }
        }
        let mut appended = 0;
        for (kind, id, occurred_at_ms, payload) in entities {
            let exists = self.connection.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM sync_events WHERE entity_kind=?1 AND entity_id=?2
                 )",
                params![kind, id],
                |row| row.get::<_, bool>(0),
            )?;
            if !exists {
                self.append_local_sync_event(occurred_at_ms, &kind, &id, "upsert", &payload)?;
                appended += 1;
            }
        }
        Ok(appended)
    }

    pub fn import_sync_events(&self, events: Vec<SyncEvent>, imported_at_ms: i64) -> Result<usize> {
        let events = merge_sync_events([], events).map_err(invalid_review)?;
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let mut inserted = 0;
        for event in &events {
            inserted += insert_sync_event_on(&transaction, event, imported_at_ms)? as usize;
            transaction.execute(
                "INSERT INTO sync_devices(device_id, created_at_ms, last_seen_at_ms)
                 VALUES (?1, ?2, ?2)
                 ON CONFLICT(device_id) DO UPDATE SET last_seen_at_ms=MAX(last_seen_at_ms, excluded.last_seen_at_ms)",
                params![event.device_id, event.occurred_at_ms],
            )?;
        }
        let all_events = {
            let mut statement = transaction.prepare(
                "SELECT event_id, device_id, sequence, occurred_at_ms, entity_kind, entity_id,
                        operation, payload_json, payload_hash
                 FROM sync_events",
            )?;
            statement
                .query_map([], sync_event_from_row)?
                .collect::<Result<Vec<_>>>()?
        };
        for event in latest_entity_events(&all_events).values() {
            apply_organization_sync_projection_on(&transaction, event)?;
        }
        transaction.commit()?;
        Ok(inserted)
    }

    pub fn import_external_context(&self, import: &ExternalContextImport) -> Result<usize> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        let imported_at_ms = import
            .items
            .iter()
            .map(|item| item.imported_at_ms)
            .max()
            .unwrap_or_else(now_millis);
        transaction.execute(
            "INSERT INTO external_context_sources(source_id, source_name, source_kind, imported_at_ms)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(source_id) DO UPDATE SET source_name=excluded.source_name,
                source_kind=excluded.source_kind, imported_at_ms=excluded.imported_at_ms",
            params![import.source_id, import.source_name, import.source_kind, imported_at_ms],
        )?;
        for item in &import.items {
            transaction.execute(
                "INSERT INTO external_context_items(
                    id, source_id, source_name, source_kind, external_id, kind, title,
                    start_at_ms, end_at_ms, project_name, status, imported_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(id) DO UPDATE SET source_name=excluded.source_name,
                    title=excluded.title, start_at_ms=excluded.start_at_ms,
                    end_at_ms=excluded.end_at_ms, project_name=excluded.project_name,
                    status=excluded.status, imported_at_ms=excluded.imported_at_ms",
                params![
                    item.id,
                    item.source_id,
                    item.source_name,
                    item.source_kind,
                    item.external_id,
                    external_context_kind_key(item.kind),
                    item.title,
                    item.start_at_ms,
                    item.end_at_ms,
                    item.project_name,
                    item.status,
                    item.imported_at_ms,
                ],
            )?;
        }
        transaction.execute(
            "DELETE FROM external_context_items
             WHERE source_id=?1 AND imported_at_ms<>?2",
            params![import.source_id, imported_at_ms],
        )?;
        transaction.commit()?;
        Ok(import.items.len())
    }

    pub fn list_external_context(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<ExternalContextItem>> {
        let mut statement = self.connection.prepare(
            "SELECT id, source_id, source_name, source_kind, external_id, kind, title,
                    start_at_ms, end_at_ms, project_name, status, imported_at_ms
             FROM external_context_items
             WHERE kind<>'calendar_event'
                OR (COALESCE(end_at_ms, start_at_ms + 1)>?1 AND start_at_ms<?2)
             ORDER BY CASE kind WHEN 'calendar_event' THEN 0 WHEN 'project' THEN 1 ELSE 2 END,
                      COALESCE(start_at_ms, 0), source_name, title, id",
        )?;
        statement
            .query_map(params![start_ms, end_ms], external_context_item_from_row)?
            .collect()
    }

    pub fn start_focus_session(
        &self,
        id: &str,
        goal_date: &str,
        goal_text: &str,
        planned_minutes: u32,
        started_at_ms: i64,
    ) -> Result<()> {
        self.start_focus_session_for_task(
            id,
            goal_date,
            goal_text,
            planned_minutes,
            started_at_ms,
            None,
        )
    }

    pub fn start_focus_session_for_task(
        &self,
        id: &str,
        goal_date: &str,
        goal_text: &str,
        planned_minutes: u32,
        started_at_ms: i64,
        task_id: Option<&str>,
    ) -> Result<()> {
        self.connection.execute(
            "INSERT INTO focus_sessions(
                id, goal_date, goal_text, planned_minutes, started_at_ms, task_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id,
                goal_date,
                goal_text,
                planned_minutes.clamp(1, 240),
                started_at_ms,
                task_id,
            ],
        )?;
        Ok(())
    }

    pub fn start_focus_session_for_task_if_none(
        &self,
        id: &str,
        goal_date: &str,
        goal_text: &str,
        planned_minutes: u32,
        started_at_ms: i64,
        task_id: Option<&str>,
    ) -> Result<bool> {
        Ok(self.connection.execute(
            "INSERT INTO focus_sessions(
                id, goal_date, goal_text, planned_minutes, started_at_ms, task_id
             )
             SELECT ?1, ?2, ?3, ?4, ?5, ?6
             WHERE NOT EXISTS (
                SELECT 1 FROM focus_sessions WHERE ended_at_ms IS NULL
             )",
            params![
                id,
                goal_date,
                goal_text,
                planned_minutes.clamp(1, 240),
                started_at_ms,
                task_id,
            ],
        )? == 1)
    }

    pub fn pause_focus_session(&self, id: &str, paused_at_ms: i64) -> Result<bool> {
        Ok(self.connection.execute(
            "UPDATE focus_sessions SET paused_at_ms=?2
             WHERE id=?1 AND ended_at_ms IS NULL AND paused_at_ms IS NULL",
            params![id, paused_at_ms],
        )? == 1)
    }

    pub fn resume_focus_session(&self, id: &str, resumed_at_ms: i64) -> Result<bool> {
        Ok(self.connection.execute(
            "UPDATE focus_sessions
             SET paused_total_ms=paused_total_ms +
                    CASE WHEN ?2 > paused_at_ms THEN ?2 - paused_at_ms ELSE 0 END,
                 paused_at_ms=NULL
             WHERE id=?1 AND ended_at_ms IS NULL AND paused_at_ms IS NOT NULL",
            params![id, resumed_at_ms],
        )? == 1)
    }

    pub fn complete_expired_focus_session(
        &self,
        id: &str,
        ended_at_ms: i64,
        notified_at_ms: i64,
    ) -> Result<bool> {
        Ok(self.connection.execute(
            "UPDATE focus_sessions
             SET ended_at_ms=?2, notified_at_ms=?3, paused_at_ms=NULL
             WHERE id=?1 AND ended_at_ms IS NULL AND notified_at_ms IS NULL",
            params![id, ended_at_ms, notified_at_ms],
        )? == 1)
    }

    pub fn complete_focus_session(
        &self,
        id: &str,
        ended_at_ms: i64,
        outcome: &str,
    ) -> Result<bool> {
        let transaction = self.connection.unchecked_transaction()?;
        let focus = transaction
            .query_row(
                "SELECT goal_date, ended_at_ms, task_id FROM focus_sessions WHERE id=?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((goal_date, existing_end, task_id)) = focus else {
            return Ok(false);
        };
        if existing_end.is_some() {
            return Ok(false);
        }
        transaction.execute(
            "UPDATE focus_sessions
             SET ended_at_ms=?2,
                 outcome=?3,
                 paused_total_ms=paused_total_ms + CASE
                    WHEN paused_at_ms IS NOT NULL AND ?2 > paused_at_ms
                    THEN ?2 - paused_at_ms ELSE 0 END,
                 paused_at_ms=NULL
             WHERE id=?1 AND ended_at_ms IS NULL",
            params![id, ended_at_ms, outcome],
        )?;
        if let Some(task_id) = task_id
            && !outcome.trim().is_empty()
        {
            Self::insert_provenance_progress(
                &transaction,
                &task_id,
                outcome,
                ended_at_ms,
                ProgressOriginKind::FocusOutcome,
                id,
                Some(&goal_date),
            )?;
        }
        transaction.commit()?;
        Ok(true)
    }

    pub fn list_focus_sessions(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<FocusSessionRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, goal_date, goal_text, planned_minutes, started_at_ms, ended_at_ms, outcome,
                    task_id, paused_at_ms, paused_total_ms, notified_at_ms
             FROM focus_sessions
             WHERE started_at_ms < ?2 AND COALESCE(ended_at_ms, started_at_ms) > ?1
             ORDER BY started_at_ms ASC, id ASC",
        )?;
        statement
            .query_map(params![start_ms, end_ms], |row| {
                Ok(FocusSessionRecord {
                    id: row.get(0)?,
                    goal_date: row.get(1)?,
                    goal_text: row.get(2)?,
                    planned_minutes: row.get(3)?,
                    started_at_ms: row.get(4)?,
                    ended_at_ms: row.get(5)?,
                    outcome: row.get(6)?,
                    task_id: row.get(7)?,
                    paused_at_ms: row.get(8)?,
                    paused_total_ms: row.get(9)?,
                    notified_at_ms: row.get(10)?,
                })
            })?
            .collect()
    }

    pub fn sync_status(&self, now_ms: i64) -> Result<SyncStatus> {
        let device_id = self.local_sync_device_id(now_ms)?;
        let known_device_count =
            self.connection
                .query_row("SELECT COUNT(*) FROM sync_devices", [], |row| row.get(0))?;
        let (event_count, last_event_at_ms) = self.connection.query_row(
            "SELECT COUNT(*), MAX(occurred_at_ms) FROM sync_events",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(SyncStatus {
            device_id,
            known_device_count,
            event_count,
            last_event_at_ms,
            encryption_available: true,
        })
    }

    pub fn active_focus_session(&self) -> Result<Option<FocusSessionRecord>> {
        self.connection
            .query_row(
                "SELECT id, goal_date, goal_text, planned_minutes, started_at_ms, ended_at_ms,
                        outcome, task_id, paused_at_ms, paused_total_ms, notified_at_ms
                 FROM focus_sessions
                 WHERE ended_at_ms IS NULL
                 ORDER BY started_at_ms DESC, id DESC
                 LIMIT 1",
                [],
                |row| {
                    Ok(FocusSessionRecord {
                        id: row.get(0)?,
                        goal_date: row.get(1)?,
                        goal_text: row.get(2)?,
                        planned_minutes: row.get(3)?,
                        started_at_ms: row.get(4)?,
                        ended_at_ms: row.get(5)?,
                        outcome: row.get(6)?,
                        task_id: row.get(7)?,
                        paused_at_ms: row.get(8)?,
                        paused_total_ms: row.get(9)?,
                        notified_at_ms: row.get(10)?,
                    })
                },
            )
            .optional()
    }

    pub fn save_daily_goal(
        &self,
        date: &str,
        goals: &str,
        expected_output: &str,
        actual_output: &str,
    ) -> Result<()> {
        self.connection.execute(
            "INSERT INTO daily_goals(date, goals, expected_output, actual_output)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(date) DO UPDATE SET goals=excluded.goals,
                expected_output=excluded.expected_output, actual_output=excluded.actual_output",
            params![date, goals, expected_output, actual_output],
        )?;
        Ok(())
    }

    pub fn get_daily_goal(&self, date: &str) -> Result<DailyGoalRecord> {
        Ok(self
            .connection
            .query_row(
                "SELECT date, goals, expected_output, actual_output FROM daily_goals WHERE date=?1",
                [date],
                |row| {
                    Ok(DailyGoalRecord {
                        date: row.get(0)?,
                        goals: row.get(1)?,
                        expected_output: row.get(2)?,
                        actual_output: row.get(3)?,
                    })
                },
            )
            .optional()?
            .unwrap_or_else(|| DailyGoalRecord {
                date: date.to_string(),
                ..DailyGoalRecord::default()
            }))
    }

    pub fn list_daily_goals(&self) -> Result<Vec<DailyGoalRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT date, goals, expected_output, actual_output
             FROM daily_goals ORDER BY date ASC",
        )?;
        statement
            .query_map([], |row| {
                Ok(DailyGoalRecord {
                    date: row.get(0)?,
                    goals: row.get(1)?,
                    expected_output: row.get(2)?,
                    actual_output: row.get(3)?,
                })
            })?
            .collect()
    }

    pub fn save_daily_analysis(&self, analysis: &DailyAnalysisRecord) -> Result<()> {
        Self::upsert_daily_analysis(&self.connection, analysis)
    }

    fn upsert_daily_analysis(
        connection: &Connection,
        analysis: &DailyAnalysisRecord,
    ) -> Result<()> {
        connection.execute(
            "INSERT INTO daily_analyses(
                date, evidence_hash, portrait, recommendation, findings_json, protocol_version,
                source, generated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(date) DO UPDATE SET
                evidence_hash=excluded.evidence_hash,
                portrait=excluded.portrait,
                recommendation=excluded.recommendation,
                findings_json=excluded.findings_json,
                protocol_version=excluded.protocol_version,
                source=excluded.source,
                generated_at_ms=excluded.generated_at_ms",
            params![
                analysis.date,
                analysis.evidence_hash,
                analysis.portrait,
                analysis.recommendation,
                analysis.findings_json,
                analysis.protocol_version,
                analysis.source,
                analysis.generated_at_ms,
            ],
        )?;
        Ok(())
    }

    pub fn get_daily_analysis(&self, date: &str) -> Result<Option<DailyAnalysisRecord>> {
        self.connection
            .query_row(
                "SELECT date, evidence_hash, portrait, recommendation, findings_json,
                        protocol_version, source, generated_at_ms
                 FROM daily_analyses WHERE date=?1",
                [date],
                |row| {
                    Ok(DailyAnalysisRecord {
                        date: row.get(0)?,
                        evidence_hash: row.get(1)?,
                        portrait: row.get(2)?,
                        recommendation: row.get(3)?,
                        findings_json: row.get(4)?,
                        protocol_version: row.get(5)?,
                        source: row.get(6)?,
                        generated_at_ms: row.get(7)?,
                    })
                },
            )
            .optional()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_daily_analysis_job_generation(
        &self,
        id: &str,
        generation: i64,
        analysis: &DailyAnalysisRecord,
        executor_id: &str,
        model: &str,
        exit_code: Option<i32>,
    ) -> Result<bool> {
        let transaction = ai_job_completion_transaction(&self.connection)?;
        let audit = AiJobCompletionAudit {
            finished_at_ms: analysis.generated_at_ms,
            duration_ms: None,
            executor_id: Some(executor_id),
            model: Some(model),
            exit_code,
        };
        if !ai_job_generation_is_current_or_retire_on(
            &transaction,
            id,
            generation,
            Some("daily_analysis"),
            audit,
        )? {
            transaction.commit()?;
            return Ok(false);
        }
        Self::upsert_daily_analysis(&transaction, analysis)?;
        if !complete_ai_job_generation_on(&transaction, id, generation, audit, "", None)? {
            transaction.rollback()?;
            return Ok(false);
        }
        transaction.commit()?;
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_daily_analysis_job_generation_scoped(
        &self,
        id: &str,
        generation: i64,
        activity_scope: ActivityScope,
        analysis: &DailyAnalysisRecord,
        executor_id: &str,
        model: &str,
        exit_code: Option<i32>,
    ) -> Result<bool> {
        let transaction = ai_job_completion_transaction(&self.connection)?;
        let audit = AiJobCompletionAudit {
            finished_at_ms: analysis.generated_at_ms,
            duration_ms: None,
            executor_id: Some(executor_id),
            model: Some(model),
            exit_code,
        };
        if !ai_job_generation_is_current_or_retire_on(
            &transaction,
            id,
            generation,
            Some("daily_analysis"),
            audit,
        )? {
            transaction.commit()?;
            return Ok(false);
        }
        Self::upsert_daily_analysis_scoped(&transaction, activity_scope, analysis)?;
        if !complete_ai_job_generation_on(&transaction, id, generation, audit, "", None)? {
            transaction.rollback()?;
            return Ok(false);
        }
        transaction.commit()?;
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_segment_classification_job_generation(
        &self,
        id: &str,
        generation: i64,
        segment_id: &str,
        category: ActivityCategory,
        video_purpose: VideoPurpose,
        confidence: f32,
        reason: &str,
        model_version: &str,
        executor_id: &str,
        model: &str,
        exit_code: Option<i32>,
        finished_at_ms: i64,
    ) -> Result<bool> {
        self.complete_segment_classification_job_generation_on(
            id,
            generation,
            segment_id,
            category,
            video_purpose,
            confidence,
            reason,
            model_version,
            executor_id,
            model,
            exit_code,
            finished_at_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn consume_segment_classification_job_generation(
        &self,
        id: &str,
        generation: i64,
        segment_id: &str,
        category: ActivityCategory,
        video_purpose: VideoPurpose,
        confidence: f32,
        reason: &str,
        model_version: &str,
        executor_id: &str,
        model: &str,
        exit_code: Option<i32>,
        finished_at_ms: i64,
    ) -> Result<bool> {
        self.complete_segment_classification_job_generation_on(
            id,
            generation,
            segment_id,
            category,
            video_purpose,
            confidence,
            reason,
            model_version,
            executor_id,
            model,
            exit_code,
            finished_at_ms,
        )
    }

    pub fn save_daily_analysis_scoped(
        &self,
        activity_scope: ActivityScope,
        analysis: &DailyAnalysisRecord,
    ) -> Result<()> {
        Self::upsert_daily_analysis_scoped(&self.connection, activity_scope, analysis)
    }

    fn upsert_daily_analysis_scoped(
        connection: &Connection,
        activity_scope: ActivityScope,
        analysis: &DailyAnalysisRecord,
    ) -> Result<()> {
        connection.execute(
            "INSERT INTO daily_analysis_scopes(
                date, activity_scope, evidence_hash, portrait, recommendation, findings_json,
                protocol_version, source, generated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(date, activity_scope) DO UPDATE SET
                evidence_hash=excluded.evidence_hash,
                portrait=excluded.portrait,
                recommendation=excluded.recommendation,
                findings_json=excluded.findings_json,
                protocol_version=excluded.protocol_version,
                source=excluded.source,
                generated_at_ms=excluded.generated_at_ms",
            params![
                analysis.date,
                activity_scope_key(activity_scope),
                analysis.evidence_hash,
                analysis.portrait,
                analysis.recommendation,
                analysis.findings_json,
                analysis.protocol_version,
                analysis.source,
                analysis.generated_at_ms,
            ],
        )?;
        Ok(())
    }

    pub fn get_daily_analysis_scoped(
        &self,
        date: &str,
        activity_scope: ActivityScope,
    ) -> Result<Option<DailyAnalysisRecord>> {
        let scoped = self
            .connection
            .query_row(
                "SELECT date, evidence_hash, portrait, recommendation, findings_json,
                    protocol_version, source, generated_at_ms
             FROM daily_analysis_scopes WHERE date=?1 AND activity_scope=?2",
                params![date, activity_scope_key(activity_scope)],
                daily_analysis_record_from_row,
            )
            .optional()?;
        if scoped.is_some() || activity_scope != ActivityScope::All {
            return Ok(scoped);
        }
        self.get_daily_analysis(date)
    }

    #[allow(clippy::too_many_arguments)]
    fn complete_segment_classification_job_generation_on(
        &self,
        id: &str,
        generation: i64,
        segment_id: &str,
        category: ActivityCategory,
        video_purpose: VideoPurpose,
        confidence: f32,
        reason: &str,
        model_version: &str,
        executor_id: &str,
        model: &str,
        exit_code: Option<i32>,
        finished_at_ms: i64,
    ) -> Result<bool> {
        let transaction = ai_job_completion_transaction(&self.connection)?;
        let audit = AiJobCompletionAudit {
            finished_at_ms,
            duration_ms: None,
            executor_id: Some(executor_id),
            model: Some(model),
            exit_code,
        };
        if !ai_job_generation_is_current_or_retire_on(
            &transaction,
            id,
            generation,
            Some("classify_segment"),
            audit,
        )? {
            transaction.commit()?;
            return Ok(false);
        }
        let Some((before_json, evidence_summary, manually_owned)) =
            classification_review_value_on(&transaction, segment_id)?
        else {
            transaction.rollback()?;
            return Ok(false);
        };
        let execution = execution_audit_for_job_on(
            &transaction,
            id,
            generation,
            Some(executor_id),
            Some(model),
            Some(finished_at_ms),
            None,
            exit_code,
            None,
            "",
        )?;
        if classification_evidence_hash_on(&transaction, segment_id)?.as_deref()
            != Some(execution.evidence_hash.as_str())
        {
            if !complete_ai_job_generation_on(&transaction, id, generation, audit, "", None)? {
                transaction.rollback()?;
                return Ok(false);
            }
            transaction.commit()?;
            return Ok(true);
        }
        let proposed_json = serde_json::to_string(&AiClassificationReviewValue {
            category,
            video_purpose,
            confidence: f64::from(confidence.clamp(0.0, 1.0)),
            reason: reason.into(),
            model_version: model_version.into(),
        })
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        if manually_owned {
            if !complete_ai_job_generation_on(&transaction, id, generation, audit, "", None)? {
                transaction.rollback()?;
                return Ok(false);
            }
            transaction.commit()?;
            return Ok(true);
        }
        insert_generated_ai_review_on(
            &transaction,
            &AiReviewDraft {
                id: format!("review-{id}-{generation}"),
                job_id: Some(id.into()),
                kind: AiReviewKind::Classification,
                subject_id: segment_id.into(),
                before_json,
                proposed_json,
                confidence: Some(f64::from(confidence.clamp(0.0, 1.0))),
                evidence_summary,
                evidence_hash: execution.evidence_hash.clone(),
                execution,
                created_at_ms: finished_at_ms,
                execution_error: None,
            },
            false,
        )?;
        if !complete_ai_job_generation_on(&transaction, id, generation, audit, "", None)? {
            transaction.rollback()?;
            return Ok(false);
        }
        transaction.commit()?;
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_page_classification_job_generation(
        &self,
        id: &str,
        generation: i64,
        visit_id: &str,
        classification_json: &str,
        executor_id: &str,
        model: &str,
        exit_code: Option<i32>,
        finished_at_ms: i64,
    ) -> Result<bool> {
        let transaction = ai_job_completion_transaction(&self.connection)?;
        let audit = AiJobCompletionAudit {
            finished_at_ms,
            duration_ms: None,
            executor_id: Some(executor_id),
            model: Some(model),
            exit_code,
        };
        if !ai_job_generation_is_current_or_retire_on(
            &transaction,
            id,
            generation,
            Some("classify_page"),
            audit,
        )? {
            transaction.commit()?;
            return Ok(false);
        }
        Self::save_page_classification_on(&transaction, visit_id, classification_json)?;
        if !complete_ai_job_generation_on(&transaction, id, generation, audit, "", None)? {
            transaction.rollback()?;
            return Ok(false);
        }
        transaction.commit()?;
        Ok(true)
    }

    pub fn save_trend_analysis(&self, analysis: &TrendAnalysisRecord) -> Result<()> {
        Self::upsert_trend_analysis(&self.connection, analysis)
    }

    pub fn save_trend_research_analysis(&self, record: &TrendResearchAnalysisRecord) -> Result<()> {
        Self::upsert_trend_research_analysis(&self.connection, record)
    }

    fn upsert_trend_research_analysis(
        connection: &Connection,
        record: &TrendResearchAnalysisRecord,
    ) -> Result<()> {
        let analysis = &record.analysis;
        let findings = serde_json::to_string(&analysis.findings)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let limitations = serde_json::to_string(&analysis.limitations)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let status = match analysis.status {
            ResearchStatus::Ready => "ready",
            ResearchStatus::LimitationsOnly => "limitations_only",
        };
        let confidence = analysis
            .findings
            .iter()
            .map(|finding| finding.confidence)
            .reduce(f64::max)
            .unwrap_or_default();
        let storage_evidence_hash = format!(
            "{TREND_RESEARCH_STORAGE_NAMESPACE}{}",
            analysis.evidence_hash
        );
        connection.execute(
            "INSERT INTO trend_analyses(
                range_start, range_end, evidence_hash, summary, observations_json,
                suggestions_json, source, model, confidence, generated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(range_start, range_end, evidence_hash) DO UPDATE SET
                summary=excluded.summary,
                observations_json=excluded.observations_json,
                suggestions_json=excluded.suggestions_json,
                source=excluded.source,
                model=excluded.model,
                confidence=excluded.confidence,
                generated_at_ms=excluded.generated_at_ms",
            params![
                record.range_start,
                record.range_end,
                storage_evidence_hash,
                status,
                findings,
                limitations,
                analysis.source,
                analysis.model,
                confidence,
                now_millis(),
            ],
        )?;
        Ok(())
    }

    fn upsert_trend_analysis(
        connection: &Connection,
        analysis: &TrendAnalysisRecord,
    ) -> Result<()> {
        let observations = serde_json::to_string(&analysis.observations)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let suggestions = serde_json::to_string(&analysis.suggestions)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        connection.execute(
            "INSERT INTO trend_analyses(
                range_start, range_end, evidence_hash, summary, observations_json,
                suggestions_json, source, model, confidence, generated_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(range_start, range_end, evidence_hash) DO UPDATE SET
                summary=excluded.summary,
                observations_json=excluded.observations_json,
                suggestions_json=excluded.suggestions_json,
                source=excluded.source,
                model=excluded.model,
                confidence=excluded.confidence,
                generated_at_ms=excluded.generated_at_ms",
            params![
                analysis.range_start,
                analysis.range_end,
                analysis.evidence_hash,
                analysis.summary,
                observations,
                suggestions,
                analysis.source,
                analysis.model,
                analysis.confidence,
                analysis.generated_at_ms,
            ],
        )?;
        Ok(())
    }

    pub fn complete_trend_analysis_job_generation(
        &self,
        id: &str,
        generation: i64,
        analysis: &TrendAnalysisRecord,
    ) -> Result<bool> {
        let transaction = ai_job_completion_transaction(&self.connection)?;
        let audit = AiJobCompletionAudit {
            finished_at_ms: now_millis(),
            duration_ms: None,
            executor_id: Some(&analysis.source),
            model: Some(&analysis.model),
            exit_code: Some(0),
        };
        if !ai_job_generation_is_current_or_retire_on(
            &transaction,
            id,
            generation,
            Some("trend_analysis"),
            audit,
        )? {
            transaction.commit()?;
            return Ok(false);
        }

        Self::upsert_trend_analysis(&transaction, analysis)?;
        if !complete_ai_job_generation_on(&transaction, id, generation, audit, "", None)? {
            transaction.rollback()?;
            return Ok(false);
        }
        transaction.commit()?;
        Ok(true)
    }

    pub fn complete_trend_research_analysis_job_generation(
        &self,
        id: &str,
        generation: i64,
        record: &TrendResearchAnalysisRecord,
    ) -> Result<bool> {
        let transaction = ai_job_completion_transaction(&self.connection)?;
        let audit = AiJobCompletionAudit {
            finished_at_ms: now_millis(),
            duration_ms: None,
            executor_id: Some(&record.analysis.source),
            model: Some(&record.analysis.model),
            exit_code: Some(0),
        };
        if !ai_job_generation_is_current_or_retire_on(
            &transaction,
            id,
            generation,
            Some("trend_research_analysis"),
            audit,
        )? {
            transaction.commit()?;
            return Ok(false);
        }
        Self::upsert_trend_research_analysis(&transaction, record)?;
        if !complete_ai_job_generation_on(&transaction, id, generation, audit, "", None)? {
            transaction.rollback()?;
            return Ok(false);
        }
        transaction.commit()?;
        Ok(true)
    }

    pub fn get_trend_analysis(
        &self,
        range_start: &str,
        range_end: &str,
        evidence_hash: &str,
    ) -> Result<Option<TrendAnalysisRecord>> {
        let row = self
            .connection
            .query_row(
                "SELECT range_start, range_end, evidence_hash, summary, observations_json,
                        suggestions_json, source, model, confidence, generated_at_ms
                 FROM trend_analyses
                 WHERE range_start=?1 AND range_end=?2 AND evidence_hash=?3",
                params![range_start, range_end, evidence_hash],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, f64>(8)?,
                        row.get::<_, i64>(9)?,
                    ))
                },
            )
            .optional()?;
        row.map(
            |(
                range_start,
                range_end,
                evidence_hash,
                summary,
                observations,
                suggestions,
                source,
                model,
                confidence,
                generated_at_ms,
            )| {
                Ok(TrendAnalysisRecord {
                    range_start,
                    range_end,
                    evidence_hash,
                    summary,
                    observations: serde_json::from_str(&observations).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            4,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    suggestions: serde_json::from_str(&suggestions).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    source,
                    model,
                    confidence,
                    generated_at_ms,
                })
            },
        )
        .transpose()
    }

    pub fn get_trend_research_analysis(
        &self,
        range_start: &str,
        range_end: &str,
        evidence_hash: &str,
    ) -> Result<Option<TrendResearchAnalysisRecord>> {
        let storage_evidence_hash = format!("{TREND_RESEARCH_STORAGE_NAMESPACE}{evidence_hash}");
        let row = self
            .connection
            .query_row(
                "SELECT summary, observations_json, suggestions_json, source, model
                 FROM trend_analyses
                 WHERE range_start=?1 AND range_end=?2
                   AND evidence_hash=?3
                   AND summary IN ('ready', 'limitations_only')",
                params![range_start, range_end, storage_evidence_hash],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?;
        row.map(|(status, findings, limitations, source, model)| {
            let status = match status.as_str() {
                "ready" => ResearchStatus::Ready,
                "limitations_only" => ResearchStatus::LimitationsOnly,
                _ => return Ok(None),
            };
            let findings =
                serde_json::from_str::<Vec<TrendResearchFinding>>(&findings).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
            let limitations =
                serde_json::from_str::<Vec<String>>(&limitations).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        2,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
            Ok(Some(TrendResearchAnalysisRecord {
                range_start: range_start.to_string(),
                range_end: range_end.to_string(),
                analysis: TrendResearchAnalysis {
                    status,
                    findings,
                    limitations,
                    source,
                    model,
                    evidence_hash: evidence_hash.to_string(),
                    activity_scope: ActivityScope::All,
                },
            }))
        })
        .transpose()
        .map(Option::flatten)
    }

    pub fn enqueue_ai_job(
        &self,
        kind: &str,
        payload_json: &str,
        now_ms: i64,
        execution: &AiExecutionSnapshot,
    ) -> Result<String> {
        self.enqueue_ai_job_hashed(kind, payload_json, payload_json, now_ms, false, execution)
            .map(|(id, _)| id)
    }

    pub fn enqueue_ai_job_for_subject(
        &self,
        kind: &str,
        subject_key: &str,
        payload_json: &str,
        now_ms: i64,
        execution: &AiExecutionSnapshot,
    ) -> Result<String> {
        self.enqueue_ai_job_hashed(kind, subject_key, payload_json, now_ms, false, execution)
            .map(|(id, _)| id)
    }

    pub(crate) fn enqueue_ai_job_for_subject_with_outcome(
        &self,
        kind: &str,
        subject_key: &str,
        payload_json: &str,
        now_ms: i64,
        execution: &AiExecutionSnapshot,
    ) -> Result<(String, bool)> {
        self.enqueue_ai_job_hashed(kind, subject_key, payload_json, now_ms, false, execution)
    }

    pub fn force_enqueue_ai_job_for_subject(
        &self,
        kind: &str,
        subject_key: &str,
        payload_json: &str,
        now_ms: i64,
        execution: &AiExecutionSnapshot,
    ) -> Result<String> {
        self.enqueue_ai_job_hashed(kind, subject_key, payload_json, now_ms, true, execution)
            .map(|(id, _)| id)
    }

    fn enqueue_ai_job_hashed(
        &self,
        kind: &str,
        hash_input: &str,
        payload_json: &str,
        now_ms: i64,
        force: bool,
        execution: &AiExecutionSnapshot,
    ) -> Result<(String, bool)> {
        let transaction = self.connection.unchecked_transaction()?;
        let result = enqueue_ai_job_hashed_on(
            &transaction,
            kind,
            hash_input,
            payload_json,
            now_ms,
            force,
            execution,
        )?;
        transaction.commit()?;
        Ok(result)
    }

    pub fn ai_job_count(&self) -> Result<i64> {
        self.connection
            .query_row("SELECT COUNT(*) FROM ai_jobs", [], |row| row.get(0))
    }

    pub fn pending_ai_job_count(&self) -> Result<i64> {
        self.connection.query_row(
            "SELECT COUNT(*) FROM ai_jobs current
             WHERE status IN ('pending', 'running')
               AND NOT EXISTS(
                 SELECT 1 FROM ai_jobs newer
                 WHERE newer.kind=current.kind
                   AND newer.subject_key=current.subject_key
                   AND current.subject_key != ''
                   AND newer.generation > current.generation
               )",
            [],
            |row| row.get(0),
        )
    }

    pub fn get_ai_job(&self, id: &str) -> Result<Option<AiJob>> {
        self.connection
            .query_row(
                &format!("SELECT {AI_JOB_COLUMNS} FROM ai_jobs WHERE id=?1"),
                [id],
                ai_job_from_row,
            )
            .optional()
    }

    pub fn list_pending_ai_jobs(
        &self,
        status: Option<AiJobStatus>,
        limit: u32,
    ) -> Result<Vec<AiQueueRecord>> {
        if limit == 0 || limit > 500 {
            return Err(rusqlite::Error::InvalidParameterName(
                "AI queue limit must be between 1 and 500".into(),
            ));
        }
        let status = status.map(AiJobStatus::as_database);
        let mut statement = self.connection.prepare(&format!(
            "SELECT {AI_JOB_COLUMNS}
             FROM ai_jobs
             WHERE (?1 IS NULL OR status=?1)
             ORDER BY CASE status
                WHEN 'awaiting-reassignment' THEN 0
                WHEN 'running' THEN 1
                WHEN 'pending' THEN 2
                ELSE 3
             END, next_attempt_at_ms ASC, id ASC
             LIMIT ?2"
        ))?;
        statement
            .query_map(params![status, limit], ai_job_from_row)?
            .map(|row| row.map(AiQueueRecord::from))
            .collect()
    }

    /// Moves queued Codex work that points at an executable which no longer exists out of the
    /// runnable queue. Codex Desktop installs its CLI under a versioned directory, so an app
    /// update can otherwise leave thousands of historical jobs retrying a path that can never
    /// succeed and starving newly queued workflow recognition.
    pub fn quarantine_unavailable_codex_jobs(&self, now_ms: i64) -> Result<usize> {
        let mut statement = self.connection.prepare(
            "SELECT DISTINCT executor_id
             FROM ai_jobs
             WHERE execution_mode='codex'
               AND status IN ('pending', 'running')
               AND executor_id != ''",
        )?;
        let executors = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>>>()?;
        drop(statement);

        let unavailable = executors
            .into_iter()
            .filter(|executor| {
                let path = std::path::Path::new(executor);
                let explicit_path = path.is_absolute() || executor.contains(['/', '\\']);
                explicit_path && !path.is_file()
            })
            .collect::<Vec<_>>();
        if unavailable.is_empty() {
            return Ok(0);
        }

        let transaction = ai_job_completion_transaction(&self.connection)?;
        let mut quarantined = 0_usize;
        for executor in unavailable {
            quarantined = quarantined.saturating_add(transaction.execute(
                "UPDATE ai_jobs
                 SET status='awaiting-reassignment',
                     next_attempt_at_ms=?2,
                     last_error='Queued Codex executable is no longer available; retry with the current executor',
                     finished_at_ms=COALESCE(finished_at_ms, ?2),
                     duration_ms=COALESCE(duration_ms, 0),
                     error_kind='codex'
                 WHERE execution_mode='codex'
                   AND executor_id=?1
                   AND status IN ('pending', 'running')",
                params![executor, now_ms],
            )?);
        }
        transaction.commit()?;
        Ok(quarantined)
    }

    pub fn bulk_retry_ai_jobs_with_current_mode(
        &self,
        validated_jobs: &[(String, String)],
        current_execution: &AiExecutionSnapshot,
        now_ms: i64,
    ) -> Result<Vec<AiQueueRecord>> {
        if validated_jobs.is_empty() || validated_jobs.len() > 500 {
            return Err(rusqlite::Error::InvalidParameterName(
                "Select between 1 and 500 paused AI jobs".into(),
            ));
        }
        if validated_jobs
            .iter()
            .any(|(_, evidence_hash)| evidence_hash.trim().is_empty())
        {
            return Err(rusqlite::Error::InvalidParameterName(
                "AI job reassignment requires validated evidence".into(),
            ));
        }
        let unique_ids = validated_jobs
            .iter()
            .map(|(job_id, _)| job_id)
            .collect::<HashSet<_>>();
        if unique_ids.len() != validated_jobs.len() {
            return Err(rusqlite::Error::InvalidParameterName(
                "AI job selection contains duplicate ids".into(),
            ));
        }

        let transaction = ai_job_completion_transaction(&self.connection)?;
        let mut jobs = Vec::with_capacity(validated_jobs.len());
        for (job_id, validated_evidence_hash) in validated_jobs {
            let job = transaction
                .query_row(
                    "SELECT subject_key, kind, payload_json, status, evidence_hash
                     FROM ai_jobs WHERE id=?1",
                    [job_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| {
                    rusqlite::Error::InvalidParameterName(format!("Unknown AI job: {job_id}"))
                })?;
            if job.3 != "awaiting-reassignment" {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "AI job {job_id} is no longer awaiting reassignment"
                )));
            }
            if !job.4.is_empty() && job.4 != *validated_evidence_hash {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "AI job {job_id} evidence changed while being reassigned"
                )));
            }
            jobs.push((
                job_id.clone(),
                job.0,
                job.1,
                job.2,
                validated_evidence_hash.clone(),
            ));
        }

        let mut created_ids = Vec::with_capacity(jobs.len());
        for (old_id, subject_key, kind, payload_json, evidence_hash) in jobs {
            let mut execution = current_execution.clone();
            execution.evidence_hash = evidence_hash;
            execution.created_at_ms = now_ms;
            let (new_id, inserted) = enqueue_ai_job_hashed_on(
                &transaction,
                &kind,
                &subject_key,
                &payload_json,
                now_ms,
                true,
                &execution,
            )?;
            if !inserted {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "AI job {old_id} could not be reassigned"
                )));
            }
            let sealed = transaction.execute(
                "UPDATE ai_jobs
                 SET status='complete', finished_at_ms=?2,
                     duration_ms=COALESCE(duration_ms, 0)
                 WHERE id=?1 AND status='awaiting-reassignment'",
                params![old_id, now_ms],
            )?;
            if sealed != 1 {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "AI job {old_id} changed while being reassigned"
                )));
            }
            created_ids.push(new_id);
        }

        let mut created = Vec::with_capacity(created_ids.len());
        for id in created_ids {
            let job = transaction.query_row(
                &format!("SELECT {AI_JOB_COLUMNS} FROM ai_jobs WHERE id=?1"),
                [id],
                ai_job_from_row,
            )?;
            created.push(AiQueueRecord::from(job));
        }
        transaction.commit()?;
        Ok(created)
    }

    pub fn claim_next_due_ai_job(&self, now_ms: i64) -> Result<Option<AiJob>> {
        self.connection
            .query_row(
                &format!(
                    "UPDATE ai_jobs SET status='running', started_at_ms=?1,
                            finished_at_ms=NULL, duration_ms=NULL,
                            actual_executor_id=NULL, actual_model=NULL,
                            exit_code=NULL, error_kind=NULL
                     WHERE id=(
                        SELECT id FROM ai_jobs
                        WHERE status='pending' AND next_attempt_at_ms <= ?1
                          AND NOT EXISTS(
                            SELECT 1 FROM ai_jobs newer
                            WHERE newer.kind=ai_jobs.kind
                              AND newer.subject_key=ai_jobs.subject_key
                              AND ai_jobs.subject_key != ''
                              AND newer.generation > ai_jobs.generation
                          )
                        ORDER BY CASE kind
                            WHEN 'work_ledger_assignment' THEN 0
                            WHEN 'daily_analysis' THEN 1
                            WHEN 'trend_analysis' THEN 1
                            WHEN 'trend_research_analysis' THEN 1
                            WHEN 'classify_segment' THEN 2
                            WHEN 'classify_page' THEN 2
                            ELSE 3
                        END ASC,
                        CASE WHEN kind='work_ledger_assignment'
                             THEN execution_created_at_ms ELSE 0 END DESC,
                        next_attempt_at_ms ASC, id ASC
                        LIMIT 1
                     ) AND status='pending'
                       AND NOT EXISTS(
                         SELECT 1 FROM ai_jobs newer
                         WHERE newer.kind=ai_jobs.kind
                           AND newer.subject_key=ai_jobs.subject_key
                           AND ai_jobs.subject_key != ''
                           AND newer.generation > ai_jobs.generation
                       )
                     RETURNING {AI_JOB_COLUMNS}"
                ),
                [now_ms],
                ai_job_from_row,
            )
            .optional()
    }

    pub fn next_due_ai_job(&self, now_ms: i64) -> Result<Option<AiJob>> {
        self.connection
            .query_row(
                &format!(
                    "SELECT {AI_JOB_COLUMNS}
                     FROM ai_jobs
                     WHERE status='pending' AND next_attempt_at_ms <= ?1
                       AND NOT EXISTS(
                         SELECT 1 FROM ai_jobs newer
                         WHERE newer.kind=ai_jobs.kind
                           AND newer.subject_key=ai_jobs.subject_key
                           AND ai_jobs.subject_key != ''
                           AND newer.generation > ai_jobs.generation
                       )
                     ORDER BY CASE kind
                        WHEN 'work_ledger_assignment' THEN 0
                        WHEN 'daily_analysis' THEN 1
                        WHEN 'trend_analysis' THEN 1
                        WHEN 'trend_research_analysis' THEN 1
                        WHEN 'classify_segment' THEN 2
                        WHEN 'classify_page' THEN 2
                        ELSE 3
                     END ASC,
                     CASE WHEN kind='work_ledger_assignment'
                          THEN execution_created_at_ms ELSE 0 END DESC,
                     next_attempt_at_ms ASC, id ASC
                     LIMIT 1"
                ),
                [now_ms],
                ai_job_from_row,
            )
            .optional()
    }

    pub fn mark_ai_job_running(&self, id: &str, now_ms: i64) -> Result<()> {
        self.connection.execute(
            "UPDATE ai_jobs SET status='running', started_at_ms=?2
             WHERE id=?1 AND status='pending'
               AND NOT EXISTS(
                 SELECT 1 FROM ai_jobs newer
                 WHERE newer.kind=ai_jobs.kind
                   AND newer.subject_key=ai_jobs.subject_key
                   AND ai_jobs.subject_key != ''
                   AND newer.generation > ai_jobs.generation
               )",
            params![id, now_ms],
        )?;
        Ok(())
    }

    pub fn complete_ai_job_generation_audit(
        &self,
        id: &str,
        generation: i64,
        finished_at_ms: i64,
        executor_id: Option<&str>,
        model: Option<&str>,
        exit_code: Option<i32>,
    ) -> Result<bool> {
        let transaction = ai_job_completion_transaction(&self.connection)?;
        let completed = complete_ai_job_generation_on(
            &transaction,
            id,
            generation,
            AiJobCompletionAudit {
                finished_at_ms,
                duration_ms: None,
                executor_id,
                model,
                exit_code,
            },
            "",
            None,
        )?;
        transaction.commit()?;
        Ok(completed)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_ai_job_generation_audit_with_duration(
        &self,
        id: &str,
        generation: i64,
        finished_at_ms: i64,
        duration_ms: i64,
        executor_id: Option<&str>,
        model: Option<&str>,
        exit_code: Option<i32>,
    ) -> Result<bool> {
        let transaction = ai_job_completion_transaction(&self.connection)?;
        let completed = complete_ai_job_generation_on(
            &transaction,
            id,
            generation,
            AiJobCompletionAudit {
                finished_at_ms,
                duration_ms: Some(duration_ms),
                executor_id,
                model,
                exit_code,
            },
            "",
            None,
        )?;
        transaction.commit()?;
        Ok(completed)
    }

    pub fn complete_ai_job_generation_error(
        &self,
        id: &str,
        generation: i64,
        finished_at_ms: i64,
        error: &str,
        error_kind: AiExecutionErrorKind,
        executor_id: Option<&str>,
        model: Option<&str>,
        exit_code: Option<i32>,
    ) -> Result<bool> {
        let sanitized = sanitize_ai_error(error);
        let transaction = ai_job_completion_transaction(&self.connection)?;
        let audit = AiJobCompletionAudit {
            finished_at_ms,
            duration_ms: None,
            executor_id,
            model,
            exit_code,
        };
        if !ai_job_generation_is_current_or_retire_on(&transaction, id, generation, None, audit)? {
            transaction.commit()?;
            return Ok(false);
        }
        insert_execution_error_review_for_job_on(
            &transaction,
            id,
            generation,
            finished_at_ms,
            &sanitized,
            error_kind,
            executor_id,
            model,
            exit_code,
            "terminal-error",
        )?;
        let completed = complete_ai_job_generation_on(
            &transaction,
            id,
            generation,
            audit,
            &sanitized,
            Some(error_kind.as_database()),
        )?;
        transaction.commit()?;
        Ok(completed)
    }

    pub fn complete_work_ledger_ai_suggestion_job(
        &self,
        id: &str,
        generation: i64,
        suggestion: &AiWorkLedgerSuggestion,
        finished_at_ms: i64,
        duration_ms: i64,
        force_review: bool,
    ) -> Result<bool> {
        let (link_table, evidence_column) = match suggestion.evidence_kind.as_str() {
            "activity" => ("task_activity_links", "activity_segment_id"),
            "browser" => ("task_browser_links", "browser_visit_id"),
            _ => return Err(rusqlite::Error::InvalidQuery),
        };
        let transaction = ai_job_completion_transaction(&self.connection)?;
        let audit = AiJobCompletionAudit {
            finished_at_ms,
            duration_ms: Some(duration_ms),
            executor_id: Some(&suggestion.provider_id),
            model: Some(&suggestion.model),
            exit_code: Some(0),
        };
        if !ai_job_generation_is_current_or_retire_on(
            &transaction,
            id,
            generation,
            Some("work_ledger_assignment"),
            audit,
        )? {
            transaction.commit()?;
            return Ok(false);
        }
        let existing_link = transaction.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {link_table} WHERE {evidence_column}=?1)"),
            [&suggestion.evidence_id],
            |row| row.get::<_, bool>(0),
        )?;
        let before = workflow_review_value_on(
            &transaction,
            &suggestion.evidence_kind,
            &suggestion.evidence_id,
            suggestion.created_at_ms,
        )?;
        let before_json = serde_json::to_string(&before)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let proposed_json = serde_json::to_string(&AiWorkflowAssignmentReviewValue {
            evidence_kind: suggestion.evidence_kind.clone(),
            evidence_id: suggestion.evidence_id.clone(),
            task_id: Some(suggestion.task_id.clone()),
            confidence: suggestion.confidence,
            reason: suggestion.reason.clone(),
            created_at_ms: suggestion.created_at_ms,
        })
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let execution = execution_audit_for_job_on(
            &transaction,
            id,
            generation,
            Some(&suggestion.provider_id),
            Some(&suggestion.model),
            Some(finished_at_ms),
            Some(duration_ms),
            Some(0),
            None,
            "",
        )?;
        let review_id = format!("review-{id}-{generation}");
        let state = insert_generated_ai_review_on(
            &transaction,
            &AiReviewDraft {
                id: review_id.clone(),
                job_id: Some(id.into()),
                kind: AiReviewKind::WorkflowAssignment,
                subject_id: suggestion.evidence_id.clone(),
                before_json,
                proposed_json,
                confidence: Some(suggestion.confidence),
                evidence_summary: format!(
                    "{} evidence {}",
                    suggestion.evidence_kind, suggestion.evidence_id
                ),
                evidence_hash: suggestion.evidence_hash.clone(),
                execution,
                created_at_ms: suggestion.created_at_ms,
                execution_error: None,
            },
            existing_link || force_review,
        )?;
        match state {
            AiReviewState::Pending => {
                transaction.execute(
                    "INSERT INTO work_ledger_ai_suggestions(
                        review_id, evidence_kind, evidence_id, evidence_hash, task_id, confidence,
                        reason, provider_id, model, created_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                     ON CONFLICT(evidence_kind, evidence_id) DO UPDATE SET
                        review_id=excluded.review_id, evidence_hash=excluded.evidence_hash,
                        task_id=excluded.task_id, confidence=excluded.confidence,
                        reason=excluded.reason, provider_id=excluded.provider_id,
                        model=excluded.model, created_at_ms=excluded.created_at_ms",
                    params![
                        review_id,
                        suggestion.evidence_kind,
                        suggestion.evidence_id,
                        suggestion.evidence_hash,
                        suggestion.task_id,
                        suggestion.confidence,
                        suggestion.reason,
                        suggestion.provider_id,
                        suggestion.model,
                        suggestion.created_at_ms,
                    ],
                )?;
            }
            AiReviewState::AutoApplied => {
                transaction.execute(
                    "DELETE FROM work_ledger_ai_suggestions
                     WHERE evidence_kind=?1 AND evidence_id=?2",
                    params![suggestion.evidence_kind, suggestion.evidence_id],
                )?;
            }
            _ => return Err(invalid_review("Unexpected workflow review state")),
        }
        if !complete_ai_job_generation_on(&transaction, id, generation, audit, "", None)? {
            transaction.rollback()?;
            return Ok(false);
        }
        transaction.commit()?;
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_project_draft_job(
        &self,
        id: &str,
        generation: i64,
        subject_id: &str,
        proposal: &ProjectDraftProposal,
        cluster_episodes: &[(String, Vec<String>)],
        provider_id: &str,
        model: &str,
        finished_at_ms: i64,
        duration_ms: i64,
        auto_apply: bool,
    ) -> Result<bool> {
        let transaction = ai_job_completion_transaction(&self.connection)?;
        let audit = AiJobCompletionAudit {
            finished_at_ms,
            duration_ms: Some(duration_ms),
            executor_id: Some(provider_id),
            model: Some(model),
            exit_code: Some(0),
        };
        if !ai_job_generation_is_current_or_retire_on(
            &transaction,
            id,
            generation,
            Some("work_ledger_assignment"),
            audit,
        )? {
            transaction.commit()?;
            return Ok(false);
        }
        for (cluster_id, _) in cluster_episodes {
            let already_open: bool = transaction.query_row(
                "SELECT EXISTS(
                    SELECT 1
                    FROM project_draft_bindings binding
                    JOIN ai_review_records review ON review.id=binding.review_id
                    WHERE binding.cluster_id=?1 AND review.state='pending'
                )",
                [cluster_id],
                |row| row.get(0),
            )?;
            if already_open {
                return Err(invalid_review(
                    "A candidate cluster is already bound to an open project draft",
                ));
            }
        }
        let proposed_json = serde_json::to_string(proposal)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let before_json = serde_json::json!({
            "targetProjectId": proposal.target_project_id,
        })
        .to_string();
        let execution = execution_audit_for_job_on(
            &transaction,
            id,
            generation,
            Some(provider_id),
            Some(model),
            Some(finished_at_ms),
            Some(duration_ms),
            Some(0),
            None,
            "",
        )?;
        let review_id = format!("review-{id}-{generation}-project-draft");
        let evidence_count: i64 = cluster_episodes
            .iter()
            .try_fold(0_i64, |total, (_, keys)| {
                keys.iter().try_fold(total, |subtotal, episode_key| {
                    let count = transaction.query_row(
                        "SELECT COUNT(*) FROM work_episode_evidence WHERE episode_key=?1",
                        [episode_key],
                        |row| row.get::<_, i64>(0),
                    )?;
                    Ok::<_, rusqlite::Error>(subtotal.saturating_add(count))
                })
            })?;
        insert_generated_ai_review_on(
            &transaction,
            &AiReviewDraft {
                id: review_id.clone(),
                job_id: Some(id.into()),
                kind: AiReviewKind::ProjectDraft,
                subject_id: subject_id.into(),
                before_json,
                proposed_json,
                confidence: Some(proposal.confidence),
                evidence_summary: format!(
                    "{} candidate clusters, {} local evidence records",
                    cluster_episodes.len(),
                    evidence_count
                ),
                evidence_hash: subject_id.into(),
                execution,
                created_at_ms: finished_at_ms,
                execution_error: None,
            },
            true,
        )?;
        for (cluster_id, episode_keys) in cluster_episodes {
            let episode_key = episode_keys
                .first()
                .ok_or_else(|| invalid_review("A candidate cluster has no local episode"))?;
            transaction.execute(
                "INSERT INTO project_draft_bindings(
                    cluster_id, review_id, episode_key, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4)",
                params![cluster_id, review_id, episode_key, finished_at_ms],
            )?;
        }
        if auto_apply {
            validate_project_draft_proposal(&transaction, subject_id, proposal)?;
            if !apply_project_draft_proposal_on(&transaction, subject_id, proposal, false)? {
                return Err(invalid_review(
                    "Automatic workflow proposal could not be applied",
                ));
            }
            transaction.execute(
                "UPDATE ai_review_records
                 SET state='auto_applied', applied_json=proposed_json, resolved_at_ms=?2
                 WHERE id=?1 AND state='pending'",
                params![review_id, finished_at_ms],
            )?;
            append_ai_review_event_on(
                &transaction,
                &review_id,
                AiReviewEventKind::AutoApplied,
                finished_at_ms,
            )?;
            transaction.execute(
                "DELETE FROM project_draft_bindings WHERE review_id=?1",
                [&review_id],
            )?;
        }
        if !complete_ai_job_generation_on(&transaction, id, generation, audit, "", None)? {
            transaction.rollback()?;
            return Ok(false);
        }
        transaction.commit()?;
        Ok(true)
    }

    pub fn project_draft_metrics(&self, review_id: &str) -> Result<(usize, i64)> {
        let evidence_count = self.connection.query_row(
            "SELECT COUNT(*)
             FROM work_episode_evidence evidence
             JOIN work_episode_members episode
               ON episode.episode_key=evidence.episode_key
             WHERE episode.cluster_key IN (
                SELECT cluster_id FROM project_draft_bindings WHERE review_id=?1
             )",
            [review_id],
            |row| row.get::<_, i64>(0),
        )?;
        let accumulated_seconds = self.connection.query_row(
            "SELECT COALESCE(SUM(duration_seconds), 0)
             FROM work_episode_members
             WHERE cluster_key IN (
                SELECT cluster_id FROM project_draft_bindings WHERE review_id=?1
             )",
            [review_id],
            |row| row.get::<_, i64>(0),
        )?;
        Ok((
            usize::try_from(evidence_count.max(0)).unwrap_or(usize::MAX),
            accumulated_seconds.max(0),
        ))
    }

    pub fn fail_ai_job_generation(
        &self,
        id: &str,
        generation: i64,
        now_ms: i64,
        error: &str,
        error_kind: AiExecutionErrorKind,
        executor_id: Option<&str>,
        model: Option<&str>,
        exit_code: Option<i32>,
    ) -> Result<bool> {
        let transaction = ai_job_completion_transaction(&self.connection)?;
        let audit = AiJobCompletionAudit {
            finished_at_ms: now_ms,
            duration_ms: None,
            executor_id,
            model,
            exit_code,
        };
        if !ai_job_generation_is_current_or_retire_on(&transaction, id, generation, None, audit)? {
            transaction.commit()?;
            return Ok(false);
        }
        let attempts = match transaction
            .query_row(
                "SELECT attempts FROM ai_jobs current
             WHERE id=?1 AND generation=?2 AND status='running'
               AND NOT EXISTS(
                 SELECT 1 FROM ai_jobs newer
                 WHERE newer.kind=current.kind
                   AND newer.subject_key=current.subject_key
                   AND current.subject_key != ''
                   AND newer.generation > current.generation
               )",
                params![id, generation],
                |row| row.get::<_, u32>(0),
            )
            .optional()?
        {
            Some(attempts) => attempts,
            None => {
                retire_superseded_ai_job_generation_on(&transaction, id, generation, audit)?;
                transaction.commit()?;
                return Ok(false);
            }
        };
        let exponent = attempts.min(8);
        let delay_ms = 60_000_i64.saturating_mul(1_i64 << exponent);
        let sanitized = sanitize_ai_error(error);
        let retried = transaction.execute(
            "UPDATE ai_jobs
             SET status='pending',
                 attempts=attempts+1,
                 next_attempt_at_ms=?2,
                 last_error=?3,
                 finished_at_ms=?5,
                 duration_ms=MAX(0, ?5-COALESCE(started_at_ms, ?5)),
                 actual_executor_id=COALESCE(?6, actual_executor_id),
                 actual_model=COALESCE(?7, actual_model),
                 exit_code=COALESCE(?8, exit_code),
                 error_kind=?9
             WHERE id=?1 AND generation=?4 AND status='running'
               AND NOT EXISTS(
                 SELECT 1 FROM ai_jobs newer
                 WHERE newer.kind=ai_jobs.kind
                   AND newer.subject_key=ai_jobs.subject_key
                   AND ai_jobs.subject_key != ''
                   AND newer.generation > ai_jobs.generation
               )",
            params![
                id,
                now_ms.saturating_add(delay_ms),
                sanitized,
                generation,
                now_ms,
                executor_id,
                model,
                exit_code,
                error_kind.as_database()
            ],
        )? == 1;
        if retried {
            insert_execution_error_review_for_job_on(
                &transaction,
                id,
                generation,
                now_ms,
                &sanitized,
                error_kind,
                executor_id,
                model,
                exit_code,
                &format!("failure-{attempts}"),
            )?;
            transaction.commit()?;
            return Ok(true);
        }
        retire_superseded_ai_job_generation_on(&transaction, id, generation, audit)?;
        transaction.commit()?;
        Ok(false)
    }

    pub fn create_work_ledger_project(&self, project: NewProject) -> Result<Project> {
        self.connection.execute(
            "INSERT INTO projects(id, name, color, status, description, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, 'active', ?4, ?5, ?5)",
            params![project.id, project.name, project.color, project.description, project.created_at_ms],
        )?;
        self.get_work_ledger_project(&project.id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn get_work_ledger_project(&self, id: &str) -> Result<Option<Project>> {
        self.connection
            .query_row(
                "SELECT id, name, color, status, description, created_at_ms, updated_at_ms, archived_at_ms
                 FROM projects WHERE id=?1",
                [id],
                project_from_row,
            )
            .optional()
    }

    pub fn list_work_ledger_projects(&self, include_archived: bool) -> Result<Vec<Project>> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, color, status, description, created_at_ms, updated_at_ms, archived_at_ms
             FROM projects WHERE ?1 OR status != 'archived' ORDER BY updated_at_ms DESC, id ASC",
        )?;
        statement
            .query_map([include_archived], project_from_row)?
            .collect()
    }

    pub fn update_work_ledger_project(
        &self,
        id: &str,
        update: ProjectUpdate,
        updated_at_ms: i64,
    ) -> Result<bool> {
        Ok(self.connection.execute(
            "UPDATE projects SET name=COALESCE(?2, name), color=COALESCE(?3, color),
                 description=COALESCE(?4, description), updated_at_ms=?5 WHERE id=?1",
            params![
                id,
                update.name,
                update.color,
                update.description,
                updated_at_ms
            ],
        )? > 0)
    }

    pub fn archive_work_ledger_project(&self, id: &str, archived_at_ms: i64) -> Result<bool> {
        Ok(self.connection.execute(
            "UPDATE projects SET status='archived', archived_at_ms=?2, updated_at_ms=?2
             WHERE id=?1 AND status != 'archived'",
            params![id, archived_at_ms],
        )? > 0)
    }

    pub fn delete_work_ledger_project(&self, id: &str) -> Result<bool> {
        Ok(self
            .connection
            .execute("DELETE FROM projects WHERE id=?1", [id])?
            > 0)
    }

    pub fn create_work_ledger_task(&self, task: NewTask) -> Result<Task> {
        self.connection.execute(
            "INSERT OR IGNORE INTO tasks(
                id, project_id, title, status, priority, expected_output, due_date, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, ?3, 'todo', ?4, ?5, ?6, ?7, ?7)",
            params![
                task.id,
                task.project_id,
                task.title,
                task.priority.as_str(),
                task.expected_output,
                task.due_date,
                task.created_at_ms,
            ],
        )?;
        self.get_work_ledger_task(&task.id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn record_work_episode_cluster(
        &self,
        cluster_key: &str,
        signature_hash: &str,
        duration_seconds: i64,
        first_seen_at_ms: i64,
        last_seen_at_ms: i64,
    ) -> Result<(i64, i64, String, Option<String>)> {
        self.connection.execute(
            "INSERT INTO work_episode_clusters(
                cluster_key, signature_hash, accumulated_seconds, episode_count, status,
                created_task_id, first_seen_at_ms, last_seen_at_ms
             ) VALUES (?1, ?2, ?3, 1, 'collecting', NULL, ?4, ?5)
             ON CONFLICT(cluster_key) DO UPDATE SET
                accumulated_seconds=work_episode_clusters.accumulated_seconds + excluded.accumulated_seconds,
                episode_count=work_episode_clusters.episode_count + 1,
                last_seen_at_ms=excluded.last_seen_at_ms
             WHERE work_episode_clusters.status='collecting'
               AND excluded.last_seen_at_ms > work_episode_clusters.last_seen_at_ms",
            params![
                cluster_key,
                signature_hash,
                duration_seconds.max(0),
                first_seen_at_ms,
                last_seen_at_ms,
            ],
        )?;
        self.connection.query_row(
            "SELECT accumulated_seconds, episode_count, status, created_task_id
             FROM work_episode_clusters WHERE cluster_key=?1",
            [cluster_key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_work_episode_members(
        &self,
        episode_key: &str,
        cluster_key: &str,
        signature_hash: &str,
        duration_seconds: i64,
        started_at_ms: i64,
        ended_at_ms: i64,
        evidence: &[WorkLedgerEvidence],
    ) -> Result<bool> {
        let transaction = self.connection.unchecked_transaction()?;
        let inserted = transaction.execute(
            "INSERT OR IGNORE INTO work_episode_members(
                episode_key, cluster_key, signature_hash, duration_seconds,
                started_at_ms, ended_at_ms, created_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?5)",
            params![
                episode_key,
                cluster_key,
                signature_hash,
                duration_seconds.max(0),
                started_at_ms,
                ended_at_ms,
            ],
        )? == 1;
        if inserted {
            transaction.execute(
                "INSERT INTO work_episode_clusters(
                    cluster_key, signature_hash, accumulated_seconds, episode_count, status,
                    created_task_id, first_seen_at_ms, last_seen_at_ms
                 ) VALUES (?1, ?2, ?3, 1, 'collecting', NULL, ?4, ?5)
                 ON CONFLICT(cluster_key) DO UPDATE SET
                    accumulated_seconds=work_episode_clusters.accumulated_seconds
                        + excluded.accumulated_seconds,
                    episode_count=work_episode_clusters.episode_count + 1,
                    first_seen_at_ms=MIN(work_episode_clusters.first_seen_at_ms, excluded.first_seen_at_ms),
                    last_seen_at_ms=MAX(work_episode_clusters.last_seen_at_ms, excluded.last_seen_at_ms)
                 WHERE work_episode_clusters.status='collecting'",
                params![
                    cluster_key,
                    signature_hash,
                    duration_seconds.max(0),
                    started_at_ms,
                    ended_at_ms,
                ],
            )?;
        }
        for item in evidence {
            transaction.execute(
                "INSERT OR IGNORE INTO work_episode_evidence(
                    episode_key, evidence_kind, evidence_id, evidence_hash,
                    occurred_at_ms, duration_seconds
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    episode_key,
                    item.kind,
                    item.id,
                    item.evidence_hash,
                    item.occurred_at_ms,
                    item.duration_seconds.max(0),
                ],
            )?;
        }
        transaction.commit()?;
        Ok(inserted)
    }

    pub fn create_ai_provisional_work_ledger_task(
        &self,
        task_id: &str,
        project_id: Option<&str>,
        cluster_key: &str,
        title: &str,
        confidence: f64,
        created_at_ms: i64,
    ) -> Result<Task> {
        let transaction = self.connection.unchecked_transaction()?;
        let project_id = project_id.unwrap_or("ai-task-inbox");
        if project_id == "ai-task-inbox" {
            transaction.execute(
                "INSERT INTO projects(id, name, color, status, description, created_at_ms, updated_at_ms)
                 VALUES ('ai-task-inbox', 'AI 任务收集箱', '#7c3aed', 'active',
                         '保守门槛下由工作片段聚类创建的暂定任务', ?1, ?1)
                 ON CONFLICT(id) DO UPDATE SET
                    status='active', archived_at_ms=NULL, updated_at_ms=excluded.updated_at_ms",
                [created_at_ms],
            )?;
        }
        transaction.execute(
            "INSERT INTO tasks(
                id, project_id, title, status, priority, expected_output, due_date,
                created_at_ms, updated_at_ms, origin_kind, origin_key, origin_confidence,
                review_state
             ) VALUES (?1, ?2, ?3, 'todo', 'medium', '', NULL, ?4, ?4, 'ai', ?5, ?6, 'provisional')",
            params![task_id, project_id, title, created_at_ms, cluster_key, confidence],
        )?;
        let actual_task_id: String = transaction.query_row(
            "SELECT id FROM tasks WHERE origin_key=?1",
            [cluster_key],
            |row| row.get(0),
        )?;
        transaction.execute(
            "UPDATE work_episode_clusters
             SET status='created', created_task_id=?2, last_seen_at_ms=MAX(last_seen_at_ms, ?3)
             WHERE cluster_key=?1 AND status='collecting'",
            params![cluster_key, actual_task_id, created_at_ms],
        )?;
        transaction.commit()?;
        self.get_work_ledger_task(&actual_task_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn work_ledger_evidence_profile_text(
        &self,
        evidence_kind: &str,
        evidence_id: &str,
    ) -> Result<Option<String>> {
        match evidence_kind {
            "activity" => self
                .connection
                .query_row(
                    "SELECT app || ' ' || title FROM activity_segments WHERE id=?1",
                    [evidence_id],
                    |row| row.get(0),
                )
                .optional(),
            "browser" => self
                .connection
                .query_row(
                    "SELECT browser || ' ' || domain || ' ' || title
                     FROM browser_visits WHERE id=?1",
                    [evidence_id],
                    |row| row.get(0),
                )
                .optional(),
            _ => Ok(None),
        }
    }

    pub fn adjust_task_match_profile(
        &self,
        profile_key: &str,
        task_id: &str,
        delta: f64,
        updated_at_ms: i64,
    ) -> Result<()> {
        self.connection.execute(
            "INSERT INTO task_match_profiles(profile_key, task_id, weight, updated_at_ms)
             VALUES (?1, ?2, MAX(0, ?3), ?4)
             ON CONFLICT(profile_key, task_id) DO UPDATE SET
                weight=MAX(0, task_match_profiles.weight + ?3),
                updated_at_ms=excluded.updated_at_ms",
            params![profile_key, task_id, delta, updated_at_ms],
        )?;
        Ok(())
    }

    pub fn list_task_match_profile_keys(&self, task_id: &str) -> Result<Vec<String>> {
        let mut statement = self.connection.prepare(
            "SELECT profile_key FROM task_match_profiles
             WHERE task_id=?1 AND weight > 0 ORDER BY weight DESC, profile_key ASC",
        )?;
        statement.query_map([task_id], |row| row.get(0))?.collect()
    }

    pub fn get_work_ledger_task(&self, id: &str) -> Result<Option<Task>> {
        self.connection
            .query_row(
                "SELECT id, project_id, title, status, priority, expected_output, due_date,
                        created_at_ms, updated_at_ms, completed_at_ms, origin_kind, origin_key,
                        origin_confidence, review_state FROM tasks WHERE id=?1",
                [id],
                task_from_row,
            )
            .optional()
    }

    pub fn list_work_ledger_tasks(&self, project_id: &str) -> Result<Vec<Task>> {
        let mut statement = self.connection.prepare(
            "SELECT id, project_id, title, status, priority, expected_output, due_date,
                    created_at_ms, updated_at_ms, completed_at_ms, origin_kind, origin_key,
                    origin_confidence, review_state
             FROM tasks WHERE project_id=?1 ORDER BY created_at_ms ASC, id ASC",
        )?;
        statement.query_map([project_id], task_from_row)?.collect()
    }

    pub fn list_completed_task_timestamps(&self, start_ms: i64, end_ms: i64) -> Result<Vec<i64>> {
        let mut statement = self.connection.prepare(
            "SELECT completed_at_ms FROM tasks
             WHERE completed_at_ms IS NOT NULL AND completed_at_ms >= ?1 AND completed_at_ms < ?2
             ORDER BY completed_at_ms ASC, id ASC",
        )?;
        statement
            .query_map(params![start_ms, end_ms], |row| row.get(0))?
            .collect()
    }

    pub fn load_trend_range_facts(&self, start_ms: i64, end_ms: i64) -> Result<TrendRangeFacts> {
        if end_ms <= start_ms {
            return Err(rusqlite::Error::InvalidParameterName(
                "invalid trend facts range".into(),
            ));
        }
        Ok(TrendRangeFacts {
            segments: self.list_clipped_segments(start_ms, end_ms)?,
            completed_task_timestamps: self.list_completed_task_timestamps(start_ms, end_ms)?,
        })
    }

    pub fn load_work_ledger_range_facts(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<WorkLedgerRangeFacts> {
        if end_ms <= start_ms {
            return Err(rusqlite::Error::InvalidParameterName(
                "invalid work ledger facts range".into(),
            ));
        }

        let activities = {
            let mut statement = self.connection.prepare(
                "SELECT activity_segments.id, activity_segments.started_at_ms,
                        activity_segments.ended_at_ms, activity_segments.app,
                        activity_segments.app_path, activity_segments.title,
                        activity_segments.category, activity_segments.video_purpose,
                        activity_segments.confidence, activity_segments.classification_source,
                        activity_segments.reason, activity_segments.model_version,
                        activity_segments.needs_review,
                        tasks.id, tasks.title, tasks.status,
                        projects.id, projects.name, projects.status,
                        task_activity_links.provenance, task_activity_links.confidence,
                        task_activity_links.reason, task_activity_links.created_at_ms
                 FROM task_activity_links
                 JOIN activity_segments
                   ON activity_segments.id=task_activity_links.activity_segment_id
                 JOIN tasks ON tasks.id=task_activity_links.task_id
                 JOIN projects ON projects.id=tasks.project_id
                 WHERE activity_segments.started_at_ms < ?2
                   AND activity_segments.ended_at_ms > ?1
                 ORDER BY activity_segments.started_at_ms ASC,
                          activity_segments.id ASC, tasks.id ASC",
            )?;
            statement
                .query_map(params![start_ms, end_ms], |row| {
                    Ok(WorkLedgerActivityRangeFact {
                        segment: ActivitySegmentRecord {
                            id: row.get(0)?,
                            started_at_ms: row.get(1)?,
                            ended_at_ms: row.get(2)?,
                            app: row.get(3)?,
                            app_path: row.get(4)?,
                            title: row.get(5)?,
                            category: category_from_key(&row.get::<_, String>(6)?),
                            video_purpose: video_purpose_from_key(&row.get::<_, String>(7)?),
                            confidence: row.get(8)?,
                            source: source_from_key(&row.get::<_, String>(9)?),
                            reason: row.get(10)?,
                            model_version: row.get(11)?,
                            needs_review: row.get(12)?,
                            inactivity_reason: inferred_inactivity_reason(
                                &row.get::<_, String>(6)?,
                                &row.get::<_, String>(11)?,
                            ),
                        },
                        task_id: row.get(13)?,
                        task_title: row.get(14)?,
                        task_status: TaskStatus::from_str(&row.get::<_, String>(15)?),
                        project_id: row.get(16)?,
                        project_name: row.get(17)?,
                        project_status: ProjectStatus::from_str(&row.get::<_, String>(18)?),
                        provenance: EvidenceProvenance::from_str(&row.get::<_, String>(19)?),
                        assignment_confidence: row.get(20)?,
                        assignment_reason: row.get(21)?,
                        assigned_at_ms: row.get(22)?,
                    })
                })?
                .collect::<Result<Vec<_>>>()?
        };

        let focus_sessions = {
            let mut statement = self.connection.prepare(
                "SELECT focus_sessions.id, focus_sessions.started_at_ms,
                        COALESCE(focus_sessions.ended_at_ms, ?2),
                        tasks.id, tasks.title, tasks.status,
                        projects.id, projects.name, projects.status
                 FROM focus_sessions
                 JOIN tasks ON tasks.id=focus_sessions.task_id
                 JOIN projects ON projects.id=tasks.project_id
                 WHERE focus_sessions.started_at_ms < ?2
                   AND COALESCE(focus_sessions.ended_at_ms, ?2) > ?1
                 ORDER BY focus_sessions.started_at_ms ASC,
                          focus_sessions.id ASC, tasks.id ASC",
            )?;
            statement
                .query_map(params![start_ms, end_ms], |row| {
                    Ok(WorkLedgerFocusRangeFact {
                        session_id: row.get(0)?,
                        started_at_ms: row.get(1)?,
                        ended_at_ms: row.get(2)?,
                        task_id: row.get(3)?,
                        task_title: row.get(4)?,
                        task_status: TaskStatus::from_str(&row.get::<_, String>(5)?),
                        project_id: row.get(6)?,
                        project_name: row.get(7)?,
                        project_status: ProjectStatus::from_str(&row.get::<_, String>(8)?),
                    })
                })?
                .collect::<Result<Vec<_>>>()?
        };

        let completed_tasks = {
            let mut statement = self.connection.prepare(
                "SELECT tasks.id, tasks.title, projects.id, projects.name, tasks.completed_at_ms
                 FROM tasks
                 JOIN projects ON projects.id=tasks.project_id
                 WHERE tasks.completed_at_ms >= ?1 AND tasks.completed_at_ms < ?2
                 ORDER BY tasks.completed_at_ms ASC, tasks.id ASC",
            )?;
            statement
                .query_map(params![start_ms, end_ms], |row| {
                    Ok(WorkLedgerCompletedTaskRangeFact {
                        task_id: row.get(0)?,
                        task_title: row.get(1)?,
                        project_id: row.get(2)?,
                        project_name: row.get(3)?,
                        completed_at_ms: row.get(4)?,
                    })
                })?
                .collect::<Result<Vec<_>>>()?
        };

        Ok(WorkLedgerRangeFacts {
            activities,
            focus_sessions,
            completed_tasks,
        })
    }

    pub fn update_work_ledger_task(
        &self,
        id: &str,
        update: TaskUpdate,
        updated_at_ms: i64,
    ) -> Result<bool> {
        let update_due_date = update.due_date.is_some();
        let due_date: Option<String> = update.due_date.flatten();
        Ok(self.connection.execute(
            "UPDATE tasks SET project_id=COALESCE(?2, project_id),
                 title=COALESCE(?3, title), priority=COALESCE(?4, priority),
                 expected_output=COALESCE(?5, expected_output),
                 due_date=CASE WHEN ?6 THEN ?7 ELSE due_date END, updated_at_ms=?8 WHERE id=?1",
            params![
                id,
                update.project_id,
                update.title,
                update.priority.map(TaskPriority::as_str),
                update.expected_output,
                update_due_date,
                due_date,
                updated_at_ms,
            ],
        )? > 0)
    }

    pub fn transition_work_ledger_task_status(
        &self,
        id: &str,
        status: TaskStatus,
        updated_at_ms: i64,
    ) -> Result<Option<Task>> {
        let completed_at_ms = (status == TaskStatus::Completed).then_some(updated_at_ms);
        let changed = self.connection.execute(
            "UPDATE tasks SET status=?2, completed_at_ms=?3, updated_at_ms=?4 WHERE id=?1",
            params![id, status.as_str(), completed_at_ms, updated_at_ms],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        self.get_work_ledger_task(id)
    }

    pub fn cancel_work_ledger_ai_task(
        &self,
        task_id: &str,
        cancelled_at_ms: i64,
    ) -> Result<AiTaskCancellation> {
        let transaction = self.connection.unchecked_transaction()?;
        let task = transaction
            .query_row(
                "SELECT id, project_id, title, status, priority, expected_output, due_date,
                        created_at_ms, updated_at_ms, completed_at_ms, origin_kind, origin_key,
                        origin_confidence, review_state
                 FROM tasks WHERE id=?1",
                [task_id],
                task_from_row,
            )
            .optional()?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        if task.origin_kind != TaskOriginKind::Ai {
            return Err(rusqlite::Error::InvalidParameterName(
                "only AI-created tasks can be cancelled as recognition errors".into(),
            ));
        }
        if task.status == TaskStatus::Cancelled {
            let project_archived = transaction.query_row(
                "SELECT status='archived' FROM projects WHERE id=?1",
                [&task.project_id],
                |row| row.get(0),
            )?;
            transaction.commit()?;
            return Ok(AiTaskCancellation {
                task,
                released_evidence_count: 0,
                dismissed_cluster_count: 0,
                project_archived,
            });
        }

        let released_activity = transaction.execute(
            "DELETE FROM task_activity_links
             WHERE task_id=?1",
            [task_id],
        )? as i64;
        let released_browser = transaction.execute(
            "DELETE FROM task_browser_links
             WHERE task_id=?1",
            [task_id],
        )? as i64;
        let dismissed_cluster_count = transaction.execute(
            "UPDATE work_episode_clusters
             SET status='dismissed', created_task_id=NULL,
                 last_seen_at_ms=MAX(last_seen_at_ms, ?2)
             WHERE created_task_id=?1 AND status='created'",
            params![task_id, cancelled_at_ms],
        )? as i64;
        transaction.execute(
            "UPDATE tasks
             SET status='cancelled', completed_at_ms=NULL, updated_at_ms=?2
             WHERE id=?1",
            params![task_id, cancelled_at_ms],
        )?;

        let project_archived = transaction.execute(
            "UPDATE projects
             SET status='archived', archived_at_ms=?2, updated_at_ms=?2
             WHERE id=?1
               AND id LIKE 'project-ai-%'
               AND NOT EXISTS(
                    SELECT 1 FROM tasks
                    WHERE project_id=?1 AND status<>'cancelled'
               )",
            params![task.project_id, cancelled_at_ms],
        )? > 0;
        let cancelled_task = transaction.query_row(
            "SELECT id, project_id, title, status, priority, expected_output, due_date,
                    created_at_ms, updated_at_ms, completed_at_ms, origin_kind, origin_key,
                    origin_confidence, review_state
             FROM tasks WHERE id=?1",
            [task_id],
            task_from_row,
        )?;
        transaction.commit()?;
        Ok(AiTaskCancellation {
            task: cancelled_task,
            released_evidence_count: released_activity.saturating_add(released_browser),
            dismissed_cluster_count,
            project_archived,
        })
    }

    pub fn delete_work_ledger_task(&self, id: &str) -> Result<bool> {
        Ok(self
            .connection
            .execute("DELETE FROM tasks WHERE id=?1", [id])?
            > 0)
    }

    pub fn assign_work_ledger_activity(
        &self,
        task_id: &str,
        activity_segment_id: &str,
        provenance: EvidenceProvenance,
        confidence: f64,
        reason: &str,
        created_at_ms: i64,
    ) -> Result<bool> {
        if self.work_ledger_evidence_is_excluded("activity", activity_segment_id)? {
            return Ok(false);
        }
        self.assign_work_ledger_evidence(
            "task_activity_links",
            "activity_segment_id",
            task_id,
            activity_segment_id,
            provenance,
            confidence,
            reason,
            created_at_ms,
        )
    }

    pub fn remove_work_ledger_activity(
        &self,
        task_id: &str,
        activity_segment_id: &str,
    ) -> Result<bool> {
        self.remove_work_ledger_evidence(
            "task_activity_links",
            "activity_segment_id",
            task_id,
            activity_segment_id,
        )
    }

    pub fn list_work_ledger_activity_links(&self, task_id: &str) -> Result<Vec<EvidenceLink>> {
        self.list_work_ledger_evidence("task_activity_links", "activity_segment_id", task_id)
    }

    pub fn assign_work_ledger_browser_visit(
        &self,
        task_id: &str,
        browser_visit_id: &str,
        provenance: EvidenceProvenance,
        confidence: f64,
        reason: &str,
        created_at_ms: i64,
    ) -> Result<bool> {
        if self.work_ledger_evidence_is_excluded("browser", browser_visit_id)? {
            return Ok(false);
        }
        self.assign_work_ledger_evidence(
            "task_browser_links",
            "browser_visit_id",
            task_id,
            browser_visit_id,
            provenance,
            confidence,
            reason,
            created_at_ms,
        )
    }

    pub fn work_ledger_evidence_is_excluded(
        &self,
        evidence_kind: &str,
        evidence_id: &str,
    ) -> Result<bool> {
        match evidence_kind {
            "activity" => self
                .connection
                .query_row(
                    "SELECT category IN ('idle', 'game')
                            OR (category='video_input' AND video_purpose='leisure')
                     FROM activity_segments WHERE id=?1",
                    [evidence_id],
                    |row| row.get(0),
                )
                .optional()
                .map(|value| value.unwrap_or(true)),
            "browser" => self
                .connection
                .query_row(
                    "SELECT domain, title FROM browser_visits WHERE id=?1",
                    [evidence_id],
                    |row| {
                        Ok(browser_workflow_evidence_is_excluded(
                            &row.get::<_, String>(0)?,
                            &row.get::<_, String>(1)?,
                        ))
                    },
                )
                .optional()
                .map(|value| value.unwrap_or(true)),
            _ => Ok(true),
        }
    }

    pub fn cleanup_ineligible_workflow_links(&self, removed_at_ms: i64) -> Result<usize> {
        let transaction = self.connection.unchecked_transaction()?;
        let mut links = {
            let mut statement = transaction.prepare(
                "SELECT 'activity', links.task_id, links.activity_segment_id, links.provenance
                 FROM task_activity_links links
                 JOIN activity_segments segment ON segment.id=links.activity_segment_id
                 WHERE segment.category IN ('idle', 'game')
                    OR (segment.category='video_input' AND segment.video_purpose='leisure')",
            )?;
            statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })?
                .collect::<Result<Vec<_>>>()?
        };
        let browser_links = {
            let mut statement = transaction.prepare(
                "SELECT links.task_id, links.browser_visit_id, links.provenance,
                        visit.domain, visit.title
                 FROM task_browser_links links
                 JOIN browser_visits visit ON visit.id=links.browser_visit_id",
            )?;
            statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                })?
                .filter_map(|row| match row {
                    Ok((task_id, evidence_id, provenance, domain, title))
                        if browser_workflow_evidence_is_excluded(&domain, &title) =>
                    {
                        Some(Ok((
                            "browser".to_string(),
                            task_id,
                            evidence_id,
                            provenance,
                        )))
                    }
                    Ok(_) => None,
                    Err(error) => Some(Err(error)),
                })
                .collect::<Result<Vec<_>>>()?
        };
        links.extend(browser_links);
        for (kind, task_id, evidence_id, provenance) in &links {
            let audit_key = monitoring_gap_key(
                &format!("workflow-cleanup:{kind}:{task_id}:{evidence_id}"),
                removed_at_ms,
                removed_at_ms,
            );
            transaction.execute(
                "INSERT OR IGNORE INTO workflow_evidence_cleanup_audit(
                    id, evidence_kind, evidence_id, task_id, provenance, reason, removed_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'Excluded game, inactivity, or leisure evidence', ?6)",
                params![
                    format!("workflow-cleanup-{}", &audit_key[..24]),
                    kind,
                    evidence_id,
                    task_id,
                    provenance,
                    removed_at_ms,
                ],
            )?;
            let (table, column) = if kind == "activity" {
                ("task_activity_links", "activity_segment_id")
            } else {
                ("task_browser_links", "browser_visit_id")
            };
            transaction.execute(
                &format!("DELETE FROM {table} WHERE task_id=?1 AND {column}=?2"),
                params![task_id, evidence_id],
            )?;
        }
        transaction.commit()?;
        Ok(links.len())
    }

    pub fn remove_work_ledger_browser_visit(
        &self,
        task_id: &str,
        browser_visit_id: &str,
    ) -> Result<bool> {
        self.remove_work_ledger_evidence(
            "task_browser_links",
            "browser_visit_id",
            task_id,
            browser_visit_id,
        )
    }

    pub fn list_work_ledger_browser_links(&self, task_id: &str) -> Result<Vec<EvidenceLink>> {
        self.list_work_ledger_evidence("task_browser_links", "browser_visit_id", task_id)
    }

    pub fn list_work_ledger_ai_suggestions(&self) -> Result<Vec<AiWorkLedgerSuggestion>> {
        let mut statement = self.connection.prepare(
            "SELECT evidence_kind, evidence_id, evidence_hash, task_id, confidence, reason,
                    provider_id, model, created_at_ms
             FROM work_ledger_ai_suggestions
             ORDER BY created_at_ms ASC, evidence_kind ASC, evidence_id ASC",
        )?;
        statement
            .query_map([], |row| {
                Ok(AiWorkLedgerSuggestion {
                    evidence_kind: row.get(0)?,
                    evidence_id: row.get(1)?,
                    evidence_hash: row.get(2)?,
                    task_id: row.get(3)?,
                    confidence: row.get(4)?,
                    reason: row.get(5)?,
                    provider_id: row.get(6)?,
                    model: row.get(7)?,
                    created_at_ms: row.get(8)?,
                })
            })?
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn confirm_daily_goal_task(
        &self,
        goal_row_id: &str,
        goal_date: &str,
        goal_text: &str,
        task_id: &str,
        new_task: Option<NewTask>,
        confirmed_at_ms: i64,
    ) -> Result<ConfirmedDailyGoalTask> {
        let goal_row_id = goal_row_id.trim();
        let goal_text = goal_text.trim();
        if goal_row_id.is_empty()
            || goal_row_id
                .chars()
                .all(|character| character.is_ascii_digit())
            || goal_date.trim().is_empty()
            || goal_text.is_empty()
            || task_id.trim().is_empty()
        {
            return Err(rusqlite::Error::InvalidParameterName(
                "invalid stable daily goal row identity".into(),
            ));
        }

        let transaction = self.connection.unchecked_transaction()?;
        if let Some(link) = Self::daily_goal_task_link(&transaction, goal_row_id)? {
            let task = transaction.query_row(
                "SELECT id, project_id, title, status, priority, expected_output, due_date,
                        created_at_ms, updated_at_ms, completed_at_ms, origin_kind, origin_key,
                        origin_confidence, review_state
                 FROM tasks WHERE id=?1",
                [&link.task_id],
                task_from_row,
            )?;
            return Ok(ConfirmedDailyGoalTask {
                link,
                task,
                task_created: false,
            });
        }

        let goals = transaction
            .query_row(
                "SELECT goals FROM daily_goals WHERE date=?1",
                [goal_date],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if !goals.is_some_and(|goals| goals.lines().any(|line| line.trim() == goal_text)) {
            return Err(rusqlite::Error::InvalidParameterName(
                "confirmed daily goal row does not exist".into(),
            ));
        }

        let existing_task = transaction
            .query_row(
                "SELECT id, project_id, title, status, priority, expected_output, due_date,
                        created_at_ms, updated_at_ms, completed_at_ms, origin_kind, origin_key,
                        origin_confidence, review_state
                 FROM tasks WHERE id=?1",
                [task_id],
                task_from_row,
            )
            .optional()?;
        let (task, task_created) = if let Some(task) = existing_task {
            (task, false)
        } else {
            let task = new_task.ok_or_else(|| {
                rusqlite::Error::InvalidParameterName(
                    "new task details are required for an unknown task".into(),
                )
            })?;
            if task.id != task_id {
                return Err(rusqlite::Error::InvalidParameterName(
                    "new task identity does not match the confirmed task".into(),
                ));
            }
            transaction.execute(
                "INSERT INTO tasks(
                    id, project_id, title, status, priority, expected_output, due_date,
                    created_at_ms, updated_at_ms
                 ) VALUES (?1, ?2, ?3, 'todo', ?4, ?5, ?6, ?7, ?7)",
                params![
                    task.id,
                    task.project_id,
                    task.title,
                    task.priority.as_str(),
                    task.expected_output,
                    task.due_date,
                    task.created_at_ms,
                ],
            )?;
            let created = transaction.query_row(
                "SELECT id, project_id, title, status, priority, expected_output, due_date,
                        created_at_ms, updated_at_ms, completed_at_ms, origin_kind, origin_key,
                        origin_confidence, review_state
                 FROM tasks WHERE id=?1",
                [task_id],
                task_from_row,
            )?;
            (created, true)
        };
        transaction.execute(
            "INSERT INTO daily_goal_task_links(
                goal_row_id, goal_date, goal_text, task_id, confirmed_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![goal_row_id, goal_date, goal_text, task.id, confirmed_at_ms],
        )?;
        let link = Self::daily_goal_task_link(&transaction, goal_row_id)?
            .expect("daily goal task link was inserted");
        transaction.commit()?;
        Ok(ConfirmedDailyGoalTask {
            link,
            task,
            task_created,
        })
    }

    pub fn list_daily_goal_task_links(&self, goal_date: &str) -> Result<Vec<DailyGoalTaskLink>> {
        let mut statement = self.connection.prepare(
            "SELECT goal_row_id, goal_date, goal_text, task_id, confirmed_at_ms
             FROM daily_goal_task_links WHERE goal_date=?1
             ORDER BY confirmed_at_ms ASC, goal_row_id ASC",
        )?;
        statement
            .query_map([goal_date], daily_goal_task_link_from_row)?
            .collect()
    }

    fn daily_goal_task_link(
        connection: &Connection,
        goal_row_id: &str,
    ) -> Result<Option<DailyGoalTaskLink>> {
        connection
            .query_row(
                "SELECT goal_row_id, goal_date, goal_text, task_id, confirmed_at_ms
                 FROM daily_goal_task_links WHERE goal_row_id=?1",
                [goal_row_id],
                daily_goal_task_link_from_row,
            )
            .optional()
    }

    fn assign_work_ledger_evidence(
        &self,
        table: &str,
        evidence_column: &str,
        task_id: &str,
        evidence_id: &str,
        provenance: EvidenceProvenance,
        confidence: f64,
        reason: &str,
        created_at_ms: i64,
    ) -> Result<bool> {
        let transaction = self.connection.unchecked_transaction()?;
        if provenance == EvidenceProvenance::Manual {
            transaction.execute(
                &format!("DELETE FROM {table} WHERE {evidence_column}=?1 AND task_id<>?2"),
                params![evidence_id, task_id],
            )?;
            let changed = transaction.execute(
                &format!(
                    "INSERT INTO {table}(task_id, {evidence_column}, provenance, confidence, reason, created_at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT({evidence_column}) DO UPDATE SET
                        task_id=excluded.task_id, provenance=excluded.provenance,
                        confidence=excluded.confidence, reason=excluded.reason,
                        created_at_ms=excluded.created_at_ms"
                ),
                params![
                    task_id,
                    evidence_id,
                    provenance.as_str(),
                    confidence.clamp(0.0, 1.0),
                    reason,
                    created_at_ms
                ],
            )? > 0;
            transaction.commit()?;
            return Ok(changed);
        }
        let changed = transaction.execute(
            &format!(
                "INSERT INTO {table}(task_id, {evidence_column}, provenance, confidence, reason, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT({evidence_column}) DO UPDATE SET
                    provenance=excluded.provenance, confidence=excluded.confidence,
                    reason=excluded.reason, created_at_ms=excluded.created_at_ms
                 WHERE task_id=excluded.task_id
                   AND CASE excluded.provenance WHEN 'manual' THEN 3 WHEN 'rule' THEN 2 ELSE 1 END
                     > CASE provenance WHEN 'manual' THEN 3 WHEN 'rule' THEN 2 ELSE 1 END"
            ),
            params![
                task_id,
                evidence_id,
                provenance.as_str(),
                confidence.clamp(0.0, 1.0),
                reason,
                created_at_ms
            ],
        )? > 0;
        transaction.commit()?;
        Ok(changed)
    }

    fn remove_work_ledger_evidence(
        &self,
        table: &str,
        evidence_column: &str,
        task_id: &str,
        evidence_id: &str,
    ) -> Result<bool> {
        Ok(self.connection.execute(
            &format!("DELETE FROM {table} WHERE task_id=?1 AND {evidence_column}=?2"),
            params![task_id, evidence_id],
        )? > 0)
    }

    fn list_work_ledger_evidence(
        &self,
        table: &str,
        evidence_column: &str,
        task_id: &str,
    ) -> Result<Vec<EvidenceLink>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT task_id, {evidence_column}, provenance, confidence, reason, created_at_ms
             FROM {table} WHERE task_id=?1 ORDER BY created_at_ms ASC, {evidence_column} ASC"
        ))?;
        statement
            .query_map([task_id], evidence_link_from_row)?
            .collect()
    }

    pub fn create_work_ledger_progress_entry(
        &self,
        entry: NewProgressEntry,
    ) -> Result<ProgressEntry> {
        self.connection.execute(
            "INSERT INTO task_progress_entries(id, task_id, note, created_at_ms) VALUES (?1, ?2, ?3, ?4)",
            params![entry.id, entry.task_id, entry.note, entry.created_at_ms],
        )?;
        self.connection.query_row(
            "SELECT id, task_id, note, created_at_ms, origin_kind, source_id, source_date
             FROM task_progress_entries WHERE id=?1",
            [entry.id],
            progress_entry_from_row,
        )
    }

    pub fn list_work_ledger_progress_entries(&self, task_id: &str) -> Result<Vec<ProgressEntry>> {
        let mut statement = self.connection.prepare(
            "SELECT id, task_id, note, created_at_ms, origin_kind, source_id, source_date
             FROM task_progress_entries
             WHERE task_id=?1 ORDER BY created_at_ms ASC, id ASC",
        )?;
        statement
            .query_map([task_id], progress_entry_from_row)?
            .collect()
    }

    pub fn delete_work_ledger_progress_entry(&self, id: &str) -> Result<bool> {
        Ok(self
            .connection
            .execute("DELETE FROM task_progress_entries WHERE id=?1", [id])?
            > 0)
    }

    pub fn record_daily_actual_output_progress(
        &self,
        task_id: &str,
        date: &str,
        created_at_ms: i64,
    ) -> Result<Option<ProgressEntry>> {
        let transaction = self.connection.unchecked_transaction()?;
        let actual_output = transaction
            .query_row(
                "SELECT actual_output FROM daily_goals WHERE date=?1",
                [date],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(actual_output) = actual_output.filter(|output| !output.trim().is_empty()) else {
            return Ok(None);
        };
        let progress = Self::insert_provenance_progress(
            &transaction,
            task_id,
            &actual_output,
            created_at_ms,
            ProgressOriginKind::DailyActualOutput,
            date,
            Some(date),
        )?;
        transaction.commit()?;
        Ok(Some(progress))
    }

    fn insert_provenance_progress(
        connection: &Connection,
        task_id: &str,
        note: &str,
        created_at_ms: i64,
        origin_kind: ProgressOriginKind,
        source_id: &str,
        source_date: Option<&str>,
    ) -> Result<ProgressEntry> {
        let id = derived_progress_entry_id(origin_kind, task_id, source_id);
        connection.execute(
            "INSERT INTO task_progress_entries(
                id, task_id, note, created_at_ms, origin_kind, source_id, source_date
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(task_id, origin_kind, source_id) WHERE source_id IS NOT NULL
             DO NOTHING",
            params![
                id,
                task_id,
                note,
                created_at_ms,
                origin_kind.as_str(),
                source_id,
                source_date,
            ],
        )?;
        connection.query_row(
            "SELECT id, task_id, note, created_at_ms, origin_kind, source_id, source_date
             FROM task_progress_entries
             WHERE task_id=?1 AND origin_kind=?2 AND source_id=?3",
            params![task_id, origin_kind.as_str(), source_id],
            progress_entry_from_row,
        )
    }

    pub fn work_ledger_range_rollup(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<WorkLedgerRangeRollup> {
        let range_facts = self.load_work_ledger_range_facts(start_ms, end_ms)?;
        self.work_ledger_range_rollup_from_facts(start_ms, end_ms, &range_facts)
    }

    pub(crate) fn work_ledger_range_rollup_from_facts(
        &self,
        start_ms: i64,
        end_ms: i64,
        range_facts: &WorkLedgerRangeFacts,
    ) -> Result<WorkLedgerRangeRollup> {
        if end_ms <= start_ms {
            return Err(rusqlite::Error::InvalidParameterName(
                "invalid work ledger rollup range".into(),
            ));
        }

        let mut projects = BTreeMap::<String, ProjectRangeRollup>::new();
        {
            let mut statement = self
                .connection
                .prepare("SELECT id, name, status FROM projects ORDER BY id")?;
            let rows = statement.query_map([], |row| {
                Ok(ProjectRangeRollup {
                    project_id: row.get(0)?,
                    project_name: row.get(1)?,
                    project_status: ProjectStatus::from_str(&row.get::<_, String>(2)?),
                    invested_seconds: 0,
                    focus_seconds: 0,
                    activity_segment_count: 0,
                    browser_visit_count: 0,
                    progress_count: 0,
                    focus_session_count: 0,
                })
            })?;
            for row in rows {
                let rollup = row?;
                projects.insert(rollup.project_id.clone(), rollup);
            }
        }

        let mut tasks = BTreeMap::<String, TaskRangeRollup>::new();
        {
            let mut statement = self
                .connection
                .prepare("SELECT id, project_id, title, status FROM tasks ORDER BY id")?;
            let rows = statement.query_map([], |row| {
                Ok(TaskRangeRollup {
                    task_id: row.get(0)?,
                    project_id: row.get(1)?,
                    task_title: row.get(2)?,
                    task_status: TaskStatus::from_str(&row.get::<_, String>(3)?),
                    invested_seconds: 0,
                    focus_seconds: 0,
                    activity_segment_count: 0,
                    browser_visit_count: 0,
                    progress_count: 0,
                    focus_session_count: 0,
                })
            })?;
            for row in rows {
                let rollup = row?;
                tasks.insert(rollup.task_id.clone(), rollup);
            }
        }

        {
            let mut task_scopes = BTreeSet::new();
            let mut project_scopes = BTreeSet::new();
            let mut task_milliseconds = BTreeMap::<String, i64>::new();
            let mut project_milliseconds = BTreeMap::<String, i64>::new();
            for fact in &range_facts.activities {
                let milliseconds = fact
                    .segment
                    .ended_at_ms
                    .min(end_ms)
                    .saturating_sub(fact.segment.started_at_ms.max(start_ms))
                    .max(0);
                if task_scopes.insert((fact.task_id.clone(), fact.segment.id.clone())) {
                    *task_milliseconds.entry(fact.task_id.clone()).or_default() += milliseconds;
                }
                if project_scopes.insert((fact.project_id.clone(), fact.segment.id.clone())) {
                    *project_milliseconds
                        .entry(fact.project_id.clone())
                        .or_default() += milliseconds;
                }
            }
            for (task_id, milliseconds) in task_milliseconds {
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.invested_seconds = milliseconds / 1_000;
                    task.activity_segment_count = task_scopes
                        .iter()
                        .filter(|(scope_task_id, _)| scope_task_id == &task_id)
                        .count() as i64;
                }
            }
            for (project_id, milliseconds) in project_milliseconds {
                if let Some(project) = projects.get_mut(&project_id) {
                    project.invested_seconds = milliseconds / 1_000;
                    project.activity_segment_count = project_scopes
                        .iter()
                        .filter(|(scope_project_id, _)| scope_project_id == &project_id)
                        .count() as i64;
                }
            }
        }

        {
            let mut statement = self.connection.prepare(
                "SELECT task_browser_links.task_id, COUNT(DISTINCT browser_visit_id)
                 FROM task_browser_links
                 JOIN browser_visits ON browser_visits.id=task_browser_links.browser_visit_id
                 WHERE browser_visits.visited_at_ms >= ?1 AND browser_visits.visited_at_ms < ?2
                 GROUP BY task_browser_links.task_id",
            )?;
            let rows = statement.query_map(params![start_ms, end_ms], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (task_id, count) = row?;
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.browser_visit_count = count;
                }
            }
        }
        {
            let mut statement = self.connection.prepare(
                "SELECT tasks.project_id, COUNT(DISTINCT task_browser_links.browser_visit_id)
                 FROM tasks
                 JOIN task_browser_links ON task_browser_links.task_id=tasks.id
                 JOIN browser_visits ON browser_visits.id=task_browser_links.browser_visit_id
                 WHERE browser_visits.visited_at_ms >= ?1 AND browser_visits.visited_at_ms < ?2
                 GROUP BY tasks.project_id",
            )?;
            let rows = statement.query_map(params![start_ms, end_ms], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (project_id, count) = row?;
                if let Some(project) = projects.get_mut(&project_id) {
                    project.browser_visit_count = count;
                }
            }
        }

        {
            let mut statement = self.connection.prepare(
                "SELECT task_id, COUNT(*) FROM task_progress_entries
                 WHERE created_at_ms >= ?1 AND created_at_ms < ?2 GROUP BY task_id",
            )?;
            let rows = statement.query_map(params![start_ms, end_ms], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (task_id, count) = row?;
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.progress_count = count;
                }
            }
        }
        {
            let mut statement = self.connection.prepare(
                "SELECT tasks.project_id, COUNT(task_progress_entries.id)
                 FROM tasks
                 JOIN task_progress_entries ON task_progress_entries.task_id=tasks.id
                 WHERE task_progress_entries.created_at_ms >= ?1
                   AND task_progress_entries.created_at_ms < ?2
                 GROUP BY tasks.project_id",
            )?;
            let rows = statement.query_map(params![start_ms, end_ms], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (project_id, count) = row?;
                if let Some(project) = projects.get_mut(&project_id) {
                    project.progress_count = count;
                }
            }
        }

        {
            let mut task_scopes = BTreeSet::new();
            let mut project_scopes = BTreeSet::new();
            let mut task_milliseconds = BTreeMap::<String, i64>::new();
            let mut project_milliseconds = BTreeMap::<String, i64>::new();
            for fact in &range_facts.focus_sessions {
                let milliseconds = fact
                    .ended_at_ms
                    .min(end_ms)
                    .saturating_sub(fact.started_at_ms.max(start_ms))
                    .max(0);
                if task_scopes.insert((fact.task_id.clone(), fact.session_id.clone())) {
                    *task_milliseconds.entry(fact.task_id.clone()).or_default() += milliseconds;
                }
                if project_scopes.insert((fact.project_id.clone(), fact.session_id.clone())) {
                    *project_milliseconds
                        .entry(fact.project_id.clone())
                        .or_default() += milliseconds;
                }
            }
            for (task_id, milliseconds) in task_milliseconds {
                if let Some(task) = tasks.get_mut(&task_id) {
                    task.focus_seconds = milliseconds / 1_000;
                    task.focus_session_count = task_scopes
                        .iter()
                        .filter(|(scope_task_id, _)| scope_task_id == &task_id)
                        .count() as i64;
                }
            }
            for (project_id, milliseconds) in project_milliseconds {
                if let Some(project) = projects.get_mut(&project_id) {
                    project.focus_seconds = milliseconds / 1_000;
                    project.focus_session_count = project_scopes
                        .iter()
                        .filter(|(scope_project_id, _)| scope_project_id == &project_id)
                        .count() as i64;
                }
            }
        }

        Ok(WorkLedgerRangeRollup {
            start_ms,
            end_ms,
            projects: projects
                .into_values()
                .filter(ProjectRangeRollup::has_evidence)
                .collect(),
            tasks: tasks
                .into_values()
                .filter(TaskRangeRollup::has_evidence)
                .collect(),
        })
    }

    pub fn work_ledger_task_duration_seconds(&self, task_id: &str) -> Result<i64> {
        Ok(self
            .work_ledger_task_time_insight(task_id, i64::MAX)?
            .summary
            .lifecycle_total_seconds)
    }

    pub fn work_ledger_project_time_insight(
        &self,
        project_id: &str,
        end_ms: i64,
        range_days: i64,
    ) -> Result<ProjectTimeInsight> {
        self.cleanup_ineligible_workflow_links(end_ms)?;
        let project = self
            .get_work_ledger_project(project_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        let analysis_end_ms = project
            .archived_at_ms
            .map(|archived| archived.min(end_ms))
            .unwrap_or(end_ms);
        let tasks = self
            .list_work_ledger_tasks(project_id)?
            .into_iter()
            .filter(|task| task.status != TaskStatus::Cancelled)
            .collect::<Vec<_>>();
        let activity_owned_intervals = {
            let mut statement = self.connection.prepare(
                "SELECT tasks.id,
                        MAX(segment.started_at_ms, 0),
                        MIN(segment.ended_at_ms, ?2)
                 FROM tasks
                 JOIN task_activity_links links ON links.task_id=tasks.id
                 JOIN activity_segments segment ON segment.id=links.activity_segment_id
                 WHERE tasks.project_id=?1
                   AND tasks.status<>'cancelled'
                   AND segment.started_at_ms < ?2
                   AND segment.ended_at_ms > segment.started_at_ms
                 ORDER BY segment.started_at_ms, segment.id",
            )?;
            statement
                .query_map(params![project_id, analysis_end_ms], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        TimeInterval {
                            start_ms: row.get(1)?,
                            end_ms: row.get(2)?,
                        },
                    ))
                })?
                .filter_map(|row| match row {
                    Ok((task_id, interval)) if interval.end_ms > interval.start_ms => {
                        Some(Ok((task_id, interval)))
                    }
                    Ok(_) => None,
                    Err(error) => Some(Err(error)),
                })
                .collect::<Result<Vec<_>>>()?
        };
        let focus_owned_intervals = {
            let mut statement = self.connection.prepare(
                "SELECT tasks.id, focus.started_at_ms,
                        MIN(COALESCE(focus.ended_at_ms, ?2), ?2)
                 FROM tasks
                 JOIN focus_sessions focus ON focus.task_id=tasks.id
                 WHERE tasks.project_id=?1
                   AND tasks.status<>'cancelled'
                   AND focus.started_at_ms < ?2
                 ORDER BY focus.started_at_ms, focus.id",
            )?;
            statement
                .query_map(params![project_id, analysis_end_ms], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        TimeInterval {
                            start_ms: row.get(1)?,
                            end_ms: row.get(2)?,
                        },
                    ))
                })?
                .filter_map(|row| match row {
                    Ok((task_id, interval)) if interval.end_ms > interval.start_ms => {
                        Some(Ok((task_id, interval)))
                    }
                    Ok(_) => None,
                    Err(error) => Some(Err(error)),
                })
                .collect::<Result<Vec<_>>>()?
        };
        let activity_intervals = activity_owned_intervals
            .iter()
            .map(|(_, interval)| *interval)
            .collect::<Vec<_>>();
        let focus_intervals = focus_owned_intervals
            .iter()
            .map(|(_, interval)| *interval)
            .collect::<Vec<_>>();
        let mut task_owned_intervals = BTreeMap::<String, Vec<TimeInterval>>::new();
        for (task_id, interval) in activity_owned_intervals
            .iter()
            .chain(focus_owned_intervals.iter())
        {
            task_owned_intervals
                .entry(task_id.clone())
                .or_default()
                .push(*interval);
        }
        let (exclusive_task_ms, shared_ms) =
            exclusive_and_shared_interval_ms(&task_owned_intervals);
        let mut all_intervals = activity_intervals.clone();
        all_intervals.extend(focus_intervals.iter().copied());
        let merged = merge_intervals(&all_intervals, 0);
        let lifecycle_total_seconds = interval_duration_ms(&merged) / 1_000;
        let focus_seconds = interval_duration_ms(&merge_intervals(&focus_intervals, 0)) / 1_000;
        let longest_continuous_seconds = merge_intervals(&all_intervals, 10 * 60 * 1_000)
            .iter()
            .map(|interval| interval.end_ms.saturating_sub(interval.start_ms) / 1_000)
            .max()
            .unwrap_or_default();
        let mut applications = {
            let mut statement = self.connection.prepare(
                "SELECT DISTINCT segment.id, segment.started_at_ms,
                        MIN(segment.ended_at_ms, ?2), segment.app
                 FROM tasks
                 JOIN task_activity_links links ON links.task_id=tasks.id
                 JOIN activity_segments segment ON segment.id=links.activity_segment_id
                 WHERE tasks.project_id=?1
                   AND tasks.status<>'cancelled'
                   AND segment.started_at_ms < ?2
                   AND segment.ended_at_ms > segment.started_at_ms
                 GROUP BY segment.id, segment.started_at_ms, segment.app
                 ORDER BY segment.started_at_ms, segment.id",
            )?;
            statement
                .query_map(params![project_id, analysis_end_ms], |row| {
                    Ok((
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })?
                .collect::<Result<Vec<_>>>()?
        };
        applications.sort_by_key(|(start_ms, _, _)| *start_ms);
        let switch_count = applications
            .windows(2)
            .filter(|pair| {
                pair[0].2 != pair[1].2 && pair[1].0.saturating_sub(pair[0].1) <= 10 * 60 * 1_000
            })
            .count() as i64;
        let switches_per_hour = if lifecycle_total_seconds == 0 {
            0.0
        } else {
            switch_count as f64 / (lifecycle_total_seconds as f64 / 3_600.0)
        };

        let mut lifecycle_daily = BTreeMap::<String, DailyIntervalGroups>::new();
        add_intervals_to_days(
            &mut lifecycle_daily,
            &all_intervals,
            DailyIntervalKind::Invested,
        );
        add_intervals_to_days(
            &mut lifecycle_daily,
            &activity_intervals,
            DailyIntervalKind::Activity,
        );
        add_intervals_to_days(
            &mut lifecycle_daily,
            &focus_intervals,
            DailyIntervalKind::Focus,
        );
        let active_day_count = lifecycle_daily
            .values()
            .filter(|groups| interval_duration_ms(&merge_intervals(&groups.invested, 0)) > 0)
            .count() as i64;
        let first_activity_ms = merged.first().map(|interval| interval.start_ms);
        let natural_day_count = first_activity_ms
            .map(|first| inclusive_local_day_count(first, analysis_end_ms.saturating_sub(1)))
            .unwrap_or_default()
            .max(active_day_count);
        let active_day_average_seconds = if active_day_count == 0 {
            0
        } else {
            lifecycle_total_seconds / active_day_count
        };
        let natural_day_average_seconds = if natural_day_count == 0 {
            0
        } else {
            lifecycle_total_seconds / natural_day_count
        };

        let mut task_contributions = Vec::new();
        let mut task_daily = BTreeMap::<String, BTreeMap<String, i64>>::new();
        for task in &tasks {
            let insight = self.work_ledger_task_time_insight(&task.id, analysis_end_ms)?;
            for point in insight.daily_points {
                task_daily
                    .entry(point.date)
                    .or_default()
                    .insert(task.id.clone(), point.invested_seconds);
            }
            let evidence_count = self.connection.query_row(
                "SELECT
                    (SELECT COUNT(*) FROM task_activity_links WHERE task_id=?1)
                    + (SELECT COUNT(*) FROM task_browser_links WHERE task_id=?1)",
                [&task.id],
                |row| row.get(0),
            )?;
            task_contributions.push(ProjectTaskContribution {
                task_id: task.id.clone(),
                task_title: task.title.clone(),
                status: task.status,
                invested_seconds: exclusive_task_ms.get(&task.id).copied().unwrap_or_default()
                    / 1_000,
                evidence_count,
            });
        }
        task_contributions.sort_by(|left, right| {
            right
                .invested_seconds
                .cmp(&left.invested_seconds)
                .then_with(|| left.task_id.cmp(&right.task_id))
        });
        let shared_seconds = shared_ms / 1_000;

        let chart_end_date = Local
            .timestamp_millis_opt(analysis_end_ms.saturating_sub(1))
            .earliest()
            .map(|value| value.date_naive())
            .unwrap_or_else(|| NaiveDate::from_ymd_opt(1970, 1, 1).unwrap());
        let chart_start_date = if range_days > 0 {
            chart_end_date
                .checked_sub_signed(chrono::Duration::days(range_days.saturating_sub(1)))
                .unwrap_or(chart_end_date)
        } else {
            lifecycle_daily
                .keys()
                .next()
                .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
                .unwrap_or(chart_end_date)
        };
        let mut daily_points = Vec::new();
        let mut date = chart_start_date;
        while date <= chart_end_date {
            let date_key = date.format("%Y-%m-%d").to_string();
            let groups = lifecycle_daily.get(&date_key);
            let invested_seconds = groups
                .map(|groups| interval_duration_ms(&merge_intervals(&groups.invested, 0)) / 1_000)
                .unwrap_or_default();
            let activity_seconds = groups
                .map(|groups| interval_duration_ms(&merge_intervals(&groups.activity, 0)) / 1_000)
                .unwrap_or_default();
            let focus_seconds = groups
                .map(|groups| interval_duration_ms(&merge_intervals(&groups.focus, 0)) / 1_000)
                .unwrap_or_default();
            let task_seconds = task_daily.remove(&date_key).unwrap_or_default();
            let raw_daily_total = task_seconds.values().copied().sum::<i64>();
            daily_points.push(ProjectDailyPoint {
                date: date_key,
                invested_seconds,
                activity_seconds,
                focus_seconds,
                task_seconds,
                shared_seconds: raw_daily_total.saturating_sub(invested_seconds).max(0),
            });
            let Some(next) = date.succ_opt() else {
                break;
            };
            date = next;
        }

        let (activity_evidence_count, browser_evidence_count) = self.connection.query_row(
            "SELECT
                (SELECT COUNT(DISTINCT links.activity_segment_id)
                 FROM tasks JOIN task_activity_links links ON links.task_id=tasks.id
                 WHERE tasks.project_id=?1 AND tasks.status<>'cancelled'),
                (SELECT COUNT(DISTINCT links.browser_visit_id)
                 FROM tasks JOIN task_browser_links links ON links.task_id=tasks.id
                 WHERE tasks.project_id=?1 AND tasks.status<>'cancelled')",
            [project_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )?;
        let completed_task_count = tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Completed)
            .count() as i64;
        Ok(ProjectTimeInsight {
            summary: ProjectTimeSummary {
                project_id: project.id,
                lifecycle_total_seconds,
                active_day_average_seconds,
                natural_day_average_seconds,
                active_day_count,
                natural_day_count,
                evidence_count: activity_evidence_count.saturating_add(browser_evidence_count),
                completed_task_count,
                task_count: tasks.len() as i64,
                latest_activity_at_ms: all_intervals.iter().map(|interval| interval.end_ms).max(),
            },
            daily_points,
            task_contributions,
            shared_seconds,
            focus_seconds,
            longest_continuous_seconds,
            switch_count,
            switches_per_hour,
            browser_evidence_count,
        })
    }

    pub fn merge_work_ledger_tasks(
        &self,
        source_task_id: &str,
        target_task_id: &str,
        updated_at_ms: i64,
    ) -> Result<bool> {
        if source_task_id == target_task_id {
            return Ok(false);
        }
        let transaction = self.connection.unchecked_transaction()?;
        let source_exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
            [source_task_id],
            |row| row.get(0),
        )?;
        let target_exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
            [target_task_id],
            |row| row.get(0),
        )?;
        if !source_exists || !target_exists {
            return Ok(false);
        }

        for (table, evidence_column) in [
            ("task_activity_links", "activity_segment_id"),
            ("task_browser_links", "browser_visit_id"),
        ] {
            transaction.execute(
                &format!(
                    "DELETE FROM {table}
                     WHERE task_id=?1 AND {evidence_column} IN (
                        SELECT {evidence_column} FROM {table} WHERE task_id=?2
                     )"
                ),
                params![source_task_id, target_task_id],
            )?;
            transaction.execute(
                &format!("UPDATE {table} SET task_id=?2 WHERE task_id=?1"),
                params![source_task_id, target_task_id],
            )?;
        }
        transaction.execute(
            "DELETE FROM task_progress_entries
             WHERE task_id=?1 AND source_id IS NOT NULL AND EXISTS(
                SELECT 1 FROM task_progress_entries target
                WHERE target.task_id=?2
                  AND target.origin_kind=task_progress_entries.origin_kind
                  AND target.source_id=task_progress_entries.source_id
             )",
            params![source_task_id, target_task_id],
        )?;
        transaction.execute(
            "UPDATE task_progress_entries SET task_id=?2 WHERE task_id=?1",
            params![source_task_id, target_task_id],
        )?;
        transaction.execute(
            "UPDATE focus_sessions SET task_id=?2 WHERE task_id=?1",
            params![source_task_id, target_task_id],
        )?;
        transaction.execute(
            "UPDATE daily_goal_task_links SET task_id=?2 WHERE task_id=?1",
            params![source_task_id, target_task_id],
        )?;
        transaction.execute(
            "UPDATE work_ledger_ai_suggestions SET task_id=?2 WHERE task_id=?1",
            params![source_task_id, target_task_id],
        )?;
        transaction.execute(
            "UPDATE work_episode_clusters
             SET created_task_id=?2, status='merged', last_seen_at_ms=MAX(last_seen_at_ms, ?3)
             WHERE created_task_id=?1",
            params![source_task_id, target_task_id, updated_at_ms],
        )?;
        transaction.execute(
            "INSERT INTO task_match_profiles(profile_key, task_id, weight, updated_at_ms)
             SELECT profile_key, ?2, weight, ?3 FROM task_match_profiles WHERE task_id=?1
             ON CONFLICT(profile_key, task_id) DO UPDATE SET
                weight=MAX(task_match_profiles.weight, excluded.weight),
                updated_at_ms=excluded.updated_at_ms",
            params![source_task_id, target_task_id, updated_at_ms],
        )?;
        transaction.execute(
            "DELETE FROM task_match_profiles WHERE task_id=?1",
            [source_task_id],
        )?;
        transaction.execute(
            "UPDATE tasks SET review_state='confirmed', updated_at_ms=?2 WHERE id=?1",
            params![target_task_id, updated_at_ms],
        )?;
        let deleted = transaction.execute("DELETE FROM tasks WHERE id=?1", [source_task_id])? > 0;
        transaction.commit()?;
        Ok(deleted)
    }

    pub fn work_ledger_task_time_insight(
        &self,
        task_id: &str,
        end_ms: i64,
    ) -> Result<TaskTimeInsight> {
        let task = self
            .get_work_ledger_task(task_id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        let mut activity_intervals = Vec::new();
        let mut applications = Vec::new();
        {
            let mut statement = self.connection.prepare(
                "SELECT activity_segments.started_at_ms,
                        MIN(activity_segments.ended_at_ms, ?2),
                        activity_segments.app
                 FROM task_activity_links
                 JOIN activity_segments
                   ON activity_segments.id=task_activity_links.activity_segment_id
                 WHERE task_activity_links.task_id=?1
                   AND activity_segments.started_at_ms < ?2
                   AND activity_segments.ended_at_ms > activity_segments.started_at_ms
                 ORDER BY activity_segments.started_at_ms, activity_segments.id",
            )?;
            let rows = statement.query_map(params![task_id, end_ms], |row| {
                Ok((
                    TimeInterval {
                        start_ms: row.get(0)?,
                        end_ms: row.get(1)?,
                    },
                    row.get::<_, String>(2)?,
                ))
            })?;
            for row in rows {
                let (interval, app) = row?;
                if interval.end_ms > interval.start_ms {
                    activity_intervals.push(interval);
                    applications.push((interval.start_ms, interval.end_ms, app));
                }
            }
        }
        let focus_intervals = {
            let mut statement = self.connection.prepare(
                "SELECT started_at_ms, MIN(COALESCE(ended_at_ms, ?2), ?2)
                 FROM focus_sessions
                 WHERE task_id=?1 AND started_at_ms < ?2
                 ORDER BY started_at_ms, id",
            )?;
            statement
                .query_map(params![task_id, end_ms], |row| {
                    Ok(TimeInterval {
                        start_ms: row.get(0)?,
                        end_ms: row.get(1)?,
                    })
                })?
                .filter_map(|row| match row {
                    Ok(interval) if interval.end_ms > interval.start_ms => Some(Ok(interval)),
                    Ok(_) => None,
                    Err(error) => Some(Err(error)),
                })
                .collect::<Result<Vec<_>>>()?
        };

        let mut all_intervals = activity_intervals.clone();
        all_intervals.extend(focus_intervals.iter().copied());
        let merged = merge_intervals(&all_intervals, 0);
        let lifecycle_total_seconds = interval_duration_ms(&merged) / 1_000;
        let focus_seconds = interval_duration_ms(&merge_intervals(&focus_intervals, 0)) / 1_000;
        let longest_continuous_seconds = merge_intervals(&all_intervals, 10 * 60 * 1_000)
            .iter()
            .map(|interval| interval.end_ms.saturating_sub(interval.start_ms) / 1_000)
            .max()
            .unwrap_or_default();

        let mut daily = BTreeMap::<String, DailyIntervalGroups>::new();
        add_intervals_to_days(&mut daily, &all_intervals, DailyIntervalKind::Invested);
        add_intervals_to_days(&mut daily, &activity_intervals, DailyIntervalKind::Activity);
        add_intervals_to_days(&mut daily, &focus_intervals, DailyIntervalKind::Focus);
        applications.sort_by_key(|(start_ms, _, _)| *start_ms);
        let mut switch_count = 0_i64;
        for pair in applications.windows(2) {
            let previous = &pair[0];
            let current = &pair[1];
            if previous.2 != current.2 && current.0.saturating_sub(previous.1) <= 10 * 60 * 1_000 {
                switch_count += 1;
                let date = local_date_key(current.0);
                daily.entry(date).or_default().switch_count += 1;
            }
        }
        let daily_points = daily
            .into_iter()
            .map(|(date, groups)| TaskDailyPoint {
                date,
                invested_seconds: interval_duration_ms(&merge_intervals(&groups.invested, 0))
                    / 1_000,
                activity_seconds: interval_duration_ms(&merge_intervals(&groups.activity, 0))
                    / 1_000,
                focus_seconds: interval_duration_ms(&merge_intervals(&groups.focus, 0)) / 1_000,
                switch_count: groups.switch_count,
            })
            .filter(|point| point.invested_seconds > 0)
            .collect::<Vec<_>>();
        let active_day_count = daily_points.len() as i64;
        let active_day_average_seconds = if active_day_count == 0 {
            0
        } else {
            lifecycle_total_seconds / active_day_count
        };
        let first_activity_ms = merged.first().map(|interval| interval.start_ms);
        let natural_day_count = first_activity_ms
            .map(|first| inclusive_local_day_count(first, end_ms.saturating_sub(1)))
            .unwrap_or_default()
            .max(active_day_count);
        let natural_day_average_seconds = if natural_day_count == 0 {
            0
        } else {
            lifecycle_total_seconds / natural_day_count
        };
        let latest_activity_at_ms = all_intervals.iter().map(|interval| interval.end_ms).max();

        let (assignment_confidence, manual_correction_rate) = self.connection.query_row(
            "SELECT AVG(confidence),
                    CASE WHEN COUNT(*)=0 THEN 0.0
                         ELSE CAST(SUM(CASE WHEN provenance='manual' THEN 1 ELSE 0 END) AS REAL)
                              / COUNT(*) END
             FROM (
                SELECT confidence, provenance FROM task_activity_links WHERE task_id=?1
                UNION ALL
                SELECT confidence, provenance FROM task_browser_links WHERE task_id=?1
             )",
            [task_id],
            |row| Ok((row.get::<_, Option<f64>>(0)?, row.get::<_, f64>(1)?)),
        )?;
        let browser_evidence_count = self.connection.query_row(
            "SELECT COUNT(*) FROM task_browser_links WHERE task_id=?1",
            [task_id],
            |row| row.get(0),
        )?;
        let progress_count = self.connection.query_row(
            "SELECT COUNT(*) FROM task_progress_entries WHERE task_id=?1",
            [task_id],
            |row| row.get(0),
        )?;
        let pending_review_seconds = self.connection.query_row(
            "SELECT COALESCE(SUM(MAX(0, activity_segments.ended_at_ms-activity_segments.started_at_ms))/1000, 0)
             FROM work_ledger_ai_suggestions
             JOIN activity_segments
               ON work_ledger_ai_suggestions.evidence_kind='activity'
              AND activity_segments.id=work_ledger_ai_suggestions.evidence_id
             WHERE work_ledger_ai_suggestions.task_id=?1",
            [task_id],
            |row| row.get(0),
        )?;
        let mut active_values = daily_points
            .iter()
            .map(|point| point.invested_seconds)
            .collect::<Vec<_>>();
        active_values.sort_unstable();
        let median_daily_seconds = median_i64(&active_values);
        let regularity = regularity_score(&active_values);
        let focus_share = if lifecycle_total_seconds == 0 {
            0.0
        } else {
            (focus_seconds as f64 / lifecycle_total_seconds as f64).clamp(0.0, 1.0)
        };
        let switches_per_hour = if lifecycle_total_seconds == 0 {
            0.0
        } else {
            switch_count as f64 / (lifecycle_total_seconds as f64 / 3_600.0)
        };
        let summary = TaskTimeSummary {
            task_id: task.id.clone(),
            lifecycle_total_seconds,
            active_day_average_seconds,
            natural_day_average_seconds,
            active_day_count,
            natural_day_count,
            latest_activity_at_ms,
            assignment_confidence,
            review_state: task.review_state,
        };
        let mut data_limitations = Vec::new();
        if active_day_count < 3 {
            data_limitations.push("活跃日少于 3 天，规律性结论仅供参考".into());
        }
        if assignment_confidence.is_none() {
            data_limitations.push("尚无可用于评估归属可信度的证据".into());
        }
        let assessment = TaskEfficiencyAssessment {
            dimensions: vec![
                TaskEfficiencyDimension {
                    key: "time_investment".into(),
                    label: "时间投入".into(),
                    conclusion: format!(
                        "累计 {} 分钟；活跃日均 {} 分钟（{} 天）",
                        lifecycle_total_seconds / 60,
                        active_day_average_seconds / 60,
                        active_day_count
                    ),
                    value: Some(lifecycle_total_seconds as f64),
                    unit: "seconds".into(),
                },
                TaskEfficiencyDimension {
                    key: "continuity".into(),
                    label: "连续性".into(),
                    conclusion: format!("最长连续片段 {} 分钟", longest_continuous_seconds / 60),
                    value: Some(longest_continuous_seconds as f64),
                    unit: "seconds".into(),
                },
                TaskEfficiencyDimension {
                    key: "switching_cost".into(),
                    label: "切换成本".into(),
                    conclusion: format!("每小时约 {:.1} 次应用切换", switches_per_hour),
                    value: Some(switches_per_hour),
                    unit: "switches_per_hour".into(),
                },
                TaskEfficiencyDimension {
                    key: "regularity".into(),
                    label: "规律性".into(),
                    conclusion: format!("投入规律性 {:.0}%", regularity * 100.0),
                    value: Some(regularity),
                    unit: "ratio".into(),
                },
                TaskEfficiencyDimension {
                    key: "evidence_confidence".into(),
                    label: "证据可信度".into(),
                    conclusion: assignment_confidence
                        .map(|value| format!("平均归属可信度 {:.0}%", value * 100.0))
                        .unwrap_or_else(|| "暂无可计算的归属可信度".into()),
                    value: assignment_confidence,
                    unit: "ratio".into(),
                },
            ],
            data_limitations,
        };
        Ok(TaskTimeInsight {
            summary,
            daily_points,
            median_daily_seconds,
            longest_continuous_seconds,
            focus_seconds,
            focus_share,
            switches_per_hour,
            regularity,
            manual_correction_rate,
            pending_review_seconds,
            browser_evidence_count,
            progress_count,
            expected_output: task.expected_output,
            assessment,
        })
    }

    pub fn work_ledger_project_duration_seconds(&self, project_id: &str) -> Result<i64> {
        self.connection.query_row(
            "SELECT COALESCE(SUM(MAX(0, activity_segments.ended_at_ms - activity_segments.started_at_ms)) / 1000, 0)
             FROM activity_segments
             JOIN (
                SELECT DISTINCT task_activity_links.activity_segment_id
                FROM tasks
                JOIN task_activity_links ON task_activity_links.task_id = tasks.id
                WHERE tasks.project_id=?1
             ) AS project_segments
                ON project_segments.activity_segment_id = activity_segments.id",
            [project_id],
            |row| row.get(0),
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct TimeInterval {
    start_ms: i64,
    end_ms: i64,
}

#[derive(Default)]
struct DailyIntervalGroups {
    invested: Vec<TimeInterval>,
    activity: Vec<TimeInterval>,
    focus: Vec<TimeInterval>,
    switch_count: i64,
}

#[derive(Clone, Copy)]
enum DailyIntervalKind {
    Invested,
    Activity,
    Focus,
}

fn merge_intervals(intervals: &[TimeInterval], allowed_gap_ms: i64) -> Vec<TimeInterval> {
    let mut sorted = intervals
        .iter()
        .copied()
        .filter(|interval| interval.end_ms > interval.start_ms)
        .collect::<Vec<_>>();
    sorted.sort_by_key(|interval| (interval.start_ms, interval.end_ms));
    let mut merged: Vec<TimeInterval> = Vec::new();
    for interval in sorted {
        if let Some(last) = merged.last_mut()
            && interval.start_ms <= last.end_ms.saturating_add(allowed_gap_ms)
        {
            last.end_ms = last.end_ms.max(interval.end_ms);
            continue;
        }
        merged.push(interval);
    }
    merged
}

fn interval_duration_ms(intervals: &[TimeInterval]) -> i64 {
    intervals
        .iter()
        .map(|interval| interval.end_ms.saturating_sub(interval.start_ms).max(0))
        .sum()
}

fn exclusive_and_shared_interval_ms(
    task_intervals: &BTreeMap<String, Vec<TimeInterval>>,
) -> (BTreeMap<String, i64>, i64) {
    let mut events = BTreeMap::<i64, Vec<(String, bool)>>::new();
    for (task_id, intervals) in task_intervals {
        for interval in merge_intervals(intervals, 0) {
            events
                .entry(interval.start_ms)
                .or_default()
                .push((task_id.clone(), true));
            events
                .entry(interval.end_ms)
                .or_default()
                .push((task_id.clone(), false));
        }
    }
    let mut active = BTreeSet::<String>::new();
    let mut exclusive = BTreeMap::<String, i64>::new();
    let mut shared_ms = 0_i64;
    let mut previous_ms = None;
    for (timestamp_ms, changes) in events {
        if let Some(previous_ms) = previous_ms {
            let duration_ms = timestamp_ms.saturating_sub(previous_ms);
            if duration_ms > 0 {
                if active.len() == 1 {
                    let task_id = active
                        .first()
                        .expect("one active task has a first member")
                        .clone();
                    *exclusive.entry(task_id).or_default() += duration_ms;
                } else if active.len() > 1 {
                    shared_ms = shared_ms.saturating_add(duration_ms);
                }
            }
        }
        for (task_id, starts) in changes {
            if starts {
                active.insert(task_id);
            } else {
                active.remove(&task_id);
            }
        }
        previous_ms = Some(timestamp_ms);
    }
    (exclusive, shared_ms)
}

fn local_date_key(timestamp_ms: i64) -> String {
    Local
        .timestamp_millis_opt(timestamp_ms)
        .single()
        .or_else(|| Local.timestamp_millis_opt(timestamp_ms).earliest())
        .map(|date_time| date_time.date_naive().format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".into())
}

fn local_midnight_ms(date: NaiveDate) -> i64 {
    let naive = date.and_hms_opt(0, 0, 0).expect("valid midnight");
    match Local.from_local_datetime(&naive) {
        LocalResult::Single(value) => value.timestamp_millis(),
        LocalResult::Ambiguous(earliest, _) => earliest.timestamp_millis(),
        LocalResult::None => {
            for minute in 1..=180 {
                if let Some(candidate) = naive.checked_add_signed(chrono::Duration::minutes(minute))
                    && let Some(value) = Local.from_local_datetime(&candidate).earliest()
                {
                    return value.timestamp_millis();
                }
            }
            0
        }
    }
}

fn split_interval_by_local_day(interval: TimeInterval) -> Vec<(String, TimeInterval)> {
    let mut pieces = Vec::new();
    let mut cursor = interval.start_ms;
    while cursor < interval.end_ms {
        let date = Local
            .timestamp_millis_opt(cursor)
            .earliest()
            .map(|value| value.date_naive())
            .unwrap_or_else(|| NaiveDate::from_ymd_opt(1970, 1, 1).unwrap());
        let next_date = date.succ_opt().unwrap_or(date);
        let next_midnight = local_midnight_ms(next_date);
        let piece_end = interval
            .end_ms
            .min(next_midnight.max(cursor.saturating_add(1)));
        pieces.push((
            date.format("%Y-%m-%d").to_string(),
            TimeInterval {
                start_ms: cursor,
                end_ms: piece_end,
            },
        ));
        cursor = piece_end;
    }
    pieces
}

fn add_intervals_to_days(
    daily: &mut BTreeMap<String, DailyIntervalGroups>,
    intervals: &[TimeInterval],
    kind: DailyIntervalKind,
) {
    for interval in intervals {
        for (date, piece) in split_interval_by_local_day(*interval) {
            let groups = daily.entry(date).or_default();
            match kind {
                DailyIntervalKind::Invested => groups.invested.push(piece),
                DailyIntervalKind::Activity => groups.activity.push(piece),
                DailyIntervalKind::Focus => groups.focus.push(piece),
            }
        }
    }
}

fn inclusive_local_day_count(start_ms: i64, end_ms: i64) -> i64 {
    if end_ms < start_ms {
        return 0;
    }
    let start = Local.timestamp_millis_opt(start_ms).earliest();
    let end = Local.timestamp_millis_opt(end_ms).earliest();
    match (start, end) {
        (Some(start), Some(end)) => end
            .date_naive()
            .signed_duration_since(start.date_naive())
            .num_days()
            .saturating_add(1),
        _ => 0,
    }
}

fn median_i64(values: &[i64]) -> i64 {
    match values.len() {
        0 => 0,
        length if length % 2 == 1 => values[length / 2],
        length => values[length / 2 - 1].saturating_add(values[length / 2]) / 2,
    }
}

fn regularity_score(values: &[i64]) -> f64 {
    if values.len() <= 1 {
        return if values.is_empty() { 0.0 } else { 1.0 };
    }
    let mean = values.iter().sum::<i64>() as f64 / values.len() as f64;
    if mean <= f64::EPSILON {
        return 0.0;
    }
    let variance = values
        .iter()
        .map(|value| {
            let difference = *value as f64 - mean;
            difference * difference
        })
        .sum::<f64>()
        / values.len() as f64;
    let coefficient_of_variation = variance.sqrt() / mean;
    (1.0 / (1.0 + coefficient_of_variation)).clamp(0.0, 1.0)
}

fn project_from_row(row: &Row<'_>) -> Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
        status: ProjectStatus::from_str(&row.get::<_, String>(3)?),
        description: row.get(4)?,
        created_at_ms: row.get(5)?,
        updated_at_ms: row.get(6)?,
        archived_at_ms: row.get(7)?,
    })
}

fn task_from_row(row: &Row<'_>) -> Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        project_id: row.get(1)?,
        title: row.get(2)?,
        status: TaskStatus::from_str(&row.get::<_, String>(3)?),
        priority: TaskPriority::from_str(&row.get::<_, String>(4)?),
        expected_output: row.get(5)?,
        due_date: row.get(6)?,
        created_at_ms: row.get(7)?,
        updated_at_ms: row.get(8)?,
        completed_at_ms: row.get(9)?,
        origin_kind: TaskOriginKind::from_str(&row.get::<_, String>(10)?),
        origin_key: row.get(11)?,
        origin_confidence: row.get(12)?,
        review_state: TaskReviewState::from_str(&row.get::<_, String>(13)?),
    })
}

fn evidence_link_from_row(row: &Row<'_>) -> Result<EvidenceLink> {
    Ok(EvidenceLink {
        task_id: row.get(0)?,
        evidence_id: row.get(1)?,
        provenance: EvidenceProvenance::from_str(&row.get::<_, String>(2)?),
        confidence: row.get(3)?,
        reason: row.get(4)?,
        created_at_ms: row.get(5)?,
    })
}

fn progress_entry_from_row(row: &Row<'_>) -> Result<ProgressEntry> {
    Ok(ProgressEntry {
        id: row.get(0)?,
        task_id: row.get(1)?,
        note: row.get(2)?,
        created_at_ms: row.get(3)?,
        origin_kind: ProgressOriginKind::from_str(&row.get::<_, String>(4)?),
        source_id: row.get(5)?,
        source_date: row.get(6)?,
    })
}

fn daily_goal_task_link_from_row(row: &Row<'_>) -> Result<DailyGoalTaskLink> {
    Ok(DailyGoalTaskLink {
        goal_row_id: row.get(0)?,
        goal_date: row.get(1)?,
        goal_text: row.get(2)?,
        task_id: row.get(3)?,
        confirmed_at_ms: row.get(4)?,
    })
}

fn derived_progress_entry_id(
    origin_kind: ProgressOriginKind,
    task_id: &str,
    source_id: &str,
) -> String {
    let hash = format!(
        "{:x}",
        Sha256::digest(format!("{}\n{task_id}\n{source_id}", origin_kind.as_str()).as_bytes())
    );
    format!("progress-{}", &hash[..24])
}

fn activity_scope_key(scope: ActivityScope) -> &'static str {
    match scope {
        ActivityScope::All => "all",
        ActivityScope::Active => "active",
        ActivityScope::Meaningful => "meaningful",
    }
}

fn daily_analysis_record_from_row(row: &Row<'_>) -> Result<DailyAnalysisRecord> {
    Ok(DailyAnalysisRecord {
        date: row.get(0)?,
        evidence_hash: row.get(1)?,
        portrait: row.get(2)?,
        recommendation: row.get(3)?,
        findings_json: row.get(4)?,
        protocol_version: row.get(5)?,
        source: row.get(6)?,
        generated_at_ms: row.get(7)?,
    })
}

pub fn category_key(category: ActivityCategory) -> &'static str {
    match category {
        ActivityCategory::Idle => "idle",
        ActivityCategory::Research => "research",
        ActivityCategory::VideoInput => "video_input",
        ActivityCategory::TextInput => "text_input",
        ActivityCategory::Game => "game",
        ActivityCategory::Social => "social",
        ActivityCategory::CreationDevelopment => "creation_development",
        ActivityCategory::FileManagement => "file_management",
        ActivityCategory::Pending => "pending",
    }
}

fn video_purpose_key(purpose: VideoPurpose) -> &'static str {
    match purpose {
        VideoPurpose::Learning => "learning",
        VideoPurpose::Leisure => "leisure",
        VideoPurpose::Unknown => "unknown",
    }
}

fn inactivity_reason_key(reason: InactivityReason) -> &'static str {
    match reason {
        InactivityReason::InputIdle => "input_idle",
        InactivityReason::ContinuityGap => "continuity_gap",
        InactivityReason::LegacyGapRepair => "legacy_gap_repair",
    }
}

fn inactivity_reason_from_key(value: Option<&str>) -> Option<InactivityReason> {
    match value {
        Some("input_idle") => Some(InactivityReason::InputIdle),
        Some("continuity_gap") => Some(InactivityReason::ContinuityGap),
        Some("legacy_gap_repair") => Some(InactivityReason::LegacyGapRepair),
        _ => None,
    }
}

fn inferred_inactivity_reason(category: &str, model_version: &str) -> Option<InactivityReason> {
    if category != "idle" {
        return None;
    }
    match model_version {
        "continuity-gap-v1" => Some(InactivityReason::ContinuityGap),
        "legacy-gap-repair-v1" => Some(InactivityReason::LegacyGapRepair),
        _ => Some(InactivityReason::InputIdle),
    }
}

fn upsert_native_segment_on(
    connection: &Connection,
    segment: &ActivitySegmentRecord,
) -> Result<()> {
    connection.execute(
        "INSERT INTO activity_segments (
            id, started_at_ms, ended_at_ms, app, app_path, title, category, video_purpose,
            confidence, classification_source, reason, model_version, needs_review,
            inactivity_reason, origin
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 'native')
         ON CONFLICT(id) DO UPDATE SET
            ended_at_ms=excluded.ended_at_ms,
            app=excluded.app,
            app_path=excluded.app_path,
            title=excluded.title,
            category=CASE WHEN activity_segments.classification_source IN ('manual', 'ai')
                THEN activity_segments.category ELSE excluded.category END,
            video_purpose=CASE WHEN activity_segments.classification_source IN ('manual', 'ai')
                THEN activity_segments.video_purpose ELSE excluded.video_purpose END,
            confidence=CASE WHEN activity_segments.classification_source IN ('manual', 'ai')
                THEN activity_segments.confidence ELSE excluded.confidence END,
            classification_source=CASE WHEN activity_segments.classification_source IN ('manual', 'ai')
                THEN activity_segments.classification_source ELSE excluded.classification_source END,
            reason=CASE WHEN activity_segments.classification_source IN ('manual', 'ai')
                THEN activity_segments.reason ELSE excluded.reason END,
            model_version=CASE WHEN activity_segments.classification_source IN ('manual', 'ai')
                THEN activity_segments.model_version ELSE excluded.model_version END,
            needs_review=CASE WHEN activity_segments.classification_source IN ('manual', 'ai')
                THEN activity_segments.needs_review ELSE excluded.needs_review END,
            inactivity_reason=excluded.inactivity_reason",
        params![
            segment.id,
            segment.started_at_ms,
            segment.ended_at_ms.max(segment.started_at_ms),
            segment.app,
            segment.app_path,
            segment.title,
            category_key(segment.category),
            video_purpose_key(segment.video_purpose),
            segment.confidence.clamp(0.0, 1.0),
            source_key(segment.source),
            segment.reason,
            segment.model_version,
            segment.needs_review,
            segment.inactivity_reason.map(inactivity_reason_key),
        ],
    )?;
    Ok(())
}

fn save_monitoring_continuity_checkpoint_on(
    connection: &Connection,
    checkpoint: &MonitoringContinuityCheckpoint,
) -> Result<()> {
    connection.execute(
        "INSERT INTO monitoring_continuity_checkpoint(
            singleton_id, expected_tracking, last_observed_at_ms, last_boot_started_at_ms,
            last_uptime_ms, updated_at_ms
         ) VALUES (1, ?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(singleton_id) DO UPDATE SET
            expected_tracking=excluded.expected_tracking,
            last_observed_at_ms=excluded.last_observed_at_ms,
            last_boot_started_at_ms=excluded.last_boot_started_at_ms,
            last_uptime_ms=excluded.last_uptime_ms,
            updated_at_ms=excluded.updated_at_ms",
        params![
            checkpoint.expected_tracking,
            checkpoint.last_observed_at_ms,
            checkpoint.last_boot_started_at_ms,
            checkpoint.last_uptime_ms,
            checkpoint.updated_at_ms,
        ],
    )?;
    Ok(())
}

fn monitoring_gap_key(reason: &str, started_at_ms: i64, ended_at_ms: i64) -> String {
    let material = format!("{reason}\n{started_at_ms}\n{ended_at_ms}");
    format!("{:x}", Sha256::digest(material.as_bytes()))
}

fn browser_workflow_evidence_is_excluded(domain: &str, title: &str) -> bool {
    let domain = domain.trim().to_ascii_lowercase();
    let title = title.trim().to_lowercase();
    let game_markers = [
        "steampowered.com",
        "steamcommunity.com",
        "epicgames.com",
        "itch.io",
        "twitch.tv",
        "huya.com",
        "douyu.com",
    ];
    if game_markers
        .iter()
        .any(|marker| domain == *marker || domain.ends_with(&format!(".{marker}")))
        || [
            " steam",
            "game ",
            "gaming",
            "游戏",
            "英雄联盟",
            "league of legends",
            "原神",
            "genshin",
            "minecraft",
        ]
        .iter()
        .any(|marker| format!(" {title} ").contains(marker))
    {
        return true;
    }

    let leisure_video_site = [
        "youtube.com",
        "bilibili.com",
        "douyin.com",
        "iqiyi.com",
        "netflix.com",
    ]
    .iter()
    .any(|marker| domain == *marker || domain.ends_with(&format!(".{marker}")));
    let learning_marker = [
        "ielts",
        "雅思",
        "learn",
        "tutorial",
        "course",
        "lecture",
        "vocabulary",
        "教程",
        "学习",
        "课程",
        "讲解",
        "备考",
        "单词",
    ]
    .iter()
    .any(|marker| title.contains(marker));
    leisure_video_site && !learning_marker
}

fn repair_activity_segments_for_gap(
    transaction: &Transaction<'_>,
    gap_start_ms: i64,
    gap_end_ms: i64,
) -> Result<()> {
    let segments = {
        let mut statement = transaction.prepare(
            "SELECT id, started_at_ms, ended_at_ms, app, app_path, title, category,
                    video_purpose, confidence, classification_source, reason, model_version,
                    needs_review, inactivity_reason, origin
             FROM activity_segments
             WHERE started_at_ms < ?2 AND ended_at_ms > ?1
             ORDER BY started_at_ms, id",
        )?;
        statement
            .query_map(params![gap_start_ms, gap_end_ms], |row| {
                Ok(RepairableActivitySegment {
                    id: row.get(0)?,
                    started_at_ms: row.get(1)?,
                    ended_at_ms: row.get(2)?,
                    app: row.get(3)?,
                    app_path: row.get(4)?,
                    title: row.get(5)?,
                    category: row.get(6)?,
                    video_purpose: row.get(7)?,
                    confidence: row.get(8)?,
                    classification_source: row.get(9)?,
                    reason: row.get(10)?,
                    model_version: row.get(11)?,
                    needs_review: row.get(12)?,
                    inactivity_reason: row.get(13)?,
                    origin: row.get(14)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?
    };

    for segment in segments {
        let keeps_prefix = segment.started_at_ms < gap_start_ms;
        let keeps_suffix = segment.ended_at_ms > gap_end_ms;
        match (keeps_prefix, keeps_suffix) {
            (true, true) => {
                let suffix_hash = monitoring_gap_key(
                    &format!("segment-suffix:{}", segment.id),
                    gap_end_ms,
                    segment.ended_at_ms,
                );
                let suffix_id = format!("gap-suffix-{}", &suffix_hash[..24]);
                transaction.execute(
                    "UPDATE activity_segments SET ended_at_ms=?2 WHERE id=?1",
                    params![segment.id, gap_start_ms],
                )?;
                transaction.execute(
                    "INSERT OR IGNORE INTO activity_segments(
                        id, started_at_ms, ended_at_ms, app, app_path, title, category,
                        video_purpose, confidence, classification_source, reason, model_version,
                        needs_review, inactivity_reason, origin
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                    params![
                        suffix_id,
                        gap_end_ms,
                        segment.ended_at_ms,
                        segment.app,
                        segment.app_path,
                        segment.title,
                        segment.category,
                        segment.video_purpose,
                        segment.confidence,
                        segment.classification_source,
                        segment.reason,
                        segment.model_version,
                        segment.needs_review,
                        segment.inactivity_reason,
                        segment.origin,
                    ],
                )?;
                transaction.execute(
                    "INSERT OR IGNORE INTO classifications(
                        segment_id, category, video_purpose, confidence, source, reason,
                        model_version, needs_review
                     )
                     SELECT ?2, category, video_purpose, confidence, source, reason,
                            model_version, needs_review
                     FROM classifications WHERE segment_id=?1",
                    params![segment.id, suffix_id],
                )?;
                transaction.execute(
                    "INSERT OR IGNORE INTO task_activity_links(
                        task_id, activity_segment_id, provenance, confidence, reason, created_at_ms
                     )
                     SELECT task_id, ?2, provenance, confidence, reason, created_at_ms
                     FROM task_activity_links WHERE activity_segment_id=?1",
                    params![segment.id, suffix_id],
                )?;
            }
            (true, false) => {
                transaction.execute(
                    "UPDATE activity_segments SET ended_at_ms=?2 WHERE id=?1",
                    params![segment.id, gap_start_ms],
                )?;
            }
            (false, true) => {
                transaction.execute(
                    "UPDATE activity_segments SET started_at_ms=?2 WHERE id=?1",
                    params![segment.id, gap_end_ms],
                )?;
            }
            (false, false) => {
                transaction.execute(
                    "DELETE FROM task_activity_links WHERE activity_segment_id=?1",
                    [&segment.id],
                )?;
                transaction.execute("DELETE FROM activity_segments WHERE id=?1", [&segment.id])?;
            }
        }
    }
    Ok(())
}

fn source_key(source: ClassificationSource) -> &'static str {
    match source {
        ClassificationSource::Manual => "manual",
        ClassificationSource::Idle => "idle",
        ClassificationSource::Rule => "rule",
        ClassificationSource::Behavior => "behavior",
        ClassificationSource::Ai => "ai",
        ClassificationSource::Pending => "pending",
    }
}

fn category_from_key(value: &str) -> ActivityCategory {
    match value {
        "idle" => ActivityCategory::Idle,
        "research" => ActivityCategory::Research,
        "video_input" => ActivityCategory::VideoInput,
        "text_input" => ActivityCategory::TextInput,
        "game" => ActivityCategory::Game,
        "social" => ActivityCategory::Social,
        "creation_development" => ActivityCategory::CreationDevelopment,
        "file_management" => ActivityCategory::FileManagement,
        _ => ActivityCategory::Pending,
    }
}

fn video_purpose_from_key(value: &str) -> VideoPurpose {
    match value {
        "learning" => VideoPurpose::Learning,
        "leisure" => VideoPurpose::Leisure,
        _ => VideoPurpose::Unknown,
    }
}

fn source_from_key(value: &str) -> ClassificationSource {
    match value {
        "manual" => ClassificationSource::Manual,
        "idle" => ClassificationSource::Idle,
        "rule" => ClassificationSource::Rule,
        "behavior" => ClassificationSource::Behavior,
        "ai" => ClassificationSource::Ai,
        _ => ClassificationSource::Pending,
    }
}

const AI_REVIEW_COLUMNS: &str = "id, kind, state, subject_id, before_json, proposed_json,
    applied_json, confidence, evidence_summary, evidence_hash, execution_mode,
    execution_executor_id, execution_model, execution_evidence_hash, generation,
    execution_created_at_ms, started_at_ms, finished_at_ms, duration_ms, exit_code,
    error_kind, diagnostic, created_at_ms, resolved_at_ms";

fn invalid_review(message: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(message.into())
}

fn sync_event_from_row(row: &Row<'_>) -> Result<SyncEvent> {
    Ok(SyncEvent {
        event_id: row.get(0)?,
        device_id: row.get(1)?,
        sequence: row.get(2)?,
        occurred_at_ms: row.get(3)?,
        entity_kind: row.get(4)?,
        entity_id: row.get(5)?,
        operation: row.get(6)?,
        payload_json: row.get(7)?,
        payload_hash: row.get(8)?,
    })
}

fn insert_sync_event_on(
    connection: &Connection,
    event: &SyncEvent,
    imported_at_ms: i64,
) -> Result<bool> {
    let inserted = connection.execute(
        "INSERT OR IGNORE INTO sync_events(
            event_id, device_id, sequence, occurred_at_ms, entity_kind, entity_id,
            operation, payload_json, payload_hash, imported_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            event.event_id,
            event.device_id,
            event.sequence,
            event.occurred_at_ms,
            event.entity_kind,
            event.entity_id,
            event.operation,
            event.payload_json,
            event.payload_hash,
            imported_at_ms,
        ],
    )?;
    if inserted == 1 {
        return Ok(true);
    }
    let existing = connection
        .query_row(
            "SELECT event_id, device_id, sequence, occurred_at_ms, entity_kind, entity_id,
                    operation, payload_json, payload_hash
             FROM sync_events
             WHERE event_id=?1 OR (device_id=?2 AND sequence=?3)",
            params![event.event_id, event.device_id, event.sequence],
            sync_event_from_row,
        )
        .optional()?;
    if existing.as_ref() == Some(event) {
        Ok(false)
    } else {
        Err(invalid_review(format!(
            "sync event collision for {} sequence {}",
            event.device_id, event.sequence
        )))
    }
}

fn apply_organization_sync_projection_on(connection: &Connection, event: &SyncEvent) -> Result<()> {
    if event.operation != "upsert" {
        return Ok(());
    }
    match event.entity_kind.as_str() {
        "project" => {
            let project: Project = serde_json::from_str(&event.payload_json)
                .map_err(|error| invalid_review(format!("invalid synced project: {error}")))?;
            if project.id != event.entity_id {
                return Err(invalid_review("synced project identity mismatch"));
            }
            connection.execute(
                "INSERT INTO projects(
                    id, name, color, status, description, created_at_ms, updated_at_ms, archived_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name, color=excluded.color,
                    status=excluded.status, description=excluded.description,
                    created_at_ms=excluded.created_at_ms, updated_at_ms=excluded.updated_at_ms,
                    archived_at_ms=excluded.archived_at_ms",
                params![
                    project.id,
                    project.name,
                    project.color,
                    project.status.as_str(),
                    project.description,
                    project.created_at_ms,
                    project.updated_at_ms,
                    project.archived_at_ms,
                ],
            )?;
        }
        "task" => {
            let task: Task = serde_json::from_str(&event.payload_json)
                .map_err(|error| invalid_review(format!("invalid synced task: {error}")))?;
            if task.id != event.entity_id {
                return Err(invalid_review("synced task identity mismatch"));
            }
            connection.execute(
                "INSERT INTO tasks(
                    id, project_id, title, status, priority, expected_output, due_date,
                    created_at_ms, updated_at_ms, completed_at_ms, origin_kind, origin_key,
                    origin_confidence, review_state
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'manual', NULL, NULL, 'confirmed')
                 ON CONFLICT(id) DO UPDATE SET project_id=excluded.project_id,
                    title=excluded.title, status=excluded.status, priority=excluded.priority,
                    expected_output=excluded.expected_output, due_date=excluded.due_date,
                    created_at_ms=excluded.created_at_ms, updated_at_ms=excluded.updated_at_ms,
                    completed_at_ms=excluded.completed_at_ms",
                params![
                    task.id,
                    task.project_id,
                    task.title,
                    task.status.as_str(),
                    task.priority.as_str(),
                    task.expected_output,
                    task.due_date,
                    task.created_at_ms,
                    task.updated_at_ms,
                    task.completed_at_ms,
                ],
            )?;
        }
        _ => {}
    }
    Ok(())
}

fn external_context_kind_key(kind: ExternalContextKind) -> &'static str {
    match kind {
        ExternalContextKind::CalendarEvent => "calendar_event",
        ExternalContextKind::Project => "project",
        ExternalContextKind::Task => "task",
    }
}

fn external_context_item_from_row(row: &Row<'_>) -> Result<ExternalContextItem> {
    let kind = match row.get::<_, String>(5)?.as_str() {
        "calendar_event" => ExternalContextKind::CalendarEvent,
        "project" => ExternalContextKind::Project,
        "task" => ExternalContextKind::Task,
        value => {
            return Err(invalid_review(format!(
                "invalid external context kind: {value}"
            )));
        }
    };
    Ok(ExternalContextItem {
        id: row.get(0)?,
        source_id: row.get(1)?,
        source_name: row.get(2)?,
        source_kind: row.get(3)?,
        external_id: row.get(4)?,
        kind,
        title: row.get(6)?,
        start_at_ms: row.get(7)?,
        end_at_ms: row.get(8)?,
        project_name: row.get(9)?,
        status: row.get(10)?,
        imported_at_ms: row.get(11)?,
    })
}

#[allow(clippy::too_many_arguments)]
fn enqueue_ai_job_hashed_on(
    connection: &Connection,
    kind: &str,
    subject_key: &str,
    payload_json: &str,
    now_ms: i64,
    force: bool,
    execution: &AiExecutionSnapshot,
) -> Result<(String, bool)> {
    if !force {
        let existing = connection
            .query_row(
                "SELECT id, status FROM ai_jobs
                 WHERE kind=?1 AND subject_key=?2
                   AND execution_mode=?3 AND executor_id=?4 AND model=?5 AND evidence_hash=?6
                 ORDER BY generation DESC, id ASC LIMIT 1",
                params![
                    kind,
                    subject_key,
                    execution.execution_mode.as_database(),
                    execution.executor_id,
                    execution.model,
                    execution.evidence_hash,
                ],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        if let Some((id, status)) = existing {
            if status == "pending" {
                connection.execute(
                    "UPDATE ai_jobs SET payload_json=?2 WHERE id=?1 AND status='pending'",
                    params![id, payload_json],
                )?;
            }
            return Ok((id, false));
        }

        // Evidence for a stable subject can change while it is still being collected (for
        // example, the duration of today's current segment). Replace an unattempted pending
        // snapshot instead of appending one queue row for every observation.
        let pending = connection
            .query_row(
                "SELECT id, generation FROM ai_jobs
                 WHERE kind=?1 AND subject_key=?2
                   AND execution_mode=?3 AND executor_id=?4 AND model=?5
                   AND status='pending' AND attempts=0
                 ORDER BY generation DESC, id ASC LIMIT 1",
                params![
                    kind,
                    subject_key,
                    execution.execution_mode.as_database(),
                    execution.executor_id,
                    execution.model,
                ],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        if let Some((id, generation)) = pending {
            let snapshot_hash_input = format!(
                "{}\n{}\n{}\n{}",
                execution.execution_mode.as_database(),
                execution.executor_id,
                execution.model,
                execution.evidence_hash
            );
            let content_hash = format!(
                "{:x}",
                Sha256::digest(
                    format!("{kind}\n{subject_key}\n{generation}\n{snapshot_hash_input}")
                        .as_bytes()
                )
            );
            connection.execute(
                "UPDATE ai_jobs SET
                    content_hash=?2, payload_json=?3, next_attempt_at_ms=?4,
                    evidence_hash=?5, execution_created_at_ms=?6
                 WHERE id=?1 AND status='pending' AND attempts=0",
                params![
                    id,
                    content_hash,
                    payload_json,
                    now_ms,
                    execution.evidence_hash,
                    execution.created_at_ms,
                ],
            )?;
            return Ok((id, false));
        }
    }
    let snapshot_hash_input = format!(
        "{}\n{}\n{}\n{}",
        execution.execution_mode.as_database(),
        execution.executor_id,
        execution.model,
        execution.evidence_hash
    );
    let generation = connection
        .query_row(
            "SELECT MAX(generation)+1 FROM ai_jobs WHERE kind=?1 AND subject_key=?2",
            params![kind, subject_key],
            |row| row.get::<_, Option<i64>>(0),
        )?
        .unwrap_or(0);
    let content_hash = format!(
        "{:x}",
        Sha256::digest(
            format!("{kind}\n{subject_key}\n{generation}\n{snapshot_hash_input}").as_bytes()
        )
    );
    let id = format!("ai-{}", &content_hash[..24]);
    connection.execute(
        "INSERT INTO ai_jobs (
            id, content_hash, subject_key, kind, payload_json, status, attempts,
            next_attempt_at_ms, generation, execution_mode, executor_id, model,
            evidence_hash, execution_created_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, 'pending', 0, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            id,
            content_hash,
            subject_key,
            kind,
            payload_json,
            now_ms,
            generation,
            execution.execution_mode.as_database(),
            execution.executor_id,
            execution.model,
            execution.evidence_hash,
            execution.created_at_ms,
        ],
    )?;
    Ok((id, true))
}

fn ai_review_from_row(row: &Row<'_>) -> Result<AiReviewRecord> {
    Ok(AiReviewRecord {
        id: row.get(0)?,
        kind: AiReviewKind::from_database(&row.get::<_, String>(1)?)
            .ok_or(rusqlite::Error::InvalidQuery)?,
        state: AiReviewState::from_database(&row.get::<_, String>(2)?)
            .ok_or(rusqlite::Error::InvalidQuery)?,
        subject_id: row.get(3)?,
        before_json: row.get(4)?,
        proposed_json: row.get(5)?,
        applied_json: row.get(6)?,
        confidence: row.get(7)?,
        evidence_summary: row.get(8)?,
        evidence_hash: row.get(9)?,
        execution: AiExecutionAuditView {
            execution_mode: row
                .get::<_, Option<String>>(10)?
                .map(|value| AiExecutionMode::from_database(&value)),
            executor_id: row.get(11)?,
            model: row.get(12)?,
            evidence_hash: row.get(13)?,
            generation: row.get(14)?,
            created_at_ms: row.get(15)?,
            started_at_ms: row.get(16)?,
            finished_at_ms: row.get(17)?,
            duration_ms: row.get(18)?,
            exit_code: row.get(19)?,
            error_kind: row
                .get::<_, Option<String>>(20)?
                .map(|value| AiExecutionErrorKind::from_database(&value)),
            diagnostic: row.get(21)?,
        },
        created_at_ms: row.get(22)?,
        resolved_at_ms: row.get(23)?,
    })
}

fn append_ai_review_event_on(
    connection: &Connection,
    review_id: &str,
    event: AiReviewEventKind,
    created_at_ms: i64,
) -> Result<()> {
    connection.execute(
        "INSERT INTO ai_review_events(review_id, event_kind, event_json, created_at_ms)
         VALUES (?1, ?2, '{}', ?3)",
        params![review_id, event.as_database(), created_at_ms],
    )?;
    Ok(())
}

fn has_manual_field_ownership_on(
    connection: &Connection,
    kind: AiReviewKind,
    subject_id: &str,
) -> Result<bool> {
    connection.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM manual_field_ownership
            WHERE kind=?1 AND subject_id=?2 AND field_name=?3 AND released_at_ms IS NULL
        )",
        params![kind.as_database(), subject_id, kind.field_name()],
        |row| row.get(0),
    )
}

fn formal_value_is_manually_owned_on(
    connection: &Connection,
    kind: AiReviewKind,
    subject_id: &str,
) -> Result<bool> {
    match kind {
        AiReviewKind::Classification => connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM activity_segments
                WHERE id=?1 AND classification_source='manual'
            )",
            [subject_id],
            |row| row.get(0),
        ),
        AiReviewKind::WorkflowAssignment => connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM task_activity_links
                WHERE activity_segment_id=?1 AND provenance='manual'
                UNION ALL
                SELECT 1 FROM task_browser_links
                WHERE browser_visit_id=?1 AND provenance='manual'
            )",
            [subject_id],
            |row| row.get(0),
        ),
        AiReviewKind::ProjectDraft => Ok(false),
    }
}

fn claim_manual_field_ownership_on(
    connection: &Connection,
    kind: AiReviewKind,
    subject_id: &str,
    owner: &str,
    claimed_at_ms: i64,
) -> Result<()> {
    connection.execute(
        "INSERT OR IGNORE INTO manual_field_ownership(
            kind, subject_id, field_name, owner, claimed_at_ms, released_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
        params![
            kind.as_database(),
            subject_id,
            kind.field_name(),
            owner,
            claimed_at_ms,
        ],
    )?;
    Ok(())
}

fn parse_classification_review(value: &str) -> Result<AiClassificationReviewValue> {
    serde_json::from_str(value)
        .map_err(|error| invalid_review(format!("Invalid classification review JSON: {error}")))
}

fn parse_workflow_review(value: &str) -> Result<AiWorkflowAssignmentReviewValue> {
    serde_json::from_str(value)
        .map_err(|error| invalid_review(format!("Invalid workflow review JSON: {error}")))
}

fn validate_project_draft_proposal(
    connection: &Connection,
    subject_id: &str,
    proposal: &ProjectDraftProposal,
) -> Result<()> {
    if proposal.name.trim().is_empty()
        || proposal.name.chars().count() > 80
        || proposal.description.chars().count() > 500
        || !proposal.confidence.is_finite()
        || !(0.0..=1.0).contains(&proposal.confidence)
        || !(1..=3).contains(&proposal.tasks.len())
    {
        return Err(invalid_review("Project draft fields are invalid"));
    }
    if let Some(project_id) = proposal.target_project_id.as_deref() {
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1 AND status='active')",
            [project_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(invalid_review("Project draft target no longer exists"));
        }
    }
    let mut task_keys = HashSet::new();
    let mut cluster_ids = HashSet::new();
    for task in &proposal.tasks {
        if task.key.trim().is_empty()
            || !task_keys.insert(task.key.as_str())
            || task.title.trim().is_empty()
            || task.title.chars().count() > 80
            || task.expected_output.chars().count() > 300
            || task.cluster_ids.is_empty()
        {
            return Err(invalid_review("Project draft task fields are invalid"));
        }
        for cluster_id in &task.cluster_ids {
            if !cluster_ids.insert(cluster_id.as_str()) {
                return Err(invalid_review(
                    "A project draft cluster can belong to only one task",
                ));
            }
            let bound: bool = connection.query_row(
                "SELECT EXISTS(
                    SELECT 1
                    FROM project_draft_bindings binding
                    JOIN ai_review_records review ON review.id=binding.review_id
                    WHERE binding.cluster_id=?1
                      AND review.subject_id=?2
                      AND review.state='pending'
                )",
                params![cluster_id, subject_id],
                |row| row.get(0),
            )?;
            if !bound {
                return Err(invalid_review("Project draft cluster binding has changed"));
            }
        }
    }
    Ok(())
}

fn apply_project_draft_proposal_on(
    connection: &Connection,
    subject_id: &str,
    proposal: &ProjectDraftProposal,
    manual: bool,
) -> Result<bool> {
    let project_id = proposal.target_project_id.clone().unwrap_or_else(|| {
        let hash = format!(
            "{:x}",
            Sha256::digest(format!("{subject_id}\n{}", proposal.name.trim()).as_bytes())
        );
        format!("project-ai-{}", &hash[..24])
    });
    let applied_at_ms = connection.query_row(
        "SELECT created_at_ms FROM ai_review_records
         WHERE kind='project_draft' AND subject_id=?1 AND state='pending'
         ORDER BY created_at_ms DESC LIMIT 1",
        [subject_id],
        |row| row.get::<_, i64>(0),
    )?;
    if proposal.target_project_id.is_some() {
        let active: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1 AND status='active')",
            [&project_id],
            |row| row.get(0),
        )?;
        if !active {
            return Ok(false);
        }
    } else {
        connection.execute(
            "INSERT INTO projects(
                id, name, color, status, description, created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, '#7c3aed', 'active', ?3, ?4, ?4)",
            params![
                project_id,
                proposal.name.trim(),
                proposal.description.trim(),
                applied_at_ms,
            ],
        )?;
    }
    for task in &proposal.tasks {
        let task_hash = format!(
            "{:x}",
            Sha256::digest(format!("{subject_id}\n{}", task.key.trim()).as_bytes())
        );
        let task_id = format!("task-ai-{}", &task_hash[..24]);
        let origin_key = format!("project-draft:{subject_id}:{}", task.key.trim());
        connection.execute(
            "INSERT INTO tasks(
                id, project_id, title, status, priority, expected_output, due_date,
                created_at_ms, updated_at_ms, origin_kind, origin_key,
                origin_confidence, review_state
             ) VALUES (?1, ?2, ?3, 'todo', 'medium', ?4, NULL, ?5, ?5,
                       'ai', ?6, ?7, 'confirmed')",
            params![
                task_id,
                project_id,
                task.title.trim(),
                task.expected_output.trim(),
                applied_at_ms,
                origin_key,
                proposal.confidence,
            ],
        )?;
        for cluster_id in &task.cluster_ids {
            let mut evidence_statement = connection.prepare(
                "SELECT DISTINCT evidence.evidence_kind, evidence.evidence_id
                 FROM work_episode_evidence evidence
                 JOIN work_episode_members episode
                   ON episode.episode_key=evidence.episode_key
                 WHERE episode.cluster_key=?1
                 ORDER BY evidence.occurred_at_ms, evidence.evidence_kind, evidence.evidence_id",
            )?;
            let evidence = evidence_statement
                .query_map([cluster_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>>>()?;
            if evidence.is_empty() {
                return Err(invalid_review("Project draft evidence is missing"));
            }
            for (kind, evidence_id) in evidence {
                let (table, column) = match kind.as_str() {
                    "activity" => ("task_activity_links", "activity_segment_id"),
                    "browser" => ("task_browser_links", "browser_visit_id"),
                    _ => return Err(invalid_review("Unknown project draft evidence kind")),
                };
                let already_owned: bool = connection.query_row(
                    &format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE {column}=?1)"),
                    [&evidence_id],
                    |row| row.get(0),
                )?;
                if already_owned {
                    return Err(invalid_review(
                        "Project draft evidence acquired another owner",
                    ));
                }
                connection.execute(
                    &format!(
                        "INSERT INTO {table}(
                            task_id, {column}, provenance, confidence, reason, created_at_ms
                         ) VALUES (?1, ?2, ?3, ?4, 'Confirmed semantic workflow draft', ?5)"
                    ),
                    params![
                        task_id,
                        evidence_id,
                        if manual { "manual" } else { "ai" },
                        proposal.confidence,
                        applied_at_ms,
                    ],
                )?;
            }
            connection.execute(
                "UPDATE work_episode_clusters
                 SET status='created', created_task_id=?2
                 WHERE cluster_key=?1 AND status='collecting'",
                params![cluster_id, task_id],
            )?;
        }
    }
    Ok(true)
}

fn apply_review_value_on(
    connection: &Connection,
    kind: AiReviewKind,
    subject_id: &str,
    value_json: &str,
    manual: bool,
) -> Result<bool> {
    match kind {
        AiReviewKind::Classification => {
            let value = parse_classification_review(value_json)?;
            if !value.confidence.is_finite() || !(0.0..=1.0).contains(&value.confidence) {
                return Err(invalid_review("Classification confidence is out of range"));
            }
            Ok(connection.execute(
                "UPDATE activity_segments
                 SET category=?2, video_purpose=?3, confidence=?4,
                     classification_source=?5, reason=?6, model_version=?7, needs_review=0
                 WHERE id=?1",
                params![
                    subject_id,
                    category_key(value.category),
                    video_purpose_key(value.video_purpose),
                    value.confidence,
                    if manual { "manual" } else { "ai" },
                    value.reason,
                    value.model_version,
                ],
            )? == 1)
        }
        AiReviewKind::WorkflowAssignment => {
            let value = parse_workflow_review(value_json)?;
            if value.evidence_id != subject_id
                || !value.confidence.is_finite()
                || !(0.0..=1.0).contains(&value.confidence)
            {
                return Err(invalid_review("Workflow review does not match its subject"));
            }
            let (table, evidence_column, evidence_table, evidence_id_column) =
                match value.evidence_kind.as_str() {
                    "activity" => (
                        "task_activity_links",
                        "activity_segment_id",
                        "activity_segments",
                        "id",
                    ),
                    "browser" => (
                        "task_browser_links",
                        "browser_visit_id",
                        "browser_visits",
                        "id",
                    ),
                    _ => return Err(invalid_review("Unknown workflow evidence kind")),
                };
            let evidence_exists: bool = connection.query_row(
                &format!(
                    "SELECT EXISTS(SELECT 1 FROM {evidence_table} WHERE {evidence_id_column}=?1)"
                ),
                [subject_id],
                |row| row.get(0),
            )?;
            if !evidence_exists {
                return Ok(false);
            }
            connection.execute(
                &format!("DELETE FROM {table} WHERE {evidence_column}=?1"),
                [subject_id],
            )?;
            let Some(task_id) = value.task_id else {
                return Ok(true);
            };
            let task_exists: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
                [&task_id],
                |row| row.get(0),
            )?;
            if !task_exists {
                return Err(invalid_review("Workflow review task no longer exists"));
            }
            Ok(connection.execute(
                &format!(
                    "INSERT INTO {table}(task_id, {evidence_column}, provenance, confidence, reason, created_at_ms)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
                ),
                params![
                    task_id,
                    subject_id,
                    if manual { "manual" } else { "ai" },
                    value.confidence,
                    value.reason,
                    value.created_at_ms,
                ],
            )? == 1)
        }
        AiReviewKind::ProjectDraft => {
            let proposal: ProjectDraftProposal =
                serde_json::from_str(value_json).map_err(|error| {
                    invalid_review(format!("Invalid project draft review JSON: {error}"))
                })?;
            validate_project_draft_proposal(connection, subject_id, &proposal)?;
            apply_project_draft_proposal_on(connection, subject_id, &proposal, manual)
        }
    }
}

fn formal_value_matches_on(
    connection: &Connection,
    kind: AiReviewKind,
    subject_id: &str,
    expected_json: &str,
) -> Result<bool> {
    match kind {
        AiReviewKind::Classification => {
            let expected = parse_classification_review(expected_json)?;
            connection
                .query_row(
                    "SELECT category, video_purpose, confidence, reason, model_version
                     FROM activity_segments WHERE id=?1",
                    [subject_id],
                    |row| {
                        Ok(
                            category_from_key(&row.get::<_, String>(0)?) == expected.category
                                && video_purpose_from_key(&row.get::<_, String>(1)?)
                                    == expected.video_purpose
                                && row.get::<_, f64>(2)? == expected.confidence
                                && row.get::<_, String>(3)? == expected.reason
                                && row.get::<_, String>(4)? == expected.model_version,
                        )
                    },
                )
                .optional()
                .map(|value| value.unwrap_or(false))
        }
        AiReviewKind::WorkflowAssignment => {
            let expected = parse_workflow_review(expected_json)?;
            if expected.evidence_id != subject_id {
                return Ok(false);
            }
            let current = match expected.evidence_kind.as_str() {
                "activity" => connection
                    .query_row(
                        "SELECT task_id FROM task_activity_links WHERE activity_segment_id=?1",
                        [subject_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?,
                "browser" => connection
                    .query_row(
                        "SELECT task_id FROM task_browser_links WHERE browser_visit_id=?1",
                        [subject_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?,
                _ => return Ok(false),
            };
            Ok(current == expected.task_id)
        }
        AiReviewKind::ProjectDraft => {
            let expected: serde_json::Value =
                serde_json::from_str(expected_json).map_err(|error| {
                    invalid_review(format!("Invalid project draft baseline JSON: {error}"))
                })?;
            let target_project_id = expected
                .get("targetProjectId")
                .and_then(serde_json::Value::as_str);
            if let Some(project_id) = target_project_id {
                connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1 AND status='active')",
                    [project_id],
                    |row| row.get(0),
                )
            } else {
                Ok(true)
            }
        }
    }
}

fn execution_audit_for_job_on(
    connection: &Connection,
    id: &str,
    generation: i64,
    executor_id: Option<&str>,
    model: Option<&str>,
    finished_at_ms: Option<i64>,
    duration_ms: Option<i64>,
    exit_code: Option<i32>,
    error_kind: Option<AiExecutionErrorKind>,
    diagnostic: &str,
) -> Result<AiExecutionAuditView> {
    connection.query_row(
        "SELECT execution_mode, executor_id, model, evidence_hash, execution_created_at_ms,
                started_at_ms
         FROM ai_jobs WHERE id=?1 AND generation=?2",
        params![id, generation],
        |row| {
            let queued_executor = row.get::<_, String>(1)?;
            let queued_model = row.get::<_, String>(2)?;
            Ok(AiExecutionAuditView {
                execution_mode: Some(AiExecutionMode::from_database(&row.get::<_, String>(0)?)),
                executor_id: Some(executor_id.unwrap_or(&queued_executor).to_string()),
                model: Some(model.unwrap_or(&queued_model).to_string()),
                evidence_hash: row.get(3)?,
                generation,
                created_at_ms: row.get(4)?,
                started_at_ms: row.get(5)?,
                finished_at_ms,
                duration_ms,
                exit_code,
                error_kind,
                diagnostic: sanitize_ai_error(diagnostic),
            })
        },
    )
}

fn classification_review_value_on(
    connection: &Connection,
    segment_id: &str,
) -> Result<Option<(String, String, bool)>> {
    connection
        .query_row(
            "SELECT category, video_purpose, confidence, reason, model_version, app,
                    classification_source
             FROM activity_segments WHERE id=?1",
            [segment_id],
            |row| {
                let value = AiClassificationReviewValue {
                    category: category_from_key(&row.get::<_, String>(0)?),
                    video_purpose: video_purpose_from_key(&row.get::<_, String>(1)?),
                    confidence: row.get(2)?,
                    reason: row.get(3)?,
                    model_version: row.get(4)?,
                };
                let value_json = serde_json::to_string(&value)
                    .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
                let app = row.get::<_, String>(5)?;
                let summary = format!("{app} activity segment {segment_id}");
                let manually_owned = row.get::<_, String>(6)? == "manual";
                Ok((value_json, summary, manually_owned))
            },
        )
        .optional()
}

fn classification_evidence_hash_on(
    connection: &Connection,
    segment_id: &str,
) -> Result<Option<String>> {
    connection
        .query_row(
            "SELECT app, app_path, title, started_at_ms, ended_at_ms
             FROM activity_segments WHERE id=?1",
            [segment_id],
            |row| {
                let input = format!(
                    "{segment_id}\n{}\n{}\n{}\n{}\n{}",
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                );
                Ok(format!("{:x}", Sha256::digest(input.as_bytes())))
            },
        )
        .optional()
}

fn workflow_evidence_hash_is_current_on(
    connection: &Connection,
    review: &AiReviewRecord,
) -> Result<bool> {
    let job_id = connection.query_row(
        "SELECT job_id FROM ai_review_records WHERE id=?1",
        [&review.id],
        |row| row.get::<_, Option<String>>(0),
    )?;
    let Some(job_id) = job_id.as_deref() else {
        return Ok(false);
    };
    let payload_json = connection
        .query_row(
            "SELECT payload_json FROM ai_jobs WHERE id=?1 AND generation=?2",
            params![job_id, review.execution.generation],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(payload_json) = payload_json else {
        return Ok(false);
    };
    let queued = parse_work_ledger_assignment_job(&payload_json).map_err(invalid_review)?;
    if queued.evidence.id != review.subject_id
        || queued.evidence_hash != review.evidence_hash
        || queued.evidence.evidence_hash != review.evidence_hash
    {
        return Ok(false);
    }
    Ok(current_work_ledger_evidence_hash_on(
        connection,
        &queued.evidence.kind,
        &queued.evidence.id,
        queued.start_ms,
        queued.end_ms,
    )?
    .as_deref()
        == Some(review.evidence_hash.as_str()))
}

fn workflow_suggestion_matches_review_on(
    connection: &Connection,
    review: &AiReviewRecord,
) -> Result<bool> {
    let suggestion = connection
        .query_row(
            "SELECT evidence_kind, evidence_id, evidence_hash, task_id
             FROM work_ledger_ai_suggestions WHERE review_id=?1",
            [&review.id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((evidence_kind, evidence_id, evidence_hash, task_id)) = suggestion else {
        return Ok(false);
    };
    let proposed: AiWorkflowAssignmentReviewValue = serde_json::from_str(&review.proposed_json)
        .map_err(|error| invalid_review(error.to_string()))?;
    Ok(review.kind == AiReviewKind::WorkflowAssignment
        && review.subject_id == evidence_id
        && review.evidence_hash == evidence_hash
        && proposed.evidence_kind == evidence_kind
        && proposed.evidence_id == evidence_id
        && proposed.task_id.as_deref() == Some(task_id.as_str()))
}

fn current_work_ledger_evidence_hash_on(
    connection: &Connection,
    evidence_kind: &str,
    evidence_id: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Option<String>> {
    match evidence_kind {
        "activity" => connection
            .query_row(
                "SELECT id, MAX(started_at_ms, ?2), MIN(ended_at_ms, ?3), app, app_path,
                        title, category, video_purpose, confidence, classification_source,
                        reason, model_version, needs_review
                 FROM activity_segments
                 WHERE id=?1 AND ended_at_ms > ?2 AND started_at_ms < ?3",
                params![evidence_id, start_ms, end_ms],
                |row| {
                    Ok(work_ledger_evidence_from_activity(ActivitySegmentRecord {
                        id: row.get(0)?,
                        started_at_ms: row.get(1)?,
                        ended_at_ms: row.get(2)?,
                        app: row.get(3)?,
                        app_path: row.get(4)?,
                        title: row.get(5)?,
                        category: category_from_key(&row.get::<_, String>(6)?),
                        video_purpose: video_purpose_from_key(&row.get::<_, String>(7)?),
                        confidence: row.get(8)?,
                        source: source_from_key(&row.get::<_, String>(9)?),
                        reason: row.get(10)?,
                        model_version: row.get(11)?,
                        needs_review: row.get(12)?,
                        inactivity_reason: inferred_inactivity_reason(
                            &row.get::<_, String>(6)?,
                            &row.get::<_, String>(11)?,
                        ),
                    })
                    .evidence_hash)
                },
            )
            .optional(),
        "browser" => connection
            .query_row(
                "SELECT id, browser, profile, visited_at_ms, url, domain, title
                 FROM browser_visits WHERE id=?1 AND visited_at_ms >= ?2 AND visited_at_ms < ?3",
                params![evidence_id, start_ms, end_ms],
                |row| {
                    Ok(work_ledger_evidence_from_browser(BrowserVisitRecord {
                        id: row.get(0)?,
                        browser: row.get(1)?,
                        profile: row.get(2)?,
                        visited_at_ms: row.get(3)?,
                        url: row.get(4)?,
                        domain: row.get(5)?,
                        title: row.get(6)?,
                    })
                    .evidence_hash)
                },
            )
            .optional(),
        _ => Ok(None),
    }
}

fn project_draft_evidence_is_current_on(connection: &Connection, review_id: &str) -> Result<bool> {
    let mut statement = connection.prepare(
        "SELECT evidence.evidence_kind, evidence.evidence_id, evidence.evidence_hash,
                evidence.occurred_at_ms, evidence.duration_seconds
         FROM work_episode_evidence evidence
         JOIN work_episode_members episode ON episode.episode_key=evidence.episode_key
         WHERE episode.cluster_key IN (
            SELECT cluster_id FROM project_draft_bindings WHERE review_id=?1
         )",
    )?;
    let evidence = statement
        .query_map([review_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>>>()?;
    if evidence.is_empty() {
        return Ok(false);
    }
    for (kind, id, expected_hash, occurred_at_ms, duration_seconds) in evidence {
        let end_ms =
            occurred_at_ms.saturating_add(duration_seconds.max(0).saturating_mul(1_000).max(1));
        if current_work_ledger_evidence_hash_on(connection, &kind, &id, occurred_at_ms, end_ms)?
            .as_deref()
            != Some(expected_hash.as_str())
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn workflow_review_value_on(
    connection: &Connection,
    evidence_kind: &str,
    evidence_id: &str,
    created_at_ms: i64,
) -> Result<AiWorkflowAssignmentReviewValue> {
    let (table, column) = match evidence_kind {
        "activity" => ("task_activity_links", "activity_segment_id"),
        "browser" => ("task_browser_links", "browser_visit_id"),
        _ => return Err(invalid_review("Unknown workflow evidence kind")),
    };
    let current = connection
        .query_row(
            &format!(
                "SELECT task_id, confidence, reason, created_at_ms FROM {table} WHERE {column}=?1"
            ),
            [evidence_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()?;
    Ok(match current {
        Some((task_id, confidence, reason, assigned_at_ms)) => AiWorkflowAssignmentReviewValue {
            evidence_kind: evidence_kind.into(),
            evidence_id: evidence_id.into(),
            task_id: Some(task_id),
            confidence,
            reason,
            created_at_ms: assigned_at_ms,
        },
        None => AiWorkflowAssignmentReviewValue {
            evidence_kind: evidence_kind.into(),
            evidence_id: evidence_id.into(),
            task_id: None,
            confidence: 1.0,
            reason: String::new(),
            created_at_ms,
        },
    })
}

fn insert_generated_ai_review_on(
    connection: &Connection,
    draft: &AiReviewDraft,
    block_auto_apply: bool,
) -> Result<AiReviewState> {
    let is_error = draft.execution_error.is_some() || draft.execution.error_kind.is_some();
    let owned = block_auto_apply
        || has_manual_field_ownership_on(connection, draft.kind, &draft.subject_id)?;
    let auto_apply = !is_error && !owned && draft.confidence.is_some_and(|value| value >= 0.85);
    let initial_state = if is_error {
        AiReviewState::ExecutionError
    } else {
        AiReviewState::Pending
    };
    let diagnostic = sanitize_ai_error(
        draft
            .execution_error
            .as_deref()
            .unwrap_or(&draft.execution.diagnostic),
    );
    connection.execute(
        "INSERT INTO ai_review_records(
            id, job_id, kind, state, subject_id, before_json, proposed_json, applied_json,
            confidence, evidence_summary, evidence_hash, execution_mode,
            execution_executor_id, execution_model, execution_evidence_hash, generation,
            execution_created_at_ms, started_at_ms, finished_at_ms, duration_ms, exit_code,
            error_kind, diagnostic, created_at_ms, resolved_at_ms
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, NULL
         )",
        params![
            draft.id,
            draft.job_id,
            draft.kind.as_database(),
            initial_state.as_database(),
            draft.subject_id,
            draft.before_json,
            draft.proposed_json,
            draft.confidence,
            draft.evidence_summary,
            draft.evidence_hash,
            draft
                .execution
                .execution_mode
                .map(AiExecutionMode::as_database),
            draft.execution.executor_id,
            draft.execution.model,
            draft.execution.evidence_hash,
            draft.execution.generation,
            draft.execution.created_at_ms,
            draft.execution.started_at_ms,
            draft.execution.finished_at_ms,
            draft.execution.duration_ms,
            draft.execution.exit_code,
            draft
                .execution
                .error_kind
                .map(AiExecutionErrorKind::as_database),
            diagnostic,
            draft.created_at_ms,
        ],
    )?;
    append_ai_review_event_on(
        connection,
        &draft.id,
        AiReviewEventKind::Generated,
        draft.created_at_ms,
    )?;
    if is_error {
        append_ai_review_event_on(
            connection,
            &draft.id,
            AiReviewEventKind::Failed,
            draft.created_at_ms,
        )?;
        return Ok(AiReviewState::ExecutionError);
    }
    if auto_apply {
        if !apply_review_value_on(
            connection,
            draft.kind,
            &draft.subject_id,
            &draft.proposed_json,
            false,
        )? {
            return Err(invalid_review("Review subject no longer exists"));
        }
        connection.execute(
            "UPDATE ai_review_records
             SET state='auto_applied', applied_json=proposed_json, resolved_at_ms=?2
             WHERE id=?1",
            params![draft.id, draft.created_at_ms],
        )?;
        append_ai_review_event_on(
            connection,
            &draft.id,
            AiReviewEventKind::AutoApplied,
            draft.created_at_ms,
        )?;
        return Ok(AiReviewState::AutoApplied);
    }
    Ok(AiReviewState::Pending)
}

#[allow(clippy::too_many_arguments)]
fn insert_execution_error_review_for_job_on(
    connection: &Connection,
    id: &str,
    generation: i64,
    finished_at_ms: i64,
    error: &str,
    error_kind: AiExecutionErrorKind,
    executor_id: Option<&str>,
    model: Option<&str>,
    exit_code: Option<i32>,
    review_suffix: &str,
) -> Result<bool> {
    let (job_kind, subject_key, payload_json) = connection.query_row(
        "SELECT kind, subject_key, payload_json FROM ai_jobs WHERE id=?1 AND generation=?2",
        params![id, generation],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )?;
    let review_input = if job_kind == "classify_segment" {
        let segment_id = serde_json::from_str::<serde_json::Value>(&payload_json)
            .ok()
            .and_then(|value| value.get("id")?.as_str().map(str::to_owned))
            .unwrap_or(subject_key);
        let (before_json, evidence_summary) =
            classification_review_value_on(connection, &segment_id)?
                .map(|(before_json, evidence_summary, _)| (before_json, evidence_summary))
                .unwrap_or_else(|| ("{}".into(), format!("Missing segment {segment_id}")));
        Some((
            AiReviewKind::Classification,
            segment_id,
            before_json,
            evidence_summary,
        ))
    } else if job_kind == "work_ledger_assignment" {
        let payload = serde_json::from_str::<serde_json::Value>(&payload_json).ok();
        let evidence_kind = payload
            .as_ref()
            .and_then(|value| value.pointer("/evidence/kind"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("activity");
        let evidence_id = payload
            .as_ref()
            .and_then(|value| value.pointer("/evidence/id"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&subject_key);
        let before =
            workflow_review_value_on(connection, evidence_kind, evidence_id, finished_at_ms)?;
        let before_json = serde_json::to_string(&before)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        Some((
            AiReviewKind::WorkflowAssignment,
            evidence_id.to_string(),
            before_json,
            format!("{evidence_kind} evidence {evidence_id}"),
        ))
    } else {
        None
    };
    let Some((kind, subject_id, before_json, evidence_summary)) = review_input else {
        return Ok(false);
    };
    let execution = execution_audit_for_job_on(
        connection,
        id,
        generation,
        executor_id,
        model,
        Some(finished_at_ms),
        None,
        exit_code,
        Some(error_kind),
        error,
    )?;
    insert_generated_ai_review_on(
        connection,
        &AiReviewDraft {
            id: format!("review-{id}-{generation}-{review_suffix}"),
            job_id: Some(id.into()),
            kind,
            subject_id,
            before_json,
            proposed_json: String::new(),
            confidence: None,
            evidence_summary,
            evidence_hash: execution.evidence_hash.clone(),
            execution,
            created_at_ms: finished_at_ms,
            execution_error: Some(sanitize_ai_error(error)),
        },
        true,
    )?;
    Ok(true)
}

fn insert_manual_override_review_on(
    connection: &Connection,
    kind: AiReviewKind,
    subject_id: &str,
    before_json: &str,
    applied_json: &str,
    evidence_summary: &str,
    reason: &str,
    created_at_ms: i64,
) -> Result<()> {
    let hash = format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}\n{subject_id}\n{created_at_ms}\n{applied_json}",
                kind.as_database()
            )
            .as_bytes()
        )
    );
    let id = format!("review-manual-{}", &hash[..24]);
    connection.execute(
        "INSERT INTO ai_review_records(
            id, job_id, kind, state, subject_id, before_json, proposed_json, applied_json,
            confidence, evidence_summary, evidence_hash, execution_mode,
            execution_executor_id, execution_model, execution_evidence_hash, generation,
            execution_created_at_ms, started_at_ms, finished_at_ms, duration_ms, exit_code,
            error_kind, diagnostic, created_at_ms, resolved_at_ms
         ) VALUES (
            ?1, NULL, ?2, 'manual_override', ?3, ?4, ?5, ?5, 1.0, ?6, '', NULL,
            NULL, NULL, '', 0, ?7, NULL, ?7, 0, NULL, NULL, ?8, ?7, ?7
         )",
        params![
            id,
            kind.as_database(),
            subject_id,
            before_json,
            applied_json,
            evidence_summary,
            created_at_ms,
            reason,
        ],
    )?;
    append_ai_review_event_on(connection, &id, AiReviewEventKind::Changed, created_at_ms)?;
    claim_manual_field_ownership_on(connection, kind, subject_id, "user", created_at_ms)
}

impl Database {
    pub fn create_ai_review(&self, draft: &AiReviewDraft) -> Result<AiReviewRecord> {
        if draft.id.trim().is_empty() || draft.subject_id.trim().is_empty() {
            return Err(invalid_review("Review id and subject id are required"));
        }
        if draft
            .confidence
            .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            return Err(invalid_review("Review confidence is out of range"));
        }
        let is_error = draft.execution_error.is_some() || draft.execution.error_kind.is_some();
        if is_error && !draft.proposed_json.is_empty() {
            return Err(invalid_review(
                "Execution errors cannot contain a suggestion",
            ));
        }
        if !is_error {
            match draft.kind {
                AiReviewKind::Classification => {
                    parse_classification_review(&draft.before_json)?;
                    parse_classification_review(&draft.proposed_json)?;
                }
                AiReviewKind::WorkflowAssignment => {
                    parse_workflow_review(&draft.before_json)?;
                    parse_workflow_review(&draft.proposed_json)?;
                }
                AiReviewKind::ProjectDraft => {
                    serde_json::from_str::<serde_json::Value>(&draft.before_json).map_err(
                        |error| {
                            invalid_review(format!("Invalid project draft baseline JSON: {error}"))
                        },
                    )?;
                    serde_json::from_str::<ProjectDraftProposal>(&draft.proposed_json).map_err(
                        |error| {
                            invalid_review(format!("Invalid project draft review JSON: {error}"))
                        },
                    )?;
                }
            }
        }

        let transaction = ai_job_completion_transaction(&self.connection)?;
        let legacy_manual =
            formal_value_is_manually_owned_on(&transaction, draft.kind, &draft.subject_id)?;
        insert_generated_ai_review_on(&transaction, draft, legacy_manual)?;
        transaction.commit()?;
        self.get_ai_review(&draft.id)?
            .ok_or_else(|| invalid_review("Created review is missing"))
    }

    pub fn get_ai_review(&self, review_id: &str) -> Result<Option<AiReviewRecord>> {
        self.connection
            .query_row(
                &format!("SELECT {AI_REVIEW_COLUMNS} FROM ai_review_records WHERE id=?1"),
                [review_id],
                ai_review_from_row,
            )
            .optional()
    }

    pub fn list_ai_reviews(&self, filter: &AiReviewFilter) -> Result<Vec<AiReviewRecord>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT {AI_REVIEW_COLUMNS} FROM ai_review_records ORDER BY created_at_ms DESC, id"
        ))?;
        let reviews = statement
            .query_map([], ai_review_from_row)?
            .collect::<Result<Vec<_>>>()?;
        Ok(reviews
            .into_iter()
            .filter(|review| {
                (filter.states.is_empty() || filter.states.contains(&review.state))
                    && (filter.kinds.is_empty() || filter.kinds.contains(&review.kind))
                    && filter
                        .subject_id
                        .as_ref()
                        .is_none_or(|value| value == &review.subject_id)
                    && filter
                        .execution_mode
                        .is_none_or(|value| review.execution.execution_mode == Some(value))
                    && filter.executor_id.as_ref().is_none_or(|value| {
                        review.execution.executor_id.as_deref() == Some(value.as_str())
                    })
                    && filter.model.as_ref().is_none_or(|value| {
                        review.execution.model.as_deref() == Some(value.as_str())
                    })
                    && filter
                        .created_from_ms
                        .is_none_or(|value| review.created_at_ms >= value)
                    && filter
                        .created_to_ms
                        .is_none_or(|value| review.created_at_ms <= value)
                    && filter.min_confidence.is_none_or(|value| {
                        review
                            .confidence
                            .is_some_and(|confidence| confidence >= value)
                    })
                    && filter.max_confidence.is_none_or(|value| {
                        review
                            .confidence
                            .is_some_and(|confidence| confidence <= value)
                    })
            })
            .collect())
    }

    pub fn list_ai_review_events(&self, review_id: &str) -> Result<Vec<AiReviewEventKind>> {
        let mut statement = self
            .connection
            .prepare("SELECT event_kind FROM ai_review_events WHERE review_id=?1 ORDER BY id")?;
        statement
            .query_map([review_id], |row| {
                AiReviewEventKind::from_database(&row.get::<_, String>(0)?)
                    .ok_or(rusqlite::Error::InvalidQuery)
            })?
            .collect()
    }

    pub fn has_manual_field_ownership(&self, kind: AiReviewKind, subject_id: &str) -> Result<bool> {
        has_manual_field_ownership_on(&self.connection, kind, subject_id)
    }

    pub fn claim_manual_field_ownership(
        &self,
        kind: AiReviewKind,
        subject_id: &str,
        owner: &str,
        claimed_at_ms: i64,
    ) -> Result<()> {
        claim_manual_field_ownership_on(&self.connection, kind, subject_id, owner, claimed_at_ms)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn assign_manual_workflow_review(
        &self,
        task_id: &str,
        evidence_kind: &str,
        evidence_id: &str,
        confidence: f64,
        reason: &str,
        created_at_ms: i64,
    ) -> Result<bool> {
        self.change_manual_workflow_review(
            Some(task_id),
            None,
            evidence_kind,
            evidence_id,
            confidence,
            reason,
            created_at_ms,
        )
    }

    pub fn remove_manual_workflow_review(
        &self,
        task_id: &str,
        evidence_kind: &str,
        evidence_id: &str,
    ) -> Result<bool> {
        self.change_manual_workflow_review(
            None,
            Some(task_id),
            evidence_kind,
            evidence_id,
            1.0,
            "Removed by user",
            now_millis(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn change_manual_workflow_review(
        &self,
        task_id: Option<&str>,
        expected_current_task: Option<&str>,
        evidence_kind: &str,
        evidence_id: &str,
        confidence: f64,
        reason: &str,
        created_at_ms: i64,
    ) -> Result<bool> {
        let transaction = ai_job_completion_transaction(&self.connection)?;
        let before =
            workflow_review_value_on(&transaction, evidence_kind, evidence_id, created_at_ms)?;
        if expected_current_task.is_some_and(|expected| before.task_id.as_deref() != Some(expected))
        {
            transaction.rollback()?;
            return Ok(false);
        }
        let before_json = serde_json::to_string(&before)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let applied_json = serde_json::to_string(&AiWorkflowAssignmentReviewValue {
            evidence_kind: evidence_kind.into(),
            evidence_id: evidence_id.into(),
            task_id: task_id.map(str::to_owned),
            confidence: confidence.clamp(0.0, 1.0),
            reason: reason.into(),
            created_at_ms,
        })
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        if !apply_review_value_on(
            &transaction,
            AiReviewKind::WorkflowAssignment,
            evidence_id,
            &applied_json,
            true,
        )? {
            transaction.rollback()?;
            return Ok(false);
        }
        insert_manual_override_review_on(
            &transaction,
            AiReviewKind::WorkflowAssignment,
            evidence_id,
            &before_json,
            &applied_json,
            &format!("{evidence_kind} evidence {evidence_id}"),
            reason,
            created_at_ms,
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn accept_pending_work_ledger_ai_suggestion(
        &self,
        task_id: &str,
        evidence_kind: &str,
        evidence_id: &str,
        evidence_hash: &str,
        resolved_at_ms: i64,
    ) -> Result<bool> {
        let transaction = ai_job_completion_transaction(&self.connection)?;
        let suggestion = transaction
            .query_row(
                "SELECT review_id, evidence_hash, task_id
                 FROM work_ledger_ai_suggestions
                 WHERE evidence_kind=?1 AND evidence_id=?2",
                params![evidence_kind, evidence_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((review_id, stored_evidence_hash, stored_task_id)) = suggestion else {
            transaction.rollback()?;
            return Ok(false);
        };
        if stored_evidence_hash != evidence_hash || stored_task_id != task_id {
            transaction.rollback()?;
            return Ok(false);
        }
        Self::resolve_ai_review_on(
            &transaction,
            &AiReviewResolution {
                review_ids: vec![review_id.clone()],
                action: AiReviewAction::Accept,
                changed_json: None,
                evidence_hashes: BTreeMap::from([(review_id.clone(), evidence_hash.into())]),
                resolved_at_ms,
            },
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn resolve_ai_review(
        &self,
        resolution: &AiReviewResolution,
    ) -> Result<Vec<AiReviewRecord>> {
        let transaction = ai_job_completion_transaction(&self.connection)?;
        Self::resolve_ai_review_on(&transaction, resolution)?;
        transaction.commit()?;
        resolution
            .review_ids
            .iter()
            .map(|id| {
                self.get_ai_review(id)?
                    .ok_or_else(|| invalid_review("Resolved review is missing"))
            })
            .collect()
    }

    fn resolve_ai_review_on(
        connection: &Connection,
        resolution: &AiReviewResolution,
    ) -> Result<()> {
        if resolution.review_ids.is_empty() {
            return Err(invalid_review("At least one review is required"));
        }
        if resolution.review_ids.len() > 1 && resolution.action != AiReviewAction::Accept {
            return Err(invalid_review("Only acceptance can be batched"));
        }
        if resolution.action == AiReviewAction::Change && resolution.changed_json.is_none() {
            return Err(invalid_review("Changed review JSON is required"));
        }
        let mut unique_review_ids = BTreeSet::new();
        if resolution
            .review_ids
            .iter()
            .any(|review_id| !unique_review_ids.insert(review_id.as_str()))
        {
            return Err(invalid_review("Batch contains duplicate review ID"));
        }

        let mut records = Vec::with_capacity(resolution.review_ids.len());
        for review_id in &resolution.review_ids {
            let review = connection
                .query_row(
                    &format!("SELECT {AI_REVIEW_COLUMNS} FROM ai_review_records WHERE id=?1"),
                    [review_id],
                    ai_review_from_row,
                )
                .optional()?
                .ok_or_else(|| invalid_review(format!("Unknown review: {review_id}")))?;
            records.push(review);
        }
        let kind = records[0].kind;
        if records.iter().any(|review| review.kind != kind) {
            return Err(invalid_review("Batch reviews must have the same kind"));
        }
        let mut unique_subject_ids = BTreeSet::new();
        if records
            .iter()
            .any(|review| !unique_subject_ids.insert(review.subject_id.as_str()))
        {
            return Err(invalid_review("Batch reviews must have distinct subjects"));
        }
        for review in &records {
            if review.state != AiReviewState::Pending {
                return Err(invalid_review("Only pending reviews can be resolved"));
            }
            if resolution.evidence_hashes.get(&review.id) != Some(&review.evidence_hash) {
                return Err(invalid_review("Review evidence has changed"));
            }
            if resolution.action == AiReviewAction::Ignore {
                continue;
            }
            if review.kind == AiReviewKind::Classification
                && classification_evidence_hash_on(connection, &review.subject_id)?.as_deref()
                    != Some(review.evidence_hash.as_str())
            {
                return Err(invalid_review("Review evidence has changed"));
            }
            if review.kind == AiReviewKind::WorkflowAssignment
                && !workflow_evidence_hash_is_current_on(connection, review)?
            {
                return Err(invalid_review("Review evidence has changed"));
            }
            if review.kind == AiReviewKind::WorkflowAssignment
                && !workflow_suggestion_matches_review_on(connection, review)?
            {
                return Err(invalid_review("Workflow suggestion binding has changed"));
            }
            if review.kind == AiReviewKind::ProjectDraft
                && !project_draft_evidence_is_current_on(connection, &review.id)?
            {
                return Err(invalid_review("Project draft evidence has changed"));
            }
            if review.kind == AiReviewKind::ProjectDraft {
                let applied = if resolution.action == AiReviewAction::Change {
                    resolution.changed_json.as_deref().unwrap_or_default()
                } else {
                    &review.proposed_json
                };
                let before: serde_json::Value =
                    serde_json::from_str(&review.before_json).map_err(|error| {
                        invalid_review(format!("Invalid project draft baseline JSON: {error}"))
                    })?;
                let proposal: ProjectDraftProposal =
                    serde_json::from_str(applied).map_err(|error| {
                        invalid_review(format!("Invalid project draft review JSON: {error}"))
                    })?;
                if before
                    .get("targetProjectId")
                    .and_then(serde_json::Value::as_str)
                    != proposal.target_project_id.as_deref()
                {
                    return Err(invalid_review("Project draft target project changed"));
                }
            }
            if has_manual_field_ownership_on(connection, review.kind, &review.subject_id)? {
                return Err(invalid_review("Review field is manually owned"));
            }
            if !formal_value_matches_on(
                connection,
                review.kind,
                &review.subject_id,
                &review.before_json,
            )? {
                return Err(invalid_review("Review subject changed after generation"));
            }
        }

        for review in &records {
            match resolution.action {
                AiReviewAction::Ignore => {
                    connection.execute(
                        "UPDATE ai_review_records
                         SET state='dismissed', resolved_at_ms=?2 WHERE id=?1 AND state='pending'",
                        params![review.id, resolution.resolved_at_ms],
                    )?;
                    append_ai_review_event_on(
                        connection,
                        &review.id,
                        AiReviewEventKind::Ignored,
                        resolution.resolved_at_ms,
                    )?;
                }
                AiReviewAction::Accept | AiReviewAction::Change => {
                    let applied = if resolution.action == AiReviewAction::Change {
                        resolution.changed_json.as_deref().unwrap_or_default()
                    } else {
                        &review.proposed_json
                    };
                    if !apply_review_value_on(
                        connection,
                        review.kind,
                        &review.subject_id,
                        applied,
                        true,
                    )? {
                        return Err(invalid_review("Review subject no longer exists"));
                    }
                    claim_manual_field_ownership_on(
                        connection,
                        review.kind,
                        &review.subject_id,
                        "user",
                        resolution.resolved_at_ms,
                    )?;
                    connection.execute(
                        "UPDATE ai_review_records
                         SET state='manual_override', applied_json=?2, resolved_at_ms=?3
                         WHERE id=?1 AND state='pending'",
                        params![review.id, applied, resolution.resolved_at_ms],
                    )?;
                    append_ai_review_event_on(
                        connection,
                        &review.id,
                        if resolution.action == AiReviewAction::Change {
                            AiReviewEventKind::Changed
                        } else {
                            AiReviewEventKind::Accepted
                        },
                        resolution.resolved_at_ms,
                    )?;
                }
            }
            if review.kind == AiReviewKind::WorkflowAssignment {
                connection.execute(
                    "DELETE FROM work_ledger_ai_suggestions WHERE review_id=?1",
                    [&review.id],
                )?;
            }
            if review.kind == AiReviewKind::ProjectDraft {
                connection.execute(
                    "DELETE FROM project_draft_bindings WHERE review_id=?1",
                    [&review.id],
                )?;
            }
        }
        Ok(())
    }

    pub fn revert_ai_auto_apply(
        &self,
        review_id: &str,
        reverted_at_ms: i64,
    ) -> Result<AiReviewRecord> {
        let transaction = ai_job_completion_transaction(&self.connection)?;
        let review = transaction
            .query_row(
                &format!("SELECT {AI_REVIEW_COLUMNS} FROM ai_review_records WHERE id=?1"),
                [review_id],
                ai_review_from_row,
            )
            .optional()?
            .ok_or_else(|| invalid_review("Unknown review"))?;
        if review.state != AiReviewState::AutoApplied {
            return Err(invalid_review("Only auto-applied reviews can be reverted"));
        }
        let applied = review
            .applied_json
            .as_deref()
            .unwrap_or(&review.proposed_json);
        if !formal_value_matches_on(&transaction, review.kind, &review.subject_id, applied)? {
            return Err(invalid_review("Applied value has changed"));
        }
        if !apply_review_value_on(
            &transaction,
            review.kind,
            &review.subject_id,
            &review.before_json,
            true,
        )? {
            return Err(invalid_review("Review subject no longer exists"));
        }
        claim_manual_field_ownership_on(
            &transaction,
            review.kind,
            &review.subject_id,
            "revert",
            reverted_at_ms,
        )?;
        transaction.execute(
            "UPDATE ai_review_records SET state='reverted', resolved_at_ms=?2
             WHERE id=?1 AND state='auto_applied'",
            params![review_id, reverted_at_ms],
        )?;
        append_ai_review_event_on(
            &transaction,
            review_id,
            AiReviewEventKind::Reverted,
            reverted_at_ms,
        )?;
        transaction.commit()?;
        self.get_ai_review(review_id)?
            .ok_or_else(|| invalid_review("Reverted review is missing"))
    }

    pub fn retry_ai_review(
        &self,
        review_id: &str,
        current_execution: Option<&AiExecutionSnapshot>,
        retried_at_ms: i64,
    ) -> Result<bool> {
        let transaction = ai_job_completion_transaction(&self.connection)?;
        let review = transaction
            .query_row(
                &format!("SELECT {AI_REVIEW_COLUMNS} FROM ai_review_records WHERE id=?1"),
                [review_id],
                ai_review_from_row,
            )
            .optional()?
            .ok_or_else(|| invalid_review("Unknown review"))?;
        if review.state != AiReviewState::ExecutionError {
            return Err(invalid_review("Only execution errors can be retried"));
        }
        let (job_id, kind, subject_key, payload_json) = transaction.query_row(
            "SELECT r.job_id, j.kind, j.subject_key, j.payload_json
                 FROM ai_review_records r JOIN ai_jobs j ON j.id=r.job_id WHERE r.id=?1",
            [review_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )?;
        let execution = current_execution.cloned().unwrap_or(AiExecutionSnapshot {
            execution_mode: review
                .execution
                .execution_mode
                .ok_or_else(|| invalid_review("Manual reviews cannot be retried"))?,
            executor_id: review
                .execution
                .executor_id
                .clone()
                .ok_or_else(|| invalid_review("Manual reviews cannot be retried"))?,
            model: review
                .execution
                .model
                .clone()
                .ok_or_else(|| invalid_review("Manual reviews cannot be retried"))?,
            evidence_hash: review.execution.evidence_hash.clone(),
            created_at_ms: retried_at_ms,
        });
        let (new_job, inserted) = enqueue_ai_job_hashed_on(
            &transaction,
            &kind,
            &subject_key,
            &payload_json,
            retried_at_ms,
            true,
            &execution,
        )?;
        if !inserted || new_job == job_id {
            return Ok(false);
        }
        append_ai_review_event_on(
            &transaction,
            review_id,
            AiReviewEventKind::Retried,
            retried_at_ms,
        )?;
        transaction.commit()?;
        Ok(true)
    }
}
