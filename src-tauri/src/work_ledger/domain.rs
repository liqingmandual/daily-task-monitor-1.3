use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Active,
    Archived,
}

impl ProjectStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }

    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "archived" => Self::Archived,
            _ => Self::Active,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Todo,
    InProgress,
    Blocked,
    Completed,
    Cancelled,
}

impl TaskStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::InProgress => "in_progress",
            Self::Blocked => "blocked",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "in_progress" => Self::InProgress,
            "blocked" => Self::Blocked,
            "completed" => Self::Completed,
            "cancelled" => Self::Cancelled,
            _ => Self::Todo,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Low,
    Medium,
    High,
    Urgent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskOriginKind {
    Manual,
    Ai,
}

impl Default for TaskOriginKind {
    fn default() -> Self {
        Self::Manual
    }
}

impl TaskOriginKind {
    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "ai" => Self::Ai,
            _ => Self::Manual,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskReviewState {
    Confirmed,
    Provisional,
    Pending,
}

impl Default for TaskReviewState {
    fn default() -> Self {
        Self::Confirmed
    }
}

impl TaskReviewState {
    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "provisional" => Self::Provisional,
            "pending" => Self::Pending,
            _ => Self::Confirmed,
        }
    }
}

impl TaskPriority {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Urgent => "urgent",
        }
    }

    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "low" => Self::Low,
            "high" => Self::High,
            "urgent" => Self::Urgent,
            _ => Self::Medium,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceProvenance {
    Manual,
    Rule,
    Ai,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressOriginKind {
    Manual,
    FocusOutcome,
    DailyActualOutput,
}

impl ProgressOriginKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::FocusOutcome => "focus_outcome",
            Self::DailyActualOutput => "daily_actual_output",
        }
    }

    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "focus_outcome" => Self::FocusOutcome,
            "daily_actual_output" => Self::DailyActualOutput,
            _ => Self::Manual,
        }
    }
}

