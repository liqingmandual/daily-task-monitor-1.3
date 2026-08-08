use std::{
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use tauri::State;

use crate::desktop::{
    AiAutomationGate, DesktopState, background_ai_gate_enabled, current_ai_execution_snapshot,
    state_service,
};

use super::{
    AiTaskCancellation, ConfirmedDailyGoalTask, DailyGoalTaskLink, EvidenceProvenance,
    NewProgressEntry, NewProject, NewTask, ProgressEntry, Project, ProjectTimeInsight,
    ProjectUpdate, Task, TaskPriority, TaskStatus, TaskTimeInsight, TaskUpdate,
    WorkLedgerRepository, WorkLedgerService, WorkLedgerSnapshot,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSaveRequest {
    pub id: String,
    pub name: String,
    pub color: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSaveRequest {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub priority: TaskPriority,
    #[serde(default)]
    pub expected_output: String,
    pub due_date: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStatusUpdateRequest {
    pub task_id: String,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEntryRequest {
    pub task_id: String,
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceAssignmentRequest {
    pub task_id: String,
    pub evidence_kind: String,
    pub evidence_id: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedAssignmentRequest {
    pub task_id: String,
    pub evidence_kind: String,
    pub evidence_id: String,
    pub evidence_hash: String,
    pub source: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub confidence: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyGoalTaskConfirmationRequest {
    pub goal_row_id: String,
    pub goal_date: String,
    pub goal_text: String,
    pub task_id: String,
    pub new_task: Option<ConfirmedGoalTaskCreation>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmedGoalTaskCreation {
    pub project_id: String,
    pub title: String,
    pub priority: TaskPriority,
    #[serde(default)]
    pub expected_output: String,
    pub due_date: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyActualOutputProgressRequest {
    pub task_id: String,
    pub date: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeTasksRequest {
    pub source_task_id: String,
    pub target_task_id: String,
}

pub fn get_work_ledger(
    state: State<'_, DesktopState>,
    start_ms: i64,
    end_ms: i64,
    project_id: Option<String>,
) -> Result<WorkLedgerSnapshot, String> {
    let service = state_service(&state)?;
    let settings = service.get_settings().map_err(|error| error.to_string())?;
    let execution = background_ai_execution_snapshot(
        &service,
        background_ai_gate_enabled(&settings, AiAutomationGate::WorkflowAssignment),
        now_ms(),
    );
    WorkLedgerService::new(WorkLedgerRepository::new(service.database()))
        .get_work_ledger_with_ai_queue(start_ms, end_ms, project_id.as_deref(), execution.as_ref())
        .map_err(|error| error.to_string())
}

fn background_ai_execution_snapshot(
    service: &crate::app::AppService,
    enabled: bool,
    created_at_ms: i64,
) -> Option<crate::ai::AiExecutionSnapshot> {
    enabled
        .then(|| current_ai_execution_snapshot(service, "", created_at_ms))
        .and_then(Result::ok)
}

pub fn queue_workflow_ai_suggestions(
    state: State<'_, DesktopState>,
    start_ms: i64,
    end_ms: i64,
) -> Result<crate::work_ledger::WorkflowAnalysisQueueResult, String> {
    let service = state_service(&state)?;
    let execution = current_ai_execution_snapshot(&service, "", now_ms())?;
    WorkLedgerService::new(WorkLedgerRepository::new(service.database()))
        .queue_workflow_ai_suggestions(start_ms, end_ms, &execution)
        .map_err(|error| error.to_string())
}

pub fn get_work_ledger_task_insight(
    state: State<'_, DesktopState>,
    task_id: String,
    end_ms: i64,
) -> Result<TaskTimeInsight, String> {
    let service = state_service(&state)?;
    WorkLedgerService::new(WorkLedgerRepository::new(service.database()))
        .task_time_insight(&task_id, end_ms)
        .map_err(|error| error.to_string())
}

pub fn get_work_ledger_project_insight(
    state: State<'_, DesktopState>,
    project_id: String,
    end_ms: i64,
    range_days: i64,
) -> Result<ProjectTimeInsight, String> {
    let service = state_service(&state)?;
    WorkLedgerService::new(WorkLedgerRepository::new(service.database()))
        .project_time_insight(&project_id, end_ms, range_days)
        .map_err(|error| error.to_string())
}

pub fn merge_work_ledger_tasks(
    state: State<'_, DesktopState>,
    request: MergeTasksRequest,
) -> Result<bool, String> {
    let service = state_service(&state)?;
    WorkLedgerService::new(WorkLedgerRepository::new(service.database()))
        .merge_tasks(&request.source_task_id, &request.target_task_id, now_ms())
        .map_err(|error| error.to_string())
}

pub fn save_work_ledger_project(
    state: State<'_, DesktopState>,
    request: ProjectSaveRequest,
) -> Result<Project, String> {
    let service = state_service(&state)?;
    let ledger = WorkLedgerService::new(WorkLedgerRepository::new(service.database()));
    if ledger
        .get_project(&request.id)
        .map_err(|error| error.to_string())?
        .is_some()
    {
        ledger
            .update_project(
                &request.id,
                ProjectUpdate {
                    name: Some(request.name),
                    color: Some(request.color),
                    description: Some(request.description),
                },
                now_ms(),
            )
            .map_err(|error| error.to_string())?;
        return ledger
            .get_project(&request.id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Project was not found after saving".to_string());
    }
    ledger
        .create_project(NewProject {
            id: request.id,
            name: request.name,
            color: request.color,
            description: request.description,
            created_at_ms: now_ms(),
        })
        .map_err(|error| error.to_string())
}

pub fn archive_work_ledger_project(
    state: State<'_, DesktopState>,
    project_id: String,
) -> Result<bool, String> {
    let service = state_service(&state)?;
    WorkLedgerService::new(WorkLedgerRepository::new(service.database()))
        .archive_project(&project_id, now_ms())
        .map_err(|error| error.to_string())
}

pub fn save_work_ledger_task(
    state: State<'_, DesktopState>,
    request: TaskSaveRequest,
) -> Result<Task, String> {
    let service = state_service(&state)?;
    let ledger = WorkLedgerService::new(WorkLedgerRepository::new(service.database()));
    if ledger
        .get_task(&request.id)
        .map_err(|error| error.to_string())?
        .is_some()
    {
        ledger
            .update_task(
                &request.id,
                TaskUpdate {
                    project_id: Some(request.project_id),
                    title: Some(request.title),
                    priority: Some(request.priority),
                    expected_output: Some(request.expected_output),
                    due_date: Some(request.due_date),
                },
                now_ms(),
            )
            .map_err(|error| error.to_string())?;
        return ledger
            .get_task(&request.id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Task was not found after saving".to_string());
    }
    ledger
        .create_task(NewTask {
            id: request.id,
            project_id: request.project_id,
            title: request.title,
            priority: request.priority,
            expected_output: request.expected_output,
            due_date: request.due_date,
            created_at_ms: now_ms(),
        })
        .map_err(|error| error.to_string())
}

pub fn update_work_ledger_task_status(
    state: State<'_, DesktopState>,
    request: TaskStatusUpdateRequest,
) -> Result<Task, String> {
    let service = state_service(&state)?;
    WorkLedgerService::new(WorkLedgerRepository::new(service.database()))
        .transition_task_status(&request.task_id, request.status, now_ms())
        .map_err(|error| error.to_string())
}

pub fn cancel_work_ledger_ai_task(
    state: State<'_, DesktopState>,
    task_id: String,
) -> Result<AiTaskCancellation, String> {
    let service = state_service(&state)?;
    WorkLedgerService::new(WorkLedgerRepository::new(service.database()))
        .cancel_ai_task(&task_id, now_ms())
        .map_err(|error| error.to_string())
}

pub fn add_work_ledger_progress(
    state: State<'_, DesktopState>,
    request: ProgressEntryRequest,
) -> Result<ProgressEntry, String> {
    let service = state_service(&state)?;
    let now = now_ms();
    WorkLedgerService::new(WorkLedgerRepository::new(service.database()))
        .add_progress_entry(NewProgressEntry {
            id: progress_entry_id(now),
            task_id: request.task_id,
            note: request.note,
            created_at_ms: now,
        })
        .map_err(|error| error.to_string())
}

pub fn assign_work_ledger_evidence(
    state: State<'_, DesktopState>,
    request: EvidenceAssignmentRequest,
) -> Result<bool, String> {
    let service = state_service(&state)?;
    let ledger = WorkLedgerService::new(WorkLedgerRepository::new(service.database()));
    let reason = if request.reason.trim().is_empty() {
        "Assigned by user"
    } else {
        request.reason.trim()
    };
    match request.evidence_kind.as_str() {
        "activity" => ledger.assign_activity(
            &request.task_id,
            &request.evidence_id,
            EvidenceProvenance::Manual,
            1.0,
            reason,
            now_ms(),
        ),
        "browser" => ledger.assign_browser_visit(
            &request.task_id,
            &request.evidence_id,
            EvidenceProvenance::Manual,
            1.0,
            reason,
            now_ms(),
        ),
        _ => Ok(false),
    }
    .map_err(|error| error.to_string())
}

pub fn remove_work_ledger_evidence(
    state: State<'_, DesktopState>,
    request: EvidenceAssignmentRequest,
) -> Result<bool, String> {
    let service = state_service(&state)?;
    let ledger = WorkLedgerService::new(WorkLedgerRepository::new(service.database()));
    match request.evidence_kind.as_str() {
        "activity" => ledger.remove_activity(&request.task_id, &request.evidence_id),
        "browser" => ledger.remove_browser_visit(&request.task_id, &request.evidence_id),
        _ => Ok(false),
    }
    .map_err(|error| error.to_string())
}

pub fn apply_work_ledger_suggestion(
    state: State<'_, DesktopState>,
    request: SuggestedAssignmentRequest,
) -> Result<bool, String> {
    let service = state_service(&state)?;
    WorkLedgerService::new(WorkLedgerRepository::new(service.database()))
        .apply_suggested_assignment(
            &request.task_id,
            &request.evidence_kind,
            &request.evidence_id,
            &request.evidence_hash,
            &request.source,
            request.start_ms,
            request.end_ms,
            request.confidence,
            &request.reason,
            now_ms(),
        )
        .map_err(|error| error.to_string())
}

pub fn confirm_daily_goal_task(
    state: State<'_, DesktopState>,
    request: DailyGoalTaskConfirmationRequest,
) -> Result<ConfirmedDailyGoalTask, String> {
    let service = state_service(&state)?;
    let ledger = WorkLedgerService::new(WorkLedgerRepository::new(service.database()));
    let now = now_ms();
    let new_task = request.new_task.map(|task| NewTask {
        id: request.task_id.clone(),
        project_id: task.project_id,
        title: task.title,
        priority: task.priority,
        expected_output: task.expected_output,
        due_date: task.due_date,
        created_at_ms: now,
    });
    ledger
        .confirm_daily_goal_task(
            &request.goal_row_id,
            &request.goal_date,
            &request.goal_text,
            &request.task_id,
            new_task,
            now,
        )
        .map_err(|error| error.to_string())
}

pub fn list_daily_goal_task_links(
    state: State<'_, DesktopState>,
    date: String,
) -> Result<Vec<DailyGoalTaskLink>, String> {
    let service = state_service(&state)?;
    WorkLedgerService::new(WorkLedgerRepository::new(service.database()))
        .list_daily_goal_task_links(&date)
        .map_err(|error| error.to_string())
}

pub fn record_daily_actual_output_progress(
    state: State<'_, DesktopState>,
    request: DailyActualOutputProgressRequest,
) -> Result<Option<ProgressEntry>, String> {
    let service = state_service(&state)?;
    WorkLedgerService::new(WorkLedgerRepository::new(service.database()))
        .record_daily_actual_output_progress(&request.task_id, &request.date, now_ms())
        .map_err(|error| error.to_string())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

static PROGRESS_ENTRY_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static PROGRESS_ENTRY_INSTANCE: OnceLock<String> = OnceLock::new();

fn progress_entry_id(created_at_ms: i64) -> String {
    let sequence = PROGRESS_ENTRY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let instance = PROGRESS_ENTRY_INSTANCE.get_or_init(|| {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        format!("{nanos}:{}", std::process::id())
    });
    let hash = format!(
        "{:x}",
        Sha256::digest(format!("progress\n{created_at_ms}\n{instance}\n{sequence}").as_bytes())
    );
    format!("progress-{}", &hash[..24])
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{background_ai_execution_snapshot, progress_entry_id};
    use crate::app::AppService;
    use crate::db::Database;

    #[test]
    fn progress_entry_ids_are_unique_for_rapid_calls_in_the_same_millisecond() {
        let ids: HashSet<_> = (0..256).map(|_| progress_entry_id(1_000)).collect();
        assert_eq!(ids.len(), 256);
    }

    #[test]
    fn missing_api_selection_disables_background_ai_without_blocking_local_ledger_data() {
        let service = AppService::new(Database::open_in_memory().unwrap());

        assert!(background_ai_execution_snapshot(&service, true, 1_000).is_none());
    }
}
