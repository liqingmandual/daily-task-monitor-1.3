use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use directories::{ProjectDirs, UserDirs};
use keyring::Entry;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::menu::MenuBuilder;
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, State};
use url::Url;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetBitmapBits,
    GetDC, GetDIBits, GetObjectW, HGDIOBJ, ReleaseDC,
};
#[cfg(target_os = "windows")]
use windows::Win32::Storage::FileSystem::{
    FILE_FLAGS_AND_ATTRIBUTES, GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON, SHGetFileInfoW};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};
#[cfg(target_os = "windows")]
use windows::core::PCWSTR;

#[cfg(test)]
use crate::ai::TrendAnalysisAllowedCandidates;
use crate::ai::{
    AiExecutionErrorKind, AiExecutionMode, AiExecutionSnapshot, AiJob, AiJobStatus,
    AiProviderConfig, AiQueueRecord, TrendAnalysisAiResult, parse_trend_analysis_response,
    parse_work_ledger_decision_response, sanitize_ai_diagnostic, validate_provider_endpoint,
};
use crate::ai_executor::{
    AiExecutionBackends, AiExecutionError as CodexExecutionError,
    AiExecutionErrorKind as ExecutorErrorKind, AiExecutionOutput, AiExecutionRequest,
    CodexExecutionConfig, CodexHealth, CodexHealthStatus, execute_ai_validated, probe_codex_health,
    resolve_codex_executable, test_codex_inference,
};
use crate::ai_review::{AiReviewFilter, AiReviewRecord, AiReviewResolution};
use crate::app::{
    AppService, AppSettings, DailyAnalysisResult, DashboardSnapshot, EvidenceBasedFinding,
    SettingsPatch, TrendAnalysisJobPayload, TrendAnalysisResult, render_daily_markdown,
    render_trend_markdown_with_workbench, trend_analysis_allowed_candidates,
};
use crate::browser::{fetch_public_html_summary, redact_url_for_storage, scan_chromium_history};
use crate::browser_watcher::{BrowserHeartbeat, BrowserWatcherEngine, is_browser_application};
use crate::browser_watcher_server::{BROWSER_WATCHER_ENDPOINT, run_browser_watcher_server};
use crate::db::{
    ActivitySampleWrite, ActivitySegmentRecord, DailyGoalRecord, Database,
    MonitoringContinuityCheckpoint,
};
use crate::domain::{ActivityCategory, ActivityScope, TrendPayload, VideoPurpose};
use crate::edition::current_edition_identity;
use crate::knowledge_graph::{KnowledgeGraphFilters, KnowledgeGraphPayload};
use crate::legacy::{LegacyImportResult, import_activity_jsonl};
#[cfg(target_os = "macos")]
use crate::macos_collector::{
    MacOsCollector as PlatformCollector, screen_recording_permission_granted, system_uptime_ms,
};
use crate::monitor::MonitorEngine;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::monitor_continuity::{continuity_gap_segment, monitoring_gap_from_checkpoint};
use crate::report::{ReportBlock, ReportDocument, render_docx, render_markdown};
use crate::trend_analysis::{
    TrendResearchAnalysis, TrendResearchJobPayload, parse_trend_research_response,
    trend_research_protocol_prompt,
};
use crate::trends::{
    TrendSelectionMode, TrendWorkbenchError, TrendWorkbenchPayload, TrendWorkbenchRequest,
};
#[cfg(target_os = "windows")]
use crate::windows_collector::{WindowsCollector as PlatformCollector, system_uptime_ms};
use crate::work_ledger::commands::{
    DailyActualOutputProgressRequest, DailyGoalTaskConfirmationRequest, EvidenceAssignmentRequest,
    MergeTasksRequest, ProgressEntryRequest, ProjectSaveRequest, SuggestedAssignmentRequest,
    TaskSaveRequest, TaskStatusUpdateRequest,
    add_work_ledger_progress as command_add_work_ledger_progress,
    apply_work_ledger_suggestion as command_apply_work_ledger_suggestion,
    archive_work_ledger_project as command_archive_work_ledger_project,
    assign_work_ledger_evidence as command_assign_work_ledger_evidence,
    cancel_work_ledger_ai_task as command_cancel_work_ledger_ai_task,
    confirm_daily_goal_task as command_confirm_daily_goal_task,
    get_work_ledger as command_get_work_ledger,
    get_work_ledger_project_insight as command_get_work_ledger_project_insight,
    get_work_ledger_task_insight as command_get_work_ledger_task_insight,
    list_daily_goal_task_links as command_list_daily_goal_task_links,
    merge_work_ledger_tasks as command_merge_work_ledger_tasks,
    queue_workflow_ai_suggestions as command_queue_workflow_ai_suggestions,
    record_daily_actual_output_progress as command_record_daily_actual_output_progress,
    remove_work_ledger_evidence as command_remove_work_ledger_evidence,
    save_work_ledger_project as command_save_work_ledger_project,
    save_work_ledger_task as command_save_work_ledger_task,
    update_work_ledger_task_status as command_update_work_ledger_task_status,
};
use crate::work_ledger::{
    AiTaskCancellation, ConfirmedDailyGoalTask, DailyGoalTaskLink, ProgressEntry, Project,
    ProjectTimeInsight, Task, TaskTimeInsight, WorkLedgerAssignmentConsumptionError,
    WorkLedgerAssignmentJobPayload, WorkLedgerRepository, WorkLedgerService, WorkLedgerSnapshot,
    WorkflowAnalysisQueueResult, parse_work_ledger_assignment_job,
};

pub const AI_CONNECTION_HEALTH_CHANGED_EVENT: &str = "ai-connection-health-changed";
pub const WORKFLOW_CHANGED_EVENT: &str = "workflow-changed";
pub const ANALYSIS_CHANGED_EVENT: &str = "analysis-changed";
pub const ACTIVITY_CHANGED_EVENT: &str = "activity-changed";
pub const COLLECTION_HEALTH_CHANGED_EVENT: &str = "collection-health-changed";
const AI_CONNECTION_HEALTH_INTERVAL: Duration = Duration::from_secs(10);
const API_HEALTH_TIMEOUT: Duration = Duration::from_secs(8);
const CODEX_HEALTH_TIMEOUT_MS: u64 = 5_000;
const CODEX_TASK_TIMEOUT_MS: u64 = 180_000;
const AI_INFERENCE_VERIFICATION_TTL_MS: i64 = 30 * 60 * 1_000;
static APP_IDENTITY_CACHE: OnceLock<Mutex<HashMap<String, CachedExecutableIdentity>>> =
    OnceLock::new();

pub struct DesktopState {
    service: Mutex<AppService>,
    ai_connection_health: AiConnectionHealthServiceState,
    collection_runtime: Mutex<CollectionRuntimeState>,
    browser_watcher: BrowserWatcherServiceState,
}

#[derive(Debug, Default)]
struct CollectionRuntimeState {
    last_persisted_at_ms: Option<i64>,
    last_app_available_at_ms: Option<i64>,
    last_title_available_at_ms: Option<i64>,
    last_idle_read_at_ms: Option<i64>,
    last_continuity_gap_at_ms: Option<i64>,
    last_browser_history_scan_at_ms: Option<i64>,
}

#[derive(Debug, Default)]
struct BrowserWatcherRuntimeState {
    engine: BrowserWatcherEngine,
    listener_ready: bool,
    listener_error: Option<String>,
    last_heartbeat_at_ms: Option<i64>,
    last_persisted_at_ms: Option<i64>,
    connected_sources: HashMap<String, i64>,
}

struct BrowserWatcherServiceState {
    token: String,
    runtime: Mutex<BrowserWatcherRuntimeState>,
}

