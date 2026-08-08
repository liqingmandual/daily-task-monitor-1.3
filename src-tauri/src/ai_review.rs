use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ai::{AiExecutionErrorKind, AiExecutionMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiReviewKind {
    Classification,
    WorkflowAssignment,
    ProjectDraft,
}

impl AiReviewKind {
    pub(crate) fn as_database(self) -> &'static str {
        match self {
            Self::Classification => "classification",
            Self::WorkflowAssignment => "workflow_assignment",
            Self::ProjectDraft => "project_draft",
        }
    }

    pub(crate) fn from_database(value: &str) -> Option<Self> {
        match value {
            "classification" => Some(Self::Classification),
            "workflow_assignment" => Some(Self::WorkflowAssignment),
            "project_draft" => Some(Self::ProjectDraft),
            _ => None,
        }
    }

    pub(crate) fn field_name(self) -> &'static str {
        match self {
            Self::Classification => "classification",
            Self::WorkflowAssignment => "workflow_assignment",
            Self::ProjectDraft => "project_draft",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiReviewState {
    Pending,
    AutoApplied,
    ManualOverride,
    ExecutionError,
    Dismissed,
    Reverted,
}

impl AiReviewState {
    pub(crate) fn as_database(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::AutoApplied => "auto_applied",
            Self::ManualOverride => "manual_override",
            Self::ExecutionError => "execution_error",
            Self::Dismissed => "dismissed",
            Self::Reverted => "reverted",
        }
    }

    pub(crate) fn from_database(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "auto_applied" => Some(Self::AutoApplied),
            "manual_override" => Some(Self::ManualOverride),
            "execution_error" => Some(Self::ExecutionError),
            "dismissed" => Some(Self::Dismissed),
            "reverted" => Some(Self::Reverted),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiExecutionAuditView {
    pub execution_mode: Option<AiExecutionMode>,
    pub executor_id: Option<String>,
    pub model: Option<String>,
    pub evidence_hash: String,
    pub generation: i64,
    pub created_at_ms: i64,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub duration_ms: Option<i64>,
    pub exit_code: Option<i32>,
    pub error_kind: Option<AiExecutionErrorKind>,
    pub diagnostic: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiReviewRecord {
    pub id: String,
    pub kind: AiReviewKind,
    pub state: AiReviewState,
    pub subject_id: String,
    pub before_json: String,
    pub proposed_json: String,
    pub applied_json: Option<String>,
    pub confidence: Option<f64>,
    pub evidence_summary: String,
    pub evidence_hash: String,
    pub execution: AiExecutionAuditView,
    pub created_at_ms: i64,
    pub resolved_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AiReviewDraft {
    pub id: String,
    pub job_id: Option<String>,
    pub kind: AiReviewKind,
    pub subject_id: String,
    pub before_json: String,
    pub proposed_json: String,
    pub confidence: Option<f64>,
    pub evidence_summary: String,
    pub evidence_hash: String,
    pub execution: AiExecutionAuditView,
    pub created_at_ms: i64,
    pub execution_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiReviewAction {
    Accept,
    Change,
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiReviewResolution {
    pub review_ids: Vec<String>,
    pub action: AiReviewAction,
    pub changed_json: Option<String>,
    #[serde(default)]
    pub evidence_hashes: BTreeMap<String, String>,
    pub resolved_at_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiReviewFilter {
    #[serde(default)]
    pub states: Vec<AiReviewState>,
    #[serde(default)]
    pub kinds: Vec<AiReviewKind>,
    pub subject_id: Option<String>,
    pub execution_mode: Option<AiExecutionMode>,
    pub executor_id: Option<String>,
    pub model: Option<String>,
    pub created_from_ms: Option<i64>,
    pub created_to_ms: Option<i64>,
    pub min_confidence: Option<f64>,
    pub max_confidence: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiReviewEventKind {
    Generated,
    AutoApplied,
    Accepted,
    Changed,
    Ignored,
    Reverted,
    Retried,
    Failed,
}

impl AiReviewEventKind {
    pub(crate) fn as_database(self) -> &'static str {
        match self {
            Self::Generated => "generated",
            Self::AutoApplied => "auto_applied",
            Self::Accepted => "accepted",
            Self::Changed => "changed",
            Self::Ignored => "ignored",
            Self::Reverted => "reverted",
            Self::Retried => "retried",
            Self::Failed => "failed",
        }
    }

    pub(crate) fn from_database(value: &str) -> Option<Self> {
        match value {
            "generated" => Some(Self::Generated),
            "auto_applied" => Some(Self::AutoApplied),
            "accepted" => Some(Self::Accepted),
            "changed" => Some(Self::Changed),
            "ignored" => Some(Self::Ignored),
            "reverted" => Some(Self::Reverted),
            "retried" => Some(Self::Retried),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}