impl EvidenceProvenance {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Rule => "rule",
            Self::Ai => "ai",
        }
    }

    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "manual" => Self::Manual,
            "rule" => Self::Rule,
            _ => Self::Ai,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub color: String,
    pub status: ProjectStatus,
    pub description: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub archived_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewProject {
    pub id: String,
    pub name: String,
    pub color: String,
    pub description: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdate {
    pub name: Option<String>,
    pub color: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub expected_output: String,
    pub due_date: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub completed_at_ms: Option<i64>,
    pub origin_kind: TaskOriginKind,
    pub origin_key: Option<String>,
    pub origin_confidence: Option<f64>,
    pub review_state: TaskReviewState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTask {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub priority: TaskPriority,
    pub expected_output: String,
    pub due_date: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskUpdate {
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub priority: Option<TaskPriority>,
    pub expected_output: Option<String>,
    pub due_date: Option<Option<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceLink {
    pub task_id: String,
    pub evidence_id: String,
    pub provenance: EvidenceProvenance,
    pub confidence: f64,
    pub reason: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiWorkLedgerSuggestion {
    pub evidence_kind: String,
    pub evidence_id: String,
    pub evidence_hash: String,
    pub task_id: String,
    pub confidence: f64,
    pub reason: String,
    pub provider_id: String,
    pub model: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEntry {
    pub id: String,
    pub task_id: String,
    pub note: String,
    pub created_at_ms: i64,
    pub origin_kind: ProgressOriginKind,
    pub source_id: Option<String>,
    pub source_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewProgressEntry {
    pub id: String,
    pub task_id: String,
    pub note: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyGoalTaskLink {
    pub goal_row_id: String,
    pub goal_date: String,
    pub goal_text: String,
    pub task_id: String,
    pub confirmed_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmedDailyGoalTask {
    pub link: DailyGoalTaskLink,
    pub task: Task,
    pub task_created: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkLedgerRangeRollup {
    pub start_ms: i64,
    pub end_ms: i64,
    pub projects: Vec<ProjectRangeRollup>,
    pub tasks: Vec<TaskRangeRollup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRangeRollup {
    pub project_id: String,
    pub project_name: String,
    pub project_status: ProjectStatus,
    pub invested_seconds: i64,
    pub focus_seconds: i64,
    pub activity_segment_count: i64,
    pub browser_visit_count: i64,
    pub progress_count: i64,
    pub focus_session_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRangeRollup {
    pub task_id: String,
    pub project_id: String,
    pub task_title: String,
    pub task_status: TaskStatus,
    pub invested_seconds: i64,
    pub focus_seconds: i64,
    pub activity_segment_count: i64,
    pub browser_visit_count: i64,
    pub progress_count: i64,
    pub focus_session_count: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDailyPoint {
    pub date: String,
    pub invested_seconds: i64,
    pub activity_seconds: i64,
    pub focus_seconds: i64,
    pub switch_count: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskTimeSummary {
    pub task_id: String,
    pub lifecycle_total_seconds: i64,
    pub active_day_average_seconds: i64,
    pub natural_day_average_seconds: i64,
    pub active_day_count: i64,
    pub natural_day_count: i64,
    pub latest_activity_at_ms: Option<i64>,
    pub assignment_confidence: Option<f64>,
    pub review_state: TaskReviewState,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskEfficiencyDimension {
    pub key: String,
    pub label: String,
    pub conclusion: String,
    pub value: Option<f64>,
    pub unit: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskEfficiencyAssessment {
    pub dimensions: Vec<TaskEfficiencyDimension>,
    pub data_limitations: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskTimeInsight {
    pub summary: TaskTimeSummary,
    pub daily_points: Vec<TaskDailyPoint>,
    pub median_daily_seconds: i64,
    pub longest_continuous_seconds: i64,
    pub focus_seconds: i64,
    pub focus_share: f64,
    pub switches_per_hour: f64,
    pub regularity: f64,
    pub manual_correction_rate: f64,
    pub pending_review_seconds: i64,
    pub browser_evidence_count: i64,
    pub progress_count: i64,
    pub expected_output: String,
    pub assessment: TaskEfficiencyAssessment,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTimeSummary {
    pub project_id: String,
    pub lifecycle_total_seconds: i64,
    pub active_day_average_seconds: i64,
    pub natural_day_average_seconds: i64,
    pub active_day_count: i64,
    pub natural_day_count: i64,
    pub evidence_count: i64,
    pub completed_task_count: i64,
    pub task_count: i64,
    pub latest_activity_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDailyPoint {
    pub date: String,
    pub invested_seconds: i64,
    pub activity_seconds: i64,
    pub focus_seconds: i64,
    pub task_seconds: BTreeMap<String, i64>,
    pub shared_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTaskContribution {
    pub task_id: String,
    pub task_title: String,
    pub status: TaskStatus,
    pub invested_seconds: i64,
    pub evidence_count: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTimeInsight {
    pub summary: ProjectTimeSummary,
    pub daily_points: Vec<ProjectDailyPoint>,
    pub task_contributions: Vec<ProjectTaskContribution>,
    pub shared_seconds: i64,
    pub focus_seconds: i64,
    pub longest_continuous_seconds: i64,
    pub switch_count: i64,
    pub switches_per_hour: f64,
    pub browser_evidence_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTaskCancellation {
    pub task: Task,
    pub released_evidence_count: i64,
    pub dismissed_cluster_count: i64,
    pub project_archived: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectDraftTaskProposal {
    pub key: String,
    pub title: String,
    pub expected_output: String,
    pub cluster_ids: Vec<String>,
    #[serde(default)]
    pub semantic_reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectDraftProposal {
    pub target_project_id: Option<String>,
    pub name: String,
    pub description: String,
    pub tasks: Vec<ProjectDraftTaskProposal>,
    pub confidence: f64,
    pub reason_code: String,
    #[serde(default)]
    pub semantic_reason: String,
    #[serde(default)]
    pub data_limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDraftView {
    pub review_id: String,
    pub subject_id: String,
    pub proposal: ProjectDraftProposal,
    pub evidence_count: usize,
    pub accumulated_seconds: i64,
    pub created_at_ms: i64,
}

impl ProjectRangeRollup {
    pub(crate) fn has_evidence(&self) -> bool {
        self.invested_seconds > 0
            || self.activity_segment_count > 0
            || self.browser_visit_count > 0
            || self.progress_count > 0
            || self.focus_session_count > 0
    }
}

impl TaskRangeRollup {
    pub(crate) fn has_evidence(&self) -> bool {
        self.invested_seconds > 0
            || self.activity_segment_count > 0
            || self.browser_visit_count > 0
            || self.progress_count > 0
            || self.focus_session_count > 0
    }
}
