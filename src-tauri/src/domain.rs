use serde::{Deserialize, Serialize};

use crate::work_ledger::WorkLedgerRangeRollup;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityCategory {
    Idle,
    Research,
    VideoInput,
    TextInput,
    Game,
    Social,
    CreationDevelopment,
    FileManagement,
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VideoPurpose {
    Learning,
    Leisure,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityScope {
    All,
    Meaningful,
}

impl Default for ActivityScope {
    fn default() -> Self {
        Self::All
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityDisplayKey {
    Idle,
    Research,
    LearningVideo,
    LeisureVideo,
    UnknownVideo,
    TextInput,
    Game,
    Social,
    CreationDevelopment,
    FileManagement,
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeaningfulReason {
    Core,
    WorkflowLink,
    Excluded,
}

impl Default for MeaningfulReason {
    fn default() -> Self {
        Self::Excluded
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityCompositionItem {
    pub key: ActivityDisplayKey,
    pub category: ActivityCategory,
    pub video_purpose: Option<VideoPurpose>,
    pub seconds: i64,
    pub share: f64,
    pub meaningful_reason: MeaningfulReason,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityComposition {
    pub total_seconds: i64,
    pub items: Vec<ActivityCompositionItem>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityCompositions {
    pub all: ActivityComposition,
    pub meaningful: ActivityComposition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InactivityReason {
    InputIdle,
    ContinuityGap,
    LegacyGapRepair,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassificationSource {
    Manual,
    Idle,
    Rule,
    Behavior,
    Ai,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiClassificationReviewValue {
    pub category: ActivityCategory,
    pub video_purpose: VideoPurpose,
    pub confidence: f64,
    pub reason: String,
    pub model_version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiWorkflowAssignmentReviewValue {
    pub evidence_kind: String,
    pub evidence_id: String,
    pub task_id: Option<String>,
    pub confidence: f64,
    pub reason: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Classification {
    pub category: ActivityCategory,
    pub video_purpose: VideoPurpose,
    pub confidence: f32,
    pub source: ClassificationSource,
    pub reason: String,
    pub model_version: String,
    pub duration_seconds: i64,
    pub needs_review: bool,
}

impl Classification {
    pub fn new_ai(
        category: ActivityCategory,
        video_purpose: VideoPurpose,
        confidence: f32,
        reason: impl Into<String>,
        model_version: impl Into<String>,
        duration_seconds: i64,
    ) -> Self {
        let confidence = confidence.clamp(0.0, 1.0);
        Self {
            category,
            video_purpose,
            confidence,
            source: ClassificationSource::Ai,
            reason: reason.into(),
            model_version: model_version.into(),
            duration_seconds: duration_seconds.max(0),
            needs_review: confidence < 0.70,
        }
    }

    pub fn counts_as_learning(&self) -> bool {
        matches!(
            self.category,
            ActivityCategory::Research
                | ActivityCategory::TextInput
                | ActivityCategory::CreationDevelopment
        ) || (self.category == ActivityCategory::VideoInput
            && self.video_purpose == VideoPurpose::Learning)
    }
}

pub fn learning_seconds(classifications: &[Classification]) -> i64 {
    classifications
        .iter()
        .filter(|item| item.counts_as_learning())
        .map(|item| item.duration_seconds.max(0))
        .sum()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrendRange {
    pub start_ms: i64,
    pub end_ms: i64,
    pub start_date: String,
    pub end_date: String,
    pub day_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrendBreakdownItem {
    pub name: String,
    pub seconds: i64,
    pub share: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrendDay {
    pub date: String,
    pub label: String,
    pub monitored_seconds: i64,
    pub active_seconds: i64,
    pub idle_seconds: i64,
    pub learning_seconds: i64,
    pub switch_count: i64,
    pub longest_focus_seconds: i64,
    #[serde(default)]
    pub completed_task_count: i64,
    pub classification_coverage: f64,
    pub top_category: Option<TrendBreakdownItem>,
    pub top_app: Option<TrendBreakdownItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrendSummary {
    pub monitored_seconds: i64,
    pub active_seconds: i64,
    pub idle_seconds: i64,
    pub learning_seconds: i64,
    pub switch_count: i64,
    pub longest_focus_seconds: i64,
    #[serde(default)]
    pub completed_task_count: i64,
    pub average_monitored_seconds: f64,
    pub average_active_seconds: f64,
    pub average_idle_seconds: f64,
    pub average_learning_seconds: f64,
    pub average_switch_count: f64,
    pub learning_ratio: f64,
    pub switches_per_active_hour: f64,
    pub productive_day_count: usize,
    pub focus_day_count: usize,
    pub category_breakdown: Vec<TrendBreakdownItem>,
    pub app_breakdown: Vec<TrendBreakdownItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrendComparison {
    pub previous_range: TrendRange,
    pub day_count: usize,
    pub previous_monitored_seconds: i64,
    pub previous_active_seconds: i64,
    pub previous_idle_seconds: i64,
    pub previous_learning_seconds: i64,
    pub previous_switch_count: i64,
    pub previous_longest_focus_seconds: i64,
    #[serde(default)]
    pub previous_completed_task_count: i64,
    pub previous_learning_ratio: f64,
    pub previous_switches_per_active_hour: f64,
    pub previous_classification_coverage: f64,
    pub previous_category_breakdown: Vec<TrendBreakdownItem>,
    pub previous_app_breakdown: Vec<TrendBreakdownItem>,
    pub monitored_seconds_delta_percent: Option<f64>,
    pub active_seconds_delta_percent: Option<f64>,
    pub idle_seconds_delta_percent: Option<f64>,
    pub learning_seconds_delta_percent: Option<f64>,
    pub switch_count_delta_percent: Option<f64>,
    pub longest_focus_seconds_delta_percent: Option<f64>,
    #[serde(default)]
    pub completed_task_count_delta_percent: Option<f64>,
    pub learning_ratio_delta_percent: Option<f64>,
    pub switches_per_active_hour_delta_percent: Option<f64>,
    pub classification_coverage_delta_percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrendDataQuality {
    pub recorded_day_count: usize,
    pub missing_day_count: usize,
    pub classified_seconds: i64,
    pub pending_seconds: i64,
    pub low_confidence_seconds: i64,
    pub classification_coverage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrendPayload {
    pub range: TrendRange,
    pub days: Vec<TrendDay>,
    pub summary: TrendSummary,
    pub comparison: TrendComparison,
    pub quality: TrendDataQuality,
    #[serde(default)]
    pub work_ledger: WorkLedgerRangeRollup,
    pub evidence_hash: String,
}
