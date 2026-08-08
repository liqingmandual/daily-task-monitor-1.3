#[cfg(feature = "desktop")]
pub mod commands;
mod domain;
mod repository;
mod service;

pub use domain::{
    AiTaskCancellation, AiWorkLedgerSuggestion, ConfirmedDailyGoalTask, DailyGoalTaskLink,
    EvidenceLink, EvidenceProvenance, NewProgressEntry, NewProject, NewTask, ProgressEntry,
    ProgressOriginKind, Project, ProjectDailyPoint, ProjectDraftProposal, ProjectDraftTaskProposal,
    ProjectDraftView, ProjectRangeRollup, ProjectStatus, ProjectTaskContribution,
    ProjectTimeInsight, ProjectTimeSummary, ProjectUpdate, Task, TaskDailyPoint,
    TaskEfficiencyAssessment, TaskEfficiencyDimension, TaskOriginKind, TaskPriority,
    TaskRangeRollup, TaskReviewState, TaskStatus, TaskTimeInsight, TaskTimeSummary, TaskUpdate,
    WorkLedgerRangeRollup,
};
pub use repository::WorkLedgerRepository;
pub use service::{
    LinkedWorkLedgerEvidence, WorkLedgerAssignmentConsumptionError, WorkLedgerAssignmentJobPayload,
    WorkLedgerAssignmentTaskCandidate, WorkLedgerCandidateCluster, WorkLedgerEvidence,
    WorkLedgerService, WorkLedgerSnapshot, WorkLedgerSuggestion, WorkLedgerSummary,
    WorkflowAnalysisQueueResult, parse_work_ledger_assignment_job,
};
pub(crate) use service::{work_ledger_evidence_from_activity, work_ledger_evidence_from_browser};