impl BrowserWatcherServiceState {
    fn new(token: String) -> Self {
        Self {
            token,
            runtime: Mutex::new(BrowserWatcherRuntimeState::default()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum CollectionChannelStatus {
    Healthy,
    Degraded,
    Paused,
    Unavailable,
    PermissionDenied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct CollectionChannelHealth {
    status: CollectionChannelStatus,
    last_success_at_ms: Option<i64>,
    detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct CollectionHealth {
    generated_at_ms: i64,
    platform: String,
    monitoring_enabled: bool,
    desktop: CollectionChannelHealth,
    window_title: CollectionChannelHealth,
    idle: CollectionChannelHealth,
    continuity: CollectionChannelHealth,
    screen_recording: CollectionChannelHealth,
    browser_watcher: CollectionChannelHealth,
    browser_history: CollectionChannelHealth,
    watcher_endpoint: String,
    watcher_token: String,
    watcher_source_count: usize,
    measured_browser_slice_count: i64,
    measured_browser_seconds: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiConnectionHealthStatus {
    Checking,
    Healthy,
    Reachable,
    RateLimited,
    Unconfigured,
    Unavailable,
    PermissionDenied,
    TimedOut,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiConnectionVerificationLevel {
    Connectivity,
    Inference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiConnectionHealthSource {
    Background,
    Manual,
    Queue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConnectionHealth {
    pub execution_mode: AiExecutionMode,
    pub executor_id: String,
    pub executor_label: String,
    pub model: String,
    pub status: AiConnectionHealthStatus,
    pub verification_level: AiConnectionVerificationLevel,
    pub source: AiConnectionHealthSource,
    pub checked_at_ms: i64,
    pub verified_at_ms: Option<i64>,
    pub diagnostic: Option<String>,
}

impl AiConnectionHealth {
    fn initial() -> Self {
        Self {
            execution_mode: AiExecutionMode::ApiKey,
            executor_id: String::new(),
            executor_label: "API provider".into(),
            model: String::new(),
            status: AiConnectionHealthStatus::Checking,
            verification_level: AiConnectionVerificationLevel::Connectivity,
            source: AiConnectionHealthSource::Background,
            checked_at_ms: now_ms(),
            verified_at_ms: None,
            diagnostic: None,
        }
    }

    fn sanitized(mut self) -> Self {
        self.diagnostic = self
            .diagnostic
            .as_deref()
            .map(sanitize_ai_diagnostic)
            .filter(|diagnostic| !diagnostic.is_empty());
        self
    }
}

struct AiConnectionHealthServiceState {
    latest: Mutex<AiConnectionHealth>,
    probe_coordinator: HealthProbeCoordinator,
}

impl AiConnectionHealthServiceState {
    fn new(initial: AiConnectionHealth) -> Self {
        Self {
            latest: Mutex::new(initial.sanitized()),
            probe_coordinator: HealthProbeCoordinator::default(),
        }
    }

    fn current(&self) -> AiConnectionHealth {
        self.latest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn store(&self, health: AiConnectionHealth) -> AiConnectionHealth {
        let mut health = health.sanitized();
        let mut latest = self
            .latest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let same_executor = latest.execution_mode == health.execution_mode
            && latest.executor_id == health.executor_id
            && latest.model == health.model;
        let inference_is_fresh = latest.verified_at_ms.is_some_and(|verified_at_ms| {
            health.checked_at_ms.saturating_sub(verified_at_ms) < AI_INFERENCE_VERIFICATION_TTL_MS
        });
        if same_executor
            && health.source == AiConnectionHealthSource::Background
            && health.verification_level == AiConnectionVerificationLevel::Connectivity
            && matches!(
                health.status,
                AiConnectionHealthStatus::Checking | AiConnectionHealthStatus::Reachable
            )
            && latest.verification_level == AiConnectionVerificationLevel::Inference
            && inference_is_fresh
        {
            health.status = latest.status;
            health.verification_level = AiConnectionVerificationLevel::Inference;
            health.source = latest.source;
            health.verified_at_ms = latest.verified_at_ms;
            health.diagnostic = latest.diagnostic.clone();
        }
        *latest = health.clone();
        health
    }

    fn request_probe(&self) -> bool {
        self.probe_coordinator.request_probe()
    }

    fn finish_probe(&self) -> bool {
        self.probe_coordinator.finish_probe()
    }
}

#[derive(Default)]
struct HealthProbeCoordinator {
    state: Mutex<HealthProbeCoordinatorState>,
}

#[derive(Default)]
struct HealthProbeCoordinatorState {
    running: bool,
    pending: bool,
    rerunning: bool,
}

impl HealthProbeCoordinator {
    fn request_probe(&self) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.running {
            if !state.rerunning {
                state.pending = true;
            }
            false
        } else {
            state.running = true;
            true
        }
    }

    fn finish_probe(&self) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.pending {
            state.pending = false;
            state.rerunning = true;
            true
        } else {
            state.running = false;
            state.rerunning = false;
            false
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSource {
    pub browser: String,
    pub profile: String,
    pub history_path: String,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserScanResult {
    pub scanned_sources: usize,
    pub visits_found: usize,
    pub errors: Vec<String>,
    #[serde(skip)]
    new_visits: Vec<(String, String, String, String)>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppIdentityRequest {
    pub raw_name: String,
    #[serde(default)]
    pub executable_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppIdentityDto {
    pub raw_name: String,
    pub display_name: String,
    pub executable_path: String,
    pub product_name: String,
    pub icon_data_url: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct CachedExecutableIdentity {
    product_name: String,
    icon_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiClassificationResult {
    category: ActivityCategory,
    #[serde(default = "unknown_video_purpose")]
    video_purpose: VideoPurpose,
    confidence: f32,
    reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiAutomationGate {
    TrendAnalysis,
    Classification,
    WorkflowAssignment,
}

pub fn background_ai_gate_enabled(settings: &AppSettings, gate: AiAutomationGate) -> bool {
    match gate {
        AiAutomationGate::TrendAnalysis => settings.ai_auto_trend_analysis_enabled,
        AiAutomationGate::Classification => settings.ai_auto_classification_enabled,
        AiAutomationGate::WorkflowAssignment => settings.ai_auto_workflow_assignment_enabled,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct DailyAnalysisAiResult {
    portrait: String,
    recommendation: String,
    findings: Vec<EvidenceBasedFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CustomProviderConfig {
    base_url: String,
    model: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualClassificationRequest {
    pub segment_id: String,
    pub category: ActivityCategory,
    #[serde(default = "unknown_video_purpose")]
    pub video_purpose: VideoPurpose,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ReportFormat {
    Markdown,
    Docx,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ReportScope {
    Daily,
    Trend,
    Task,
    Project,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportReportRequest {
    format: ReportFormat,
    scope: ReportScope,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    start_ms: Option<i64>,
    #[serde(default)]
    end_ms: Option<i64>,
    #[serde(default)]
    start_date: Option<String>,
    #[serde(default)]
    end_date: Option<String>,
    #[serde(default)]
    day_boundaries_ms: Vec<i64>,
    #[serde(default)]
    comparison_start_date: Option<String>,
    #[serde(default)]
    comparison_end_date: Option<String>,
    #[serde(default)]
    comparison_day_boundaries_ms: Vec<i64>,
    #[serde(default)]
    trend_request: Option<TrendWorkbenchRequest>,
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

fn unknown_video_purpose() -> VideoPurpose {
    VideoPurpose::Unknown
}

pub(crate) fn state_service<'a>(
    state: &'a State<'a, DesktopState>,
) -> Result<MutexGuard<'a, AppService>, String> {
    state
        .service
        .lock()
        .map_err(|_| "Application state is unavailable".into())
}

#[tauri::command]
fn get_work_ledger(
    state: State<'_, DesktopState>,
    start_ms: i64,
    end_ms: i64,
    project_id: Option<String>,
) -> Result<WorkLedgerSnapshot, String> {
    command_get_work_ledger(state, start_ms, end_ms, project_id)
}

#[tauri::command]
fn get_work_ledger_task_insight(
    state: State<'_, DesktopState>,
    task_id: String,
    end_ms: i64,
) -> Result<TaskTimeInsight, String> {
    command_get_work_ledger_task_insight(state, task_id, end_ms)
}

#[tauri::command]
fn get_work_ledger_project_insight(
    state: State<'_, DesktopState>,
    project_id: String,
    end_ms: i64,
    range_days: i64,
) -> Result<ProjectTimeInsight, String> {
    command_get_work_ledger_project_insight(state, project_id, end_ms, range_days)
}

#[tauri::command]
fn merge_work_ledger_tasks(
    state: State<'_, DesktopState>,
    request: MergeTasksRequest,
) -> Result<bool, String> {
    command_merge_work_ledger_tasks(state, request)
}

#[tauri::command]
fn queue_workflow_ai_suggestions(
    state: State<'_, DesktopState>,
    start_ms: i64,
    end_ms: i64,
) -> Result<WorkflowAnalysisQueueResult, String> {
    command_queue_workflow_ai_suggestions(state, start_ms, end_ms)
}

#[tauri::command]
fn queue_segment_classification(
    state: State<'_, DesktopState>,
    segment_id: String,
) -> Result<String, String> {
    let service = state_service(&state)?;
    let settings = service.get_settings().map_err(|error| error.to_string())?;
    let segment = service
        .database()
        .get_segment(&segment_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("Activity segment not found: {segment_id}"))?;
    let queued_at_ms = now_ms();
    let execution = current_ai_execution_snapshot(&service, "", queued_at_ms)?;
    enqueue_segment_classification_job_with_privacy(
        service.database(),
        &segment,
        &settings.excluded_apps,
        &execution,
        queued_at_ms,
    )
}

#[tauri::command]
fn save_work_ledger_project(
    state: State<'_, DesktopState>,
    request: ProjectSaveRequest,
) -> Result<Project, String> {
    command_save_work_ledger_project(state, request)
}

#[tauri::command]
fn archive_work_ledger_project(
    state: State<'_, DesktopState>,
    project_id: String,
) -> Result<bool, String> {
    command_archive_work_ledger_project(state, project_id)
}

#[tauri::command]
fn save_work_ledger_task(
    state: State<'_, DesktopState>,
    request: TaskSaveRequest,
) -> Result<Task, String> {
    command_save_work_ledger_task(state, request)
}

#[tauri::command]
fn update_work_ledger_task_status(
    state: State<'_, DesktopState>,
    request: TaskStatusUpdateRequest,
) -> Result<Task, String> {
    command_update_work_ledger_task_status(state, request)
}

#[tauri::command]
fn cancel_work_ledger_ai_task(
    state: State<'_, DesktopState>,
    task_id: String,
) -> Result<AiTaskCancellation, String> {
    command_cancel_work_ledger_ai_task(state, task_id)
}

#[tauri::command]
fn add_work_ledger_progress(
    state: State<'_, DesktopState>,
    request: ProgressEntryRequest,
) -> Result<ProgressEntry, String> {
    command_add_work_ledger_progress(state, request)
}

#[tauri::command]
fn assign_work_ledger_evidence(
    state: State<'_, DesktopState>,
    request: EvidenceAssignmentRequest,
) -> Result<bool, String> {
    command_assign_work_ledger_evidence(state, request)
}

#[tauri::command]
fn remove_work_ledger_evidence(
    state: State<'_, DesktopState>,
    request: EvidenceAssignmentRequest,
) -> Result<bool, String> {
    command_remove_work_ledger_evidence(state, request)
}

#[tauri::command]
fn apply_work_ledger_suggestion(
    state: State<'_, DesktopState>,
    request: SuggestedAssignmentRequest,
) -> Result<bool, String> {
    command_apply_work_ledger_suggestion(state, request)
}

#[tauri::command]
fn confirm_daily_goal_task(
    state: State<'_, DesktopState>,
    request: DailyGoalTaskConfirmationRequest,
) -> Result<ConfirmedDailyGoalTask, String> {
    command_confirm_daily_goal_task(state, request)
}

#[tauri::command]
fn list_daily_goal_task_links(
    state: State<'_, DesktopState>,
    date: String,
) -> Result<Vec<DailyGoalTaskLink>, String> {
    command_list_daily_goal_task_links(state, date)
}

#[tauri::command]
fn record_daily_actual_output_progress(
    state: State<'_, DesktopState>,
    request: DailyActualOutputProgressRequest,
) -> Result<Option<ProgressEntry>, String> {
    command_record_daily_actual_output_progress(state, request)
}

#[tauri::command]
fn get_today_dashboard(
    state: State<'_, DesktopState>,
    start_ms: i64,
    end_ms: i64,
) -> Result<DashboardSnapshot, String> {
    state_service(&state)?
        .get_dashboard(start_ms, end_ms)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn resolve_app_identities(apps: Vec<AppIdentityRequest>) -> Vec<AppIdentityDto> {
    let mut seen = HashSet::new();
    apps.into_iter()
        .filter_map(|app| {
            let raw_name = app.raw_name.trim().to_string();
            let executable_path = app.executable_path.trim().to_string();
            let request_key = format!(
                "{}\n{}",
                raw_name.to_lowercase(),
                normalize_executable_path(&executable_path)
            );
            if raw_name.is_empty() || !seen.insert(request_key) {
                return None;
            }

            let cached = resolve_executable_identity(&executable_path);
            Some(AppIdentityDto {
                display_name: canonical_display_name(
                    &raw_name,
                    &cached.product_name,
                    &executable_path,
                ),
                raw_name,
                executable_path,
                product_name: cached.product_name,
                icon_data_url: cached.icon_data_url,
            })
        })
        .collect()
}

fn resolve_executable_identity(executable_path: &str) -> CachedExecutableIdentity {
    let cache_key = normalize_executable_path(executable_path);
    if cache_key.is_empty() {
        return CachedExecutableIdentity::default();
    }
    let cache = APP_IDENTITY_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(values) = cache.lock()
        && let Some(identity) = values.get(&cache_key)
    {
        return identity.clone();
    }

    let identity = CachedExecutableIdentity {
        product_name: executable_product_name(executable_path).unwrap_or_default(),
        icon_data_url: executable_icon_data_url(executable_path, &cache_key),
    };
    if let Ok(mut values) = cache.lock() {
        values.insert(cache_key, identity.clone());
    }
    identity
}

fn normalize_executable_path(value: &str) -> String {
    value.trim().replace('/', "\\").to_lowercase()
}

fn canonical_display_name(raw_name: &str, product_name: &str, executable_path: &str) -> String {
    let raw = raw_name.trim();
    let path = normalize_executable_path(executable_path);
    if raw.eq_ignore_ascii_case("ChatGPT")
        && path.contains("\\windowsapps\\openai.codex_")
        && path.ends_with("\\app\\chatgpt.exe")
    {
        return "ChatGPT".into();
    }
    let product = product_name.trim();
    if !product.is_empty() {
        return product.into();
    }
    match raw.to_ascii_lowercase().as_str() {
        "msedge" => "Microsoft Edge".into(),
        "explorer" => "File Explorer".into(),
        _ => raw.into(),
    }
}

#[cfg(target_os = "windows")]
fn executable_product_name(executable_path: &str) -> Option<String> {
    let path = wide_string(executable_path);
    let size = unsafe { GetFileVersionInfoSizeW(PCWSTR(path.as_ptr()), None) };
    if size == 0 {
        return None;
    }
    let mut data = vec![0_u8; size as usize];
    unsafe {
        GetFileVersionInfoW(PCWSTR(path.as_ptr()), None, size, data.as_mut_ptr().cast()).ok()?;
    }

    let mut translations = Vec::new();
    let mut pointer = std::ptr::null_mut();
    let mut length = 0_u32;
    let translation_query = wide_string(r"\VarFileInfo\Translation");
    if unsafe {
        VerQueryValueW(
            data.as_ptr().cast(),
            PCWSTR(translation_query.as_ptr()),
            &mut pointer,
            &mut length,
        )
        .as_bool()
    } && !pointer.is_null()
    {
        let values =
            unsafe { std::slice::from_raw_parts(pointer.cast::<u16>(), length as usize / 2) };
        translations.extend(values.chunks_exact(2).map(|pair| (pair[0], pair[1])));
    }
    translations.extend([(0x0409, 0x04b0), (0x0000, 0x04b0)]);
    translations.into_iter().find_map(|(language, code_page)| {
        version_string(
            &data,
            &format!(r"\StringFileInfo\{language:04x}{code_page:04x}\ProductName"),
        )
    })
}

#[cfg(not(target_os = "windows"))]
fn executable_product_name(_executable_path: &str) -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
fn version_string(data: &[u8], query: &str) -> Option<String> {
    let query = wide_string(query);
    let mut pointer = std::ptr::null_mut();
    let mut length = 0_u32;
    if !unsafe {
        VerQueryValueW(
            data.as_ptr().cast(),
            PCWSTR(query.as_ptr()),
            &mut pointer,
            &mut length,
        )
        .as_bool()
    } || pointer.is_null()
        || length == 0
    {
        return None;
    }
    let value = unsafe { std::slice::from_raw_parts(pointer.cast::<u16>(), length as usize) };
    let value = String::from_utf16_lossy(value)
        .trim_matches('\0')
        .trim()
        .to_string();
    (!value.is_empty()).then_some(value)
}

fn executable_icon_data_url(executable_path: &str, cache_key: &str) -> Option<String> {
    let cache_dir = icon_cache_dir();
    let hash = format!("{:x}", Sha256::digest(cache_key.as_bytes()));
    let cache_path = cache_dir.join(format!("{hash}.png"));
    let bytes = fs::read(&cache_path)
        .ok()
        .filter(|bytes| is_png(bytes))
        .or_else(|| {
            let bytes = extract_executable_icon(executable_path)?;
            if fs::create_dir_all(&cache_dir).is_ok() {
                let _ = fs::write(&cache_path, &bytes);
            }
            Some(bytes)
        })?;
    Some(png_data_url(&bytes))
}

fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10])
}

fn png_data_url(bytes: &[u8]) -> String {
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

#[cfg(target_os = "windows")]
fn extract_executable_icon(executable_path: &str) -> Option<Vec<u8>> {
    if executable_path.is_empty() || !Path::new(executable_path).is_file() {
        return None;
    }
    let path = wide_string(executable_path);
    let mut file_info = SHFILEINFOW::default();
    let found = unsafe {
        SHGetFileInfoW(
            PCWSTR(path.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut file_info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_SMALLICON,
        )
    };
    if found == 0 || file_info.hIcon.0.is_null() {
        return None;
    }
    let bytes = unsafe { icon_to_png(file_info.hIcon) };
    let _ = unsafe { DestroyIcon(file_info.hIcon) };
    bytes
}

#[cfg(not(target_os = "windows"))]
fn extract_executable_icon(_executable_path: &str) -> Option<Vec<u8>> {
    None
}

#[cfg(target_os = "windows")]
unsafe fn icon_to_png(icon: windows::Win32::UI::WindowsAndMessaging::HICON) -> Option<Vec<u8>> {
    let mut icon_info = ICONINFO::default();
    unsafe { GetIconInfo(icon, &mut icon_info).ok()? };
    let result = (|| {
        if icon_info.hbmColor.0.is_null() {
            return None;
        }
        let mut bitmap = BITMAP::default();
        if unsafe {
            GetObjectW(
                HGDIOBJ(icon_info.hbmColor.0),
                std::mem::size_of::<BITMAP>() as i32,
                Some((&mut bitmap as *mut BITMAP).cast()),
            )
        } == 0
        {
            return None;
        }
        let width = bitmap.bmWidth.unsigned_abs() as usize;
        let height = bitmap.bmHeight.unsigned_abs() as usize;
        if width == 0 || height == 0 || width > 256 || height > 256 {
            return None;
        }

        let mut pixels = vec![0_u8; width * height * 4];
        let mut bitmap_info = BITMAPINFO::default();
        bitmap_info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: height as i32,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: pixels.len() as u32,
            ..BITMAPINFOHEADER::default()
        };
        let hdc = unsafe { GetDC(None) };
        if hdc.0.is_null() {
            return None;
        }
        let copied = unsafe {
            GetDIBits(
                hdc,
                icon_info.hbmColor,
                0,
                height as u32,
                Some(pixels.as_mut_ptr().cast()),
                &mut bitmap_info,
                DIB_RGB_COLORS,
            )
        };
        unsafe { ReleaseDC(None, hdc) };
        if copied != height as i32 {
            return None;
        }

        let mut mask_bitmap = BITMAP::default();
        let (mask, mask_stride) = if !icon_info.hbmMask.0.is_null()
            && unsafe {
                GetObjectW(
                    HGDIOBJ(icon_info.hbmMask.0),
                    std::mem::size_of::<BITMAP>() as i32,
                    Some((&mut mask_bitmap as *mut BITMAP).cast()),
                )
            } != 0
        {
            let stride = mask_bitmap.bmWidthBytes.unsigned_abs() as usize;
            let rows = mask_bitmap.bmHeight.unsigned_abs() as usize;
            let mut mask = vec![0_u8; stride.saturating_mul(rows)];
            let copied = unsafe {
                GetBitmapBits(
                    icon_info.hbmMask,
                    mask.len().min(i32::MAX as usize) as i32,
                    mask.as_mut_ptr().cast(),
                )
            };
            if copied <= 0 {
                (Vec::new(), 0)
            } else {
                mask.truncate(copied as usize);
                (mask, stride)
            }
        } else {
            (Vec::new(), 0)
        };
        let rgba = rgba_from_bgra_and_mask(width, height, &pixels, &mask, mask_stride)?;
        encode_png(width, height, &rgba)
    })();
    if !icon_info.hbmColor.0.is_null() {
        let _ = unsafe { DeleteObject(HGDIOBJ(icon_info.hbmColor.0)) };
    }
    if !icon_info.hbmMask.0.is_null() {
        let _ = unsafe { DeleteObject(HGDIOBJ(icon_info.hbmMask.0)) };
    }
    result
}

fn rgba_from_bgra_and_mask(
    width: usize,
    height: usize,
    pixels: &[u8],
    mask: &[u8],
    mask_stride: usize,
) -> Option<Vec<u8>> {
    if pixels.len() != width.checked_mul(height)?.checked_mul(4)? {
        return None;
    }
    let has_alpha = pixels.chunks_exact(4).any(|pixel| pixel[3] != 0);
    let has_mask = mask_stride > 0 && mask.len() >= mask_stride.checked_mul(height)?;
    let mut rgba = vec![0_u8; pixels.len()];
    for target_y in 0..height {
        let source_y = height - 1 - target_y;
        for x in 0..width {
            let source = (source_y * width + x) * 4;
            let target = (target_y * width + x) * 4;
            rgba[target] = pixels[source + 2];
            rgba[target + 1] = pixels[source + 1];
            rgba[target + 2] = pixels[source];
            let masked = has_mask && mask[source_y * mask_stride + x / 8] & (0x80 >> (x % 8)) != 0;
            rgba[target + 3] = if has_alpha {
                pixels[source + 3]
            } else if masked {
                0
            } else {
                255
            };
        }
    }
    Some(rgba)
}

fn encode_png(width: usize, height: usize, rgba: &[u8]) -> Option<Vec<u8>> {
    if rgba.len() != width.checked_mul(height)?.checked_mul(4)? {
        return None;
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(bytes)
}

#[cfg(target_os = "windows")]
fn wide_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[tauri::command]
fn get_timeline(
    state: State<'_, DesktopState>,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<ActivitySegmentRecord>, String> {
    state_service(&state)?
        .get_timeline(start_ms, end_ms)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_trends(
    state: State<'_, DesktopState>,
    start_date: String,
    end_date: String,
    day_boundaries_ms: Vec<i64>,
    comparison_start_date: String,
    comparison_end_date: String,
    comparison_day_boundaries_ms: Vec<i64>,
    timezone_offset_minutes: i32,
) -> Result<TrendPayload, String> {
    let _ = timezone_offset_minutes;
    state_service(&state)?
        .get_trends(
            &start_date,
            &end_date,
            day_boundaries_ms,
            &comparison_start_date,
            &comparison_end_date,
            comparison_day_boundaries_ms,
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_trend_workbench(
    state: State<'_, DesktopState>,
    request: TrendWorkbenchRequest,
    activity_scope: Option<ActivityScope>,
) -> Result<TrendWorkbenchPayload, TrendWorkbenchError> {
    state_service(&state)
        .map_err(|_| TrendWorkbenchError::runtime_unavailable())?
        .get_trend_workbench_scoped(request, activity_scope.unwrap_or_default())
}

#[tauri::command]
fn get_workflow(
    state: State<'_, DesktopState>,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<ActivitySegmentRecord>, String> {
    state_service(&state)?
        .get_timeline(start_ms, end_ms)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_knowledge_graph(
    state: State<'_, DesktopState>,
    start_ms: i64,
    end_ms: i64,
    filters: KnowledgeGraphFilters,
) -> Result<KnowledgeGraphPayload, String> {
    state_service(&state)?
        .get_knowledge_graph(start_ms, end_ms, filters)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_settings(state: State<'_, DesktopState>) -> Result<AppSettings, String> {
    state_service(&state)?
        .get_settings()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_system_fonts() -> Result<Vec<String>, String> {
    let output = if cfg!(target_os = "windows") {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); (Get-Item 'HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Fonts').Property",
            ])
            .output()
    } else if cfg!(target_os = "macos") {
        Command::new("system_profiler")
            .args(["SPFontsDataType", "-json", "-detailLevel", "mini"])
            .output()
    } else {
        Command::new("fc-list").args([":", "family"]).output()
    }
    .map_err(|error| format!("无法读取系统字体：{error}"))?;
    if !output.status.success() {
        return Err("系统字体枚举命令执行失败".to_string());
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let mut fonts = HashSet::new();
    if cfg!(target_os = "macos") {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
            collect_font_families(&value, &mut fonts);
        }
    } else if cfg!(target_os = "windows") {
        for line in raw.lines() {
            insert_font_family(line, &mut fonts);
        }
    } else {
        for family_group in raw.lines() {
            for family in family_group.split(',') {
                insert_font_family(family, &mut fonts);
            }
        }
    }
    fonts.insert("Ubuntu".to_string());
    let mut fonts = fonts.into_iter().collect::<Vec<_>>();
    fonts.sort_by_key(|font| font.to_lowercase());
    Ok(fonts)
}

fn insert_font_family(value: &str, fonts: &mut HashSet<String>) {
    let raw_name = value
        .split(" (")
        .next()
        .unwrap_or(value)
        .trim()
        .trim_matches('"');
    let lowercase = raw_name.to_lowercase();
    let name = [".ttf", ".tff", ".otf", ".ttc", ".dfont"]
        .iter()
        .find(|extension| lowercase.ends_with(*extension))
        .map(|extension| &raw_name[..raw_name.len() - extension.len()])
        .unwrap_or(raw_name)
        .trim();
    if !name.is_empty()
        && !name.starts_with('.')
        && name.len() <= 120
        && !name.chars().any(char::is_control)
    {
        fonts.insert(name.to_string());
    }
}

fn collect_font_families(value: &serde_json::Value, fonts: &mut HashSet<String>) {
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                if matches!(key.as_str(), "family" | "family_name" | "_name") {
                    if let Some(name) = value.as_str() {
                        insert_font_family(name, fonts);
                    }
                }
                collect_font_families(value, fonts);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                collect_font_families(value, fonts);
            }
        }
        _ => {}
    }
}

#[tauri::command]
fn list_ai_reviews(
    state: State<'_, DesktopState>,
    filter: AiReviewFilter,
) -> Result<Vec<AiReviewRecord>, String> {
    state_service(&state)?
        .database()
        .list_ai_reviews(&filter)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_pending_ai_jobs(
    state: State<'_, DesktopState>,
    status: Option<AiJobStatus>,
    limit: Option<u32>,
) -> Result<Vec<AiQueueRecord>, String> {
    state_service(&state)?
        .database()
        .list_pending_ai_jobs(status, limit.unwrap_or(500))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn bulk_retry_ai_jobs_with_current_mode(
    state: State<'_, DesktopState>,
    job_ids: Vec<String>,
) -> Result<Vec<AiQueueRecord>, String> {
    let service = state_service(&state)?;
    let now = now_ms();
    let mut validated_jobs = Vec::with_capacity(job_ids.len());
    for job_id in &job_ids {
        let job = service
            .database()
            .get_ai_job(job_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Unknown AI job: {job_id}"))?;
        let evidence_hash = service
            .ai_job_reassignment_evidence_hash(&job)?
            .ok_or_else(|| format!("AI job {job_id} is no longer valid for reassignment"))?;
        validated_jobs.push((job_id.clone(), evidence_hash));
    }
    let execution = current_ai_execution_snapshot(&service, "", now)?;
    service
        .database()
        .bulk_retry_ai_jobs_with_current_mode(&validated_jobs, &execution, now)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn resolve_ai_review(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
    request: AiReviewResolution,
) -> Result<Vec<AiReviewRecord>, String> {
    let resolved = state_service(&state)?
        .database()
        .resolve_ai_review(&request)
        .map_err(|error| error.to_string())?;
    let _ = app.emit(
        WORKFLOW_CHANGED_EVENT,
        serde_json::json!({ "status": "review_resolved" }),
    );
    Ok(resolved)
}

#[tauri::command]
fn revert_ai_auto_apply(
    state: State<'_, DesktopState>,
    review_id: String,
) -> Result<AiReviewRecord, String> {
    state_service(&state)?
        .database()
        .revert_ai_auto_apply(&review_id, now_ms())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn retry_ai_review(
    state: State<'_, DesktopState>,
    review_id: String,
    use_current_mode: bool,
) -> Result<bool, String> {
    let service = state_service(&state)?;
    let now = now_ms();
    let current = if use_current_mode {
        let review = service
            .database()
            .get_ai_review(&review_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Unknown AI review".to_string())?;
        Some(current_ai_execution_snapshot(
            &service,
            &review.evidence_hash,
            now,
        )?)
    } else {
        None
    };
    service
        .database()
        .retry_ai_review(&review_id, current.as_ref(), now)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn update_settings(
    state: State<'_, DesktopState>,
    patch: SettingsPatch,
) -> Result<AppSettings, String> {
    let service = state_service(&state)?;
    let current = service.get_settings().map_err(|error| error.to_string())?;
    if settings_patch_requires_api_provider_validation(&patch) {
        let providers = provider_templates(Some(service.database()))
            .into_iter()
            .map(|mut provider| {
                provider.has_credential = credential_exists(&provider.id);
                provider
            })
            .collect::<Vec<_>>();
        let mut candidate = current.clone();
        if let Some(provider_id) = patch.selected_api_provider_id.as_ref() {
            candidate.selected_api_provider_id = provider_id
                .as_ref()
                .map(|provider_id| provider_id.trim().to_string())
                .filter(|provider_id| !provider_id.is_empty());
        }
        if let Some(mode) = patch.ai_execution_mode {
            candidate.ai_execution_mode = mode;
        }
        selected_api_provider(&candidate, &providers)?;
    }
    service
        .update_settings(patch)
        .map_err(|error| error.to_string())
}

fn settings_patch_requires_api_provider_validation(patch: &SettingsPatch) -> bool {
    matches!(patch.selected_api_provider_id.as_ref(), Some(Some(_)))
        || matches!(patch.ai_execution_mode, Some(AiExecutionMode::ApiKey))
}

#[tauri::command]
fn set_monitoring_state(
    state: State<'_, DesktopState>,
    enabled: bool,
) -> Result<AppSettings, String> {
    let service = state_service(&state)?;
    let settings = service
        .update_settings(SettingsPatch {
            monitoring_enabled: Some(enabled),
            ..SettingsPatch::default()
        })
        .map_err(|error| error.to_string())?;
    let observed_at_ms = now_ms();
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    let uptime_ms = system_uptime_ms();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let uptime_ms = 0;
    service
        .database()
        .save_monitoring_continuity_checkpoint(&MonitoringContinuityCheckpoint {
            expected_tracking: enabled,
            last_observed_at_ms: observed_at_ms,
            last_boot_started_at_ms: observed_at_ms.saturating_sub(uptime_ms),
            last_uptime_ms: uptime_ms,
            updated_at_ms: observed_at_ms,
        })
        .map_err(|error| error.to_string())?;
    Ok(settings)
}

#[tauri::command]
fn save_manual_classification(
    state: State<'_, DesktopState>,
    request: ManualClassificationRequest,
) -> Result<bool, String> {
    let reason = if request.reason.trim().is_empty() {
        "User correction"
    } else {
        request.reason.trim()
    };
    state_service(&state)?
        .save_manual_classification(
            &request.segment_id,
            request.category,
            request.video_purpose,
            reason,
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn save_daily_goal(
    state: State<'_, DesktopState>,
    date: String,
    goals: String,
    expected_output: String,
    actual_output: String,
) -> Result<(), String> {
    state_service(&state)?
        .database()
        .save_daily_goal(&date, &goals, &expected_output, &actual_output)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_daily_goal(state: State<'_, DesktopState>, date: String) -> Result<DailyGoalRecord, String> {
    state_service(&state)?
        .database()
        .get_daily_goal(&date)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_daily_analysis(
    state: State<'_, DesktopState>,
    date: String,
    start_ms: i64,
    end_ms: i64,
    activity_scope: Option<ActivityScope>,
) -> Result<DailyAnalysisResult, String> {
    state_service(&state)?
        .get_daily_analysis_scoped(&date, start_ms, end_ms, activity_scope.unwrap_or_default())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn queue_daily_analysis(
    state: State<'_, DesktopState>,
    date: String,
    start_ms: i64,
    end_ms: i64,
    activity_scope: Option<ActivityScope>,
) -> Result<String, String> {
    let service = state_service(&state)?;
    let queued_at_ms = now_ms();
    let execution = current_ai_execution_snapshot(&service, "", queued_at_ms)?;
    service
        .queue_daily_analysis_scoped(
            &date,
            start_ms,
            end_ms,
            activity_scope.unwrap_or_default(),
            queued_at_ms,
            Some(&execution),
        )
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "No valid AI execution provider is selected".to_string())
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
fn get_trend_analysis(
    state: State<'_, DesktopState>,
    start_date: String,
    end_date: String,
    day_boundaries_ms: Vec<i64>,
    comparison_start_date: String,
    comparison_end_date: String,
    comparison_day_boundaries_ms: Vec<i64>,
    timezone_offset_minutes: i32,
    activity_scope: Option<ActivityScope>,
) -> Result<TrendAnalysisResult, String> {
    let _ = timezone_offset_minutes;
    let activity_scope = activity_scope.unwrap_or_default();
    let service = state_service(&state)?;
    let evidence = service
        .get_trends_scoped(
            &start_date,
            &end_date,
            day_boundaries_ms,
            &comparison_start_date,
            &comparison_end_date,
            comparison_day_boundaries_ms,
            activity_scope,
        )
        .map_err(|error| error.to_string())?;
    service
        .get_trend_analysis_scoped(&evidence, now_ms(), activity_scope)
        .map_err(|error| error.to_string())
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
fn queue_trend_analysis(
    state: State<'_, DesktopState>,
    start_date: String,
    end_date: String,
    day_boundaries_ms: Vec<i64>,
    comparison_start_date: String,
    comparison_end_date: String,
    comparison_day_boundaries_ms: Vec<i64>,
    timezone_offset_minutes: i32,
    force: Option<bool>,
    activity_scope: Option<ActivityScope>,
) -> Result<Option<String>, String> {
    let _ = timezone_offset_minutes;
    let activity_scope = activity_scope.unwrap_or_default();
    let service = state_service(&state)?;
    let evidence = service
        .get_trends_scoped(
            &start_date,
            &end_date,
            day_boundaries_ms.clone(),
            &comparison_start_date,
            &comparison_end_date,
            comparison_day_boundaries_ms.clone(),
            activity_scope,
        )
        .map_err(|error| error.to_string())?;
    let providers: Vec<_> = provider_templates(Some(service.database()))
        .into_iter()
        .map(|mut provider| {
            provider.has_credential = credential_exists(&provider.id);
            provider
        })
        .collect();
    let settings = service.get_settings().map_err(|error| error.to_string())?;
    let force = force.unwrap_or(false);
    let eligible = trend_provider_eligible(
        force || background_ai_gate_enabled(&settings, AiAutomationGate::TrendAnalysis),
        &settings,
        &providers,
    );
    let execution = eligible
        .then(|| {
            build_ai_execution_snapshot(&settings, &providers, &evidence.evidence_hash, now_ms())
        })
        .transpose()?;
    service
        .queue_trend_analysis_scoped(
            &evidence,
            day_boundaries_ms,
            comparison_day_boundaries_ms,
            now_ms(),
            execution.as_ref(),
            force,
            activity_scope,
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_trend_research_analysis(
    state: State<'_, DesktopState>,
    request: TrendWorkbenchRequest,
    activity_scope: Option<ActivityScope>,
) -> Result<TrendResearchAnalysis, String> {
    state_service(&state)?
        .get_trend_research_analysis_scoped(request, activity_scope.unwrap_or_default())
}

#[tauri::command]
fn queue_trend_research_analysis(
    state: State<'_, DesktopState>,
    request: TrendWorkbenchRequest,
    force: Option<bool>,
    activity_scope: Option<ActivityScope>,
) -> Result<Option<String>, String> {
    let service = state_service(&state)?;
    let providers: Vec<_> = provider_templates(Some(service.database()))
        .into_iter()
        .map(|mut provider| {
            provider.has_credential = credential_exists(&provider.id);
            provider
        })
        .collect();
    let settings = service.get_settings().map_err(|error| error.to_string())?;
    let force = force.unwrap_or(false);
    let activity_scope = activity_scope.unwrap_or_default();
    let now_ms = now_ms();
    let eligible = trend_provider_eligible(
        force || background_ai_gate_enabled(&settings, AiAutomationGate::TrendAnalysis),
        &settings,
        &providers,
    );
    let execution = if eligible {
        let workbench = service
            .get_trend_workbench_scoped(request.clone(), activity_scope)
            .map_err(|error| error.to_string())?;
        Some(build_ai_execution_snapshot(
            &settings,
            &providers,
            &workbench.evidence_hash,
            now_ms,
        )?)
    } else {
        None
    };
    service.queue_trend_research_analysis_scoped(request, activity_scope, now_ms, execution, force)
}

#[tauri::command]
fn start_focus_session(
    state: State<'_, DesktopState>,
    goal_date: String,
    goal_text: String,
    planned_minutes: u32,
    task_id: Option<String>,
) -> Result<String, String> {
    let now = now_ms();
    let id = format!("focus-{now}");
    state_service(&state)?
        .database()
        .start_focus_session_for_task(
            &id,
            &goal_date,
            &goal_text,
            planned_minutes,
            now,
            task_id.as_deref(),
        )
        .map_err(|error| error.to_string())?;
    Ok(id)
}

#[tauri::command]
fn complete_focus_session(
    state: State<'_, DesktopState>,
    id: String,
    outcome: String,
) -> Result<bool, String> {
    state_service(&state)?
        .database()
        .complete_focus_session(&id, now_ms(), &outcome)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_browser_sources() -> Vec<BrowserSource> {
    discover_browser_sources()
}

#[tauri::command]
fn get_collection_health(state: State<'_, DesktopState>) -> Result<CollectionHealth, String> {
    let now = now_ms();
    let service = state_service(&state)?;
    let settings = service.get_settings().map_err(|error| error.to_string())?;
    let (measured_browser_slice_count, measured_browser_ms, _) = service
        .database()
        .browser_activity_summary(now.saturating_sub(24 * 60 * 60 * 1_000), now)
        .map_err(|error| error.to_string())?;
    drop(service);

    let runtime = state
        .collection_runtime
        .lock()
        .map_err(|_| "Collection runtime state is unavailable")?;
    let watcher = state
        .browser_watcher
        .runtime
        .lock()
        .map_err(|_| "Browser watcher state is unavailable")?;
    let desktop = runtime_channel(
        settings.monitoring_enabled,
        runtime.last_app_available_at_ms,
        now,
        "前台应用身份采集正常",
        "尚未收到成功的桌面采样",
    );
    let idle = runtime_channel(
        settings.monitoring_enabled,
        runtime.last_idle_read_at_ms,
        now,
        "键盘与鼠标空闲时钟可读取",
        "尚未读取空闲时钟",
    );
    let screen_recording = screen_recording_channel(now);
    let window_title = if !settings.monitoring_enabled {
        paused_channel("桌面监测已暂停")
    } else if screen_recording.status == CollectionChannelStatus::PermissionDenied {
        CollectionChannelHealth {
            status: CollectionChannelStatus::PermissionDenied,
            last_success_at_ms: runtime.last_title_available_at_ms,
            detail: "未获得屏幕录制权限；应用名仍可采集，但窗口标题可能为空".into(),
        }
    } else {
        runtime_channel(
            true,
            runtime.last_title_available_at_ms,
            now,
            "窗口标题采集正常",
            "尚未采集到窗口标题",
        )
    };
    let continuity = if !settings.monitoring_enabled {
        paused_channel("桌面监测已暂停；恢复后继续记录连续性")
    } else {
        CollectionChannelHealth {
            status: desktop.status,
            last_success_at_ms: runtime
                .last_continuity_gap_at_ms
                .or(runtime.last_persisted_at_ms),
            detail: runtime
                .last_continuity_gap_at_ms
                .map(|_| "已检测并单独记录最近一次采集断档")
                .unwrap_or("连续性检查运行中；睡眠和采样中断不会计入活跃时间")
                .into(),
        }
    };
    let watcher_fresh = watcher
        .last_heartbeat_at_ms
        .is_some_and(|last| now.saturating_sub(last) <= 90_000);
    let browser_watcher = if !watcher.listener_ready {
        CollectionChannelHealth {
            status: CollectionChannelStatus::Unavailable,
            last_success_at_ms: watcher.last_persisted_at_ms,
            detail: watcher
                .listener_error
                .clone()
                .unwrap_or_else(|| "浏览器 watcher 监听器尚未启动".into()),
        }
    } else if watcher_fresh {
        CollectionChannelHealth {
            status: CollectionChannelStatus::Healthy,
            last_success_at_ms: watcher
                .last_persisted_at_ms
                .or(watcher.last_heartbeat_at_ms),
            detail: "浏览器扩展心跳正常；时长按相邻心跳区间计量".into(),
        }
    } else {
        CollectionChannelHealth {
            status: CollectionChannelStatus::Degraded,
            last_success_at_ms: watcher
                .last_persisted_at_ms
                .or(watcher.last_heartbeat_at_ms),
            detail: "监听器已就绪，等待浏览器扩展连接".into(),
        }
    };
    let available_history_sources = discover_browser_sources()
        .into_iter()
        .filter(|source| source.available)
        .count();
    let browser_history = CollectionChannelHealth {
        status: if available_history_sources > 0 {
            CollectionChannelStatus::Healthy
        } else {
            CollectionChannelStatus::Unavailable
        },
        last_success_at_ms: runtime.last_browser_history_scan_at_ms,
        detail: if available_history_sources > 0 {
            format!("{available_history_sources} 个历史数据库可读；仅作为访问证据，不计停留时长")
        } else {
            "未发现可读取的 Chromium 历史数据库".into()
        },
    };

    Ok(CollectionHealth {
        generated_at_ms: now,
        platform: std::env::consts::OS.into(),
        monitoring_enabled: settings.monitoring_enabled,
        desktop,
        window_title,
        idle,
        continuity,
        screen_recording,
        browser_watcher,
        browser_history,
        watcher_endpoint: BROWSER_WATCHER_ENDPOINT.into(),
        watcher_token: state.browser_watcher.token.clone(),
        watcher_source_count: watcher
            .connected_sources
            .values()
            .filter(|observed_at_ms| now.saturating_sub(**observed_at_ms) <= 90_000)
            .count(),
        measured_browser_slice_count,
        measured_browser_seconds: measured_browser_ms / 1_000,
    })
}

fn runtime_channel(
    monitoring_enabled: bool,
    last_success_at_ms: Option<i64>,
    now_ms: i64,
    healthy_detail: &str,
    missing_detail: &str,
) -> CollectionChannelHealth {
    if !monitoring_enabled {
        return paused_channel("桌面监测已暂停");
    }
    match last_success_at_ms {
        Some(last) if now_ms.saturating_sub(last) <= 20_000 => CollectionChannelHealth {
            status: CollectionChannelStatus::Healthy,
            last_success_at_ms: Some(last),
            detail: healthy_detail.into(),
        },
        Some(last) => CollectionChannelHealth {
            status: CollectionChannelStatus::Degraded,
            last_success_at_ms: Some(last),
            detail: "采集信号已超过 20 秒未更新".into(),
        },
        None => CollectionChannelHealth {
            status: CollectionChannelStatus::Degraded,
            last_success_at_ms: None,
            detail: missing_detail.into(),
        },
    }
}

fn paused_channel(detail: &str) -> CollectionChannelHealth {
    CollectionChannelHealth {
        status: CollectionChannelStatus::Paused,
        last_success_at_ms: None,
        detail: detail.into(),
    }
}

#[cfg(target_os = "macos")]
fn screen_recording_channel(now_ms: i64) -> CollectionChannelHealth {
    if screen_recording_permission_granted() {
        CollectionChannelHealth {
            status: CollectionChannelStatus::Healthy,
            last_success_at_ms: Some(now_ms),
            detail: "屏幕录制权限已授予，可读取窗口标题".into(),
        }
    } else {
        CollectionChannelHealth {
            status: CollectionChannelStatus::PermissionDenied,
            last_success_at_ms: None,
            detail: "需要在系统设置的隐私与安全性中授予屏幕录制权限".into(),
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn screen_recording_channel(now_ms: i64) -> CollectionChannelHealth {
    CollectionChannelHealth {
        status: CollectionChannelStatus::Healthy,
        last_success_at_ms: Some(now_ms),
        detail: "当前平台不需要 macOS 屏幕录制权限".into(),
    }
}

#[tauri::command]
fn scan_browsers(
    state: State<'_, DesktopState>,
    start_ms: i64,
    end_ms: i64,
) -> Result<BrowserScanResult, String> {
    let service = state_service(&state)?;
    let settings = service.get_settings().map_err(|error| error.to_string())?;
    Ok(scan_browser_sources(
        service.database(),
        start_ms,
        end_ms,
        &settings.excluded_domains,
    ))
}

fn scan_browser_sources(
    database: &Database,
    start_ms: i64,
    end_ms: i64,
    excluded_domains: &[String],
) -> BrowserScanResult {
    let mut result = BrowserScanResult {
        scanned_sources: 0,
        visits_found: 0,
        errors: Vec::new(),
        new_visits: Vec::new(),
    };
    for source in discover_browser_sources()
        .into_iter()
        .filter(|source| source.available)
    {
        result.scanned_sources += 1;
        match scan_chromium_history(Path::new(&source.history_path), start_ms, end_ms, 2_000) {
            Ok(visits) => {
                result.visits_found += visits.len();
                for mut visit in visits {
                    visit.url = redact_url_for_storage(&visit.url);
                    if visit.url.is_empty() {
                        continue;
                    }
                    let domain = domain_from_url(&visit.url);
                    if matches_domain_exclusion(&domain, excluded_domains) {
                        continue;
                    }
                    let identity = format!(
                        "{}\n{}\n{}\n{}",
                        source.browser, source.profile, visit.visited_at_ms, visit.url
                    );
                    let hash = format!("{:x}", Sha256::digest(identity.as_bytes()));
                    let id = format!("visit-{}", &hash[..24]);
                    let inserted = database.insert_browser_visit(
                        &id,
                        &source.browser,
                        &source.profile,
                        &visit,
                        &domain,
                    );
                    if inserted.is_ok_and(|value| value) {
                        result.new_visits.push((
                            id,
                            visit.url,
                            domain,
                            truncate_text(&visit.title, 240),
                        ));
                    }
                }
            }
            Err(error) => result
                .errors
                .push(format!("{} {}: {error}", source.browser, source.profile)),
        }
    }
    result
}

#[tauri::command]
fn list_ai_providers(state: State<'_, DesktopState>) -> Result<Vec<AiProviderConfig>, String> {
    let service = state_service(&state)?;
    Ok(provider_templates(Some(service.database()))
        .into_iter()
        .map(|mut provider| {
            provider.has_credential = credential_exists(&provider.id);
            provider
        })
        .collect())
}

#[tauri::command]
fn save_custom_ai_provider(
    state: State<'_, DesktopState>,
    base_url: String,
    model: String,
) -> Result<(), String> {
    if !validate_provider_endpoint(base_url.trim()) || model.trim().is_empty() {
        return Err(
            "Provider URL must use HTTPS; HTTP is allowed only for localhost or loopback".into(),
        );
    }
    let value = serde_json::to_string(&CustomProviderConfig {
        base_url: base_url.trim_end_matches('/').into(),
        model: model.trim().into(),
    })
    .map_err(|error| error.to_string())?;
    state_service(&state)?
        .database()
        .set_setting_json("custom_ai_provider", &value)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn save_ai_provider_key(
    state: State<'_, DesktopState>,
    provider_id: String,
    api_key: String,
) -> Result<(), String> {
    {
        let service = state_service(&state)?;
        if !provider_templates(Some(service.database()))
            .iter()
            .any(|provider| provider.id == provider_id)
        {
            return Err(format!("Unknown AI provider: {provider_id}"));
        }
    }
    let entry = Entry::new(current_edition_identity().credential_service, &provider_id)
        .map_err(|error| error.to_string())?;
    if api_key.trim().is_empty() {
        let _ = entry.delete_credential();
        let service = state_service(&state)?;
        let mut settings = service.get_settings().map_err(|error| error.to_string())?;
        if clear_selected_provider_for_deleted_key(&mut settings, &provider_id) {
            service
                .update_settings(SettingsPatch {
                    selected_api_provider_id: Some(None),
                    ..SettingsPatch::default()
                })
                .map_err(|error| error.to_string())?;
        }
    } else {
        entry
            .set_password(api_key.trim())
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn clear_selected_provider_for_deleted_key(settings: &mut AppSettings, provider_id: &str) -> bool {
    if settings.selected_api_provider_id.as_deref() != Some(provider_id) {
        return false;
    }
    settings.selected_api_provider_id = None;
    true
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ApiHealthProbeError {
    Unconfigured(String),
    Unavailable(String),
    RateLimited(String),
    PermissionDenied(String),
    TimedOut(String),
    Error(String),
}

impl ApiHealthProbeError {
    fn diagnostic(self) -> String {
        sanitize_ai_diagnostic(
            match self {
                Self::Unconfigured(diagnostic)
                | Self::Unavailable(diagnostic)
                | Self::RateLimited(diagnostic)
                | Self::PermissionDenied(diagnostic)
                | Self::TimedOut(diagnostic)
                | Self::Error(diagnostic) => diagnostic,
            }
            .as_str(),
        )
    }
}

fn selected_api_provider<'a>(
    settings: &AppSettings,
    providers: &'a [AiProviderConfig],
) -> Result<&'a AiProviderConfig, String> {
    let provider_id = settings
        .selected_api_provider_id
        .as_deref()
        .filter(|provider_id| !provider_id.trim().is_empty())
        .ok_or_else(|| "Select an API provider with saved credentials".to_string())?;
    let provider = providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| format!("Unknown selected API provider: {provider_id}"))?;
    if !provider.enabled || !provider.has_credential {
        return Err(format!(
            "Selected API provider {} does not have saved credentials",
            provider.name
        ));
    }
    Ok(provider)
}

async fn probe_selected_api_provider_with<F, Fut>(
    settings: &AppSettings,
    providers: &[AiProviderConfig],
    probe: F,
) -> Option<(AiProviderConfig, Result<(), ApiHealthProbeError>)>
where
    F: FnOnce(AiProviderConfig) -> Fut,
    Fut: std::future::Future<Output = Result<(), ApiHealthProbeError>>,
{
    let selected = selected_api_provider(settings, providers).ok()?.clone();
    let result = probe(selected.clone()).await;
    Some((selected, result))
}

fn map_api_health_result(
    result: Result<(), ApiHealthProbeError>,
) -> (AiConnectionHealthStatus, Option<String>) {
    match result {
        Ok(()) => (AiConnectionHealthStatus::Reachable, None),
        Err(error) => {
            let status = match &error {
                ApiHealthProbeError::Unconfigured(_) => AiConnectionHealthStatus::Unconfigured,
                ApiHealthProbeError::Unavailable(_) => AiConnectionHealthStatus::Unavailable,
                ApiHealthProbeError::RateLimited(_) => AiConnectionHealthStatus::RateLimited,
                ApiHealthProbeError::PermissionDenied(_) => {
                    AiConnectionHealthStatus::PermissionDenied
                }
                ApiHealthProbeError::TimedOut(_) => AiConnectionHealthStatus::TimedOut,
                ApiHealthProbeError::Error(_) => AiConnectionHealthStatus::Error,
            };
            (status, Some(error.diagnostic()))
        }
    }
}

fn map_codex_health_status(status: CodexHealthStatus) -> AiConnectionHealthStatus {
    match status {
        CodexHealthStatus::Healthy => AiConnectionHealthStatus::Reachable,
        CodexHealthStatus::PermissionDenied => AiConnectionHealthStatus::PermissionDenied,
        CodexHealthStatus::TimedOut => AiConnectionHealthStatus::TimedOut,
        CodexHealthStatus::Unavailable => AiConnectionHealthStatus::Unavailable,
        CodexHealthStatus::Error => AiConnectionHealthStatus::Error,
    }
}

async fn probe_api_provider_health(
    provider: &AiProviderConfig,
    api_key: &str,
) -> Result<(), ApiHealthProbeError> {
    if api_key.trim().is_empty() {
        return Err(ApiHealthProbeError::Unconfigured(format!(
            "No API key is configured for {}",
            provider.name
        )));
    }
    let endpoint = format!("{}/models", provider.base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(API_HEALTH_TIMEOUT)
        .build()
        .map_err(|_| ApiHealthProbeError::Error("Unable to initialize the health probe".into()))?;
    let response = client
        .get(endpoint)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                ApiHealthProbeError::TimedOut(format!(
                    "{} health probe timed out after 8 seconds",
                    provider.name
                ))
            } else if error.is_connect() {
                ApiHealthProbeError::Unavailable(format!(
                    "{} is unavailable because the network connection failed",
                    provider.name
                ))
            } else {
                ApiHealthProbeError::Error(format!("{} health probe request failed", provider.name))
            }
        })?;
    let status = response.status();
    if status.is_success() {
        Ok(())
    } else if matches!(status.as_u16(), 401 | 403) {
        Err(ApiHealthProbeError::PermissionDenied(format!(
            "{} rejected the configured credential with HTTP {status}",
            provider.name
        )))
    } else if status.as_u16() == 429 {
        Err(ApiHealthProbeError::RateLimited(format!(
            "{} returned HTTP {status}",
            provider.name
        )))
    } else if status.is_server_error() {
        Err(ApiHealthProbeError::Unavailable(format!(
            "{} returned HTTP {status}",
            provider.name
        )))
    } else {
        Err(ApiHealthProbeError::Error(format!(
            "{} returned HTTP {status}",
            provider.name
        )))
    }
}

enum AiConnectionHealthProbePlan {
    Api { provider: AiProviderConfig },
    Codex { config: CodexExecutionConfig },
    Unconfigured,
}

impl AiConnectionHealthProbePlan {
    fn health(
        &self,
        status: AiConnectionHealthStatus,
        checked_at_ms: i64,
        diagnostic: Option<String>,
    ) -> AiConnectionHealth {
        match self {
            Self::Api { provider } => AiConnectionHealth {
                execution_mode: AiExecutionMode::ApiKey,
                executor_id: provider.id.clone(),
                executor_label: provider.name.clone(),
                model: provider.model.clone(),
                status,
                verification_level: AiConnectionVerificationLevel::Connectivity,
                source: AiConnectionHealthSource::Background,
                checked_at_ms,
                verified_at_ms: None,
                diagnostic,
            },
            Self::Codex { config } => AiConnectionHealth {
                execution_mode: AiExecutionMode::Codex,
                executor_id: "codex".into(),
                executor_label: "Codex".into(),
                model: config.model_label().into(),
                status,
                verification_level: AiConnectionVerificationLevel::Connectivity,
                source: AiConnectionHealthSource::Background,
                checked_at_ms,
                verified_at_ms: None,
                diagnostic,
            },
            Self::Unconfigured => AiConnectionHealth {
                execution_mode: AiExecutionMode::ApiKey,
                executor_id: String::new(),
                executor_label: "API provider".into(),
                model: String::new(),
                status,
                verification_level: AiConnectionVerificationLevel::Connectivity,
                source: AiConnectionHealthSource::Background,
                checked_at_ms,
                verified_at_ms: None,
                diagnostic,
            },
        }
    }
}

fn build_ai_connection_health_probe_plan(
    state: &DesktopState,
) -> Result<AiConnectionHealthProbePlan, String> {
    let (settings, mut providers) = {
        let service = state
            .service
            .lock()
            .map_err(|_| "Application state is unavailable".to_string())?;
        let settings = service.get_settings().map_err(|error| error.to_string())?;
        let providers = provider_templates(Some(service.database()));
        (settings, providers)
    };
    match settings.ai_execution_mode {
        AiExecutionMode::Codex => Ok(AiConnectionHealthProbePlan::Codex {
            config: codex_execution_config_from_settings(&settings),
        }),
        AiExecutionMode::ApiKey => {
            for provider in &mut providers {
                provider.has_credential = credential_exists(&provider.id);
            }
            Ok(selected_api_provider(&settings, &providers)
                .cloned()
                .map(|provider| AiConnectionHealthProbePlan::Api { provider })
                .unwrap_or(AiConnectionHealthProbePlan::Unconfigured))
        }
    }
}

fn emit_ai_connection_health(app: &tauri::AppHandle, health: &AiConnectionHealth) {
    let health = health.clone().sanitized();
    let _ = app.emit(AI_CONNECTION_HEALTH_CHANGED_EVENT, &health);
}

async fn run_ai_connection_health_probe(plan: &AiConnectionHealthProbePlan) -> AiConnectionHealth {
    match plan {
        AiConnectionHealthProbePlan::Api { provider } => {
            let settings = AppSettings {
                ai_execution_mode: AiExecutionMode::ApiKey,
                selected_api_provider_id: Some(provider.id.clone()),
                ..AppSettings::default()
            };
            let Some((selected, result)) = probe_selected_api_provider_with(
                &settings,
                std::slice::from_ref(provider),
                |provider| async move {
                    let key =
                        Entry::new(current_edition_identity().credential_service, &provider.id)
                            .map_err(|error| ApiHealthProbeError::Error(error.to_string()))?
                            .get_password()
                            .map_err(|_| {
                                ApiHealthProbeError::Unconfigured(format!(
                                    "No API key is configured for {}",
                                    provider.name
                                ))
                            })?;
                    probe_api_provider_health(&provider, &key).await
                },
            )
            .await
            else {
                return plan.health(
                    AiConnectionHealthStatus::Unconfigured,
                    now_ms(),
                    Some("No selected API provider is configured".into()),
                );
            };
            let (status, diagnostic) = map_api_health_result(result);
            AiConnectionHealth {
                execution_mode: AiExecutionMode::ApiKey,
                executor_id: selected.id,
                executor_label: selected.name,
                model: selected.model,
                status,
                verification_level: AiConnectionVerificationLevel::Connectivity,
                source: AiConnectionHealthSource::Background,
                checked_at_ms: now_ms(),
                verified_at_ms: None,
                diagnostic,
            }
        }
        AiConnectionHealthProbePlan::Codex { config } => {
            let configured_path = config.executable.clone();
            let probe = tokio::task::spawn_blocking(move || {
                let (configured_path, detected_path) =
                    resolve_codex_health_paths_with(&configured_path, detect_executable_on_path);
                probe_codex_health(&configured_path, detected_path, CODEX_HEALTH_TIMEOUT_MS)
            })
            .await;
            match probe {
                Ok(health) => plan.health(
                    map_codex_health_status(health.status),
                    health.checked_at_ms,
                    health.diagnostic,
                ),
                Err(error) => plan.health(
                    AiConnectionHealthStatus::Error,
                    now_ms(),
                    Some(format!("Codex health probe task failed: {error}")),
                ),
            }
        }
        AiConnectionHealthProbePlan::Unconfigured => plan.health(
            AiConnectionHealthStatus::Unconfigured,
            now_ms(),
            Some("No selected API provider is configured".into()),
        ),
    }
}

async fn refresh_ai_connection_health_inner(
    app: &tauri::AppHandle,
) -> Result<AiConnectionHealth, String> {
    let state = app
        .try_state::<DesktopState>()
        .ok_or_else(|| "Desktop state is unavailable".to_string())?;
    if !state.ai_connection_health.request_probe() {
        return Ok(state.ai_connection_health.current());
    }

    loop {
        let final_health = match build_ai_connection_health_probe_plan(&state) {
            Ok(plan) => {
                let checking = plan.health(AiConnectionHealthStatus::Checking, now_ms(), None);
                let checking = state.ai_connection_health.store(checking);
                emit_ai_connection_health(app, &checking);

                let final_health = run_ai_connection_health_probe(&plan).await;
                let final_health = state.ai_connection_health.store(final_health);
                emit_ai_connection_health(app, &final_health);
                final_health
            }
            Err(error) => {
                let mut checking = state.ai_connection_health.current();
                checking.status = AiConnectionHealthStatus::Checking;
                checking.checked_at_ms = now_ms();
                checking.diagnostic = None;
                let checking = state.ai_connection_health.store(checking);
                emit_ai_connection_health(app, &checking);

                let mut final_health = checking;
                final_health.status = AiConnectionHealthStatus::Error;
                final_health.checked_at_ms = now_ms();
                final_health.diagnostic = Some(error);
                let final_health = state.ai_connection_health.store(final_health);
                emit_ai_connection_health(app, &final_health);
                final_health
            }
        };

        if !state.ai_connection_health.finish_probe() {
            return Ok(final_health);
        }
    }
}

#[tauri::command]
fn get_ai_connection_health(state: State<'_, DesktopState>) -> AiConnectionHealth {
    state.ai_connection_health.current()
}

#[tauri::command]
async fn refresh_ai_connection_health(app: tauri::AppHandle) -> Result<AiConnectionHealth, String> {
    refresh_ai_connection_health_inner(&app).await
}

#[tauri::command]
async fn test_ai_provider(
    state: State<'_, DesktopState>,
    provider_id: String,
) -> Result<String, String> {
    let provider = provider_templates(Some(state_service(&state)?.database()))
        .into_iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| "Unknown AI provider".to_string())?;
    let key = Entry::new(current_edition_identity().credential_service, &provider.id)
        .map_err(|error| error.to_string())?
        .get_password()
        .map_err(|_| "No API key is configured for this provider".to_string())?;
    probe_api_provider_health(&provider, &key)
        .await
        .map(|_| format!("{} is available", provider.name))
        .map_err(ApiHealthProbeError::diagnostic)
}

#[tauri::command]
fn get_codex_health(state: State<'_, DesktopState>) -> Result<CodexHealth, String> {
    let settings = state_service(&state)?
        .get_settings()
        .map_err(|error| error.to_string())?;
    let (configured_path, detected_path) =
        resolve_codex_health_paths_with(&settings.codex_executable, detect_executable_on_path);
    let mut health = probe_codex_health(&configured_path, detected_path, CODEX_HEALTH_TIMEOUT_MS);
    health.diagnostic = health
        .diagnostic
        .as_deref()
        .map(sanitize_ai_diagnostic)
        .filter(|diagnostic| !diagnostic.is_empty());
    Ok(health)
}

fn resolve_codex_health_paths_with<F>(configured_path: &str, detect: F) -> (String, Option<String>)
where
    F: FnOnce(&str) -> Option<String>,
{
    let configured_path = configured_path.trim().to_string();
    let executable = if configured_path.is_empty() {
        "codex"
    } else {
        &configured_path
    };
    let is_bare_name = !executable.contains(['/', '\\']);
    let detected_path = (configured_path.is_empty() || is_bare_name)
        .then(|| detect(executable))
        .flatten();
    (configured_path, detected_path)
}

fn detect_executable_on_path(name: &str) -> Option<String> {
    let search_path = std::env::var_os("PATH")?;
    #[allow(unused_mut)]
    let mut names = vec![name.to_string()];
    #[cfg(target_os = "windows")]
    if Path::new(name).extension().is_none() {
        let extensions = std::env::var_os("PATHEXT")
            .and_then(|value| value.into_string().ok())
            .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_string());
        names.extend(
            extensions
                .split(';')
                .filter(|extension| !extension.is_empty())
                .map(|extension| format!("{name}{extension}")),
        );
    }

    std::env::split_paths(&search_path).find_map(|directory| {
        names.iter().find_map(|candidate| {
            let path = directory.join(candidate);
            path.is_file().then(|| path.to_string_lossy().into_owned())
        })
    })
}

#[tauri::command]
async fn test_codex_cli(
    app: tauri::AppHandle,
    state: State<'_, DesktopState>,
) -> Result<CodexHealth, String> {
    let settings = state_service(&state)?
        .get_settings()
        .map_err(|error| error.to_string())?;
    let config = codex_execution_config_from_settings(&settings);
    let health = test_codex_inference(
        &config.executable,
        config.model_label(),
        CODEX_HEALTH_TIMEOUT_MS.saturating_mul(6),
    )
    .await;
    let status = if health.status == CodexHealthStatus::Healthy {
        AiConnectionHealthStatus::Healthy
    } else {
        map_codex_health_status(health.status)
    };
    let checked_at_ms = now_ms();
    let connection_health = AiConnectionHealth {
        execution_mode: AiExecutionMode::Codex,
        executor_id: "codex".into(),
        executor_label: "Codex".into(),
        model: config.model_label().into(),
        status,
        verification_level: AiConnectionVerificationLevel::Inference,
        source: AiConnectionHealthSource::Manual,
        checked_at_ms,
        verified_at_ms: Some(checked_at_ms),
        diagnostic: health.diagnostic.clone(),
    };
    let connection_health = state.ai_connection_health.store(connection_health);
    emit_ai_connection_health(&app, &connection_health);
    Ok(health)
}

#[tauri::command]
fn run_ai_backfill(state: State<'_, DesktopState>) -> Result<i64, String> {
    let service = state_service(&state)?;
    service
        .update_settings(SettingsPatch {
            ai_backfill_enabled: Some(true),
            ..SettingsPatch::default()
        })
        .map_err(|error| error.to_string())?;
    service
        .database()
        .pending_ai_job_count()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn import_legacy_data(
    state: State<'_, DesktopState>,
    path: String,
) -> Result<LegacyImportResult, String> {
    import_activity_jsonl(state_service(&state)?.database(), Path::new(&path))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn open_data_folder() -> Result<String, String> {
    let data_dir = app_data_dir();
    fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
    #[cfg(target_os = "windows")]
    let opener = "explorer.exe";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let opener = "xdg-open";
    std::process::Command::new(opener)
        .arg(&data_dir)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(data_dir.to_string_lossy().into_owned())
}

#[tauri::command]
fn export_markdown_report(
    state: State<'_, DesktopState>,
    path: String,
    date: String,
    start_ms: i64,
    end_ms: i64,
) -> Result<String, String> {
    let dashboard = state_service(&state)?
        .get_dashboard(start_ms, end_ms)
        .map_err(|error| error.to_string())?;
    let legacy = render_daily_markdown(&date, &dashboard);
    let body = render_markdown(&ReportDocument::from_markdown(
        format!("今日日报 · {date}"),
        &legacy,
    ));
    let output = if path.trim().is_empty() {
        let root = UserDirs::new()
            .and_then(|dirs| dirs.document_dir().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        root.join("每日任务报告").join(format!("{date}.md"))
    } else {
        PathBuf::from(path)
    };
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(&output, body).map_err(|error| error.to_string())?;
    Ok(output.to_string_lossy().into_owned())
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
fn export_trend_markdown(
    state: State<'_, DesktopState>,
    path: String,
    start_date: String,
    end_date: String,
    day_boundaries_ms: Vec<i64>,
    comparison_start_date: String,
    comparison_end_date: String,
    comparison_day_boundaries_ms: Vec<i64>,
    timezone_offset_minutes: i32,
    request: TrendWorkbenchRequest,
) -> Result<String, String> {
    let _ = timezone_offset_minutes;
    if request.start_date != start_date || request.end_date != end_date {
        return Err("Trend export request range does not match legacy report range".into());
    }
    let service = state_service(&state)?;
    let evidence = service
        .get_trends(
            &start_date,
            &end_date,
            day_boundaries_ms,
            &comparison_start_date,
            &comparison_end_date,
            comparison_day_boundaries_ms,
        )
        .map_err(|error| error.to_string())?;
    let analysis = service
        .get_trend_analysis(&evidence, now_ms())
        .map_err(|error| error.to_string())?;
    let workbench = service
        .get_trend_workbench(request)
        .map_err(|error| error.to_string())?;
    let legacy = render_trend_markdown_with_workbench(&evidence, &analysis, &workbench, now_ms());
    let body = render_markdown(&ReportDocument::from_markdown(
        format!("趋势报告 · {start_date} 至 {end_date}"),
        &legacy,
    ));
    let output = if path.trim().is_empty() {
        let root = UserDirs::new()
            .and_then(|dirs| dirs.document_dir().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        let file_name = if workbench.range.selection_mode == TrendSelectionMode::SelectedDates {
            format!(
                "trend-{start_date}-{end_date}-{}.md",
                &workbench.evidence_hash[..12]
            )
        } else {
            format!("trend-{start_date}-{end_date}.md")
        };
        root.join("每日任务报告").join(file_name)
    } else {
        PathBuf::from(path)
    };
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(&output, body.as_bytes()).map_err(|error| error.to_string())?;
    Ok(output.to_string_lossy().into_owned())
}

#[tauri::command]
fn export_report(
    state: State<'_, DesktopState>,
    request: ExportReportRequest,
) -> Result<Option<String>, String> {
    let (report, default_stem) = {
        let service = state_service(&state)?;
        match request.scope {
            ReportScope::Daily => {
                let date = required_export_field(request.date.as_deref(), "date")?;
                let start_ms = request
                    .start_ms
                    .ok_or_else(|| "Daily report requires startMs".to_string())?;
                let end_ms = request
                    .end_ms
                    .ok_or_else(|| "Daily report requires endMs".to_string())?;
                let dashboard = service
                    .get_dashboard(start_ms, end_ms)
                    .map_err(|error| error.to_string())?;
                let legacy = render_daily_markdown(date, &dashboard);
                (
                    ReportDocument::from_markdown(format!("今日日报 · {date}"), &legacy),
                    format!("今日日报-{date}"),
                )
            }
            ReportScope::Trend => {
                let start_date = required_export_field(request.start_date.as_deref(), "startDate")?;
                let end_date = required_export_field(request.end_date.as_deref(), "endDate")?;
                let comparison_start_date = required_export_field(
                    request.comparison_start_date.as_deref(),
                    "comparisonStartDate",
                )?;
                let comparison_end_date = required_export_field(
                    request.comparison_end_date.as_deref(),
                    "comparisonEndDate",
                )?;
                let trend_request = request
                    .trend_request
                    .clone()
                    .ok_or_else(|| "Trend report requires trendRequest".to_string())?;
                let evidence = service
                    .get_trends(
                        start_date,
                        end_date,
                        request.day_boundaries_ms.clone(),
                        comparison_start_date,
                        comparison_end_date,
                        request.comparison_day_boundaries_ms.clone(),
                    )
                    .map_err(|error| error.to_string())?;
                let analysis = service
                    .get_trend_analysis(&evidence, now_ms())
                    .map_err(|error| error.to_string())?;
                let workbench = service
                    .get_trend_workbench(trend_request)
                    .map_err(|error| error.to_string())?;
                let legacy = render_trend_markdown_with_workbench(
                    &evidence,
                    &analysis,
                    &workbench,
                    now_ms(),
                );
                (
                    ReportDocument::from_markdown(
                        format!("趋势报告 · {start_date} 至 {end_date}"),
                        &legacy,
                    ),
                    format!("趋势报告-{start_date}-{end_date}"),
                )
            }
            ReportScope::Task => {
                let task_id = required_export_field(request.task_id.as_deref(), "taskId")?;
                let end_ms = request.end_ms.unwrap_or_else(now_ms);
                let ledger = WorkLedgerService::new(WorkLedgerRepository::new(service.database()));
                let task = ledger
                    .get_task(task_id)
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "Task was not found".to_string())?;
                let insight = ledger
                    .task_time_insight(task_id, end_ms)
                    .map_err(|error| error.to_string())?;
                (
                    task_report_document(&task, &insight),
                    format!("任务报告-{}", safe_file_stem(&task.title)),
                )
            }
            ReportScope::Project => {
                let project_id = required_export_field(request.project_id.as_deref(), "projectId")?;
                let end_ms = request.end_ms.unwrap_or_else(now_ms);
                let ledger = WorkLedgerService::new(WorkLedgerRepository::new(service.database()));
                let project = ledger
                    .get_project(project_id)
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "Project was not found".to_string())?;
                let tasks = ledger
                    .list_tasks(project_id)
                    .map_err(|error| error.to_string())?;
                let insights = tasks
                    .iter()
                    .map(|task| {
                        ledger
                            .task_time_insight(&task.id, end_ms)
                            .map(|insight| (task.clone(), insight))
                    })
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(|error| error.to_string())?;
                (
                    project_report_document(&project.name, &insights),
                    format!("项目报告-{}", safe_file_stem(&project.name)),
                )
            }
        }
    };
    let extension = match request.format {
        ReportFormat::Markdown => "md",
        ReportFormat::Docx => "docx",
    };
    let default_name = format!("{}.{}", safe_file_stem(&default_stem), extension);
    let Some(output) = rfd::FileDialog::new()
        .add_filter(
            match request.format {
                ReportFormat::Markdown => "Markdown 文档",
                ReportFormat::Docx => "Word 文档",
            },
            &[extension],
        )
        .set_file_name(&default_name)
        .save_file()
    else {
        return Ok(None);
    };
    match request.format {
        ReportFormat::Markdown => {
            fs::write(&output, render_markdown(&report)).map_err(|error| error.to_string())?
        }
        ReportFormat::Docx => {
            fs::write(&output, render_docx(&report)?).map_err(|error| error.to_string())?
        }
    }
    Ok(Some(output.to_string_lossy().into_owned()))
}

fn required_export_field<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, String> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("Report request requires {field}"))
}

fn safe_file_stem(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|character| {
            if r#"<>:"/\|?*"#.contains(character) || character.is_control() {
                '-'
            } else {
                character
            }
        })
        .collect::<String>();
    let trimmed = cleaned.trim().trim_matches('.');
    if trimmed.is_empty() {
        "工作报告".into()
    } else {
        trimmed.chars().take(80).collect()
    }
}

fn format_ledger_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    match (hours, minutes) {
        (0, 0) => format!("{seconds} 秒"),
        (0, minutes) => format!("{minutes} 分钟"),
        (hours, 0) => format!("{hours} 小时"),
        (hours, minutes) => format!("{hours} 小时 {minutes} 分钟"),
    }
}

fn task_report_document(task: &Task, insight: &TaskTimeInsight) -> ReportDocument {
    let summary = &insight.summary;
    let daily_rows = insight
        .daily_points
        .iter()
        .map(|point| {
            vec![
                point.date.clone(),
                format_ledger_duration(point.invested_seconds),
                format_ledger_duration(point.focus_seconds),
                point.switch_count.to_string(),
            ]
        })
        .collect();
    let dimension_rows = insight
        .assessment
        .dimensions
        .iter()
        .map(|dimension| vec![dimension.label.clone(), dimension.conclusion.clone()])
        .collect();
    ReportDocument {
        title: format!("任务报告：{}", task.title),
        metadata: vec![
            ("任务状态".into(), format!("{:?}", task.status)),
            ("归属状态".into(), format!("{:?}", summary.review_state)),
        ],
        blocks: vec![
            ReportBlock::Heading {
                level: 2,
                text: "摘要".into(),
            },
            ReportBlock::Table {
                headers: vec!["指标".into(), "数值".into()],
                rows: vec![
                    vec![
                        "生命周期总投入".into(),
                        format_ledger_duration(summary.lifecycle_total_seconds),
                    ],
                    vec![
                        format!("活跃日均（{} 天）", summary.active_day_count),
                        format_ledger_duration(summary.active_day_average_seconds),
                    ],
                    vec![
                        format!("自然日均（{} 天）", summary.natural_day_count),
                        format_ledger_duration(summary.natural_day_average_seconds),
                    ],
                    vec![
                        "最长连续片段".into(),
                        format_ledger_duration(insight.longest_continuous_seconds),
                    ],
                    vec![
                        "专注占比".into(),
                        format!("{:.0}%", insight.focus_share * 100.0),
                    ],
                    vec![
                        "每小时切换".into(),
                        format!("{:.1} 次", insight.switches_per_hour),
                    ],
                ],
            },
            ReportBlock::Heading {
                level: 2,
                text: "每日时间".into(),
            },
            ReportBlock::Table {
                headers: vec!["日期".into(), "总投入".into(), "专注".into(), "切换".into()],
                rows: daily_rows,
            },
            ReportBlock::Heading {
                level: 2,
                text: "任务质量（透明维度）".into(),
            },
            ReportBlock::Table {
                headers: vec!["维度".into(), "事实性结论".into()],
                rows: dimension_rows,
            },
            ReportBlock::Callout {
                kind: "info".into(),
                title: "数据质量".into(),
                body: if insight.assessment.data_limitations.is_empty() {
                    "活动片段与专注会话按时间区间并集计时；浏览记录仅作为证据。".into()
                } else {
                    insight.assessment.data_limitations.join("；")
                },
            },
            ReportBlock::Heading {
                level: 2,
                text: "预期产出".into(),
            },
            ReportBlock::Paragraph(if task.expected_output.trim().is_empty() {
                "尚未填写预期产出。".into()
            } else {
                task.expected_output.clone()
            }),
        ],
    }
}

fn project_report_document(
    project_name: &str,
    tasks: &[(Task, TaskTimeInsight)],
) -> ReportDocument {
    let total_seconds = tasks
        .iter()
        .map(|(_, insight)| insight.summary.lifecycle_total_seconds)
        .sum::<i64>();
    let rows = tasks
        .iter()
        .map(|(task, insight)| {
            vec![
                task.title.clone(),
                format!("{:?}", task.status),
                format_ledger_duration(insight.summary.lifecycle_total_seconds),
                format_ledger_duration(insight.summary.active_day_average_seconds),
                insight
                    .summary
                    .assignment_confidence
                    .map(|value| format!("{:.0}%", value * 100.0))
                    .unwrap_or_else(|| "—".into()),
            ]
        })
        .collect();
    ReportDocument {
        title: format!("项目报告：{project_name}"),
        metadata: vec![("任务数".into(), tasks.len().to_string())],
        blocks: vec![
            ReportBlock::Paragraph(format!(
                "项目内任务累计投入 {}。",
                format_ledger_duration(total_seconds)
            )),
            ReportBlock::Table {
                headers: vec![
                    "任务".into(),
                    "状态".into(),
                    "累计投入".into(),
                    "活跃日均".into(),
                    "归属可信度".into(),
                ],
                rows,
            },
            ReportBlock::Callout {
                kind: "info".into(),
                title: "方法".into(),
                body: "任务时长由活动片段与专注会话的区间并集计算；项目合计为任务汇总，任务间共享证据可能造成项目层面的重复，应结合证据质量复核。".into(),
            },
        ],
    }
}

fn is_dashboard_open_gesture(is_double_click: bool, button: MouseButton) -> bool {
    is_double_click && button == MouseButton::Left
}

fn show_main_dashboard(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app_data_dir();
            fs::create_dir_all(&data_dir)?;
            backup_database_before_1_3_migration(&data_dir)?;
            let database = Database::open(data_dir.join("monitor.db"))?;
            database.quarantine_unavailable_codex_jobs(now_ms())?;
            let browser_watcher_token = load_or_create_browser_watcher_token(&database)?;
            app.manage(DesktopState {
                service: Mutex::new(AppService::new(database)),
                ai_connection_health: AiConnectionHealthServiceState::new(
                    AiConnectionHealth::initial(),
                ),
                collection_runtime: Mutex::new(CollectionRuntimeState::default()),
                browser_watcher: BrowserWatcherServiceState::new(browser_watcher_token),
            });
            start_monitoring_worker(app.handle().clone());
            start_browser_watcher(app.handle().clone());
            start_browser_worker(app.handle().clone());
            start_ai_worker(app.handle().clone());
            start_ai_connection_health_worker(app.handle().clone());

            let menu = MenuBuilder::new(app)
                .text("show", "打开表盘")
                .text("pause", "暂停/恢复监测")
                .separator()
                .quit()
                .build()?;
            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("每日任务监测系统");
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.on_menu_event(|app, event| match event.id().as_ref() {
                "show" => show_main_dashboard(app),
                "pause" => {
                    if let Some(state) = app.try_state::<DesktopState>() {
                        if let Ok(service) = state.service.lock() {
                            if let Ok(settings) = service.get_settings() {
                                let _ = service.update_settings(SettingsPatch {
                                    monitoring_enabled: Some(!settings.monitoring_enabled),
                                    ..SettingsPatch::default()
                                });
                            }
                        }
                    }
                }
                _ => {}
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::DoubleClick { button, .. } = event {
                    if is_dashboard_open_gesture(true, button) {
                        show_main_dashboard(tray.app_handle());
                    }
                }
            })
            .build(app)?;

            if let Some(window) = app.get_webview_window("main") {
                let hidden_window = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = hidden_window.hide();
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_today_dashboard,
            resolve_app_identities,
            get_timeline,
            get_trends,
            get_trend_workbench,
            get_workflow,
            get_knowledge_graph,
            get_settings,
            list_system_fonts,
            list_ai_reviews,
            list_pending_ai_jobs,
            resolve_ai_review,
            revert_ai_auto_apply,
            retry_ai_review,
            bulk_retry_ai_jobs_with_current_mode,
            update_settings,
            set_monitoring_state,
            save_manual_classification,
            queue_segment_classification,
            save_daily_goal,
            get_daily_goal,
            get_daily_analysis,
            queue_daily_analysis,
            get_trend_analysis,
            queue_trend_analysis,
            get_trend_research_analysis,
            queue_trend_research_analysis,
            start_focus_session,
            complete_focus_session,
            get_browser_sources,
            get_collection_health,
            scan_browsers,
            list_ai_providers,
            save_custom_ai_provider,
            save_ai_provider_key,
            test_ai_provider,
            get_codex_health,
            test_codex_cli,
            get_ai_connection_health,
            refresh_ai_connection_health,
            run_ai_backfill,
            import_legacy_data,
            open_data_folder,
            export_markdown_report,
            export_trend_markdown,
            export_report,
            get_work_ledger,
            get_work_ledger_task_insight,
            get_work_ledger_project_insight,
            merge_work_ledger_tasks,
            save_work_ledger_project,
            archive_work_ledger_project,
            save_work_ledger_task,
            update_work_ledger_task_status,
            cancel_work_ledger_ai_task,
            add_work_ledger_progress,
            assign_work_ledger_evidence,
            remove_work_ledger_evidence,
            apply_work_ledger_suggestion,
            queue_workflow_ai_suggestions,
            run_ai_job_now,
            confirm_daily_goal_task,
            list_daily_goal_task_links,
            record_daily_actual_output_progress,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Daily Task Monitor");
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn is_browser_app(app: &str) -> bool {
    let app = app.to_ascii_lowercase();
    [
        "chrome",
        "google chrome",
        "msedge",
        "microsoft edge",
        "safari",
        "arc",
        "brave browser",
        "brave",
        "opera",
        "vivaldi",
        "360",
        "qqbrowser",
        "sogou",
    ]
    .iter()
    .any(|name| app.contains(name))
}

fn truncate_text(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn matches_app_exclusion(app: &str, exclusions: &[String]) -> bool {
    let app = app.trim().to_ascii_lowercase();
    exclusions.iter().any(|value| app.contains(value))
}

fn matches_domain_exclusion(domain: &str, exclusions: &[String]) -> bool {
    let domain = domain.trim().to_ascii_lowercase();
    exclusions
        .iter()
        .any(|value| domain == *value || domain.ends_with(&format!(".{value}")))
}

fn trend_provider_eligible(
    automation_enabled: bool,
    settings: &AppSettings,
    providers: &[AiProviderConfig],
) -> bool {
    if !automation_enabled {
        return false;
    }
    match settings.ai_execution_mode {
        AiExecutionMode::Codex => true,
        AiExecutionMode::ApiKey => selected_api_provider(settings, providers).is_ok(),
    }
}

fn build_ai_execution_snapshot(
    settings: &AppSettings,
    providers: &[AiProviderConfig],
    evidence_hash: &str,
    created_at_ms: i64,
) -> Result<AiExecutionSnapshot, String> {
    Ok(match settings.ai_execution_mode {
        AiExecutionMode::Codex => {
            let config = codex_execution_config_from_settings(settings);
            let resolved = resolve_codex_executable(&config.executable, CODEX_HEALTH_TIMEOUT_MS)
                .map_err(|error| error.to_string())?;
            AiExecutionSnapshot {
                execution_mode: AiExecutionMode::Codex,
                executor_id: resolved.path.to_string_lossy().into_owned(),
                model: config.model_label().to_string(),
                evidence_hash: evidence_hash.into(),
                created_at_ms,
            }
        }
        AiExecutionMode::ApiKey => {
            let selected = selected_api_provider(settings, providers)?;
            AiExecutionSnapshot {
                execution_mode: AiExecutionMode::ApiKey,
                executor_id: selected.id.clone(),
                model: selected.model.clone(),
                evidence_hash: evidence_hash.into(),
                created_at_ms,
            }
        }
    })
}

pub(crate) fn current_ai_execution_snapshot(
    service: &AppService,
    evidence_hash: &str,
    created_at_ms: i64,
) -> Result<AiExecutionSnapshot, String> {
    let settings = service.get_settings().map_err(|error| error.to_string())?;
    let providers = provider_templates(Some(service.database()))
        .into_iter()
        .map(|mut provider| {
            provider.has_credential = credential_exists(&provider.id);
            provider
        })
        .collect::<Vec<_>>();
    build_ai_execution_snapshot(&settings, &providers, evidence_hash, created_at_ms)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn enqueue_segment_classification_job(
    database: &Database,
    segment: &ActivitySegmentRecord,
    execution: &AiExecutionSnapshot,
    queued_at_ms: i64,
) -> rusqlite::Result<String> {
    let payload = serde_json::json!({
        "id": segment.id,
        "application": truncate_text(&segment.app, 80),
        "windowTitle": truncate_text(&segment.title, 240),
        "durationSeconds": segment.ended_at_ms.saturating_sub(segment.started_at_ms) / 1_000,
        "localCategory": segment.category,
    });
    let payload = serde_json::to_string(&payload)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let mut execution = execution.clone();
    execution.evidence_hash = database
        .classification_evidence_hash(&segment.id)?
        .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
    execution.created_at_ms = queued_at_ms;
    database.enqueue_ai_job_for_subject(
        "classify_segment",
        &segment.id,
        &payload,
        queued_at_ms,
        &execution,
    )
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn enqueue_segment_classification_job_with_privacy(
    database: &Database,
    segment: &ActivitySegmentRecord,
    excluded_apps: &[String],
    execution: &AiExecutionSnapshot,
    queued_at_ms: i64,
) -> Result<String, String> {
    if matches_app_exclusion(&segment.app, excluded_apps) {
        return Err("Activity segment is excluded from AI classification".into());
    }
    enqueue_segment_classification_job(database, segment, execution, queued_at_ms)
        .map_err(|error| error.to_string())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn enqueue_segment_for_ai(
    database: &Database,
    settings: &AppSettings,
    providers: &[AiProviderConfig],
    segment: &ActivitySegmentRecord,
    queued_at_ms: i64,
) {
    if let Ok(execution) = build_ai_execution_snapshot(settings, providers, "", queued_at_ms) {
        let _ = enqueue_segment_classification_job(database, segment, &execution, queued_at_ms);
    }
}

fn load_or_create_browser_watcher_token(
    database: &Database,
) -> Result<String, Box<dyn std::error::Error>> {
    const KEY: &str = "browser_watcher_token_v1";
    if let Some(value) = database.get_setting_json(KEY)? {
        if let Ok(token) = serde_json::from_str::<String>(&value) {
            if token.len() >= 32 {
                return Ok(token);
            }
        }
    }
    let entropy = format!(
        "{}\n{}\n{}\n{:p}",
        now_ms(),
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default(),
        database
    );
    let token = format!("{:x}", Sha256::digest(entropy.as_bytes()));
    database.set_setting_json(KEY, &serde_json::to_string(&token)?)?;
    Ok(token)
}

fn start_browser_watcher(app: tauri::AppHandle) {
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    let token = state.browser_watcher.token.clone();
    drop(state);
    let status_app = app.clone();
    let heartbeat_app = app.clone();
    std::thread::spawn(move || {
        run_browser_watcher_server(
            token,
            move |ready, error| {
                if let Some(state) = status_app.try_state::<DesktopState>() {
                    if let Ok(mut runtime) = state.browser_watcher.runtime.lock() {
                        runtime.listener_ready = ready;
                        runtime.listener_error = error;
                    }
                }
                let _ = status_app.emit(
                    COLLECTION_HEALTH_CHANGED_EVENT,
                    serde_json::json!({ "observedAtMs": now_ms() }),
                );
            },
            move |heartbeat| process_browser_heartbeat(&heartbeat_app, heartbeat),
        );
    });
}

fn process_browser_heartbeat(
    app: &tauri::AppHandle,
    heartbeat: BrowserHeartbeat,
) -> Result<(), String> {
    let received_at_ms = now_ms();
    let source_id = heartbeat.source_id.clone();
    let active = heartbeat.active && !heartbeat.private;
    let state = app
        .try_state::<DesktopState>()
        .ok_or("Application state is unavailable")?;
    let settings = state
        .service
        .lock()
        .map_err(|_| "Application service is unavailable")?
        .get_settings()
        .map_err(|error| error.to_string())?;
    if !settings.monitoring_enabled {
        let mut runtime = state
            .browser_watcher
            .runtime
            .lock()
            .map_err(|_| "Browser watcher state is unavailable")?;
        runtime.engine = BrowserWatcherEngine::default();
        runtime.connected_sources.clear();
        return Ok(());
    }
    let slice = {
        let mut runtime = state
            .browser_watcher
            .runtime
            .lock()
            .map_err(|_| "Browser watcher state is unavailable")?;
        let slice = runtime
            .engine
            .ingest(heartbeat, received_at_ms, &settings.excluded_domains)?;
        runtime.last_heartbeat_at_ms = Some(received_at_ms);
        if active {
            runtime
                .connected_sources
                .insert(source_id.clone(), received_at_ms);
        } else {
            runtime.connected_sources.remove(&source_id);
        }
        slice
    };
    if let Some(slice) = slice {
        state
            .service
            .lock()
            .map_err(|_| "Application service is unavailable")?
            .database()
            .insert_browser_activity_slice(&slice)
            .map_err(|error| error.to_string())?;
        if let Ok(mut runtime) = state.browser_watcher.runtime.lock() {
            runtime.last_persisted_at_ms = Some(slice.ended_at_ms);
        }
    }
    let _ = app.emit(
        COLLECTION_HEALTH_CHANGED_EVENT,
        serde_json::json!({ "observedAtMs": received_at_ms }),
    );
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn start_monitoring_worker(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        const GAP_THRESHOLD_MS: i64 = 15_000;
        const HISTORY_REPAIR_LOOKBACK_MS: i64 = 30 * 24 * 60 * 60 * 1_000;
        let mut collector = PlatformCollector::default();
        let mut engine = MonitorEngine::new(6 * 60 * 1_000);
        let mut was_monitoring = false;
        let mut historical_repair_checked = false;
        loop {
            let settings = app
                .try_state::<DesktopState>()
                .and_then(|state| state.service.lock().ok()?.get_settings().ok())
                .unwrap_or_default();
            if settings.monitoring_enabled {
                let idle_threshold_ms = settings.idle_threshold_minutes as i64 * 60_000;
                engine.set_idle_threshold_ms(idle_threshold_ms);
                let mut sample = collector.sample();
                if sample.domain.is_empty() && is_browser_application(&sample.app) {
                    let watcher_context = app.try_state::<DesktopState>().and_then(|state| {
                        state
                            .browser_watcher
                            .runtime
                            .lock()
                            .ok()?
                            .engine
                            .current_context(sample.observed_at_ms)
                    });
                    if let Some(context) = watcher_context {
                        sample.domain = context.domain;
                        if sample.title.is_empty() {
                            sample.title = context.title;
                        }
                    }
                }
                if let Some(state) = app.try_state::<DesktopState>() {
                    if let Ok(mut runtime) = state.collection_runtime.lock() {
                        runtime.last_idle_read_at_ms = Some(sample.observed_at_ms);
                        if !sample.app.trim().is_empty() && sample.app != "Unknown" {
                            runtime.last_app_available_at_ms = Some(sample.observed_at_ms);
                        }
                        if !sample.title.trim().is_empty() {
                            runtime.last_title_available_at_ms = Some(sample.observed_at_ms);
                        }
                    }
                }
                let idle_seconds = sample
                    .observed_at_ms
                    .saturating_sub(sample.last_input_at_ms)
                    / 1_000;
                let sample_id = format!("sample-{}", sample.observed_at_ms);
                let uptime_ms = system_uptime_ms();
                let boot_started_at_ms = sample.observed_at_ms.saturating_sub(uptime_ms);
                if let Some(state) = app.try_state::<DesktopState>() {
                    if let Ok(service) = state.service.lock() {
                        let database = service.database();
                        if !historical_repair_checked {
                            let already_repaired = database
                                .get_setting_json("monitoring_gap_repair_v1_completed")
                                .ok()
                                .flatten()
                                .as_deref()
                                == Some("true");
                            if already_repaired
                                || database
                                    .repair_recent_monitoring_gaps(
                                        sample.observed_at_ms,
                                        HISTORY_REPAIR_LOOKBACK_MS,
                                        GAP_THRESHOLD_MS,
                                    )
                                    .and_then(|_| {
                                        database.set_setting_json(
                                            "monitoring_gap_repair_v1_completed",
                                            "true",
                                        )
                                    })
                                    .is_ok()
                            {
                                historical_repair_checked = true;
                            }
                        }
                        let rules = database.list_manual_rules().unwrap_or_default();
                        engine.set_manual_rules(rules.clone());
                        if sample.domain.is_empty() && is_browser_app(&sample.app) {
                            if let Ok(Some((domain, _title))) = database.latest_browser_context(
                                sample.observed_at_ms.saturating_sub(5 * 60_000),
                                sample.observed_at_ms,
                            ) {
                                sample.domain = domain;
                            }
                        }
                        let checkpoint = database
                            .load_monitoring_continuity_checkpoint()
                            .ok()
                            .flatten();
                        let mut persisted_segments = Vec::new();
                        if let Some(gap) = checkpoint.as_ref().and_then(|checkpoint| {
                            monitoring_gap_from_checkpoint(
                                checkpoint,
                                sample.observed_at_ms,
                                boot_started_at_ms,
                                uptime_ms,
                                GAP_THRESHOLD_MS,
                            )
                        }) {
                            if let Ok(mut runtime) = state.collection_runtime.lock() {
                                runtime.last_continuity_gap_at_ms = Some(sample.observed_at_ms);
                            }
                            if let Some(previous) = engine.take_current() {
                                persisted_segments.push(previous);
                            }
                            persisted_segments.push(continuity_gap_segment(&gap));
                            engine = MonitorEngine::new(idle_threshold_ms);
                            engine.set_manual_rules(rules);
                        }
                        let output = engine.ingest(sample.clone());
                        let providers = provider_templates(Some(database))
                            .into_iter()
                            .map(|mut provider| {
                                provider.has_credential = credential_exists(&provider.id);
                                provider
                            })
                            .collect::<Vec<_>>();
                        persisted_segments.extend(output.completed.iter().cloned());
                        persisted_segments.push(output.current.clone());
                        let persisted = database.record_monitoring_tick(
                            &ActivitySampleWrite {
                                id: sample_id,
                                sampled_at_ms: sample.observed_at_ms,
                                app: sample.app.clone(),
                                app_path: sample.app_path.clone(),
                                title: sample.title.clone(),
                                idle_seconds,
                                key_presses: sample.key_presses,
                                mouse_events: sample.mouse_events,
                                media_playing: sample.media_playing,
                            },
                            &persisted_segments,
                            &MonitoringContinuityCheckpoint {
                                expected_tracking: true,
                                last_observed_at_ms: sample.observed_at_ms,
                                last_boot_started_at_ms: boot_started_at_ms,
                                last_uptime_ms: uptime_ms,
                                updated_at_ms: sample.observed_at_ms,
                            },
                        );
                        if persisted.is_ok() {
                            if let Ok(mut runtime) = state.collection_runtime.lock() {
                                runtime.last_persisted_at_ms = Some(sample.observed_at_ms);
                            }
                            let _ = app.emit(
                                ACTIVITY_CHANGED_EVENT,
                                serde_json::json!({
                                    "observedAtMs": sample.observed_at_ms,
                                }),
                            );
                            for segment in output.completed {
                                if background_ai_gate_enabled(
                                    &settings,
                                    AiAutomationGate::Classification,
                                ) && segment.needs_review
                                    && !matches_app_exclusion(&segment.app, &settings.excluded_apps)
                                {
                                    enqueue_segment_for_ai(
                                        database,
                                        &settings,
                                        &providers,
                                        &segment,
                                        sample.observed_at_ms,
                                    );
                                }
                            }
                            if background_ai_gate_enabled(
                                &settings,
                                AiAutomationGate::Classification,
                            ) && output.current.needs_review
                                && !matches_app_exclusion(
                                    &output.current.app,
                                    &settings.excluded_apps,
                                )
                            {
                                enqueue_segment_for_ai(
                                    database,
                                    &settings,
                                    &providers,
                                    &output.current,
                                    sample.observed_at_ms,
                                );
                            }
                        }
                    }
                }
            } else if was_monitoring {
                let observed_at_ms = now_ms();
                let uptime_ms = system_uptime_ms();
                let checkpoint = MonitoringContinuityCheckpoint {
                    expected_tracking: false,
                    last_observed_at_ms: observed_at_ms,
                    last_boot_started_at_ms: observed_at_ms.saturating_sub(uptime_ms),
                    last_uptime_ms: uptime_ms,
                    updated_at_ms: observed_at_ms,
                };
                let segment = engine.take_current();
                if let Some(state) = app.try_state::<DesktopState>() {
                    if let Ok(service) = state.service.lock() {
                        let persisted = service
                            .database()
                            .record_monitoring_pause(segment.as_ref(), &checkpoint);
                        if persisted.is_ok() {
                            let _ = app.emit(
                                ACTIVITY_CHANGED_EVENT,
                                serde_json::json!({
                                    "observedAtMs": observed_at_ms,
                                }),
                            );
                        }
                    }
                }
            }
            was_monitoring = settings.monitoring_enabled;
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
    });
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn start_monitoring_worker(_app: tauri::AppHandle) {}

fn start_browser_worker(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok();
        loop {
            let end_ms = now_ms();
            let start_ms = end_ms.saturating_sub(30 * 60 * 1_000);
            let scan = if let Some(state) = app.try_state::<DesktopState>() {
                if let Ok(service) = state.service.lock() {
                    let settings = service.get_settings().unwrap_or_default();
                    if settings.monitoring_enabled {
                        Some(scan_browser_sources(
                            service.database(),
                            start_ms,
                            end_ms,
                            &settings.excluded_domains,
                        ))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            if scan.is_some() {
                if let Some(state) = app.try_state::<DesktopState>() {
                    if let Ok(mut runtime) = state.collection_runtime.lock() {
                        runtime.last_browser_history_scan_at_ms = Some(end_ms);
                    }
                }
            }
            if let (Some(runtime), Some(scan)) = (&runtime, scan) {
                for (visit_id, url, domain, title) in scan.new_visits.into_iter().take(20) {
                    let monitoring_enabled = app
                        .try_state::<DesktopState>()
                        .and_then(|state| {
                            state
                                .service
                                .lock()
                                .ok()?
                                .get_settings()
                                .ok()
                                .map(|settings| settings.monitoring_enabled)
                        })
                        .unwrap_or(false);
                    if !monitoring_enabled {
                        break;
                    }
                    let fetched = runtime.block_on(fetch_public_html_summary(&url, 2_000));
                    if let Some(state) = app.try_state::<DesktopState>() {
                        if let Ok(service) = state.service.lock() {
                            if !service
                                .get_settings()
                                .map(|settings| settings.monitoring_enabled)
                                .unwrap_or(false)
                            {
                                continue;
                            }
                            let (summary_text, text_snippet, fetch_status) = match fetched {
                                Ok(summary) => {
                                    let summary_text = if summary.description.is_empty() {
                                        summary.title
                                    } else {
                                        summary.description
                                    };
                                    (summary_text, summary.text, "public_html".to_string())
                                }
                                Err(error) => (
                                    String::new(),
                                    String::new(),
                                    format!("metadata_only:{error}"),
                                ),
                            };
                            let _ = service.database().save_page_snapshot(
                                &visit_id,
                                &summary_text,
                                &text_snippet,
                                &fetch_status,
                            );
                            let payload = serde_json::json!({
                                "visitId": &visit_id,
                                "domain": domain,
                                "title": title,
                                "summary": truncate_text(&summary_text, 500),
                                "textSnippet": truncate_text(&text_snippet, 1_000),
                            });
                            if let Ok(payload) = serde_json::to_string(&payload) {
                                let settings = service.get_settings().unwrap_or_default();
                                let providers = provider_templates(Some(service.database()))
                                    .into_iter()
                                    .map(|mut provider| {
                                        provider.has_credential = credential_exists(&provider.id);
                                        provider
                                    })
                                    .collect::<Vec<_>>();
                                let queued_at_ms = now_ms();
                                let execution = build_ai_execution_snapshot(
                                    &settings,
                                    &providers,
                                    &visit_id,
                                    queued_at_ms,
                                );
                                if let Ok(execution) = execution {
                                    let _ = service.database().enqueue_ai_job_for_subject(
                                        "classify_page",
                                        &visit_id,
                                        &payload,
                                        queued_at_ms,
                                        &execution,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(120));
        }
    });
}

fn start_ai_worker(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let Ok(runtime) = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(1)
            .build()
        else {
            return;
        };
        runtime.block_on(async move {
            loop {
                let _ = process_one_ai_job(&app).await;
                tokio::time::sleep(std::time::Duration::from_secs(20)).await;
            }
        });
    });
}

fn start_ai_connection_health_worker(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        runtime.block_on(async move {
            let mut interval = tokio::time::interval(AI_CONNECTION_HEALTH_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                let _ = refresh_ai_connection_health_inner(&app).await;
            }
        });
    });
}

async fn dispatch_ai_job_by_snapshot<T, ApiFuture, CodexFuture>(
    execution: &AiExecutionSnapshot,
    api_backend: impl FnOnce() -> ApiFuture,
    codex_backend: impl FnOnce() -> CodexFuture,
) -> T
where
    ApiFuture: std::future::Future<Output = T>,
    CodexFuture: std::future::Future<Output = T>,
{
    match execution.execution_mode {
        AiExecutionMode::ApiKey => api_backend().await,
        AiExecutionMode::Codex => codex_backend().await,
    }
}

#[derive(Debug)]
struct ApiDailyAnalysisJobPayload {
    date: String,
    evidence_hash: String,
    start_ms: i64,
    end_ms: i64,
    activity_scope: ActivityScope,
}

#[derive(Debug)]
struct ApiAiJobFailure {
    message: String,
    kind: AiExecutionErrorKind,
}

enum ExecutedAiJob {
    WorkLedgerAssignment {
        response_content: String,
        execution: AiExecutionOutput,
    },
    DailyAnalysis {
        payload: ApiDailyAnalysisJobPayload,
        analysis: DailyAnalysisAiResult,
        execution: AiExecutionOutput,
    },
    TrendAnalysis {
        queued: TrendAnalysisJobPayload,
        analysis: TrendAnalysisAiResult,
        execution: AiExecutionOutput,
    },
    TrendResearchAnalysis {
        queued: TrendResearchJobPayload,
        analysis: TrendResearchAnalysis,
        execution: AiExecutionOutput,
    },
    Classification {
        payload: serde_json::Value,
        classification: AiClassificationResult,
        execution: AiExecutionOutput,
    },
}

async fn execute_production_ai_job(
    job: &AiJob,
    backends: &AiExecutionBackends,
) -> Result<ExecutedAiJob, CodexExecutionError> {
    if job.kind == "work_ledger_assignment" {
        let queued = parse_work_ledger_assignment_job(&job.payload_json).map_err(|error| {
            CodexExecutionError::invalid_job(format!(
                "Invalid queued work ledger assignment payload: {error}"
            ))
        })?;
        let (response_content, execution) =
            execute_work_ledger_assignment_request(job, &queued, backends).await?;
        return Ok(ExecutedAiJob::WorkLedgerAssignment {
            response_content,
            execution,
        });
    }

    if job.kind == "daily_analysis" {
        let payload =
            parse_api_daily_analysis_job_payload(&job.payload_json).map_err(|error| match error
                .kind
            {
                AiExecutionErrorKind::InvalidJob => CodexExecutionError::invalid_job(error.message),
                _ => CodexExecutionError::new(ExecutorErrorKind::Unknown, error.message, None),
            })?;
        let (analysis, execution) =
            execute_daily_analysis_request(job, &job.payload_json, backends).await?;
        return Ok(ExecutedAiJob::DailyAnalysis {
            payload,
            analysis,
            execution,
        });
    }

    if job.kind == "trend_analysis" {
        let queued: TrendAnalysisJobPayload =
            serde_json::from_str(&job.payload_json).map_err(|error| {
                CodexExecutionError::invalid_job(format!(
                    "Invalid queued trend analysis payload: {error}"
                ))
            })?;
        let (analysis, execution) =
            execute_trend_analysis_request(job, &queued.evidence, backends).await?;
        return Ok(ExecutedAiJob::TrendAnalysis {
            queued,
            analysis,
            execution,
        });
    }

    if job.kind == "trend_research_analysis" {
        let queued: TrendResearchJobPayload =
            serde_json::from_str(&job.payload_json).map_err(|error| {
                CodexExecutionError::invalid_job(format!(
                    "Invalid queued trend research payload: {error}"
                ))
            })?;
        let (analysis, execution) = execute_trend_research_request(job, &queued, backends).await?;
        return Ok(ExecutedAiJob::TrendResearchAnalysis {
            queued,
            analysis,
            execution,
        });
    }

    if !is_generic_classification_job(&job.kind) {
        return Err(CodexExecutionError::invalid_job(format!(
            "Unsupported AI job kind: {}",
            job.kind
        )));
    }
    let payload: serde_json::Value = serde_json::from_str(&job.payload_json).map_err(|error| {
        CodexExecutionError::invalid_job(format!("Invalid queued payload: {error}"))
    })?;
    let (classification, execution) =
        execute_classification_request(job, &job.payload_json, backends).await?;
    Ok(ExecutedAiJob::Classification {
        payload,
        classification,
        execution,
    })
}

impl ApiAiJobFailure {
    fn invalid_job(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: AiExecutionErrorKind::InvalidJob,
        }
    }
}

fn parse_api_daily_analysis_job_payload(
    payload_json: &str,
) -> Result<ApiDailyAnalysisJobPayload, ApiAiJobFailure> {
    let payload: serde_json::Value = serde_json::from_str(payload_json).map_err(|error| {
        ApiAiJobFailure::invalid_job(format!("Invalid queued daily analysis payload: {error}"))
    })?;
    let date = payload
        .get("date")
        .and_then(|value| value.as_str())
        .ok_or_else(|| ApiAiJobFailure::invalid_job("Queued daily analysis payload has no date"))?;
    let evidence_hash = payload
        .get("evidenceHash")
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            ApiAiJobFailure::invalid_job("Queued daily analysis payload has no evidence hash")
        })?;
    let start_ms = payload
        .get("startMs")
        .and_then(|value| value.as_i64())
        .ok_or_else(|| {
            ApiAiJobFailure::invalid_job("Queued daily analysis payload has no start time")
        })?;
    let end_ms = payload
        .get("endMs")
        .and_then(|value| value.as_i64())
        .ok_or_else(|| {
            ApiAiJobFailure::invalid_job("Queued daily analysis payload has no end time")
        })?;
    let activity_scope = match payload.get("activityScope") {
        Some(value) => serde_json::from_value(value.clone()).map_err(|_| {
            ApiAiJobFailure::invalid_job(
                "Queued daily analysis payload has an invalid activity scope",
            )
        })?,
        None => ActivityScope::All,
    };
    Ok(ApiDailyAnalysisJobPayload {
        date: date.to_owned(),
        evidence_hash: evidence_hash.to_owned(),
        start_ms,
        end_ms,
        activity_scope,
    })
}

pub fn complete_terminal_ai_job_error(
    database: &Database,
    job: &AiJob,
    finished_at_ms: i64,
    error: &str,
    error_kind: AiExecutionErrorKind,
    executor_id: Option<&str>,
    model: Option<&str>,
    exit_code: Option<i32>,
) -> Result<bool, rusqlite::Error> {
    database.complete_ai_job_generation_error(
        &job.id,
        job.generation,
        finished_at_ms,
        error,
        error_kind,
        executor_id,
        model,
        exit_code,
    )
}

#[cfg(test)]
fn complete_terminal_api_ai_job_failure(
    database: &Database,
    job: &AiJob,
    finished_at_ms: i64,
    error: &ApiAiJobFailure,
) -> Result<bool, rusqlite::Error> {
    complete_terminal_ai_job_error(
        database,
        job,
        finished_at_ms,
        &error.message,
        error.kind,
        Some(job.execution.executor_id.as_str()),
        Some(job.execution.model.as_str()),
        None,
    )
}

fn process_invalid_work_ledger_assignment_job(
    database: &Database,
    job: &AiJob,
) -> Result<bool, String> {
    WorkLedgerService::new(WorkLedgerRepository::new(database))
        .complete_invalid_ai_assignment_job(
            job,
            &job.execution.executor_id,
            &job.execution.model,
            Some(0),
        )
        .map_err(|error| error.to_string())
}

async fn process_one_ai_job(app: &tauri::AppHandle) -> Result<bool, String> {
    process_one_ai_job_with_manual_override(app, false).await
}

async fn process_one_ai_job_with_manual_override(
    app: &tauri::AppHandle,
    manual_override: bool,
) -> Result<bool, String> {
    let (job, _settings, providers) = {
        let Some(state) = app.try_state::<DesktopState>() else {
            return Ok(false);
        };
        let service = state
            .service
            .lock()
            .map_err(|_| "Application state is unavailable")?;
        let settings = service.get_settings().map_err(|error| error.to_string())?;
        if !settings.ai_backfill_enabled && !manual_override {
            return Ok(false);
        }
        let Some(job) = service
            .database()
            .claim_next_due_ai_job(now_ms())
            .map_err(|error| error.to_string())?
        else {
            return Ok(false);
        };
        (
            job,
            settings,
            provider_templates(Some(service.database()))
                .into_iter()
                .map(|mut provider| {
                    provider.has_credential = credential_exists(&provider.id);
                    provider
                })
                .collect::<Vec<_>>(),
        )
    };

    if job.kind == "work_ledger_assignment"
        && parse_work_ledger_assignment_job(&job.payload_json).is_err()
    {
        let state = app
            .try_state::<DesktopState>()
            .ok_or_else(|| "Application state is unavailable".to_string())?;
        let service = state
            .service
            .lock()
            .map_err(|_| "Application state is unavailable".to_string())?;
        let result = process_invalid_work_ledger_assignment_job(service.database(), &job);
        let _ = app.emit(
            WORKFLOW_CHANGED_EVENT,
            serde_json::json!({
                "jobId": job.id,
                "status": "failed",
            }),
        );
        return result;
    }

    let result = dispatch_ai_job_by_snapshot(
        &job.execution,
        || process_api_ai_job(app, &job, providers),
        || process_codex_ai_job(app, &job),
    )
    .await;
    if matches!(result, Ok(true)) {
        emit_analysis_changed(app, &job);
    }
    if job.kind == "work_ledger_assignment" {
        let _ = app.emit(
            WORKFLOW_CHANGED_EVENT,
            serde_json::json!({
                "jobId": job.id,
                "status": if result.is_ok() { "completed" } else { "failed" },
            }),
        );
    }
    result
}

fn emit_analysis_changed(app: &tauri::AppHandle, job: &AiJob) {
    let page = match job.kind.as_str() {
        "daily_analysis" => "daily",
        "trend_analysis" | "trend_research_analysis" => "trends",
        _ => return,
    };
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(&job.payload_json) else {
        return;
    };
    let evidence = payload.get("evidence").unwrap_or(&payload);
    let scope = payload
        .get("activityScope")
        .and_then(|value| serde_json::from_value::<ActivityScope>(value.clone()).ok())
        .unwrap_or_default();
    let evidence_hash = payload
        .get("evidenceHash")
        .or_else(|| evidence.get("evidenceHash"))
        .or_else(|| {
            payload
                .get("input")
                .and_then(|input| input.get("evidenceHash"))
        })
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let mut event = serde_json::json!({
        "page": page,
        "scope": scope,
        "evidenceHash": evidence_hash,
    });
    if page == "daily" {
        if let Some(date) = payload.get("date").and_then(serde_json::Value::as_str) {
            event["date"] = serde_json::Value::String(date.to_owned());
        }
    } else if let Some(request) = payload.get("request") {
        if let Some(start) = request.get("startDate").and_then(serde_json::Value::as_str) {
            event["rangeStart"] = serde_json::Value::String(start.to_owned());
        }
        if let Some(end) = request.get("endDate").and_then(serde_json::Value::as_str) {
            event["rangeEnd"] = serde_json::Value::String(end.to_owned());
        }
    } else if let Some(range) = evidence.get("range") {
        if let Some(start) = range.get("startDate").and_then(serde_json::Value::as_str) {
            event["rangeStart"] = serde_json::Value::String(start.to_owned());
        }
        if let Some(end) = range.get("endDate").and_then(serde_json::Value::as_str) {
            event["rangeEnd"] = serde_json::Value::String(end.to_owned());
        }
    }
    let _ = app.emit(ANALYSIS_CHANGED_EVENT, event);
}

async fn process_api_ai_job(
    app: &tauri::AppHandle,
    job: &AiJob,
    providers: Vec<AiProviderConfig>,
) -> Result<bool, String> {
    let api_providers = providers
        .into_iter()
        .filter_map(|provider| {
            let api_key = Entry::new(current_edition_identity().credential_service, &provider.id)
                .ok()?
                .get_password()
                .ok()?;
            Some(crate::ai_executor::ApiProviderCredential { provider, api_key })
        })
        .collect();
    let backends = AiExecutionBackends {
        api_providers,
        configured_codex: None,
    };
    let result = process_executed_ai_job(app, job, &backends).await;
    update_ai_connection_health_from_queue(app, job, &result);
    if let Err(error) = &result {
        let audit = execution_failure_audit(job, error);
        if let Some(state) = app.try_state::<DesktopState>() {
            if let Ok(service) = state.service.lock() {
                let _ = record_ai_job_failure(service.database(), job, error, &audit);
            }
        }
    }
    result.map_err(|error| error.to_string())
}

#[tauri::command]
async fn run_ai_job_now(app: tauri::AppHandle) -> Result<bool, String> {
    process_one_ai_job_with_manual_override(&app, true).await
}

async fn process_codex_ai_job(app: &tauri::AppHandle, job: &AiJob) -> Result<bool, String> {
    let result = process_codex_ai_job_inner(app, job).await;
    update_ai_connection_health_from_queue(app, job, &result);
    if let Err(error) = &result {
        let audit = execution_failure_audit(job, error);
        if let Some(state) = app.try_state::<DesktopState>() {
            if let Ok(service) = state.service.lock() {
                let _ = record_ai_job_failure(service.database(), job, error, &audit);
            }
        }
    }
    result.map_err(|error| error.to_string())
}

fn update_ai_connection_health_from_queue(
    app: &tauri::AppHandle,
    job: &AiJob,
    result: &Result<bool, CodexExecutionError>,
) {
    let Some(state) = app.try_state::<DesktopState>() else {
        return;
    };
    let current = state.ai_connection_health.current();
    let current_execution = state
        .service
        .lock()
        .ok()
        .and_then(|service| current_ai_execution_snapshot(&service, "", now_ms()).ok());
    if !current_execution
        .as_ref()
        .is_some_and(|execution| queue_job_matches_current_execution(job, execution))
    {
        return;
    }
    let checked_at_ms = now_ms();
    let (status, diagnostic, model, verified_at_ms) = match result {
        Ok(_) => (
            AiConnectionHealthStatus::Healthy,
            None,
            job.execution.model.clone(),
            Some(checked_at_ms),
        ),
        Err(error) => {
            let status = match error.kind {
                ExecutorErrorKind::Authentication | ExecutorErrorKind::PermissionDenied => {
                    AiConnectionHealthStatus::PermissionDenied
                }
                ExecutorErrorKind::RateLimited => AiConnectionHealthStatus::RateLimited,
                ExecutorErrorKind::Timeout => AiConnectionHealthStatus::TimedOut,
                ExecutorErrorKind::Network
                | ExecutorErrorKind::CliUnavailable
                | ExecutorErrorKind::NotConfigured => AiConnectionHealthStatus::Unavailable,
                _ => AiConnectionHealthStatus::Error,
            };
            (
                status,
                Some(sanitize_ai_diagnostic(&error.to_string())),
                error
                    .model
                    .clone()
                    .unwrap_or_else(|| job.execution.model.clone()),
                Some(checked_at_ms),
            )
        }
    };
    let health = state.ai_connection_health.store(AiConnectionHealth {
        execution_mode: current.execution_mode,
        executor_id: current.executor_id,
        executor_label: current.executor_label,
        model,
        status,
        verification_level: AiConnectionVerificationLevel::Inference,
        source: AiConnectionHealthSource::Queue,
        checked_at_ms,
        verified_at_ms,
        diagnostic,
    });
    emit_ai_connection_health(app, &health);
}

fn queue_job_matches_current_execution(job: &AiJob, current: &AiExecutionSnapshot) -> bool {
    job.execution.execution_mode == current.execution_mode
        && job.execution.executor_id == current.executor_id
        && job.execution.model == current.model
}

struct ExecutionFailureAudit {
    finished_at_ms: i64,
    executor_id: String,
    model: String,
}

fn record_ai_job_failure(
    database: &Database,
    job: &AiJob,
    error: &CodexExecutionError,
    audit: &ExecutionFailureAudit,
) -> Result<bool, rusqlite::Error> {
    let error_kind = codex_audit_kind(error);
    if error_kind == AiExecutionErrorKind::InvalidJob {
        complete_terminal_ai_job_error(
            database,
            job,
            audit.finished_at_ms,
            &error.to_string(),
            error_kind,
            Some(audit.executor_id.as_str()),
            Some(audit.model.as_str()),
            error.exit_code,
        )
    } else {
        database.fail_ai_job_generation(
            &job.id,
            job.generation,
            audit.finished_at_ms,
            &error.to_string(),
            error_kind,
            Some(audit.executor_id.as_str()),
            Some(audit.model.as_str()),
            error.exit_code,
        )
    }
}

fn execution_failure_audit(job: &AiJob, error: &CodexExecutionError) -> ExecutionFailureAudit {
    let executor_id = error
        .executor_id
        .clone()
        .unwrap_or_else(|| job.execution.executor_id.clone());
    let model = error.model.clone().unwrap_or_else(|| {
        if job.execution.execution_mode == AiExecutionMode::Codex
            && job.execution.model.trim().is_empty()
        {
            "cli-default".to_string()
        } else {
            job.execution.model.clone()
        }
    });
    let finished_at_ms = error.duration_ms.map_or_else(now_ms, |duration_ms| {
        let duration_ms = i64::try_from(duration_ms).unwrap_or(i64::MAX);
        job.started_at_ms
            .unwrap_or_else(now_ms)
            .saturating_add(duration_ms)
    });
    ExecutionFailureAudit {
        finished_at_ms,
        executor_id,
        model,
    }
}

async fn process_executed_ai_job(
    app: &tauri::AppHandle,
    job: &AiJob,
    backends: &AiExecutionBackends,
) -> Result<bool, CodexExecutionError> {
    let executed = execute_production_ai_job(job, backends).await?;
    persist_executed_ai_job(app, job, executed)
}

#[allow(clippy::too_many_arguments)]
pub fn persist_page_classification_job_result(
    database: &Database,
    job: &AiJob,
    visit_id: &str,
    classification_json: &str,
    executor_id: &str,
    model: &str,
    exit_code: Option<i32>,
    finished_at_ms: i64,
) -> Result<(), String> {
    let applied = database
        .complete_page_classification_job_generation(
            &job.id,
            job.generation,
            visit_id,
            classification_json,
            executor_id,
            model,
            exit_code,
            finished_at_ms,
        )
        .map_err(|error| error.to_string())?;
    require_classification_completion_applied(applied)
}

fn persist_executed_ai_job(
    app: &tauri::AppHandle,
    job: &AiJob,
    executed: ExecutedAiJob,
) -> Result<bool, CodexExecutionError> {
    let state = app
        .try_state::<DesktopState>()
        .ok_or_else(|| CodexExecutionError::persistence("Application state is unavailable"))?;
    let service = state
        .service
        .lock()
        .map_err(|_| CodexExecutionError::persistence("Application state is unavailable"))?;

    match executed {
        ExecutedAiJob::WorkLedgerAssignment {
            response_content,
            execution,
        } => {
            let finished_at_ms = execution_finished_at_ms(job, &execution);
            let duration_ms = i64::try_from(execution.duration_ms).unwrap_or(i64::MAX);
            let result = WorkLedgerService::new(WorkLedgerRepository::new(service.database()))
                .consume_ai_assignment_job_response(
                    job,
                    &response_content,
                    &execution.executor_id,
                    &execution.model,
                    finished_at_ms,
                    duration_ms,
                );
            result
                .map_err(|error| match error {
                    WorkLedgerAssignmentConsumptionError::InvalidResponse(error) => {
                        CodexExecutionError::invalid_response(error)
                    }
                    WorkLedgerAssignmentConsumptionError::InvalidJob(error) => {
                        CodexExecutionError::invalid_job(error)
                    }
                    WorkLedgerAssignmentConsumptionError::Persistence(error) => {
                        CodexExecutionError::persistence(error)
                    }
                })
                .map_err(|error| error.with_output_context(&execution))
        }
        ExecutedAiJob::DailyAnalysis {
            payload,
            analysis,
            execution,
        } => service
            .complete_ai_daily_analysis_job_scoped(
                &job.id,
                job.generation,
                &payload.date,
                payload.start_ms,
                payload.end_ms,
                payload.activity_scope,
                &payload.evidence_hash,
                &analysis.portrait,
                &analysis.recommendation,
                &analysis.findings,
                &execution.executor_id,
                &execution.model,
                execution.exit_code,
                execution_finished_at_ms(job, &execution),
            )
            .map_err(|error| CodexExecutionError::persistence(error.to_string()))
            .map_err(|error| error.with_output_context(&execution)),
        ExecutedAiJob::TrendAnalysis {
            queued,
            analysis,
            execution,
        } => {
            let result = service
                .complete_ai_trend_analysis_job(
                    &job.id,
                    job.generation,
                    &queued,
                    &analysis.summary,
                    &analysis.observations,
                    &analysis.suggestions,
                    &execution.executor_id,
                    &execution.model,
                    analysis.confidence,
                    execution_finished_at_ms(job, &execution),
                )
                .map_err(|error| CodexExecutionError::persistence(error.to_string()))
                .and_then(|applied| {
                    require_trend_completion_applied(applied)
                        .map_err(CodexExecutionError::persistence)
                        .map(|()| true)
                });
            result.map_err(|error| error.with_output_context(&execution))
        }
        ExecutedAiJob::TrendResearchAnalysis {
            queued,
            analysis,
            execution,
        } => {
            let result = service
                .complete_ai_trend_research_job(&job.id, job.generation, &queued, &analysis)
                .map_err(CodexExecutionError::persistence)
                .and_then(|applied| {
                    require_trend_completion_applied(applied)
                        .map_err(CodexExecutionError::persistence)
                        .map(|()| true)
                });
            result.map_err(|error| error.with_output_context(&execution))
        }
        ExecutedAiJob::Classification {
            payload,
            classification,
            execution,
        } => {
            let result = (|| {
                let finished_at_ms = execution_finished_at_ms(job, &execution);
                let model_version = match job.execution.execution_mode {
                    AiExecutionMode::ApiKey => {
                        format!("{}/{}", execution.executor_id, execution.model)
                    }
                    AiExecutionMode::Codex => format!("codex/{}", execution.model),
                };
                if job.kind == "classify_segment" {
                    let segment_id = payload
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| {
                            CodexExecutionError::invalid_job("Queued segment payload has no id")
                        })?;
                    let applied = service
                        .database()
                        .consume_segment_classification_job_generation(
                            &job.id,
                            job.generation,
                            segment_id,
                            classification.category,
                            classification.video_purpose,
                            classification.confidence,
                            &classification.reason,
                            &model_version,
                            &execution.executor_id,
                            &execution.model,
                            execution.exit_code,
                            finished_at_ms,
                        )
                        .map_err(|error| CodexExecutionError::persistence(error.to_string()))?;
                    require_classification_completion_applied(applied)
                        .map_err(CodexExecutionError::persistence)?;
                } else {
                    let visit_id = payload
                        .get("visitId")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| {
                            CodexExecutionError::invalid_job("Queued page payload has no visit id")
                        })?;
                    let summary = serde_json::to_string(&classification).map_err(|error| {
                        CodexExecutionError::invalid_response(error.to_string())
                    })?;
                    persist_page_classification_job_result(
                        service.database(),
                        job,
                        visit_id,
                        &summary,
                        &execution.executor_id,
                        &execution.model,
                        execution.exit_code,
                        finished_at_ms,
                    )
                    .map_err(CodexExecutionError::persistence)?;
                }
                Ok(true)
            })();
            result.map_err(|error: CodexExecutionError| error.with_output_context(&execution))
        }
    }
}

fn execution_finished_at_ms(job: &AiJob, execution: &AiExecutionOutput) -> i64 {
    let duration_ms = i64::try_from(execution.duration_ms).unwrap_or(i64::MAX);
    job.started_at_ms
        .unwrap_or_else(now_ms)
        .saturating_add(duration_ms)
}

async fn process_codex_ai_job_inner(
    app: &tauri::AppHandle,
    job: &AiJob,
) -> Result<bool, CodexExecutionError> {
    process_executed_ai_job(app, job, &AiExecutionBackends::default()).await
}

fn is_generic_classification_job(kind: &str) -> bool {
    matches!(kind, "classify_segment" | "classify_page")
}

fn require_trend_completion_applied(applied: bool) -> Result<(), String> {
    if applied {
        Ok(())
    } else {
        Err("Trend analysis completion lease or evidence is no longer current".to_string())
    }
}

fn require_classification_completion_applied(applied: bool) -> Result<(), String> {
    if applied {
        Ok(())
    } else {
        Err("Segment classification completion lease or segment is no longer current".to_string())
    }
}

#[cfg(test)]
fn api_failure_kind_from_consumption_error(
    error: &WorkLedgerAssignmentConsumptionError,
) -> AiExecutionErrorKind {
    error.ai_error_kind()
}

#[cfg(test)]
fn build_trend_analysis_request_body(
    provider: &AiProviderConfig,
    payload_json: &str,
    allowed: &TrendAnalysisAllowedCandidates,
) -> serde_json::Value {
    let mut evidence: serde_json::Value =
        serde_json::from_str(payload_json).expect("validated trend evidence serializes");
    if let Some(evidence) = evidence.as_object_mut() {
        evidence.remove("workLedger");
    }
    let user_content = serde_json::json!({
        "evidence": evidence,
        "allowedCandidates": allowed,
    })
    .to_string();
    let mut body = serde_json::json!({
        "model": provider.model,
        "temperature": 0.1,
        "messages": [
            {
                "role": "system",
                "content": "Analyze only the supplied aggregate TrendEvidence. Return one strict JSON object with exactly summary, observations, suggestions, and confidence. Select text exactly from allowedCandidates: summary must equal one summaries item; every observation and suggestion must equal an item from its corresponding list. Do not rewrite, combine, or add any text. Do not repeat items. Keep observations and suggestions non-empty. Confidence must be a number from zero to one."
            },
            { "role": "user", "content": user_content }
        ]
    });
    apply_provider_chat_options(provider, &mut body);
    body
}

#[cfg(test)]
fn apply_provider_chat_options(provider: &AiProviderConfig, body: &mut serde_json::Value) {
    if provider.id == "modelscope" {
        body["enable_thinking"] = serde_json::Value::Bool(false);
    }
}

fn codex_execution_config_from_settings(settings: &AppSettings) -> CodexExecutionConfig {
    CodexExecutionConfig {
        executable: settings.codex_executable.clone(),
        model: settings.codex_model.trim().to_string(),
    }
}

fn codex_audit_kind(error: &CodexExecutionError) -> AiExecutionErrorKind {
    match error.kind {
        ExecutorErrorKind::InvalidJob => AiExecutionErrorKind::InvalidJob,
        ExecutorErrorKind::InvalidResponse => AiExecutionErrorKind::InvalidResponse,
        ExecutorErrorKind::Persistence => AiExecutionErrorKind::Persistence,
        ExecutorErrorKind::Authentication
        | ExecutorErrorKind::Network
        | ExecutorErrorKind::Provider
        | ExecutorErrorKind::RateLimited => AiExecutionErrorKind::Provider,
        ExecutorErrorKind::CliFailed
        | ExecutorErrorKind::CliUnavailable
        | ExecutorErrorKind::NotConfigured
        | ExecutorErrorKind::PermissionDenied
        | ExecutorErrorKind::Timeout => AiExecutionErrorKind::Codex,
        ExecutorErrorKind::Unknown => AiExecutionErrorKind::Unknown,
    }
}

async fn run_ai_request<F>(
    job: &AiJob,
    system_prompt: &str,
    minimal_payload_json: String,
    backends: &AiExecutionBackends,
    validate: F,
) -> Result<AiExecutionOutput, CodexExecutionError>
where
    F: Fn(&AiExecutionOutput) -> Result<(), String>,
{
    execute_ai_validated(
        &AiExecutionRequest {
            job_id: job.id.clone(),
            kind: job.kind.clone(),
            snapshot: job.execution.clone(),
            system_prompt: system_prompt.to_string(),
            minimal_payload_json,
            timeout_ms: CODEX_TASK_TIMEOUT_MS,
        },
        backends,
        validate,
    )
    .await
}

async fn execute_trend_analysis_request(
    job: &AiJob,
    evidence: &TrendPayload,
    backends: &AiExecutionBackends,
) -> Result<(TrendAnalysisAiResult, AiExecutionOutput), CodexExecutionError> {
    let allowed = trend_analysis_allowed_candidates(evidence);
    let mut bounded_evidence = serde_json::to_value(evidence).map_err(|error| {
        CodexExecutionError::invalid_job(format!("Could not serialize trend evidence: {error}"))
    })?;
    if let Some(evidence) = bounded_evidence.as_object_mut() {
        evidence.remove("workLedger");
    }
    let user_content = serde_json::json!({
        "evidence": bounded_evidence,
        "allowedCandidates": allowed,
    })
    .to_string();
    let output = run_ai_request(
        job,
        "Analyze only the supplied aggregate TrendEvidence. Return one strict JSON object with exactly summary, observations, suggestions, and confidence. Select text exactly from allowedCandidates: summary must equal one summaries item; every observation and suggestion must equal an item from its corresponding list. Do not rewrite, combine, or add any text. Do not repeat items. Keep observations and suggestions non-empty. Confidence must be a number from zero to one.",
        user_content,
        backends,
        |output| parse_trend_analysis_response(&output.content, &allowed).map(|_| ()),
    )
    .await?;
    let analysis = parse_trend_analysis_response(&output.content, &allowed)
        .map_err(CodexExecutionError::invalid_response)?;
    Ok((analysis, output))
}

async fn execute_trend_research_request(
    job: &AiJob,
    queued: &TrendResearchJobPayload,
    backends: &AiExecutionBackends,
) -> Result<(TrendResearchAnalysis, AiExecutionOutput), CodexExecutionError> {
    let request = build_trend_research_execution_request(job, queued)?;
    let exact_api_backends;
    let execution_backends = if job.execution.execution_mode == AiExecutionMode::ApiKey {
        exact_api_backends = AiExecutionBackends {
            api_providers: backends
                .api_providers
                .iter()
                .filter(|credential| credential.provider.id == job.execution.executor_id)
                .cloned()
                .collect(),
            configured_codex: None,
        };
        &exact_api_backends
    } else {
        backends
    };
    let mut output = execute_ai_validated(&request, execution_backends, |output| {
        parse_trend_research_response(
            &output.content,
            &queued.input,
            &output.executor_id,
            &output.model,
        )
        .map(|_| ())
    })
    .await
    .map_err(|mut error| {
        error.executor_id = Some(job.execution.executor_id.clone());
        error.model = Some(job.execution.model.clone());
        error
    })?;
    output.executor_id.clone_from(&job.execution.executor_id);
    output.model.clone_from(&job.execution.model);
    let analysis = parse_trend_research_response(
        &output.content,
        &queued.input,
        &output.executor_id,
        &output.model,
    )
    .map_err(CodexExecutionError::invalid_response)?;
    Ok((analysis, output))
}

fn build_trend_research_execution_request(
    job: &AiJob,
    queued: &TrendResearchJobPayload,
) -> Result<AiExecutionRequest, CodexExecutionError> {
    let minimal_payload_json = serde_json::to_string(&queued.input).map_err(|error| {
        CodexExecutionError::invalid_job(format!(
            "Could not serialize bounded trend research evidence: {error}"
        ))
    })?;
    Ok(AiExecutionRequest {
        job_id: job.id.clone(),
        kind: job.kind.clone(),
        snapshot: job.execution.clone(),
        system_prompt: trend_research_protocol_prompt(),
        minimal_payload_json,
        timeout_ms: CODEX_TASK_TIMEOUT_MS,
    })
}

async fn execute_work_ledger_assignment_request(
    job: &AiJob,
    queued: &WorkLedgerAssignmentJobPayload,
    backends: &AiExecutionBackends,
) -> Result<(String, AiExecutionOutput), CodexExecutionError> {
    let fragments = if queued.fragments.is_empty() {
        vec![queued.evidence.clone()]
    } else {
        queued.fragments.clone()
    };
    let evidence: Vec<_> = fragments
        .iter()
        .map(|item| {
            serde_json::json!({
                "kind": item.kind,
                "occurredAtMs": item.occurred_at_ms,
                "durationSeconds": item.duration_seconds,
                "application": truncate_text(&item.application, 80),
                "title": truncate_text(&item.title, 160),
                "domain": truncate_text(&item.domain, 120),
                "classificationReason": truncate_text(&item.classification_reason, 120),
            })
        })
        .collect();
    let candidates: Vec<_> = queued
        .tasks
        .iter()
        .map(|task| {
            serde_json::json!({
                "id": task.id,
                "projectId": task.project_id,
                "title": truncate_text(&task.title, 160),
                "expectedOutput": truncate_text(&task.expected_output, 160),
            })
        })
        .collect();
    let candidate_clusters: Vec<_> = queued
        .candidate_clusters
        .iter()
        .map(|cluster| {
            serde_json::json!({
                "clusterId": cluster.cluster_id,
                "evidenceCount": cluster.evidence_count,
                "durationSeconds": cluster.duration_seconds,
                "startedAtMs": cluster.started_at_ms,
                "endedAtMs": cluster.ended_at_ms,
            })
        })
        .collect();
    let user_content = serde_json::json!({
        "protocolVersion": queued.protocol_version,
        "workFragment": {
            "episodeKey": queued.episode_key,
            "totalDurationSeconds": fragments.iter()
                .filter(|item| item.kind == "activity")
                .map(|item| item.duration_seconds.max(0))
                .sum::<i64>(),
            "evidence": evidence,
        },
        "contextHints": queued.context_hints,
        "candidateClusters": candidate_clusters,
        "allowedCandidates": candidates,
    })
    .to_string();
    let allowed_task_ids = queued
        .tasks
        .iter()
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    let mut allowed_project_ids = queued
        .tasks
        .iter()
        .map(|task| task.project_id.clone())
        .collect::<Vec<_>>();
    allowed_project_ids.sort();
    allowed_project_ids.dedup();
    let output = run_ai_request(
        job,
        "Infer the concrete tasks represented by the supplied bounded evidence, then decide whether those tasks serve one shared user goal. Return one strict camelCase JSON object. decision must be match_existing_task, draft_existing_workflow, draft_new_workflow, or unassigned. match_existing_task may select only an allowed taskId. draft_existing_workflow may select only an allowed projectId and must propose workflowName, description, and one to three tasks. draft_new_workflow must use a null projectId and also propose workflowName, description, and one to three tasks. Every proposed task must have key, title, expectedOutput, and clusterIds; every clusterId must come from candidateClusters, each candidate cluster must appear exactly once, and no database id may be invented. unassigned must not select ids or propose tasks. confidence values must be numbers from zero to one. reasonCode must be exactly one of title_overlap, expected_output_overlap, application_history, domain_history, goal_context, focus_context, shared_goal, insufficient_evidence. Use short factual goal/task labels. Never reproduce full URLs, paths, document contents, private text, or infer emotion, mood, personality, health, ability, intelligence, or other private facts.",
        user_content,
        backends,
        |output| {
            parse_work_ledger_decision_response(
                &output.content,
                &allowed_task_ids,
                &allowed_project_ids,
            )
            .map(|_| ())
        },
    )
    .await?;
    Ok((output.content.clone(), output))
}

async fn execute_daily_analysis_request(
    job: &AiJob,
    payload_json: &str,
    backends: &AiExecutionBackends,
) -> Result<(DailyAnalysisAiResult, AiExecutionOutput), CodexExecutionError> {
    let output = run_ai_request(
        job,
        "Analyze only the supplied daily work statistics. Return one strict camelCase JSON object with exactly portrait, recommendation, and findings. findings must contain 2 to 4 objects, each with exactly observation, hypothesis, validation, action, evidenceIds, limitations, confidence. observation states supplied facts; hypothesis uses uncertain language; validation gives a concrete comparison or small experiment; action gives one executable next step. evidenceIds must only use metric:monitored_seconds, metric:active_seconds, metric:learning_seconds, metric:idle_seconds, metric:switch_count, metric:longest_focus_seconds, metric:creation_seconds, metric:classification_coverage, metric:browser_visit_count, goal:daily, output:expected, output:actual. confidence is a number from zero to one. Keep portrait and recommendation under 500 Chinese characters and every finding text field under 280. Never invent numbers or evidence ids, diagnose mood/personality/intelligence/mental health/general ability, or claim causation from time statistics.",
        payload_json.to_string(),
        backends,
        |output| parse_daily_analysis_response(&output.content).map(|_| ()),
    )
    .await?;
    let analysis = parse_daily_analysis_response(&output.content)
        .map_err(CodexExecutionError::invalid_response)?;
    Ok((analysis, output))
}

async fn execute_classification_request(
    job: &AiJob,
    payload_json: &str,
    backends: &AiExecutionBackends,
) -> Result<(AiClassificationResult, AiExecutionOutput), CodexExecutionError> {
    let output = run_ai_request(
        job,
        "Classify active Windows activity. Return one JSON object with category, videoPurpose, confidence, reason. category must be research, video_input, text_input, game, social, creation_development, file_management, or pending. Never return idle: idle is determined only by local input and media evidence. videoPurpose must be learning, leisure, or unknown. Do not reproduce private content in reason.",
        payload_json.to_string(),
        backends,
        |output| {
            serde_json::from_str::<AiClassificationResult>(&output.content)
                .map(|_| ())
                .map_err(|error| error.to_string())
        },
    )
    .await?;
    let mut classification: AiClassificationResult = serde_json::from_str(&output.content)
        .map_err(|error| CodexExecutionError::invalid_response(error.to_string()))?;
    classification.confidence = classification.confidence.clamp(0.0, 1.0);
    classification.reason = classification.reason.chars().take(240).collect();
    Ok((classification, output))
}

fn parse_daily_analysis_response(content: &str) -> Result<DailyAnalysisAiResult, String> {
    let mut result: DailyAnalysisAiResult =
        serde_json::from_str(content.trim()).map_err(|error| error.to_string())?;
    result.portrait = result.portrait.trim().to_string();
    result.recommendation = result.recommendation.trim().to_string();
    if result.portrait.is_empty() || result.recommendation.is_empty() {
        return Err("Daily analysis response fields cannot be empty".to_string());
    }
    if result.portrait.chars().count() > 500 || result.recommendation.chars().count() > 500 {
        return Err("Daily analysis response fields exceed 500 characters".to_string());
    }
    if !(2..=4).contains(&result.findings.len()) {
        return Err("Daily analysis must contain two to four findings".to_string());
    }
    const ALLOWED_EVIDENCE_IDS: &[&str] = &[
        "metric:monitored_seconds",
        "metric:active_seconds",
        "metric:learning_seconds",
        "metric:idle_seconds",
        "metric:switch_count",
        "metric:longest_focus_seconds",
        "metric:creation_seconds",
        "metric:classification_coverage",
        "metric:browser_visit_count",
        "goal:daily",
        "output:expected",
        "output:actual",
    ];
    for finding in &mut result.findings {
        finding.observation = finding.observation.trim().to_string();
        finding.hypothesis = finding.hypothesis.trim().to_string();
        finding.validation = finding.validation.trim().to_string();
        finding.action = finding.action.trim().to_string();
        finding.evidence_ids = finding
            .evidence_ids
            .iter()
            .map(|value| value.trim().to_string())
            .collect();
        finding.limitations = finding
            .limitations
            .iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect();
        if finding.observation.is_empty()
            || finding.hypothesis.is_empty()
            || finding.validation.is_empty()
            || finding.action.is_empty()
        {
            return Err("Daily analysis finding fields cannot be empty".to_string());
        }
        if [
            &finding.observation,
            &finding.hypothesis,
            &finding.validation,
            &finding.action,
        ]
        .iter()
        .any(|value| value.chars().count() > 280)
        {
            return Err("Daily analysis finding text exceeds 280 characters".to_string());
        }
        if finding.evidence_ids.is_empty()
            || finding
                .evidence_ids
                .iter()
                .any(|id| !ALLOWED_EVIDENCE_IDS.contains(&id.as_str()))
        {
            return Err("Daily analysis contains an unknown evidence id".to_string());
        }
        if !finding.confidence.is_finite() || !(0.0..=1.0).contains(&finding.confidence) {
            return Err("Daily analysis confidence must be between zero and one".to_string());
        }
    }
    let combined = format!(
        "{}{}{}",
        result.portrait,
        result.recommendation,
        result
            .findings
            .iter()
            .map(|finding| {
                format!(
                    "{}{}{}{}",
                    finding.observation, finding.hypothesis, finding.validation, finding.action
                )
            })
            .collect::<String>()
    )
    .to_lowercase();
    if [
        "心情",
        "情绪",
        "人格",
        "性格",
        "能力",
        "智力",
        "聪明",
        "愚笨",
        "焦虑",
        "抑郁",
        "精神状态",
        "mood",
        "emotion",
        "personality",
        "temperament",
        "ability",
        "intelligence",
        "smart",
        "stupid",
        "anxiety",
        "depression",
        "mental",
    ]
    .iter()
    .any(|term| combined.contains(term))
    {
        return Err("Daily analysis response contains a prohibited judgment".to_string());
    }
    Ok(result)
}

fn discover_browser_sources() -> Vec<BrowserSource> {
    #[cfg(target_os = "windows")]
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_default();
    #[cfg(target_os = "windows")]
    let roots = [
        ("Chrome", local_app_data.join("Google/Chrome/User Data")),
        ("Edge", local_app_data.join("Microsoft/Edge/User Data")),
        (
            "Brave",
            local_app_data.join("BraveSoftware/Brave-Browser/User Data"),
        ),
    ];
    #[cfg(target_os = "macos")]
    let roots = UserDirs::new()
        .map(|dirs| dirs.home_dir().join("Library/Application Support"))
        .map(|support| {
            [
                ("Chrome", support.join("Google/Chrome")),
                ("Edge", support.join("Microsoft Edge")),
                ("Brave", support.join("BraveSoftware/Brave-Browser")),
                ("Arc", support.join("Arc/User Data")),
            ]
        })
        .unwrap_or_default();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let roots: [(&str, PathBuf); 0] = [];
    let mut sources = Vec::new();
    for (browser, root) in roots {
        let profiles = fs::read_dir(&root)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .filter(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                name == "Default" || name.starts_with("Profile ")
            });
        for profile in profiles {
            let history = profile.path().join("History");
            sources.push(BrowserSource {
                browser: browser.into(),
                profile: profile.file_name().to_string_lossy().into_owned(),
                history_path: history.to_string_lossy().into_owned(),
                available: history.is_file(),
            });
        }
    }
    sources
}

fn provider_templates(database: Option<&Database>) -> Vec<AiProviderConfig> {
    let mut providers: Vec<_> = [
        (
            "openai",
            "OpenAI",
            "https://api.openai.com/v1",
            "gpt-4.1-mini",
        ),
        (
            "zhipu",
            "智谱",
            "https://open.bigmodel.cn/api/paas/v4",
            "glm-4-flash",
        ),
        (
            "siliconflow",
            "硅基流动",
            "https://api.siliconflow.cn/v1",
            "Qwen/Qwen3-8B",
        ),
        (
            "modelscope",
            "ModelScope",
            "https://api-inference.modelscope.cn/v1",
            "Qwen/Qwen3-8B",
        ),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (id, name, base_url, model))| AiProviderConfig {
        id: id.into(),
        name: name.into(),
        base_url: base_url.into(),
        model: model.into(),
        enabled: true,
        auto_safe: true,
        priority: index as i32,
        has_credential: false,
    })
    .collect();
    let custom = database
        .and_then(|database| {
            database
                .get_setting_json("custom_ai_provider")
                .ok()
                .flatten()
        })
        .and_then(|value| serde_json::from_str::<CustomProviderConfig>(&value).ok())
        .unwrap_or(CustomProviderConfig {
            base_url: "http://127.0.0.1:11434/v1".into(),
            model: "local-model".into(),
        });
    providers.push(AiProviderConfig {
        id: "custom".into(),
        name: "自定义兼容接口".into(),
        base_url: custom.base_url,
        model: custom.model,
        enabled: true,
        auto_safe: true,
        priority: providers.len() as i32,
        has_credential: false,
    });
    providers
}

fn credential_exists(provider_id: &str) -> bool {
    Entry::new(current_edition_identity().credential_service, provider_id)
        .and_then(|entry| entry.get_password())
        .is_ok_and(|value| !value.trim().is_empty())
}

#[allow(dead_code)]
fn render_markdown_report(date: &str, dashboard: &DashboardSnapshot) -> String {
    let totals = &dashboard.totals;
    format!(
        "---\ndate: {date}\ntype: daily-review\ntags:\n  - daily-task-monitor\n---\n\n# {date} · 每日复盘\n\n> [!summary] 今日概览\n> 总监测 **{} 分钟** · 活跃 **{} 分钟** · 学习 **{} 分钟** · 不活跃 **{} 分钟**\n\n## 时间结构\n\n| 指标 | 时长 |\n| --- | ---: |\n| 总监测 | {} 分钟 |\n| 活跃 | {} 分钟 |\n| 学习 | {} 分钟 |\n| 不活跃 | {} 分钟 |\n\n## 今日活动\n\n{}\n\n## 今日复盘\n\n- 实际完成：\n- 保持的策略：\n- 明天调整：\n",
        totals.monitored_seconds / 60,
        totals.active_seconds / 60,
        totals.learning_seconds / 60,
        totals.idle_seconds / 60,
        totals.monitored_seconds / 60,
        totals.active_seconds / 60,
        totals.learning_seconds / 60,
        totals.idle_seconds / 60,
        dashboard
            .timeline
            .iter()
            .take(30)
            .map(|segment| {
                format!(
                    "- **{}** · {} · {} 分钟",
                    segment.app,
                    segment.title,
                    (segment.ended_at_ms - segment.started_at_ms) / 60_000
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

fn app_data_dir() -> PathBuf {
    platform_storage_root().join("data")
}

fn backup_database_before_1_3_migration(data_dir: &Path) -> std::io::Result<()> {
    let database_path = data_dir.join("monitor.db");
    let backup_path = data_dir.join("monitor.pre-1.3-continuity.bak");
    if database_path.exists() && !backup_path.exists() {
        fs::copy(&database_path, &backup_path)?;
        for suffix in ["-wal", "-shm"] {
            let source = data_dir.join(format!("monitor.db{suffix}"));
            if source.exists() {
                fs::copy(
                    &source,
                    data_dir.join(format!("monitor.pre-1.3-continuity.bak{suffix}")),
                )?;
            }
        }
    }
    Ok(())
}

fn icon_cache_dir() -> PathBuf {
    platform_storage_root().join("icon-cache")
}

fn platform_storage_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(path).join(current_edition_identity().storage_name);
    }
    ProjectDirs::from(
        "com",
        "DailyTaskMonitor",
        current_edition_identity().storage_name,
    )
    .map(|dirs| dirs.data_local_dir().to_path_buf())
    .unwrap_or_else(|| PathBuf::from("data").join(current_edition_identity().storage_name))
}

#[cfg(test)]
mod tray_interaction_tests {
    use super::is_dashboard_open_gesture;
    use tauri::tray::MouseButton;

    #[test]
    fn only_left_double_click_opens_dashboard() {
        assert!(is_dashboard_open_gesture(true, MouseButton::Left));
        assert!(!is_dashboard_open_gesture(false, MouseButton::Left));
        assert!(!is_dashboard_open_gesture(true, MouseButton::Right));
        assert!(!is_dashboard_open_gesture(true, MouseButton::Middle));
    }
}

#[cfg(test)]
mod app_identity_tests {
    use super::{
        canonical_display_name, encode_png, normalize_executable_path, png_data_url,
        rgba_from_bgra_and_mask,
    };

    #[test]
    fn normalized_windows_paths_share_one_cache_identity() {
        assert_eq!(
            normalize_executable_path(r"C:\Program Files\OpenAI\ChatGPT.exe"),
            normalize_executable_path(r"c:/program files/openai/CHATGPT.exe")
        );
    }

    #[test]
    fn packaged_chatgpt_beats_stale_codex_metadata_without_renaming_codex_cli() {
        let packaged = r"C:\Program Files\WindowsApps\OpenAI.Codex_26.707.3748.0_x64__2p2nqsd0c76g0\app\ChatGPT.exe";
        assert_eq!(
            canonical_display_name("ChatGPT", "Codex", packaged),
            "ChatGPT"
        );
        assert_eq!(
            canonical_display_name("codex", "Codex", r"C:\Users\me\bin\codex.exe"),
            "Codex"
        );
    }

    #[test]
    fn native_icon_encoding_returns_a_png_data_url() {
        let png = encode_png(1, 1, &[255, 0, 0, 255]).expect("one RGBA pixel encodes");

        assert!(png.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]));
        assert!(png_data_url(&png).starts_with("data:image/png;base64,"));
    }

    #[test]
    fn word_aligned_16px_mask_rows_do_not_shift_transparency() {
        let pixels = vec![0_u8; 16 * 2 * 4];
        let mask = [0_u8, 0, 0x80, 0];

        let rgba =
            rgba_from_bgra_and_mask(16, 2, &pixels, &mask, 2).expect("WORD-aligned mask is valid");

        assert_eq!(rgba[3], 0, "top-left pixel uses the second source row");
        assert_eq!(rgba[7], 255, "the adjacent top-row pixel remains opaque");
        assert_eq!(rgba[16 * 4 + 3], 255, "bottom-left pixel remains opaque");
    }
}

#[cfg(test)]
mod daily_analysis_ai_tests {
    use super::parse_daily_analysis_response;

    fn valid_response() -> &'static str {
        r#"{
            "portrait":"学习占比较高",
            "recommendation":"减少切换并核对产出",
            "findings":[
                {
                    "observation":"学习时间占活跃时间的一部分",
                    "hypothesis":"这可能反映今日以输入活动为主",
                    "validation":"与前七天学习占比比较",
                    "action":"明日结束时记录一条可核验产出",
                    "evidenceIds":["metric:learning_seconds","metric:active_seconds"],
                    "limitations":["时间占比不能证明学习质量"],
                    "confidence":0.72
                },
                {
                    "observation":"记录了窗口切换次数",
                    "hypothesis":"切换可能来自资料查找",
                    "validation":"做一次四十五分钟单任务实验",
                    "action":"实验期间把临时查找项记入待办",
                    "evidenceIds":["metric:switch_count"],
                    "limitations":["部分切换是任务必要步骤"],
                    "confidence":0.61
                }
            ]
        }"#
    }

    #[test]
    fn daily_analysis_parser_requires_structured_findings() {
        let result = parse_daily_analysis_response(valid_response()).unwrap();

        assert_eq!(result.portrait, "学习占比较高");
        assert_eq!(result.findings.len(), 2);
        assert!(parse_daily_analysis_response(r#"{"portrait":"only"}"#).is_err());
        assert!(parse_daily_analysis_response("not json").is_err());
        assert!(
            parse_daily_analysis_response(&valid_response().replace(
                r#""evidenceIds":["metric:switch_count"]"#,
                r#""evidenceIds":["metric:invented"]"#,
            ),)
            .is_err()
        );
        assert!(
            parse_daily_analysis_response(&valid_response().replace(
                r#""validation":"与前七天学习占比比较""#,
                r#""validation":"""#,
            ),)
            .is_err()
        );
    }

    #[test]
    fn daily_analysis_parser_rejects_prohibited_judgments() {
        let result = parse_daily_analysis_response(
            &valid_response().replace("学习占比较高", "工作能力较差"),
        );

        assert!(result.is_err());
    }
}

#[cfg(test)]
mod trend_analysis_backend_tests {
    use super::{
        CodexExecutionError, api_failure_kind_from_consumption_error,
        build_trend_analysis_request_body, codex_audit_kind, is_generic_classification_job,
        require_classification_completion_applied, require_trend_completion_applied,
        trend_provider_eligible,
    };
    use crate::{
        ai::{AiExecutionErrorKind, AiExecutionMode},
        ai::{AiProviderConfig, TrendAnalysisAllowedCandidates},
        ai_executor::{
            AiExecutionErrorKind as ExecutorErrorKind, CodexExecutionConfig, codex_exec_args,
        },
        app::AppSettings,
        work_ledger::WorkLedgerAssignmentConsumptionError,
    };

    fn provider(has_credential: bool) -> AiProviderConfig {
        AiProviderConfig {
            id: "openai".into(),
            name: "OpenAI".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-test".into(),
            enabled: true,
            auto_safe: true,
            priority: 1,
            has_credential,
        }
    }

    #[test]
    fn trend_queue_requires_enabled_automation_and_an_automatic_provider() {
        let api_settings = AppSettings {
            ai_execution_mode: AiExecutionMode::ApiKey,
            selected_api_provider_id: Some("openai".into()),
            ..AppSettings::default()
        };
        assert!(!trend_provider_eligible(
            false,
            &api_settings,
            &[provider(true)]
        ));
        assert!(!trend_provider_eligible(
            true,
            &api_settings,
            &[provider(false)]
        ));

        let mut disabled = provider(true);
        disabled.enabled = false;
        assert!(!trend_provider_eligible(true, &api_settings, &[disabled]));

        let missing_selection = AppSettings {
            ai_execution_mode: AiExecutionMode::ApiKey,
            selected_api_provider_id: None,
            ..AppSettings::default()
        };
        assert!(!trend_provider_eligible(
            true,
            &missing_selection,
            &[provider(true)]
        ));
        assert!(trend_provider_eligible(
            true,
            &api_settings,
            &[provider(true)]
        ));
        assert!(trend_provider_eligible(
            true,
            &AppSettings {
                ai_execution_mode: AiExecutionMode::Codex,
                ..AppSettings::default()
            },
            &[]
        ));
    }

    #[test]
    fn codex_exec_args_omit_model_for_cli_default_and_label_snapshots() {
        let cli_default = CodexExecutionConfig {
            executable: "codex".into(),
            model: String::new(),
        };
        assert_eq!(cli_default.model_label(), "cli-default");
        assert_eq!(
            codex_exec_args(&cli_default.model, "prompt"),
            vec![
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
                "prompt"
            ],
        );

        let configured = CodexExecutionConfig {
            executable: "codex".into(),
            model: "gpt-5-codex".into(),
        };
        assert_eq!(configured.model_label(), "gpt-5-codex");
        assert_eq!(
            codex_exec_args(&configured.model, "prompt"),
            vec![
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
                "gpt-5-codex",
                "prompt"
            ],
        );
    }

    #[test]
    fn api_work_ledger_consumption_errors_keep_normalized_audit_kind() {
        assert_eq!(
            api_failure_kind_from_consumption_error(
                &WorkLedgerAssignmentConsumptionError::InvalidJob("bad payload".into())
            ),
            AiExecutionErrorKind::InvalidJob
        );
        assert_eq!(
            api_failure_kind_from_consumption_error(
                &WorkLedgerAssignmentConsumptionError::InvalidResponse("bad response".into())
            ),
            AiExecutionErrorKind::InvalidResponse
        );
        assert_eq!(
            api_failure_kind_from_consumption_error(
                &WorkLedgerAssignmentConsumptionError::Persistence("db failed".into())
            ),
            AiExecutionErrorKind::Persistence
        );
    }

    #[test]
    fn codex_audit_kind_distinguishes_execution_from_contract_and_persistence_errors() {
        assert_eq!(
            codex_audit_kind(&CodexExecutionError::new(
                ExecutorErrorKind::CliFailed,
                "Codex exited with status 1",
                Some(1),
            )),
            AiExecutionErrorKind::Codex
        );
        assert_eq!(
            codex_audit_kind(&CodexExecutionError::invalid_response(
                "Codex did not return JSON"
            )),
            AiExecutionErrorKind::InvalidResponse
        );
        assert_eq!(
            codex_audit_kind(&CodexExecutionError::invalid_job(
                "Queued payload is invalid"
            )),
            AiExecutionErrorKind::InvalidJob
        );
        assert_eq!(
            codex_audit_kind(&CodexExecutionError::persistence("database is locked")),
            AiExecutionErrorKind::Persistence
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn codex_nonzero_exit_preserves_process_exit_code() {
        let error = CodexExecutionError::new(
            ExecutorErrorKind::CliFailed,
            "Codex exited with status 23",
            Some(23),
        );

        assert_eq!(error.exit_code, Some(23));
        assert_eq!(codex_audit_kind(&error), AiExecutionErrorKind::Codex);
    }

    #[test]
    fn trend_provider_request_uses_only_the_queued_aggregate_payload() {
        let payload = serde_json::json!({
            "range": { "startDate": "2026-07-06", "endDate": "2026-07-12" },
            "summary": { "activeSeconds": 3600 },
            "quality": { "recordedDayCount": 7 },
            "evidenceHash": "hash"
        })
        .to_string();
        let allowed = TrendAnalysisAllowedCandidates {
            summaries: vec!["本区间记录显示活动分布可供复核".into()],
            observations: vec!["当前区间存在有效活动记录".into()],
            suggestions: vec!["建议尝试固定一段连续任务".into()],
        };

        let body = build_trend_analysis_request_body(&provider(true), &payload, &allowed);

        assert_eq!(body["model"], "gpt-test");
        assert_eq!(body["temperature"], 0.1);
        let user: serde_json::Value =
            serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
        assert_eq!(
            user["evidence"],
            serde_json::from_str::<serde_json::Value>(&payload).unwrap()
        );
        assert_eq!(
            user["allowedCandidates"],
            serde_json::to_value(&allowed).unwrap()
        );
        assert_eq!(user["allowedCandidates"].as_object().unwrap().len(), 3);
        let system = body["messages"][0]["content"].as_str().unwrap();
        assert!(system.contains("exactly summary, observations, suggestions, and confidence"));
        assert!(system.contains("Select text exactly from allowedCandidates"));
        assert!(system.contains("Do not rewrite, combine, or add any text"));
        for forbidden in [
            "raw file",
            "browser body",
            "window title",
            "source record",
            "text snippet",
        ] {
            assert!(!user.to_string().contains(forbidden));
        }
    }

    #[test]
    fn modelscope_requests_disable_thinking_for_structured_json_responses() {
        let payload = serde_json::json!({
            "range": { "startDate": "2026-07-06", "endDate": "2026-07-12" },
            "summary": { "activeSeconds": 3600 },
            "quality": { "recordedDayCount": 7 },
            "evidenceHash": "hash"
        })
        .to_string();
        let allowed = TrendAnalysisAllowedCandidates {
            summaries: vec!["Aggregate summary".into()],
            observations: vec!["Aggregate observation".into()],
            suggestions: vec!["Aggregate suggestion".into()],
        };
        let mut modelscope = provider(true);
        modelscope.id = "modelscope".into();
        modelscope.name = "ModelScope".into();
        modelscope.base_url = "https://api-inference.modelscope.cn/v1".into();
        modelscope.model = "Qwen/Qwen3-8B".into();

        let modelscope_body = build_trend_analysis_request_body(&modelscope, &payload, &allowed);
        let openai_body = build_trend_analysis_request_body(&provider(true), &payload, &allowed);

        assert_eq!(modelscope_body["enable_thinking"], false);
        assert!(openai_body.get("enable_thinking").is_none());
    }

    #[test]
    fn trend_provider_request_omits_work_ledger_names() {
        let payload = serde_json::json!({
            "range": { "startDate": "2026-07-06", "endDate": "2026-07-12" },
            "summary": { "activeSeconds": 3600 },
            "quality": { "recordedDayCount": 7 },
            "workLedger": {
                "projects": [{ "projectName": "Private project name" }],
                "tasks": [{ "taskTitle": "Private task title" }]
            },
            "evidenceHash": "hash"
        })
        .to_string();
        let allowed = TrendAnalysisAllowedCandidates {
            summaries: vec!["Aggregate summary".into()],
            observations: vec!["Aggregate observation".into()],
            suggestions: vec!["Aggregate suggestion".into()],
        };

        let body = build_trend_analysis_request_body(&provider(true), &payload, &allowed);
        let user_content = body["messages"][1]["content"].as_str().unwrap();
        let user: serde_json::Value = serde_json::from_str(user_content).unwrap();
        assert!(user["evidence"].get("workLedger").is_none());
        assert!(!user_content.contains("Private project name"));
        assert!(!user_content.contains("Private task title"));
    }

    #[test]
    fn trend_worker_does_not_ignore_a_rejected_atomic_completion() {
        assert!(require_trend_completion_applied(true).is_ok());
        assert!(require_trend_completion_applied(false).is_err());
    }

    #[test]
    fn classification_worker_does_not_ignore_a_rejected_atomic_completion() {
        assert!(require_classification_completion_applied(true).is_ok());
        assert!(require_classification_completion_applied(false).is_err());
    }

    #[test]
    fn work_ledger_assignment_is_not_a_generic_classification_job() {
        assert!(is_generic_classification_job("classify_segment"));
        assert!(is_generic_classification_job("classify_page"));
        assert!(!is_generic_classification_job("work_ledger_assignment"));
        assert!(!is_generic_classification_job("unknown"));
    }
}

#[cfg(test)]
mod ai_worker_dispatch_tests {
    use std::cell::Cell;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use super::{
        ExecutedAiJob, build_trend_research_execution_request,
        complete_terminal_api_ai_job_failure, dispatch_ai_job_by_snapshot,
        execute_production_ai_job, execution_failure_audit, parse_api_daily_analysis_job_payload,
        process_invalid_work_ledger_assignment_job, record_ai_job_failure,
    };
    use crate::{
        ai::{
            AiExecutionErrorKind, AiExecutionMode, AiExecutionSnapshot, AiJob, AiJobStatus,
            AiProviderConfig,
        },
        ai_executor::{
            AiExecutionBackends, AiExecutionError as ExecutorError,
            AiExecutionErrorKind as ExecutorErrorKind, ApiProviderCredential, CodexExecutionConfig,
        },
        db::Database,
        domain::ActivityScope,
        trend_analysis::{
            EvidenceRelation, TREND_RESEARCH_CANONICAL_OBSERVATIONS,
            TREND_RESEARCH_LOW_COVERAGE_EXPLANATION, TREND_RESEARCH_OPERATIONAL_HYPOTHESES,
            TREND_RESEARCH_OPERATIONAL_VALIDATION_METHODS, TREND_RESEARCH_RELATION_RULES,
            TrendResearchEvidence, TrendResearchInput, TrendResearchJobPayload,
            trend_research_protocol_prompt, trend_research_structural_rules,
        },
        trends::{TrendBaselineKind, TrendGranularity, TrendMetric, TrendWorkbenchRequest},
    };

    fn snapshot(mode: AiExecutionMode) -> AiExecutionSnapshot {
        AiExecutionSnapshot {
            execution_mode: mode,
            executor_id: match mode {
                AiExecutionMode::ApiKey => "openai",
                AiExecutionMode::Codex => "codex",
            }
            .into(),
            model: "queued-model".into(),
            evidence_hash: "queued-evidence".into(),
            created_at_ms: 1_000,
        }
    }

    fn run_async_test(future: impl std::future::Future<Output = ()>) {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future);
    }

    fn spawn_api_server_with_response(
        response: impl Into<String>,
    ) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
        let response = response.into();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, receiver) = mpsc::channel();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0_u8; 4096];
            let header_end = loop {
                let read = stream.read(&mut chunk).unwrap();
                assert!(read > 0);
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
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                })
                .unwrap();
            while bytes.len() - header_end < content_length {
                let read = stream.read(&mut chunk).unwrap();
                assert!(read > 0);
                bytes.extend_from_slice(&chunk[..read]);
            }
            sender
                .send(
                    String::from_utf8(bytes[header_end..header_end + content_length].to_vec())
                        .unwrap(),
                )
                .unwrap();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.len(),
                response
            )
            .unwrap();
        });
        (format!("http://{address}/v1"), receiver, handle)
    }

    fn spawn_api_server() -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
        spawn_api_server_with_response(
            r#"{"model":"actual-api-model","choices":[{"message":{"content":"{\"category\":\"research\",\"videoPurpose\":\"unknown\",\"confidence\":0.9,\"reason\":\"bounded\"}"}}]}"#,
        )
    }

    fn trend_research_payload() -> TrendResearchJobPayload {
        TrendResearchJobPayload {
            activity_scope: ActivityScope::All,
            request: TrendWorkbenchRequest {
                start_date: "2026-07-06".into(),
                end_date: "2026-07-12".into(),
                selected_dates: None,
                timezone_offset_minutes: 480,
                granularity: Some(TrendGranularity::Day),
                metric: TrendMetric::ActiveSeconds,
                custom_baseline: None,
            },
            input: TrendResearchInput {
                evidence_hash: "queued-evidence".into(),
                metric: TrendMetric::ActiveSeconds,
                selection_mode: Default::default(),
                selected_dates: Vec::new(),
                selected_date_count: 7,
                envelope_day_count: 7,
                effective_activity_day_count: 3,
                classification_coverage: 0.8,
                direction_tolerance_percent: 8.0,
                enabled_baselines: vec![TrendBaselineKind::Current],
                evidence: vec![TrendResearchEvidence {
                    id: "current.summary.activeSeconds".into(),
                    value: 3600.0,
                    allowed_relations: vec![EvidenceRelation::Supports],
                }],
            },
        }
    }

    fn trend_research_response(observation: &str, model: &str) -> String {
        let content = serde_json::json!({
            "evidenceHash": "queued-evidence",
            "findings": [{
                "observation": observation,
                "possibleExplanation": "可能与记录节奏有关",
                "validationMethod": "后续周期复核同类聚合证据",
                "evidenceIds": ["current.summary.activeSeconds"],
                "claims": [{
                    "evidenceId": "current.summary.activeSeconds",
                    "relation": "supports"
                }],
                "confidence": 0.8,
                "limitations": ["仅基于聚合数据"]
            }],
            "limitations": []
        })
        .to_string();
        serde_json::json!({
            "model": model,
            "choices": [{"message": {"content": content}}]
        })
        .to_string()
    }

    fn assert_complete_trend_research_protocol(prompt: &str) {
        for required in TREND_RESEARCH_CANONICAL_OBSERVATIONS
            .iter()
            .chain(TREND_RESEARCH_OPERATIONAL_HYPOTHESES)
            .chain(TREND_RESEARCH_OPERATIONAL_VALIDATION_METHODS)
            .chain(TREND_RESEARCH_RELATION_RULES)
            .copied()
            .chain([TREND_RESEARCH_LOW_COVERAGE_EXPLANATION])
        {
            assert!(
                prompt.contains(required),
                "worker prompt omitted parser requirement: {required}"
            );
        }
        assert!(prompt.contains("limitations array must be empty"));
        assert!(prompt.contains("provider limitation prose is discarded"));
        assert!(prompt.contains("findings must be nonempty."));
        assert!(prompt.contains(
            "Every provider free-text field (observation, possibleExplanation, and validationMethod) must avoid personal or psychological state or trait inference terms; below 0.5 classificationCoverage, possibleExplanation is deterministically replaced before this inference check."
        ));
        for required in trend_research_structural_rules() {
            assert!(
                prompt.contains(&required),
                "worker prompt omitted structural parser rule: {required}"
            );
        }
    }

    #[test]
    fn trend_research_api_and_codex_requests_share_authoritative_protocol() {
        let queued = trend_research_payload();
        for mode in [AiExecutionMode::ApiKey, AiExecutionMode::Codex] {
            let job = AiJob {
                id: format!("trend-{mode:?}"),
                generation: 1,
                kind: "trend_research_analysis".into(),
                payload_json: serde_json::to_string(&queued).unwrap(),
                status: AiJobStatus::Running,
                attempts: 0,
                next_attempt_at_ms: 0,
                last_error: String::new(),
                execution: snapshot(mode),
                started_at_ms: Some(1_000),
                finished_at_ms: None,
                duration_ms: None,
                executor_id: None,
                model: None,
                exit_code: None,
                error_kind: None,
            };
            let request = build_trend_research_execution_request(&job, &queued)
                .expect("trend research request builds");
            assert_eq!(request.system_prompt, trend_research_protocol_prompt());
            assert_complete_trend_research_protocol(&request.system_prompt);
            assert_eq!(
                request.minimal_payload_json,
                serde_json::to_string(&queued.input).unwrap()
            );
            assert_eq!(request.timeout_ms, 180_000);
        }
    }

    #[test]
    fn trend_research_api_worker_uses_only_frozen_provider_model_and_minimized_body() {
        run_async_test(async {
            let response = trend_research_response("当前聚合证据可供复核", "response-model");
            let (base_url, request_body, server) = spawn_api_server_with_response(response);
            let queued = trend_research_payload();
            let job = AiJob {
                id: "trend-job".into(),
                generation: 1,
                kind: "trend_research_analysis".into(),
                payload_json: serde_json::to_string(&queued).unwrap(),
                status: AiJobStatus::Running,
                attempts: 0,
                next_attempt_at_ms: 0,
                last_error: String::new(),
                execution: AiExecutionSnapshot {
                    execution_mode: AiExecutionMode::ApiKey,
                    executor_id: "queued-provider".into(),
                    model: "queued-model".into(),
                    evidence_hash: "queued-evidence".into(),
                    created_at_ms: 1_000,
                },
                started_at_ms: Some(1_000),
                finished_at_ms: None,
                duration_ms: None,
                executor_id: None,
                model: None,
                exit_code: None,
                error_kind: None,
            };
            let backends = AiExecutionBackends {
                api_providers: vec![
                    ApiProviderCredential {
                        provider: AiProviderConfig {
                            id: "queued-provider".into(),
                            name: "Queued provider".into(),
                            base_url,
                            model: "registry-changed-model".into(),
                            enabled: true,
                            auto_safe: true,
                            priority: 1,
                            has_credential: true,
                        },
                        api_key: "test-key".into(),
                    },
                    ApiProviderCredential {
                        provider: AiProviderConfig {
                            id: "other-provider".into(),
                            name: "Other provider".into(),
                            base_url: "http://127.0.0.1:9/v1".into(),
                            model: "other-model".into(),
                            enabled: true,
                            auto_safe: true,
                            priority: 99,
                            has_credential: true,
                        },
                        api_key: "other-key".into(),
                    },
                ],
                configured_codex: Some(CodexExecutionConfig {
                    executable: "must-not-run-codex".into(),
                    model: "must-not-run-model".into(),
                }),
            };

            let executed = execute_production_ai_job(&job, &backends)
                .await
                .expect("frozen trend provider succeeds");
            let ExecutedAiJob::TrendResearchAnalysis {
                analysis,
                execution,
                ..
            } = executed
            else {
                panic!("trend research result expected");
            };
            assert_eq!(execution.executor_id, "queued-provider");
            assert_eq!(execution.model, "queued-model");
            assert_eq!(analysis.source, "queued-provider");
            assert_eq!(analysis.model, "queued-model");

            let body = request_body.recv_timeout(Duration::from_secs(1)).unwrap();
            let body: serde_json::Value = serde_json::from_str(&body).unwrap();
            assert_eq!(body["model"], "queued-model");
            let system_prompt = body["messages"][0]["content"].as_str().unwrap();
            assert_eq!(system_prompt, trend_research_protocol_prompt());
            assert_complete_trend_research_protocol(system_prompt);
            assert_eq!(
                body["messages"][1]["content"],
                serde_json::to_string(&queued.input).unwrap()
            );
            let serialized = body.to_string();
            for forbidden in [
                "request",
                "timeline",
                "windowTitle",
                "browserVisits",
                "titleSummary",
                "rawRows",
                "Private App",
            ] {
                assert!(
                    !serialized.contains(forbidden),
                    "provider body leaked {forbidden}"
                );
            }
            server.join().unwrap();
        });
    }

    #[test]
    fn trend_research_invalid_response_does_not_fallback_and_persists_frozen_audit() {
        run_async_test(async {
            let response = trend_research_response("当前聚合证据提升20%", "response-model");
            let (base_url, _request_body, server) = spawn_api_server_with_response(response);
            let database = Database::open_in_memory().unwrap();
            let queued = trend_research_payload();
            let id = database
                .enqueue_ai_job(
                    "trend_research_analysis",
                    &serde_json::to_string(&queued).unwrap(),
                    1_000,
                    &AiExecutionSnapshot {
                        execution_mode: AiExecutionMode::ApiKey,
                        executor_id: "queued-provider".into(),
                        model: "queued-model".into(),
                        evidence_hash: "queued-evidence".into(),
                        created_at_ms: 1_000,
                    },
                )
                .unwrap();
            let job = database.claim_next_due_ai_job(1_000).unwrap().unwrap();
            let backends = AiExecutionBackends {
                api_providers: vec![
                    ApiProviderCredential {
                        provider: AiProviderConfig {
                            id: "queued-provider".into(),
                            name: "Queued provider".into(),
                            base_url,
                            model: "registry-changed-model".into(),
                            enabled: true,
                            auto_safe: true,
                            priority: 1,
                            has_credential: true,
                        },
                        api_key: "test-key".into(),
                    },
                    ApiProviderCredential {
                        provider: AiProviderConfig {
                            id: "other-provider".into(),
                            name: "Other provider".into(),
                            base_url: "http://127.0.0.1:9/v1".into(),
                            model: "other-model".into(),
                            enabled: true,
                            auto_safe: true,
                            priority: 99,
                            has_credential: true,
                        },
                        api_key: "other-key".into(),
                    },
                ],
                configured_codex: None,
            };

            let error = match execute_production_ai_job(&job, &backends).await {
                Ok(_) => panic!("numeric free text must be rejected without fallback"),
                Err(error) => error,
            };
            assert_eq!(error.kind, ExecutorErrorKind::InvalidResponse);
            let audit = execution_failure_audit(&job, &error);
            assert_eq!(audit.executor_id, "queued-provider");
            assert_eq!(audit.model, "queued-model");
            record_ai_job_failure(&database, &job, &error, &audit).unwrap();
            let persisted = database.get_ai_job(&id).unwrap().unwrap();
            assert_eq!(
                persisted.error_kind,
                Some(AiExecutionErrorKind::InvalidResponse)
            );
            assert_eq!(persisted.executor_id.as_deref(), Some("queued-provider"));
            assert_eq!(persisted.model.as_deref(), Some("queued-model"));
            server.join().unwrap();
        });
    }

    #[test]
    fn production_api_worker_uses_unified_executor_with_bounded_snapshot_request() {
        run_async_test(async {
            let (base_url, request_body, server) = spawn_api_server();
            let payload = serde_json::json!({
                "visitId": "visit-1",
                "application": "Browser",
                "domain": "example.com",
            })
            .to_string();
            let job = AiJob {
                id: "job-1".into(),
                generation: 1,
                kind: "classify_page".into(),
                payload_json: payload.clone(),
                status: AiJobStatus::Running,
                attempts: 0,
                next_attempt_at_ms: 0,
                last_error: String::new(),
                execution: AiExecutionSnapshot {
                    execution_mode: AiExecutionMode::ApiKey,
                    executor_id: "queued-provider".into(),
                    model: "queued-model".into(),
                    evidence_hash: "queued-evidence".into(),
                    created_at_ms: 1_000,
                },
                started_at_ms: Some(1_000),
                finished_at_ms: None,
                duration_ms: None,
                executor_id: None,
                model: None,
                exit_code: None,
                error_kind: None,
            };
            let backends = AiExecutionBackends {
                api_providers: vec![ApiProviderCredential {
                    provider: AiProviderConfig {
                        id: "queued-provider".into(),
                        name: "Queued provider".into(),
                        base_url,
                        model: "changed-model".into(),
                        enabled: true,
                        auto_safe: true,
                        priority: 99,
                        has_credential: true,
                    },
                    api_key: "test-key".into(),
                }],
                configured_codex: Some(CodexExecutionConfig {
                    executable: "must-not-run-codex".into(),
                    model: "must-not-run-model".into(),
                }),
            };

            let executed = execute_production_ai_job(&job, &backends)
                .await
                .expect("production API worker succeeds");
            let ExecutedAiJob::Classification { execution, .. } = executed else {
                panic!("classification result expected");
            };
            assert_eq!(execution.executor_id, "queued-provider");
            assert_eq!(execution.model, "actual-api-model");
            assert!(execution.duration_ms <= 5_000);

            let body = request_body.recv_timeout(Duration::from_secs(1)).unwrap();
            let body: serde_json::Value = serde_json::from_str(&body).unwrap();
            assert_eq!(body["model"], "queued-model");
            assert_eq!(body["messages"][1]["content"], payload);
            let serialized = body.to_string();
            assert!(!serialized.contains("fullTimeline"));
            assert!(!serialized.contains("windowTitle"));
            server.join().unwrap();
        });
    }

    #[test]
    fn production_api_worker_does_not_fallback_after_semantic_validation_failure() {
        run_async_test(async {
            let invalid = r#"{"model":"invalid-model","choices":[{"message":{"content":"{\"category\":\"not-allowed\",\"videoPurpose\":\"unknown\",\"confidence\":0.9,\"reason\":\"invalid\"}"}}]}"#;
            let (primary_url, primary_body, primary_server) =
                spawn_api_server_with_response(invalid);
            let payload = serde_json::json!({
                "visitId": "visit-1",
                "application": "Browser",
                "domain": "example.com",
            })
            .to_string();
            let mut queued = snapshot(AiExecutionMode::ApiKey);
            queued.executor_id = "queued-provider".into();
            queued.model = "queued-model".into();
            let job = AiJob {
                id: "job-semantic-fallback".into(),
                generation: 1,
                kind: "classify_page".into(),
                payload_json: payload.clone(),
                status: AiJobStatus::Running,
                attempts: 0,
                next_attempt_at_ms: 0,
                last_error: String::new(),
                execution: queued,
                started_at_ms: Some(1_000),
                finished_at_ms: None,
                duration_ms: None,
                executor_id: None,
                model: None,
                exit_code: None,
                error_kind: None,
            };
            let backends = AiExecutionBackends {
                api_providers: vec![
                    ApiProviderCredential {
                        provider: AiProviderConfig {
                            id: "fallback-provider".into(),
                            name: "Fallback provider".into(),
                            base_url: "http://127.0.0.1:9/v1".into(),
                            model: "fallback-configured-model".into(),
                            enabled: true,
                            auto_safe: true,
                            priority: 0,
                            has_credential: true,
                        },
                        api_key: "fallback-key".into(),
                    },
                    ApiProviderCredential {
                        provider: AiProviderConfig {
                            id: "queued-provider".into(),
                            name: "Queued provider".into(),
                            base_url: primary_url,
                            model: "changed-model".into(),
                            enabled: true,
                            auto_safe: true,
                            priority: 99,
                            has_credential: true,
                        },
                        api_key: "queued-key".into(),
                    },
                ],
                configured_codex: Some(CodexExecutionConfig {
                    executable: "must-not-run-codex".into(),
                    model: "must-not-run-model".into(),
                }),
            };

            let error = match execute_production_ai_job(&job, &backends).await {
                Ok(_) => panic!("invalid primary response must not use a fallback provider"),
                Err(error) => error,
            };
            assert_eq!(error.kind, ExecutorErrorKind::InvalidResponse);
            assert_eq!(error.executor_id.as_deref(), Some("queued-provider"));
            assert_eq!(error.model.as_deref(), Some("invalid-model"));

            let primary: serde_json::Value =
                serde_json::from_str(&primary_body.recv_timeout(Duration::from_secs(1)).unwrap())
                    .unwrap();
            assert_eq!(primary["model"], "queued-model");
            assert_eq!(primary["messages"][1]["content"], payload);
            primary_server.join().unwrap();
        });
    }

    #[test]
    fn queued_api_snapshot_dispatches_only_api_after_settings_switch() {
        run_async_test(async {
            let current_mode = Cell::new(AiExecutionMode::ApiKey);
            let queued = snapshot(current_mode.get());
            current_mode.set(AiExecutionMode::Codex);

            for should_fail in [false, true] {
                let api_calls = Cell::new(0);
                let codex_calls = Cell::new(0);
                let result = dispatch_ai_job_by_snapshot(
                    &queued,
                    || async {
                        api_calls.set(api_calls.get() + 1);
                        if should_fail {
                            Err("api failed")
                        } else {
                            Ok(())
                        }
                    },
                    || async {
                        codex_calls.set(codex_calls.get() + 1);
                        Err("codex must not run")
                    },
                )
                .await;

                assert_eq!(current_mode.get(), AiExecutionMode::Codex);
                assert_eq!(
                    result,
                    if should_fail {
                        Err("api failed")
                    } else {
                        Ok(())
                    }
                );
                assert_eq!(api_calls.get(), 1);
                assert_eq!(codex_calls.get(), 0);
            }
        });
    }

    #[test]
    fn failure_audit_uses_actual_attempt_metadata_and_duration() {
        let job = AiJob {
            id: "job-audit".into(),
            generation: 1,
            kind: "classify_page".into(),
            payload_json: "{}".into(),
            status: AiJobStatus::Running,
            attempts: 0,
            next_attempt_at_ms: 0,
            last_error: String::new(),
            execution: snapshot(AiExecutionMode::ApiKey),
            started_at_ms: Some(10_000),
            finished_at_ms: None,
            duration_ms: None,
            executor_id: None,
            model: None,
            exit_code: None,
            error_kind: None,
        };
        let error = ExecutorError::new(ExecutorErrorKind::Provider, "failed", None)
            .with_execution_context("actual-fallback", "actual-model", 275);

        let audit = execution_failure_audit(&job, &error);

        assert_eq!(audit.executor_id, "actual-fallback");
        assert_eq!(audit.model, "actual-model");
        assert_eq!(audit.finished_at_ms, 10_275);
    }

    #[test]
    fn queued_codex_snapshot_dispatches_only_codex_after_settings_switch() {
        run_async_test(async {
            let current_mode = Cell::new(AiExecutionMode::Codex);
            let queued = snapshot(current_mode.get());
            current_mode.set(AiExecutionMode::ApiKey);

            for should_fail in [false, true] {
                let api_calls = Cell::new(0);
                let codex_calls = Cell::new(0);
                let result = dispatch_ai_job_by_snapshot(
                    &queued,
                    || async {
                        api_calls.set(api_calls.get() + 1);
                        Err("api must not run")
                    },
                    || async {
                        codex_calls.set(codex_calls.get() + 1);
                        if should_fail {
                            Err("codex failed")
                        } else {
                            Ok(())
                        }
                    },
                )
                .await;

                assert_eq!(current_mode.get(), AiExecutionMode::ApiKey);
                assert_eq!(
                    result,
                    if should_fail {
                        Err("codex failed")
                    } else {
                        Ok(())
                    }
                );
                assert_eq!(api_calls.get(), 0);
                assert_eq!(codex_calls.get(), 1);
            }
        });
    }

    #[test]
    fn api_worker_treats_daily_analysis_without_date_as_invalid_job() {
        run_async_test(async {
            let db = Database::open_in_memory().unwrap();
            let id = db
                .enqueue_ai_job(
                    "daily_analysis",
                    r#"{"evidenceHash":"hash","startMs":1000,"endMs":2000}"#,
                    1_000,
                    &snapshot(AiExecutionMode::ApiKey),
                )
                .unwrap();
            let job = db.claim_next_due_ai_job(1_000).unwrap().unwrap();
            let api_calls = Cell::new(0);
            let codex_calls = Cell::new(0);
            let error = dispatch_ai_job_by_snapshot(
                &job.execution,
                || async {
                    api_calls.set(api_calls.get() + 1);
                    parse_api_daily_analysis_job_payload(&job.payload_json)
                },
                || async {
                    codex_calls.set(codex_calls.get() + 1);
                    panic!("an API snapshot must not fall back to Codex")
                },
            )
            .await
            .unwrap_err();

            assert_eq!(error.kind, AiExecutionErrorKind::InvalidJob);
            assert_eq!(error.message, "Queued daily analysis payload has no date");
            assert_eq!(api_calls.get(), 1);
            assert_eq!(codex_calls.get(), 0);
            assert!(complete_terminal_api_ai_job_failure(&db, &job, 1_500, &error).unwrap());
            let completed = db.get_ai_job(&id).unwrap().unwrap();
            assert_eq!(completed.status, AiJobStatus::Complete);
            assert_eq!(completed.error_kind, Some(AiExecutionErrorKind::InvalidJob));
            assert_eq!(completed.finished_at_ms, Some(1_500));
        });
    }

    #[test]
    fn codex_failure_dispatcher_terminalizes_invalid_job_without_retry() {
        run_async_test(async {
            let db = Database::open_in_memory().unwrap();
            let id = db
                .enqueue_ai_job(
                    "unsupported_job",
                    "{}",
                    1_000,
                    &snapshot(AiExecutionMode::Codex),
                )
                .unwrap();
            let job = db.claim_next_due_ai_job(1_000).unwrap().unwrap();
            let error = match execute_production_ai_job(&job, &AiExecutionBackends::default()).await
            {
                Ok(_) => panic!("unsupported job must fail validation"),
                Err(error) => error,
            };
            let audit = execution_failure_audit(&job, &error);

            record_ai_job_failure(&db, &job, &error, &audit).unwrap();

            let completed = db.get_ai_job(&id).unwrap().unwrap();
            assert_eq!(completed.status, AiJobStatus::Complete);
            assert_eq!(completed.attempts, 0);
            assert_eq!(completed.error_kind, Some(AiExecutionErrorKind::InvalidJob));
        });
    }

    #[test]
    fn invalid_work_ledger_dispatch_helper_reports_whether_completion_applied() {
        let db = Database::open_in_memory().unwrap();
        let normal_id = db
            .enqueue_ai_job_for_subject(
                "work_ledger_assignment",
                "normal-invalid",
                "{}",
                1_000,
                &snapshot(AiExecutionMode::Codex),
            )
            .unwrap();
        let normal_run = db.claim_next_due_ai_job(1_000).unwrap().unwrap();

        assert!(process_invalid_work_ledger_assignment_job(&db, &normal_run).unwrap());
        let completed = db.get_ai_job(&normal_id).unwrap().unwrap();
        assert_eq!(completed.status, AiJobStatus::Complete);
        assert_eq!(completed.attempts, 0);

        let old_id = db
            .enqueue_ai_job_for_subject(
                "work_ledger_assignment",
                "superseded-invalid",
                "{}",
                2_000,
                &snapshot(AiExecutionMode::Codex),
            )
            .unwrap();
        let old_run = db.claim_next_due_ai_job(2_000).unwrap().unwrap();
        let current_id = db
            .force_enqueue_ai_job_for_subject(
                "work_ledger_assignment",
                "superseded-invalid",
                "{}",
                3_000,
                &snapshot(AiExecutionMode::Codex),
            )
            .unwrap();
        let current_run = db.claim_next_due_ai_job(3_000).unwrap().unwrap();

        assert!(!process_invalid_work_ledger_assignment_job(&db, &old_run).unwrap());
        let retired = db.get_ai_job(&old_id).unwrap().unwrap();
        assert_eq!(retired.status, AiJobStatus::Complete);
        assert_eq!(retired.attempts, old_run.attempts);
        assert_eq!(db.get_ai_job(&current_id).unwrap().unwrap(), current_run);
    }
}

#[allow(dead_code)]
fn domain_from_url(value: &str) -> String {
    Url::parse(value)
        .ok()
        .and_then(|url| url.domain().map(str::to_owned))
        .unwrap_or_default()
}

#[cfg(test)]
mod codex_health_path_resolution_tests {
    use super::resolve_codex_health_paths_with;

    #[test]
    fn empty_and_name_based_configuration_may_detect_on_path() {
        let (configured, detected) = resolve_codex_health_paths_with("", |name| {
            assert_eq!(name, "codex");
            Some("C:\\Tools\\codex.exe".to_string())
        });
        assert_eq!(configured, "");
        assert_eq!(detected.as_deref(), Some("C:\\Tools\\codex.exe"));

        let (configured, detected) = resolve_codex_health_paths_with("codex-preview", |name| {
            assert_eq!(name, "codex-preview");
            Some("C:\\Tools\\codex-preview.exe".to_string())
        });
        assert_eq!(configured, "codex-preview");
        assert_eq!(detected.as_deref(), Some("C:\\Tools\\codex-preview.exe"));
    }

    #[test]
    fn explicit_configuration_never_calls_path_detection() {
        let (configured, detected) =
            resolve_codex_health_paths_with("C:\\Fixed\\codex.exe", |_| {
                panic!("explicit paths must not fall back to PATH")
            });

        assert_eq!(configured, "C:\\Fixed\\codex.exe");
        assert!(detected.is_none());
    }
}

#[cfg(test)]
mod ai_connection_health_tests {
    use std::sync::{Arc, Mutex};

    use super::{
        AI_INFERENCE_VERIFICATION_TTL_MS, AiConnectionHealth, AiConnectionHealthProbePlan,
        AiConnectionHealthServiceState, AiConnectionHealthSource, AiConnectionHealthStatus,
        AiConnectionVerificationLevel, ApiHealthProbeError, HealthProbeCoordinator,
        build_ai_execution_snapshot, clear_selected_provider_for_deleted_key,
        map_api_health_result, map_codex_health_status, probe_selected_api_provider_with,
        queue_job_matches_current_execution, selected_api_provider,
        settings_patch_requires_api_provider_validation,
    };
    use crate::ai::{AiExecutionMode, AiExecutionSnapshot, AiJob, AiJobStatus, AiProviderConfig};
    use crate::ai_executor::{CodexExecutionConfig, CodexHealthStatus};
    use crate::app::AppSettings;

    fn provider(
        id: &str,
        priority: i32,
        enabled: bool,
        auto_safe: bool,
        has_credential: bool,
    ) -> AiProviderConfig {
        AiProviderConfig {
            id: id.into(),
            name: format!("Provider {id}"),
            base_url: format!("https://{id}.example/v1"),
            model: format!("model-{id}"),
            enabled,
            auto_safe,
            priority,
            has_credential,
        }
    }

    #[test]
    fn queue_health_updates_only_for_the_exact_current_execution_snapshot() {
        let current = AiExecutionSnapshot {
            execution_mode: AiExecutionMode::Codex,
            executor_id: r#"C:\Current\codex.exe"#.into(),
            model: "cli-default".into(),
            evidence_hash: String::new(),
            created_at_ms: 2_000,
        };
        let mut job = AiJob {
            id: "job-1".into(),
            generation: 1,
            kind: "daily_analysis".into(),
            payload_json: "{}".into(),
            status: AiJobStatus::Running,
            attempts: 0,
            next_attempt_at_ms: 0,
            last_error: String::new(),
            execution: current.clone(),
            started_at_ms: Some(2_000),
            finished_at_ms: None,
            duration_ms: None,
            executor_id: None,
            model: None,
            exit_code: None,
            error_kind: None,
        };

        assert!(queue_job_matches_current_execution(&job, &current));
        job.execution.executor_id = r#"C:\Old\codex.exe"#.into();
        assert!(!queue_job_matches_current_execution(&job, &current));
        job.execution.executor_id = current.executor_id.clone();
        job.execution.model = "old-model".into();
        assert!(!queue_job_matches_current_execution(&job, &current));
    }

    #[test]
    fn api_selection_rejects_missing_unknown_and_uncredentialed_providers() {
        let providers = vec![
            provider("missing-key", 0, true, true, false),
            provider("configured", 1, true, true, true),
        ];
        let mut settings = AppSettings::default();

        assert!(selected_api_provider(&settings, &providers).is_err());
        settings.selected_api_provider_id = Some("unknown".into());
        assert!(selected_api_provider(&settings, &providers).is_err());
        settings.selected_api_provider_id = Some("missing-key".into());
        assert!(selected_api_provider(&settings, &providers).is_err());
        settings.selected_api_provider_id = Some("configured".into());
        assert_eq!(
            selected_api_provider(&settings, &providers).unwrap().id,
            "configured"
        );
    }

    #[test]
    fn deleting_the_selected_provider_key_clears_only_that_selection() {
        let mut settings = AppSettings {
            selected_api_provider_id: Some("openai".into()),
            ..AppSettings::default()
        };

        assert!(!clear_selected_provider_for_deleted_key(
            &mut settings,
            "zhipu"
        ));
        assert_eq!(settings.selected_api_provider_id.as_deref(), Some("openai"));
        assert!(clear_selected_provider_for_deleted_key(
            &mut settings,
            "openai"
        ));
        assert_eq!(settings.selected_api_provider_id, None);
    }

    #[test]
    fn clearing_a_provider_selection_does_not_require_another_valid_provider() {
        assert!(!settings_patch_requires_api_provider_validation(
            &crate::app::SettingsPatch {
                selected_api_provider_id: Some(None),
                ..crate::app::SettingsPatch::default()
            }
        ));
        assert!(settings_patch_requires_api_provider_validation(
            &crate::app::SettingsPatch {
                selected_api_provider_id: Some(Some("openai".into())),
                ..crate::app::SettingsPatch::default()
            }
        ));
        assert!(settings_patch_requires_api_provider_validation(
            &crate::app::SettingsPatch {
                ai_execution_mode: Some(AiExecutionMode::ApiKey),
                ..crate::app::SettingsPatch::default()
            }
        ));
    }

    #[test]
    fn snapshot_freezes_the_manually_selected_provider_and_model() {
        let settings = AppSettings {
            ai_execution_mode: AiExecutionMode::ApiKey,
            selected_api_provider_id: Some("selected".into()),
            ..AppSettings::default()
        };
        let providers = vec![
            provider("priority", 0, true, true, true),
            provider("selected", 99, true, true, true),
        ];

        let snapshot = build_ai_execution_snapshot(&settings, &providers, "hash", 42).unwrap();

        assert_eq!(snapshot.executor_id, "selected");
        assert_eq!(snapshot.model, "model-selected");
    }

    #[test]
    fn api_health_failure_does_not_probe_a_fallback() {
        let providers = vec![
            provider("first", 1, true, true, true),
            provider("second", 2, true, true, true),
        ];
        let settings = AppSettings {
            ai_execution_mode: AiExecutionMode::ApiKey,
            selected_api_provider_id: Some("second".into()),
            ..AppSettings::default()
        };
        let attempts = Arc::new(Mutex::new(Vec::new()));
        let recorded_attempts = attempts.clone();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let (selected, result) = runtime
            .block_on(probe_selected_api_provider_with(
                &settings,
                &providers,
                move |provider| {
                    recorded_attempts.lock().unwrap().push(provider.id.clone());
                    async {
                        Err(ApiHealthProbeError::Unavailable(
                            "network unavailable".into(),
                        ))
                    }
                },
            ))
            .expect("a selected provider");

        let status = map_api_health_result(result);

        assert_eq!(selected.id, "second");
        assert_eq!(*attempts.lock().unwrap(), vec!["second"]);
        assert_eq!(status.0, AiConnectionHealthStatus::Unavailable);
        assert_eq!(status.1.as_deref(), Some("network unavailable"));
    }

    #[test]
    fn health_status_mapping_preserves_codex_diagnostics() {
        assert_eq!(
            map_codex_health_status(CodexHealthStatus::Healthy),
            AiConnectionHealthStatus::Reachable
        );
        assert_eq!(
            map_codex_health_status(CodexHealthStatus::PermissionDenied),
            AiConnectionHealthStatus::PermissionDenied
        );
        assert_eq!(
            map_codex_health_status(CodexHealthStatus::TimedOut),
            AiConnectionHealthStatus::TimedOut
        );
        assert_eq!(
            map_codex_health_status(CodexHealthStatus::Unavailable),
            AiConnectionHealthStatus::Unavailable
        );
        assert_eq!(
            map_codex_health_status(CodexHealthStatus::Error),
            AiConnectionHealthStatus::Error
        );
    }

    #[test]
    fn api_health_mapping_sanitizes_sensitive_diagnostics() {
        let malicious = format!(
            "request https://user:pass@example.test/models?api_key=secret Bearer bearer-secret sk-1234567890 {}",
            "x".repeat(2_000)
        );

        let (_, diagnostic) = map_api_health_result(Err(ApiHealthProbeError::Error(malicious)));
        let diagnostic = diagnostic.expect("diagnostic");

        for secret in [
            "user",
            "pass",
            "api_key",
            "secret",
            "bearer-secret",
            "sk-1234567890",
        ] {
            assert!(
                !diagnostic.contains(secret),
                "exposed {secret}: {diagnostic}"
            );
        }
        assert!(diagnostic.chars().count() <= 240);
    }

    #[test]
    fn active_probe_coalesces_many_refreshes_into_one_non_overlapping_rerun() {
        let coordinator = HealthProbeCoordinator::default();
        let mut active_runners = 0;
        let mut max_active_runners = 0;
        let mut reruns = 0;

        assert!(coordinator.request_probe());
        active_runners += 1;
        max_active_runners = max_active_runners.max(active_runners);

        for _ in 0..32 {
            assert!(!coordinator.request_probe());
            max_active_runners = max_active_runners.max(active_runners);
        }

        assert!(coordinator.finish_probe());
        reruns += 1;
        max_active_runners = max_active_runners.max(active_runners);
        assert!(!coordinator.finish_probe());
        active_runners -= 1;

        assert_eq!(reruns, 1);
        assert_eq!(max_active_runners, 1);
        assert_eq!(active_runners, 0);
        assert!(coordinator.request_probe());
        assert!(!coordinator.finish_probe());
    }

    #[test]
    fn refreshes_during_the_coalesced_rerun_do_not_schedule_a_third_probe() {
        let coordinator = HealthProbeCoordinator::default();

        assert!(coordinator.request_probe());
        assert!(!coordinator.request_probe());
        assert!(coordinator.finish_probe());

        for _ in 0..32 {
            assert!(!coordinator.request_probe());
        }

        assert!(!coordinator.finish_probe());
        assert!(coordinator.request_probe());
        assert!(!coordinator.finish_probe());
    }

    #[test]
    fn codex_health_never_serializes_the_configured_executable_path() {
        let private_path = r#"C:\Users\alice\private\codex.exe"#;
        let health = AiConnectionHealthProbePlan::Codex {
            config: CodexExecutionConfig {
                executable: private_path.into(),
                model: String::new(),
            },
        }
        .health(
            AiConnectionHealthStatus::Error,
            20,
            Some(format!("failed to start {private_path}")),
        )
        .sanitized();

        let serialized = serde_json::to_string(&health).unwrap();
        assert_eq!(health.executor_id, "codex");
        assert!(!serialized.contains(private_path), "{serialized}");
        assert_eq!(
            health.diagnostic.as_deref(),
            Some("Diagnostic details redacted")
        );
    }

    #[test]
    fn health_service_state_caches_the_latest_serializable_value() {
        let initial = AiConnectionHealth {
            execution_mode: AiExecutionMode::ApiKey,
            executor_id: String::new(),
            executor_label: "API provider".into(),
            model: String::new(),
            status: AiConnectionHealthStatus::Checking,
            verification_level: AiConnectionVerificationLevel::Connectivity,
            source: AiConnectionHealthSource::Background,
            checked_at_ms: 10,
            verified_at_ms: None,
            diagnostic: None,
        };
        let state = AiConnectionHealthServiceState::new(initial.clone());
        assert_eq!(state.current(), initial);

        let final_health = AiConnectionHealth {
            status: AiConnectionHealthStatus::Unconfigured,
            checked_at_ms: 20,
            diagnostic: Some("No automatic API provider is configured".into()),
            ..initial
        };
        state.store(final_health.clone());

        assert_eq!(state.current(), final_health);
        assert_eq!(
            serde_json::to_value(state.current()).unwrap(),
            serde_json::json!({
                "executionMode": "api-key",
                "executorId": "",
                "executorLabel": "API provider",
                "model": "",
                "status": "unconfigured",
                "verificationLevel": "connectivity",
                "source": "background",
                "checkedAtMs": 20,
                "verifiedAtMs": null,
                "diagnostic": "No automatic API provider is configured"
            })
        );
    }

    #[test]
    fn fresh_inference_failure_survives_background_connectivity_refresh() {
        let inference = AiConnectionHealth {
            execution_mode: AiExecutionMode::Codex,
            executor_id: "codex".into(),
            executor_label: "Codex".into(),
            model: "cli-default".into(),
            status: AiConnectionHealthStatus::TimedOut,
            verification_level: AiConnectionVerificationLevel::Inference,
            source: AiConnectionHealthSource::Manual,
            checked_at_ms: 100,
            verified_at_ms: Some(100),
            diagnostic: Some("Codex inference timed out".into()),
        };
        let state = AiConnectionHealthServiceState::new(inference.clone());

        for (status, checked_at_ms) in [
            (AiConnectionHealthStatus::Checking, 110),
            (AiConnectionHealthStatus::Reachable, 120),
        ] {
            let stored = state.store(AiConnectionHealth {
                status,
                verification_level: AiConnectionVerificationLevel::Connectivity,
                source: AiConnectionHealthSource::Background,
                checked_at_ms,
                verified_at_ms: None,
                diagnostic: None,
                ..inference.clone()
            });
            assert_eq!(
                stored,
                AiConnectionHealth {
                    checked_at_ms,
                    ..inference.clone()
                }
            );
        }

        let unavailable = AiConnectionHealth {
            status: AiConnectionHealthStatus::Unavailable,
            verification_level: AiConnectionVerificationLevel::Connectivity,
            source: AiConnectionHealthSource::Background,
            checked_at_ms: 130,
            verified_at_ms: None,
            diagnostic: Some("Codex is unreachable".into()),
            ..inference
        };
        assert_eq!(state.store(unavailable.clone()), unavailable);
    }

    #[test]
    fn fresh_inference_cache_expires_and_does_not_cross_models() {
        let inference = AiConnectionHealth {
            execution_mode: AiExecutionMode::Codex,
            executor_id: "codex".into(),
            executor_label: "Codex".into(),
            model: "model-a".into(),
            status: AiConnectionHealthStatus::Healthy,
            verification_level: AiConnectionVerificationLevel::Inference,
            source: AiConnectionHealthSource::Queue,
            checked_at_ms: 100,
            verified_at_ms: Some(100),
            diagnostic: None,
        };
        let different_model = AiConnectionHealth {
            model: "model-b".into(),
            status: AiConnectionHealthStatus::Reachable,
            verification_level: AiConnectionVerificationLevel::Connectivity,
            source: AiConnectionHealthSource::Background,
            checked_at_ms: 110,
            verified_at_ms: None,
            ..inference.clone()
        };
        let state = AiConnectionHealthServiceState::new(inference.clone());
        assert_eq!(state.store(different_model.clone()), different_model);

        let state = AiConnectionHealthServiceState::new(inference.clone());
        let expired = AiConnectionHealth {
            status: AiConnectionHealthStatus::Reachable,
            verification_level: AiConnectionVerificationLevel::Connectivity,
            source: AiConnectionHealthSource::Background,
            checked_at_ms: 100 + AI_INFERENCE_VERIFICATION_TTL_MS,
            verified_at_ms: None,
            ..inference
        };
        assert_eq!(state.store(expired.clone()), expired);
    }

    #[test]
    fn health_cache_sanitizes_diagnostics_before_serialization() {
        let initial = AiConnectionHealth {
            execution_mode: AiExecutionMode::ApiKey,
            executor_id: "openai".into(),
            executor_label: "OpenAI".into(),
            model: "gpt-test".into(),
            status: AiConnectionHealthStatus::Checking,
            verification_level: AiConnectionVerificationLevel::Connectivity,
            source: AiConnectionHealthSource::Background,
            checked_at_ms: 10,
            verified_at_ms: None,
            diagnostic: None,
        };
        let state = AiConnectionHealthServiceState::new(initial.clone());
        state.store(AiConnectionHealth {
            status: AiConnectionHealthStatus::Error,
            checked_at_ms: 20,
            diagnostic: Some(format!(
                "https://user:pass@example.test/models?api_key=secret Authorization: Bearer bearer-secret sk-1234567890 {}",
                "x".repeat(2_000)
            )),
            ..initial
        });

        let serialized = serde_json::to_string(&state.current()).unwrap();
        for secret in [
            "user",
            "pass",
            "api_key",
            "secret",
            "bearer-secret",
            "sk-1234567890",
        ] {
            assert!(
                !serialized.contains(secret),
                "exposed {secret}: {serialized}"
            );
        }
        assert!(state.current().diagnostic.unwrap().chars().count() <= 240);
    }
}
