use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::activity_composition::{
    ActivityCompositionSlice, activity_is_meaningful, build_activity_compositions,
    effective_meaningful_reason,
};
use crate::db::{
    ActivitySegmentRecord, Database, TrendRangeFacts, WorkLedgerActivityRangeFact,
    WorkLedgerFocusRangeFact, WorkLedgerRangeFacts,
};
use crate::domain::{
    ActivityCategory, ActivityCompositions, ActivityScope, InactivityReason, MeaningfulReason,
    VideoPurpose,
};
use crate::segment_overlap::canonicalize_activity_segments;
use crate::work_ledger::WorkLedgerRepository;

const MILLIS_PER_MINUTE: i64 = 60_000;
const LOW_CONFIDENCE_THRESHOLD: f32 = 0.7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrendGranularity {
    Day,
    Week,
    Month,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrendSelectionMode {
    #[default]
    Continuous,
    SelectedDates,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrendMetric {
    MonitoredSeconds,
    ActiveSeconds,
    LearningSeconds,
    IdleSeconds,
    SwitchCount,
    LongestFocusSeconds,
    ClassificationCoverage,
    CompletedTaskCount,
    LinkedTaskSeconds,
}

const ALL_TREND_METRICS: [TrendMetric; 9] = [
    TrendMetric::MonitoredSeconds,
    TrendMetric::ActiveSeconds,
    TrendMetric::LearningSeconds,
    TrendMetric::IdleSeconds,
    TrendMetric::SwitchCount,
    TrendMetric::LongestFocusSeconds,
    TrendMetric::ClassificationCoverage,
    TrendMetric::CompletedTaskCount,
    TrendMetric::LinkedTaskSeconds,
];

const AVAILABLE_TREND_METRICS: [TrendMetric; 9] = [
    TrendMetric::MonitoredSeconds,
    TrendMetric::ActiveSeconds,
    TrendMetric::LearningSeconds,
    TrendMetric::IdleSeconds,
    TrendMetric::SwitchCount,
    TrendMetric::LongestFocusSeconds,
    TrendMetric::ClassificationCoverage,
    TrendMetric::CompletedTaskCount,
    TrendMetric::LinkedTaskSeconds,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrendWorkbenchErrorCode {
    InvalidRequest,
    MetricUnavailable,
    DataAccess,
    RuntimeUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendWorkbenchError {
    pub code: TrendWorkbenchErrorCode,
    pub message: String,
    pub metric: Option<TrendMetric>,
}

impl TrendWorkbenchError {
    pub fn runtime_unavailable() -> Self {
        Self {
            code: TrendWorkbenchErrorCode::RuntimeUnavailable,
            message: "trend workbench service is unavailable".into(),
            metric: None,
        }
    }
}

impl fmt::Display for TrendWorkbenchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for TrendWorkbenchError {}

impl From<rusqlite::Error> for TrendWorkbenchError {
    fn from(error: rusqlite::Error) -> Self {
        match error {
            rusqlite::Error::InvalidParameterName(_) => Self {
                code: TrendWorkbenchErrorCode::InvalidRequest,
                message: "invalid trend workbench request".into(),
                metric: None,
            },
            _ => Self {
                code: TrendWorkbenchErrorCode::DataAccess,
                message: "unable to load trend workbench data".into(),
                metric: None,
            },
        }
    }
}

pub type TrendWorkbenchResult<T> = std::result::Result<T, TrendWorkbenchError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrendMetricAvailabilityStatus {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendMetricAvailability {
    pub metric: TrendMetric,
    pub status: TrendMetricAvailabilityStatus,
    pub reason_code: Option<TrendWorkbenchErrorCode>,
}

pub fn trend_metric_availability() -> Vec<TrendMetricAvailability> {
    ALL_TREND_METRICS
        .iter()
        .map(|metric| TrendMetricAvailability {
            metric: *metric,
            status: TrendMetricAvailabilityStatus::Available,
            reason_code: None,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendDateRange {
    pub start_date: String,
    pub end_date: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendWorkbenchRange {
    pub start_ms: i64,
    pub end_ms: i64,
    pub start_date: String,
    pub end_date: String,
    pub day_count: usize,
    #[serde(default)]
    pub selection_mode: TrendSelectionMode,
    #[serde(default)]
    pub selected_dates: Vec<String>,
    #[serde(default)]
    pub selected_date_count: usize,
    #[serde(default)]
    pub envelope_day_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendWorkbenchRequest {
    pub start_date: String,
    pub end_date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_dates: Option<Vec<String>>,
    pub timezone_offset_minutes: i32,
    pub granularity: Option<TrendGranularity>,
    pub metric: TrendMetric,
    pub custom_baseline: Option<TrendDateRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrendBaselineKind {
    Current,
    PreviousEqualLength,
    PreviousMonthSamePeriod,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrendEvidenceScope {
    Summary,
    Bucket,
    Baseline,
    Rate,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendMetricValues {
    pub monitored_seconds: i64,
    pub active_seconds: i64,
    pub learning_seconds: i64,
    pub idle_seconds: i64,
    pub switch_count: i64,
    pub longest_focus_seconds: i64,
    pub classification_coverage: f64,
    pub completed_task_count: i64,
    pub linked_task_seconds: i64,
}

impl TrendMetricValues {
    pub(crate) fn get(&self, metric: TrendMetric) -> f64 {
        match metric {
            TrendMetric::MonitoredSeconds => self.monitored_seconds as f64,
            TrendMetric::ActiveSeconds => self.active_seconds as f64,
            TrendMetric::LearningSeconds => self.learning_seconds as f64,
            TrendMetric::IdleSeconds => self.idle_seconds as f64,
            TrendMetric::SwitchCount => self.switch_count as f64,
            TrendMetric::LongestFocusSeconds => self.longest_focus_seconds as f64,
            TrendMetric::ClassificationCoverage => self.classification_coverage,
            TrendMetric::CompletedTaskCount => self.completed_task_count as f64,
            TrendMetric::LinkedTaskSeconds => self.linked_task_seconds as f64,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendStatisticalMetricValues {
    pub monitored_seconds: f64,
    pub active_seconds: f64,
    pub learning_seconds: f64,
    pub idle_seconds: f64,
    pub switch_count: f64,
    pub longest_focus_seconds: f64,
    pub classification_coverage: f64,
    pub completed_task_count: f64,
    pub linked_task_seconds: f64,
}

impl TrendStatisticalMetricValues {
    fn get(&self, metric: TrendMetric) -> f64 {
        match metric {
            TrendMetric::MonitoredSeconds => self.monitored_seconds,
            TrendMetric::ActiveSeconds => self.active_seconds,
            TrendMetric::LearningSeconds => self.learning_seconds,
            TrendMetric::IdleSeconds => self.idle_seconds,
            TrendMetric::SwitchCount => self.switch_count,
            TrendMetric::LongestFocusSeconds => self.longest_focus_seconds,
            TrendMetric::ClassificationCoverage => self.classification_coverage,
            TrendMetric::CompletedTaskCount => self.completed_task_count,
            TrendMetric::LinkedTaskSeconds => self.linked_task_seconds,
        }
    }

    fn set(&mut self, metric: TrendMetric, value: f64) {
        match metric {
            TrendMetric::MonitoredSeconds => self.monitored_seconds = value,
            TrendMetric::ActiveSeconds => self.active_seconds = value,
            TrendMetric::LearningSeconds => self.learning_seconds = value,
            TrendMetric::IdleSeconds => self.idle_seconds = value,
            TrendMetric::SwitchCount => self.switch_count = value,
            TrendMetric::LongestFocusSeconds => self.longest_focus_seconds = value,
            TrendMetric::ClassificationCoverage => self.classification_coverage = value,
            TrendMetric::CompletedTaskCount => self.completed_task_count = value,
            TrendMetric::LinkedTaskSeconds => self.linked_task_seconds = value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendBucket {
    pub id: String,
    pub start_date: String,
    pub end_date: String,
    pub values: TrendMetricValues,
    pub recorded_day_count: usize,
    pub missing_day_count: usize,
    #[serde(default)]
    pub selection_mode: TrendSelectionMode,
    #[serde(default)]
    pub selected_dates: Vec<String>,
    #[serde(default)]
    pub selected_date_count: usize,
    #[serde(default)]
    pub envelope_day_count: usize,
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub activity_composition: ActivityCompositions,
    #[serde(default)]
    pub drilldown: TrendBucketDrilldown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendRawRow {
    pub row_id: String,
    pub bucket_id: String,
    pub evidence_kind: TrendRawEvidenceKind,
    pub evidence_id: String,
    pub date: String,
    pub start_time: String,
    pub end_time: String,
    pub app: String,
    pub title_summary: String,
    pub category: String,
    #[serde(default)]
    pub video_purpose: Option<VideoPurpose>,
    #[serde(default)]
    pub meaningful: bool,
    #[serde(default)]
    pub meaningful_reason: MeaningfulReason,
    pub task_id: Option<String>,
    pub task_title: Option<String>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub clipped_duration_seconds: i64,
    pub confidence: Option<f64>,
    pub review_state: TrendReviewState,
    pub shared: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrendRawEvidenceKind {
    Activity,
    Focus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrendReviewState {
    Confirmed,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendDistributionItem {
    pub key: String,
    pub label: String,
    pub seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendCompletedTaskItem {
    pub task_id: String,
    pub task_title: String,
    pub project_id: String,
    pub project_name: String,
    pub completed_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendLinkedTaskRollup {
    pub task_id: String,
    pub task_title: String,
    pub project_id: String,
    pub project_name: String,
    pub linked_seconds: i64,
    pub activity_seconds: i64,
    pub focus_seconds: i64,
    pub evidence_count: usize,
    pub shared_evidence_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendLinkedProjectRollup {
    pub project_id: String,
    pub project_name: String,
    pub linked_seconds: i64,
    pub activity_seconds: i64,
    pub focus_seconds: i64,
    pub evidence_count: usize,
    pub shared_evidence_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendWorkflowOwnership {
    pub ownership_id: String,
    pub evidence_kind: TrendRawEvidenceKind,
    pub evidence_id: String,
    pub task_id: String,
    pub task_title: String,
    pub project_id: String,
    pub project_name: String,
    pub shared: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendDrilldownQuality {
    pub recorded_day_count: usize,
    pub missing_day_count: usize,
    pub classified_seconds: i64,
    pub classification_coverage: f64,
    pub low_confidence_seconds: i64,
    pub pending_seconds: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendBucketDrilldown {
    pub bucket_id: String,
    pub raw_rows: Vec<TrendRawRow>,
    pub application_distribution: Vec<TrendDistributionItem>,
    pub category_distribution: Vec<TrendDistributionItem>,
    pub completed_tasks: Vec<TrendCompletedTaskItem>,
    pub linked_task_rollups: Vec<TrendLinkedTaskRollup>,
    pub linked_project_rollups: Vec<TrendLinkedProjectRollup>,
    pub workflow_ownership: Vec<TrendWorkflowOwnership>,
    pub data_quality: TrendDrilldownQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendBaselineSeries {
    pub kind: TrendBaselineKind,
    pub range: TrendDateRange,
    pub is_valid: bool,
    pub value: Option<f64>,
    pub absolute_delta: Option<f64>,
    pub percent_delta: Option<f64>,
    pub recorded_day_count: usize,
    pub missing_day_count: usize,
    #[serde(default)]
    pub selection_mode: TrendSelectionMode,
    #[serde(default)]
    pub selected_dates: Vec<String>,
    #[serde(default)]
    pub selected_date_count: usize,
    #[serde(default)]
    pub envelope_day_count: usize,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendWorkbenchSummary {
    pub totals: TrendMetricValues,
    pub daily_average: TrendDailyAverage,
    pub average_sample_day_count: usize,
    pub switches_per_active_hour: Option<f64>,
    pub mean_per_bucket: TrendStatisticalMetricValues,
    pub daily_median: TrendStatisticalMetricValues,
    pub daily_max: TrendStatisticalMetricValues,
    pub daily_sample_stddev: TrendStatisticalMetricValues,
    pub daily_coefficient_of_variation: TrendStatisticalMetricValues,
    pub recorded_day_count: usize,
    pub effective_activity_day_count: usize,
    pub missing_day_count: usize,
    pub classified_seconds: i64,
    pub classification_coverage: f64,
    pub low_confidence_seconds: i64,
    pub pending_seconds: i64,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendDailyAverage {
    pub monitored_seconds: Option<f64>,
    pub active_seconds: Option<f64>,
    pub idle_seconds: Option<f64>,
    pub learning_seconds: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendEvidence {
    pub id: String,
    pub scope: TrendEvidenceScope,
    pub series_kind: TrendBaselineKind,
    pub bucket_id: Option<String>,
    pub metric: Option<TrendMetric>,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendInactivityReasonDistribution {
    pub reason: InactivityReason,
    pub seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendWorkbenchPayload {
    pub range: TrendWorkbenchRange,
    pub granularity: TrendGranularity,
    pub metric: TrendMetric,
    pub metric_availability: Vec<TrendMetricAvailability>,
    pub buckets: Vec<TrendBucket>,
    pub summary: TrendWorkbenchSummary,
    #[serde(default)]
    pub activity_composition: ActivityCompositions,
    #[serde(default)]
    pub inactivity_reason_distribution: Vec<TrendInactivityReasonDistribution>,
    pub baselines: Vec<TrendBaselineSeries>,
    pub evidence: Vec<TrendEvidence>,
    pub evidence_hash: String,
}

#[derive(Debug, Clone)]
struct ValidatedDateRange {
    start: NaiveDate,
    end: NaiveDate,
    day_count: usize,
}

impl ValidatedDateRange {
    fn dto(&self) -> TrendDateRange {
        TrendDateRange {
            start_date: format_date(self.start),
            end_date: format_date(self.end),
        }
    }
}

#[derive(Debug, Clone)]
struct ValidatedDateSelection {
    envelope: ValidatedDateRange,
    selected_dates: Option<Vec<NaiveDate>>,
}

impl ValidatedDateSelection {
    fn continuous(envelope: ValidatedDateRange) -> Self {
        Self {
            envelope,
            selected_dates: None,
        }
    }

    fn selection_mode(&self) -> TrendSelectionMode {
        if self.selected_dates.is_some() {
            TrendSelectionMode::SelectedDates
        } else {
            TrendSelectionMode::Continuous
        }
    }

    fn selected_date_count(&self) -> usize {
        self.selected_dates
            .as_ref()
            .map_or(self.envelope.day_count, Vec::len)
    }

    fn selected_date_strings(&self) -> Vec<String> {
        self.selected_dates
            .as_ref()
            .map(|dates| dates.iter().copied().map(format_date).collect())
            .unwrap_or_default()
    }

    fn includes(&self, date: NaiveDate) -> bool {
        self.selected_dates
            .as_ref()
            .is_none_or(|dates| dates.binary_search(&date).is_ok())
    }
}

#[derive(Debug, Clone)]
struct BucketPlan {
    envelope: ValidatedDateRange,
    dates: Vec<NaiveDate>,
    selection_mode: TrendSelectionMode,
}

impl BucketPlan {
    fn selected_date_strings(&self) -> Vec<String> {
        if self.selection_mode == TrendSelectionMode::SelectedDates {
            self.dates.iter().copied().map(format_date).collect()
        } else {
            Vec::new()
        }
    }
}

#[derive(Clone, Default)]
struct DailyAccumulator {
    monitored_seconds: i64,
    active_seconds: i64,
    learning_seconds: i64,
    idle_seconds: i64,
    classified_seconds: i64,
    pending_seconds: i64,
    low_confidence_seconds: i64,
    segment_count: usize,
    longest_focus_seconds: i64,
    completed_task_count: i64,
    linked_task_seconds: i64,
    has_task_evidence: bool,
    inactivity_reason_seconds: BTreeMap<InactivityReason, i64>,
}

#[derive(Clone)]
struct DailyFacts {
    date: NaiveDate,
    values: TrendMetricValues,
    classified_seconds: i64,
    pending_seconds: i64,
    low_confidence_seconds: i64,
    has_task_evidence: bool,
    inactivity_reason_seconds: BTreeMap<InactivityReason, i64>,
}

struct RangeAggregate {
    range: ValidatedDateRange,
    selection: ValidatedDateSelection,
    days: Vec<DailyFacts>,
    boundaries_ms: Vec<i64>,
    work_ledger_facts: WorkLedgerRangeFacts,
    activity_segments: Vec<ActivitySegmentRecord>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrendWorkbenchEvidenceHashInput<'a> {
    range: &'a TrendWorkbenchRange,
    #[serde(skip_serializing_if = "Option::is_none")]
    activity_scope: Option<ActivityScope>,
    granularity: TrendGranularity,
    metric: TrendMetric,
    metric_availability: &'a [TrendMetricAvailability],
    buckets: &'a [TrendBucket],
    summary: &'a TrendWorkbenchSummary,
    activity_composition: &'a ActivityCompositions,
    inactivity_reason_distribution: &'a [TrendInactivityReasonDistribution],
    baselines: &'a [TrendBaselineSeries],
    evidence: &'a [TrendEvidence],
}

pub fn get_trend_workbench(
    database: &Database,
    request: TrendWorkbenchRequest,
) -> TrendWorkbenchResult<TrendWorkbenchPayload> {
    get_trend_workbench_scoped(database, request, ActivityScope::All)
}

pub fn get_trend_workbench_scoped(
    database: &Database,
    request: TrendWorkbenchRequest,
    activity_scope: ActivityScope,
) -> TrendWorkbenchResult<TrendWorkbenchPayload> {
    let TrendWorkbenchRequest {
        start_date,
        end_date,
        selected_dates,
        timezone_offset_minutes,
        granularity,
        metric,
        custom_baseline,
    } = request;
    let metric_availability = trend_metric_availability();
    validate_timezone_offset(timezone_offset_minutes)?;
    let current_selection = validate_date_selection(
        &TrendDateRange {
            start_date,
            end_date,
        },
        selected_dates,
    )?;
    let granularity =
        granularity.unwrap_or_else(|| default_granularity(current_selection.envelope.day_count));
    let current = load_range(
        database,
        current_selection.clone(),
        timezone_offset_minutes,
        activity_scope,
    )?;
    let bucket_plans = build_bucket_plans(&current_selection, granularity)?;
    let (buckets, mut evidence) = build_buckets(&current, &bucket_plans);
    let (summary, summary_evidence) = build_summary(&current, &buckets);
    let activity_composition = build_range_activity_composition(&current);
    evidence.extend(summary_evidence);

    let current_value = summary.totals.get(metric);
    let mut baselines = Vec::with_capacity(if custom_baseline.is_some() { 4 } else { 3 });
    let current_is_valid = summary.recorded_day_count > 0;
    let (mut current_evidence_ids, current_quality_evidence) = baseline_quality_evidence(
        TrendBaselineKind::Current,
        summary.recorded_day_count,
        summary.missing_day_count,
    );
    if current_is_valid {
        current_evidence_ids.insert(0, summary_evidence_id(TrendBaselineKind::Current, metric));
    }
    evidence.extend(current_quality_evidence);
    baselines.push(TrendBaselineSeries {
        kind: TrendBaselineKind::Current,
        range: current_selection.envelope.dto(),
        is_valid: current_is_valid,
        value: current_is_valid.then_some(current_value),
        absolute_delta: None,
        percent_delta: None,
        recorded_day_count: summary.recorded_day_count,
        missing_day_count: summary.missing_day_count,
        selection_mode: current_selection.selection_mode(),
        selected_dates: current_selection.selected_date_strings(),
        selected_date_count: current_selection.selected_date_count(),
        envelope_day_count: current_selection.envelope.day_count,
        evidence_ids: current_evidence_ids,
    });

    let comparison_selections = [
        (
            TrendBaselineKind::PreviousEqualLength,
            previous_equal_selection(&current_selection)?,
        ),
        (
            TrendBaselineKind::PreviousMonthSamePeriod,
            previous_month_selection(&current_selection)?,
        ),
    ];
    for (kind, selection) in comparison_selections {
        let (series, items) = build_baseline(
            database,
            kind,
            selection,
            timezone_offset_minutes,
            metric,
            current_value,
            activity_scope,
        )?;
        baselines.push(series);
        evidence.extend(items);
    }
    if let Some(custom) = custom_baseline {
        let selection = ValidatedDateSelection::continuous(validate_date_range(&custom)?);
        let (series, items) = build_baseline(
            database,
            TrendBaselineKind::Custom,
            selection,
            timezone_offset_minutes,
            metric,
            current_value,
            activity_scope,
        )?;
        baselines.push(series);
        evidence.extend(items);
    }

    let boundaries = day_boundaries(&current_selection.envelope, timezone_offset_minutes)?;
    let range = TrendWorkbenchRange {
        start_ms: boundaries[0],
        end_ms: *boundaries
            .last()
            .expect("validated range has an end boundary"),
        start_date: format_date(current_selection.envelope.start),
        end_date: format_date(current_selection.envelope.end),
        day_count: current_selection.envelope.day_count,
        selection_mode: current_selection.selection_mode(),
        selected_dates: current_selection.selected_date_strings(),
        selected_date_count: current_selection.selected_date_count(),
        envelope_day_count: current_selection.envelope.day_count,
    };
    let inactivity_reason_distribution = current
        .days
        .iter()
        .flat_map(|day| day.inactivity_reason_seconds.iter())
        .fold(
            BTreeMap::<InactivityReason, i64>::new(),
            |mut totals, (reason, seconds)| {
                *totals.entry(*reason).or_default() += seconds;
                totals
            },
        )
        .into_iter()
        .map(|(reason, seconds)| TrendInactivityReasonDistribution { reason, seconds })
        .collect();
    let mut payload = TrendWorkbenchPayload {
        range,
        granularity,
        metric,
        metric_availability,
        buckets,
        summary,
        activity_composition,
        inactivity_reason_distribution,
        baselines,
        evidence,
        evidence_hash: String::new(),
    };
    let canonical = serde_json::to_vec(&TrendWorkbenchEvidenceHashInput {
        range: &payload.range,
        activity_scope: (activity_scope != ActivityScope::All).then_some(activity_scope),
        granularity: payload.granularity,
        metric: payload.metric,
        metric_availability: &payload.metric_availability,
        buckets: &payload.buckets,
        summary: &payload.summary,
        activity_composition: &payload.activity_composition,
        inactivity_reason_distribution: &payload.inactivity_reason_distribution,
        baselines: &payload.baselines,
        evidence: &payload.evidence,
    })
    .expect("trend workbench evidence always serializes");
    payload.evidence_hash = format!("{:x}", Sha256::digest(canonical));
    Ok(payload)
}

fn build_baseline(
    database: &Database,
    kind: TrendBaselineKind,
    selection: ValidatedDateSelection,
    timezone_offset_minutes: i32,
    metric: TrendMetric,
    current_value: f64,
    activity_scope: ActivityScope,
) -> Result<(TrendBaselineSeries, Vec<TrendEvidence>)> {
    let aggregate = load_range(
        database,
        selection.clone(),
        timezone_offset_minutes,
        activity_scope,
    )?;
    let value = total_values(&aggregate.days).get(metric);
    let evidence_id = summary_evidence_id(kind, metric);
    let recorded_day_count = recorded_day_count(&aggregate.days);
    let missing_day_count = selection.selected_date_count() - recorded_day_count;
    let is_valid = recorded_day_count > 0;
    let (mut evidence_ids, mut evidence) =
        baseline_quality_evidence(kind, recorded_day_count, missing_day_count);
    if is_valid {
        evidence_ids.insert(0, evidence_id.clone());
        evidence.push(TrendEvidence {
            id: evidence_id,
            scope: TrendEvidenceScope::Baseline,
            series_kind: kind,
            bucket_id: None,
            metric: Some(metric),
            value,
        });
    }
    Ok((
        TrendBaselineSeries {
            kind,
            range: selection.envelope.dto(),
            is_valid,
            value: is_valid.then_some(value),
            absolute_delta: is_valid.then_some(current_value - value),
            percent_delta: is_valid
                .then(|| percentage_delta(current_value, value))
                .flatten(),
            recorded_day_count,
            missing_day_count,
            selection_mode: selection.selection_mode(),
            selected_dates: selection.selected_date_strings(),
            selected_date_count: selection.selected_date_count(),
            envelope_day_count: selection.envelope.day_count,
            evidence_ids,
        },
        evidence,
    ))
}

fn baseline_quality_evidence(
    kind: TrendBaselineKind,
    recorded_day_count: usize,
    missing_day_count: usize,
) -> (Vec<String>, Vec<TrendEvidence>) {
    let values = [
        ("valid", (recorded_day_count > 0) as u8 as f64),
        ("recordedDayCount", recorded_day_count as f64),
        ("missingDayCount", missing_day_count as f64),
    ];
    let mut evidence_ids = Vec::with_capacity(values.len());
    let evidence = values
        .into_iter()
        .map(|(name, value)| {
            let id = format!("{}.quality.{name}", baseline_key(kind));
            evidence_ids.push(id.clone());
            TrendEvidence {
                id,
                scope: TrendEvidenceScope::Baseline,
                series_kind: kind,
                bucket_id: None,
                metric: None,
                value,
            }
        })
        .collect();
    (evidence_ids, evidence)
}

fn load_range(
    database: &Database,
    selection: ValidatedDateSelection,
    timezone_offset_minutes: i32,
    activity_scope: ActivityScope,
) -> Result<RangeAggregate> {
    let boundaries = day_boundaries(&selection.envelope, timezone_offset_minutes)?;
    let mut facts = database.load_trend_range_facts(
        boundaries[0],
        *boundaries
            .last()
            .expect("validated range has an end boundary"),
    )?;
    let mut work_ledger_facts = WorkLedgerRepository::new(database).range_facts(
        boundaries[0],
        *boundaries
            .last()
            .expect("validated range has an end boundary"),
    )?;
    if activity_scope == ActivityScope::Meaningful {
        let linked_activity_ids = work_ledger_facts
            .activities
            .iter()
            .map(|fact| fact.segment.id.as_str())
            .collect::<BTreeSet<_>>();
        facts.segments.retain(|segment| {
            activity_is_meaningful(
                segment.category,
                segment.video_purpose,
                linked_activity_ids.contains(segment.id.as_str()),
            )
        });
    } else if activity_scope == ActivityScope::Active {
        facts
            .segments
            .retain(|segment| segment.category != ActivityCategory::Idle);
    }
    facts.segments = canonicalize_activity_segments(
        &facts.segments,
        boundaries[0],
        *boundaries
            .last()
            .expect("validated range has an end boundary"),
    );
    align_linked_activity_facts(&mut work_ledger_facts, &facts.segments);
    aggregate_range(selection, facts, work_ledger_facts, boundaries)
}

fn align_linked_activity_facts(
    work_ledger_facts: &mut WorkLedgerRangeFacts,
    canonical_segments: &[ActivitySegmentRecord],
) {
    let mut segments_by_id = BTreeMap::<&str, Vec<&ActivitySegmentRecord>>::new();
    for segment in canonical_segments {
        segments_by_id
            .entry(segment.id.as_str())
            .or_default()
            .push(segment);
    }

    let mut aligned_facts = Vec::new();
    for fact in std::mem::take(&mut work_ledger_facts.activities) {
        if let Some(segments) = segments_by_id.get(fact.segment.id.as_str()) {
            for segment in segments {
                let mut aligned = fact.clone();
                aligned.segment = (*segment).clone();
                aligned_facts.push(aligned);
            }
        }
    }
    work_ledger_facts.activities = aligned_facts;
}

fn aggregate_range(
    selection: ValidatedDateSelection,
    facts: TrendRangeFacts,
    work_ledger_facts: WorkLedgerRangeFacts,
    boundaries_ms: Vec<i64>,
) -> Result<RangeAggregate> {
    let range = selection.envelope.clone();
    let mut accumulators = vec![DailyAccumulator::default(); range.day_count];
    let activity_segments = facts.segments;
    for segment in &activity_segments {
        apply_segment(&mut accumulators, segment, &boundaries_ms)?;
    }
    for completed_at_ms in facts.completed_task_timestamps {
        let day_index = boundaries_ms
            .partition_point(|boundary| *boundary <= completed_at_ms)
            .saturating_sub(1);
        if let Some(day) = accumulators.get_mut(day_index) {
            day.completed_task_count += 1;
            day.has_task_evidence = true;
        }
    }
    apply_linked_time(&mut accumulators, &work_ledger_facts, &boundaries_ms)?;
    let days = accumulators
        .into_iter()
        .enumerate()
        .map(|(index, day)| {
            let date = range.start + Duration::days(index as i64);
            DailyFacts {
                date,
                values: TrendMetricValues {
                    monitored_seconds: day.monitored_seconds,
                    active_seconds: day.active_seconds,
                    learning_seconds: day.learning_seconds,
                    idle_seconds: day.idle_seconds,
                    switch_count: day.segment_count.saturating_sub(1) as i64,
                    longest_focus_seconds: day.longest_focus_seconds,
                    classification_coverage: ratio(day.classified_seconds, day.monitored_seconds),
                    completed_task_count: day.completed_task_count,
                    linked_task_seconds: day.linked_task_seconds,
                },
                classified_seconds: day.classified_seconds,
                pending_seconds: day.pending_seconds,
                low_confidence_seconds: day.low_confidence_seconds,
                has_task_evidence: day.has_task_evidence,
                inactivity_reason_seconds: day.inactivity_reason_seconds,
            }
        })
        .filter(|day| selection.includes(day.date))
        .collect();
    Ok(RangeAggregate {
        range,
        selection,
        days,
        boundaries_ms,
        work_ledger_facts,
        activity_segments,
    })
}

fn apply_linked_time(
    accumulators: &mut [DailyAccumulator],
    facts: &WorkLedgerRangeFacts,
    boundaries_ms: &[i64],
) -> Result<()> {
    let mut intervals_by_day = vec![Vec::new(); accumulators.len()];
    for fact in &facts.activities {
        collect_linked_span_by_day(
            &mut intervals_by_day,
            fact.segment.started_at_ms,
            fact.segment.ended_at_ms,
            boundaries_ms,
        )?;
    }
    for fact in &facts.focus_sessions {
        collect_linked_span_by_day(
            &mut intervals_by_day,
            fact.started_at_ms,
            fact.ended_at_ms,
            boundaries_ms,
        )?;
    }
    for (day, intervals) in accumulators.iter_mut().zip(intervals_by_day) {
        if intervals.iter().any(|(start, end)| end > start) {
            day.has_task_evidence = true;
        }
        day.linked_task_seconds = merged_interval_milliseconds(intervals) / 1_000;
    }
    Ok(())
}

fn collect_linked_span_by_day(
    intervals_by_day: &mut [Vec<(i64, i64)>],
    started_at_ms: i64,
    ended_at_ms: i64,
    boundaries_ms: &[i64],
) -> Result<()> {
    let range_start = *boundaries_ms.first().ok_or_else(invalid_trend_request)?;
    let range_end = *boundaries_ms.last().ok_or_else(invalid_trend_request)?;
    let mut cursor_ms = started_at_ms.max(range_start);
    let clipped_end_ms = ended_at_ms.min(range_end);
    while cursor_ms < clipped_end_ms {
        let day_index = boundaries_ms
            .partition_point(|boundary| *boundary <= cursor_ms)
            .saturating_sub(1);
        let next_boundary = *boundaries_ms
            .get(day_index + 1)
            .ok_or_else(invalid_trend_request)?;
        let piece_end_ms = clipped_end_ms.min(next_boundary);
        if piece_end_ms > cursor_ms {
            intervals_by_day
                .get_mut(day_index)
                .ok_or_else(invalid_trend_request)?
                .push((cursor_ms, piece_end_ms));
        }
        cursor_ms = piece_end_ms;
    }
    Ok(())
}

fn merge_intervals(mut intervals: Vec<(i64, i64)>) -> Vec<(i64, i64)> {
    intervals.retain(|(start, end)| end > start);
    intervals
        .sort_unstable_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    let mut merged = Vec::<(i64, i64)>::with_capacity(intervals.len());
    for (start, end) in intervals {
        if let Some((_, current_end)) = merged.last_mut()
            && start <= *current_end
        {
            *current_end = (*current_end).max(end);
        } else {
            merged.push((start, end));
        }
    }
    merged
}

fn merged_interval_milliseconds(intervals: Vec<(i64, i64)>) -> i64 {
    merge_intervals(intervals)
        .into_iter()
        .fold(0_i64, |total, (start, end)| {
            total.saturating_add(end.saturating_sub(start))
        })
}

fn apply_segment(
    accumulators: &mut [DailyAccumulator],
    segment: &ActivitySegmentRecord,
    boundaries_ms: &[i64],
) -> Result<()> {
    let mut cursor_ms = segment.started_at_ms;
    while cursor_ms < segment.ended_at_ms {
        let boundary_index = boundaries_ms
            .partition_point(|boundary| *boundary <= cursor_ms)
            .saturating_sub(1);
        let Some(next_boundary_ms) = boundaries_ms.get(boundary_index + 1) else {
            return Err(invalid_trend_request());
        };
        let piece_end_ms = segment.ended_at_ms.min(*next_boundary_ms);
        if piece_end_ms <= cursor_ms {
            return Err(invalid_trend_request());
        }
        let duration_seconds = (piece_end_ms - cursor_ms) / 1_000;
        if duration_seconds > 0 {
            let day = accumulators
                .get_mut(boundary_index)
                .ok_or_else(invalid_trend_request)?;
            day.monitored_seconds += duration_seconds;
            day.segment_count += 1;
            if segment.category == ActivityCategory::Idle {
                day.idle_seconds += duration_seconds;
                let reason = segment
                    .inactivity_reason
                    .unwrap_or(InactivityReason::InputIdle);
                *day.inactivity_reason_seconds.entry(reason).or_default() += duration_seconds;
            } else {
                day.active_seconds += duration_seconds;
                day.longest_focus_seconds = day.longest_focus_seconds.max(duration_seconds);
            }
            if counts_as_learning(segment.category, segment.video_purpose) {
                day.learning_seconds += duration_seconds;
            }
            if segment.category == ActivityCategory::Pending
                || (segment.category == ActivityCategory::VideoInput
                    && segment.video_purpose == VideoPurpose::Unknown)
            {
                day.pending_seconds += duration_seconds;
            } else {
                day.classified_seconds += duration_seconds;
            }
            if segment.confidence < LOW_CONFIDENCE_THRESHOLD {
                day.low_confidence_seconds += duration_seconds;
            }
        }
        cursor_ms = piece_end_ms;
    }
    Ok(())
}

fn build_range_activity_composition(aggregate: &RangeAggregate) -> ActivityCompositions {
    let selected_day_indices = (0..aggregate.range.day_count)
        .filter(|index| {
            aggregate
                .selection
                .includes(aggregate.range.start + Duration::days(*index as i64))
        })
        .collect::<BTreeSet<_>>();
    build_activity_composition_for_day_indices(aggregate, &selected_day_indices)
}

fn build_activity_composition_for_day_indices(
    aggregate: &RangeAggregate,
    selected_day_indices: &BTreeSet<usize>,
) -> ActivityCompositions {
    let Some(start_index) = selected_day_indices.first().copied() else {
        return ActivityCompositions::default();
    };
    let end_index = selected_day_indices
        .last()
        .copied()
        .expect("a first selected day implies a last selected day")
        + 1;
    let linked_activity_ids = aggregate
        .work_ledger_facts
        .activities
        .iter()
        .map(|fact| fact.segment.id.as_str())
        .collect::<BTreeSet<_>>();
    let slices = aggregate.activity_segments.iter().flat_map(|segment| {
        let workflow_linked = linked_activity_ids.contains(segment.id.as_str());
        linked_pieces(
            segment.started_at_ms,
            segment.ended_at_ms,
            start_index,
            end_index,
            &aggregate.boundaries_ms,
            selected_day_indices,
        )
        .into_iter()
        .map(move |piece| ActivityCompositionSlice {
            scope_key: format!(
                "activity:{}:{}:{}:{}",
                segment.id, piece.day_index, piece.started_at_ms, piece.ended_at_ms
            ),
            category: segment.category,
            video_purpose: segment.video_purpose,
            seconds: piece.seconds,
            workflow_linked,
        })
    });
    build_activity_compositions(slices)
}

fn build_buckets(
    aggregate: &RangeAggregate,
    plans: &[BucketPlan],
) -> (Vec<TrendBucket>, Vec<TrendEvidence>) {
    let mut evidence = Vec::new();
    let buckets = plans
        .iter()
        .map(|plan| {
            let days = aggregate
                .days
                .iter()
                .filter(|day| plan.dates.binary_search(&day.date).is_ok())
                .cloned()
                .collect::<Vec<_>>();
            let values = total_values(&days);
            let recorded_day_count = recorded_day_count(&days);
            let id = format!(
                "{}_{}",
                format_date(plan.envelope.start),
                format_date(plan.envelope.end)
            );
            let selected_day_indices = plan
                .dates
                .iter()
                .map(|date| (*date - aggregate.range.start).num_days() as usize)
                .collect::<BTreeSet<_>>();
            let activity_composition =
                build_activity_composition_for_day_indices(aggregate, &selected_day_indices);
            let drilldown = build_bucket_drilldown(aggregate, plan, &id, &days);
            let evidence_ids = AVAILABLE_TREND_METRICS
                .iter()
                .map(|metric| {
                    let evidence_id = format!("current.bucket.{}.{}", id, metric_key(*metric));
                    evidence.push(TrendEvidence {
                        id: evidence_id.clone(),
                        scope: TrendEvidenceScope::Bucket,
                        series_kind: TrendBaselineKind::Current,
                        bucket_id: Some(id.clone()),
                        metric: Some(*metric),
                        value: values.get(*metric),
                    });
                    evidence_id
                })
                .collect();
            TrendBucket {
                id,
                start_date: format_date(plan.envelope.start),
                end_date: format_date(plan.envelope.end),
                values,
                recorded_day_count,
                missing_day_count: plan.dates.len() - recorded_day_count,
                selection_mode: plan.selection_mode,
                selected_dates: plan.selected_date_strings(),
                selected_date_count: plan.dates.len(),
                envelope_day_count: plan.envelope.day_count,
                evidence_ids,
                activity_composition,
                drilldown,
            }
        })
        .collect();
    (buckets, evidence)
}

#[derive(Default)]
struct MutableLinkedRollup {
    linked_intervals_by_day: BTreeMap<usize, Vec<(i64, i64)>>,
    activity_intervals_by_day: BTreeMap<usize, Vec<(i64, i64)>>,
    focus_intervals_by_day: BTreeMap<usize, Vec<(i64, i64)>>,
    evidence_ids: BTreeSet<String>,
    shared_evidence_ids: BTreeSet<String>,
}

fn build_bucket_drilldown(
    aggregate: &RangeAggregate,
    plan: &BucketPlan,
    bucket_id: &str,
    days: &[DailyFacts],
) -> TrendBucketDrilldown {
    let selected_day_indices = plan
        .dates
        .iter()
        .map(|date| (*date - aggregate.range.start).num_days() as usize)
        .collect::<BTreeSet<_>>();
    let start_index = *selected_day_indices
        .first()
        .expect("bucket plans always contain a date");
    let end_index = selected_day_indices
        .last()
        .expect("bucket plans always contain a date")
        + 1;
    let bucket_start_ms = aggregate.boundaries_ms[start_index];
    let bucket_end_ms = aggregate.boundaries_ms[end_index];

    let shared_activity_ids = aggregate
        .work_ledger_facts
        .activities
        .iter()
        .fold(
            BTreeMap::<String, BTreeSet<String>>::new(),
            |mut groups, fact| {
                groups
                    .entry(fact.segment.id.clone())
                    .or_default()
                    .insert(fact.task_id.clone());
                groups
            },
        )
        .into_iter()
        .filter_map(|(id, task_ids)| (task_ids.len() > 1).then_some(id))
        .collect::<BTreeSet<_>>();

    let mut raw_rows = Vec::new();
    let mut workflow_ownership = Vec::new();
    let mut task_rollups = BTreeMap::<String, (String, String, String, MutableLinkedRollup)>::new();
    let mut project_rollups = BTreeMap::<String, (String, MutableLinkedRollup)>::new();
    let mut project_evidence = BTreeSet::<(String, String)>::new();
    let mut app_seconds = BTreeMap::<String, i64>::new();
    let mut category_seconds = BTreeMap::<String, i64>::new();
    let mut distribution_scopes = BTreeSet::<String>::new();

    for fact in &aggregate.work_ledger_facts.activities {
        let shared = shared_activity_ids.contains(&fact.segment.id);
        for (piece_index, piece) in linked_pieces(
            fact.segment.started_at_ms,
            fact.segment.ended_at_ms,
            start_index,
            end_index,
            &aggregate.boundaries_ms,
            &selected_day_indices,
        )
        .into_iter()
        .enumerate()
        {
            add_activity_drilldown_piece(
                fact,
                shared,
                piece_index,
                piece,
                bucket_id,
                &aggregate.range,
                &mut raw_rows,
                &mut workflow_ownership,
                &mut task_rollups,
                &mut project_rollups,
                &mut project_evidence,
                &mut app_seconds,
                &mut category_seconds,
                &mut distribution_scopes,
            );
        }
    }
    for fact in &aggregate.work_ledger_facts.focus_sessions {
        for (piece_index, piece) in linked_pieces(
            fact.started_at_ms,
            fact.ended_at_ms,
            start_index,
            end_index,
            &aggregate.boundaries_ms,
            &selected_day_indices,
        )
        .into_iter()
        .enumerate()
        {
            add_focus_drilldown_piece(
                fact,
                piece_index,
                piece,
                bucket_id,
                &aggregate.range,
                &mut raw_rows,
                &mut workflow_ownership,
                &mut task_rollups,
                &mut project_rollups,
                &mut project_evidence,
                &mut app_seconds,
                &mut category_seconds,
                &mut distribution_scopes,
            );
        }
    }

    let linked_activity_ids = aggregate
        .work_ledger_facts
        .activities
        .iter()
        .map(|fact| fact.segment.id.as_str())
        .collect::<BTreeSet<_>>();
    for segment in &aggregate.activity_segments {
        if linked_activity_ids.contains(segment.id.as_str()) {
            continue;
        }
        for (piece_index, piece) in linked_pieces(
            segment.started_at_ms,
            segment.ended_at_ms,
            start_index,
            end_index,
            &aggregate.boundaries_ms,
            &selected_day_indices,
        )
        .into_iter()
        .enumerate()
        {
            add_unlinked_activity_row(
                segment,
                piece_index,
                piece,
                bucket_id,
                &aggregate.range,
                &mut raw_rows,
            );
        }
    }

    app_seconds.clear();
    category_seconds.clear();
    distribution_scopes.clear();
    for segment in &aggregate.activity_segments {
        for piece in linked_pieces(
            segment.started_at_ms,
            segment.ended_at_ms,
            start_index,
            end_index,
            &aggregate.boundaries_ms,
            &selected_day_indices,
        ) {
            let scope = format!(
                "activity:{}:{}:{}:{}",
                segment.id, piece.day_index, piece.started_at_ms, piece.ended_at_ms
            );
            if distribution_scopes.insert(scope) {
                *app_seconds.entry(segment.app.clone()).or_default() += piece.seconds;
                *category_seconds
                    .entry(activity_category_key(segment.category).into())
                    .or_default() += piece.seconds;
            }
        }
    }

    raw_rows.sort_by(|left, right| left.row_id.cmp(&right.row_id));
    workflow_ownership.sort_by(|left, right| left.ownership_id.cmp(&right.ownership_id));
    let linked_task_rollups = task_rollups
        .into_iter()
        .map(
            |(task_id, (task_title, project_id, project_name, rollup))| TrendLinkedTaskRollup {
                task_id,
                task_title,
                project_id,
                project_name,
                linked_seconds: rollup_interval_seconds(&rollup.linked_intervals_by_day),
                activity_seconds: rollup_interval_seconds(&rollup.activity_intervals_by_day),
                focus_seconds: rollup_interval_seconds(&rollup.focus_intervals_by_day),
                evidence_count: rollup.evidence_ids.len(),
                shared_evidence_count: rollup.shared_evidence_ids.len(),
            },
        )
        .collect();
    let linked_project_rollups = project_rollups
        .into_iter()
        .map(
            |(project_id, (project_name, rollup))| TrendLinkedProjectRollup {
                project_id,
                project_name,
                linked_seconds: rollup_interval_seconds(&rollup.linked_intervals_by_day),
                activity_seconds: rollup_interval_seconds(&rollup.activity_intervals_by_day),
                focus_seconds: rollup_interval_seconds(&rollup.focus_intervals_by_day),
                evidence_count: rollup.evidence_ids.len(),
                shared_evidence_count: rollup.shared_evidence_ids.len(),
            },
        )
        .collect();
    let completed_tasks = aggregate
        .work_ledger_facts
        .completed_tasks
        .iter()
        .filter(|task| {
            if task.completed_at_ms < bucket_start_ms || task.completed_at_ms >= bucket_end_ms {
                return false;
            }
            let day_index = aggregate
                .boundaries_ms
                .partition_point(|boundary| *boundary <= task.completed_at_ms)
                .saturating_sub(1);
            selected_day_indices.contains(&day_index)
        })
        .map(|task| TrendCompletedTaskItem {
            task_id: task.task_id.clone(),
            task_title: task.task_title.clone(),
            project_id: task.project_id.clone(),
            project_name: task.project_name.clone(),
            completed_at_ms: task.completed_at_ms,
        })
        .collect();
    let recorded_day_count = recorded_day_count(days);
    let classified_seconds = days.iter().map(|day| day.classified_seconds).sum();
    let monitored_seconds = days.iter().map(|day| day.values.monitored_seconds).sum();

    TrendBucketDrilldown {
        bucket_id: bucket_id.into(),
        raw_rows,
        application_distribution: distribution_items(app_seconds),
        category_distribution: distribution_items(category_seconds),
        completed_tasks,
        linked_task_rollups,
        linked_project_rollups,
        workflow_ownership,
        data_quality: TrendDrilldownQuality {
            recorded_day_count,
            missing_day_count: plan.dates.len() - recorded_day_count,
            classified_seconds,
            classification_coverage: ratio(classified_seconds, monitored_seconds),
            low_confidence_seconds: days.iter().map(|day| day.low_confidence_seconds).sum(),
            pending_seconds: days.iter().map(|day| day.pending_seconds).sum(),
        },
    }
}

#[derive(Clone, Copy)]
struct LinkedPiece {
    day_index: usize,
    day_start_ms: i64,
    started_at_ms: i64,
    ended_at_ms: i64,
    seconds: i64,
}

fn linked_pieces(
    started_at_ms: i64,
    ended_at_ms: i64,
    start_index: usize,
    end_index: usize,
    boundaries_ms: &[i64],
    selected_day_indices: &BTreeSet<usize>,
) -> Vec<LinkedPiece> {
    let mut pieces = Vec::new();
    let mut cursor = started_at_ms.max(boundaries_ms[start_index]);
    let clipped_end = ended_at_ms.min(boundaries_ms[end_index]);
    while cursor < clipped_end {
        let day_index = boundaries_ms
            .partition_point(|boundary| *boundary <= cursor)
            .saturating_sub(1);
        let piece_end = clipped_end.min(boundaries_ms[day_index + 1]);
        let seconds = piece_end.saturating_sub(cursor) / 1_000;
        if seconds > 0 && selected_day_indices.contains(&day_index) {
            pieces.push(LinkedPiece {
                day_index,
                day_start_ms: boundaries_ms[day_index],
                started_at_ms: cursor,
                ended_at_ms: piece_end,
                seconds,
            });
        }
        cursor = piece_end;
    }
    pieces
}

fn add_unlinked_activity_row(
    segment: &ActivitySegmentRecord,
    piece_index: usize,
    piece: LinkedPiece,
    bucket_id: &str,
    aggregate_range: &ValidatedDateRange,
    raw_rows: &mut Vec<TrendRawRow>,
) {
    let category = activity_category_key(segment.category);
    raw_rows.push(TrendRawRow {
        row_id: format!(
            "{bucket_id}.activity.{}.unlinked.{}.{}.{}",
            segment.id, piece.started_at_ms, piece.ended_at_ms, piece_index
        ),
        bucket_id: bucket_id.into(),
        evidence_kind: TrendRawEvidenceKind::Activity,
        evidence_id: segment.id.clone(),
        date: format_date(aggregate_range.start + Duration::days(piece.day_index as i64)),
        start_time: format_piece_time(piece.started_at_ms, piece.day_start_ms),
        end_time: format_piece_time(piece.ended_at_ms, piece.day_start_ms),
        app: segment.app.clone(),
        title_summary: format!("{} / {category}", segment.app),
        category: category.into(),
        video_purpose: (segment.category == ActivityCategory::VideoInput)
            .then_some(segment.video_purpose),
        meaningful: activity_is_meaningful(segment.category, segment.video_purpose, false),
        meaningful_reason: effective_meaningful_reason(
            segment.category,
            segment.video_purpose,
            false,
        ),
        task_id: None,
        task_title: None,
        project_id: None,
        project_name: None,
        clipped_duration_seconds: piece.seconds,
        confidence: Some(segment.confidence.into()),
        review_state: if segment.needs_review {
            TrendReviewState::Pending
        } else {
            TrendReviewState::Confirmed
        },
        shared: false,
    });
}

#[allow(clippy::too_many_arguments)]
fn add_activity_drilldown_piece(
    fact: &WorkLedgerActivityRangeFact,
    shared: bool,
    piece_index: usize,
    piece: LinkedPiece,
    bucket_id: &str,
    aggregate_range: &ValidatedDateRange,
    raw_rows: &mut Vec<TrendRawRow>,
    ownership: &mut Vec<TrendWorkflowOwnership>,
    task_rollups: &mut BTreeMap<String, (String, String, String, MutableLinkedRollup)>,
    project_rollups: &mut BTreeMap<String, (String, MutableLinkedRollup)>,
    project_evidence: &mut BTreeSet<(String, String)>,
    app_seconds: &mut BTreeMap<String, i64>,
    category_seconds: &mut BTreeMap<String, i64>,
    distribution_scopes: &mut BTreeSet<String>,
) {
    let category = activity_category_key(fact.segment.category);
    let evidence_key = format!("activity:{}", fact.segment.id);
    let piece_key = format!("{evidence_key}:{}", piece.day_index);
    let ownership_id = format!(
        "{bucket_id}.ownership.activity.{}.{}",
        fact.segment.id, fact.task_id
    );
    if !ownership
        .iter()
        .any(|item| item.ownership_id == ownership_id)
    {
        ownership.push(TrendWorkflowOwnership {
            ownership_id,
            evidence_kind: TrendRawEvidenceKind::Activity,
            evidence_id: fact.segment.id.clone(),
            task_id: fact.task_id.clone(),
            task_title: fact.task_title.clone(),
            project_id: fact.project_id.clone(),
            project_name: fact.project_name.clone(),
            shared,
        });
    }
    raw_rows.push(TrendRawRow {
        row_id: format!(
            "{bucket_id}.activity.{}.{}.{}.{}.{}",
            fact.segment.id, fact.task_id, piece.started_at_ms, piece.ended_at_ms, piece_index
        ),
        bucket_id: bucket_id.into(),
        evidence_kind: TrendRawEvidenceKind::Activity,
        evidence_id: fact.segment.id.clone(),
        date: format_date(aggregate_range.start + Duration::days(piece.day_index as i64)),
        start_time: format_piece_time(piece.started_at_ms, piece.day_start_ms),
        end_time: format_piece_time(piece.ended_at_ms, piece.day_start_ms),
        app: fact.segment.app.clone(),
        title_summary: format!("{} / {category}", fact.segment.app),
        category: category.into(),
        video_purpose: (fact.segment.category == ActivityCategory::VideoInput)
            .then_some(fact.segment.video_purpose),
        meaningful: activity_is_meaningful(fact.segment.category, fact.segment.video_purpose, true),
        meaningful_reason: effective_meaningful_reason(
            fact.segment.category,
            fact.segment.video_purpose,
            true,
        ),
        task_id: Some(fact.task_id.clone()),
        task_title: Some(fact.task_title.clone()),
        project_id: Some(fact.project_id.clone()),
        project_name: Some(fact.project_name.clone()),
        clipped_duration_seconds: piece.seconds,
        confidence: Some(fact.segment.confidence.into()),
        review_state: if fact.segment.needs_review {
            TrendReviewState::Pending
        } else {
            TrendReviewState::Confirmed
        },
        shared,
    });
    add_task_rollup(
        task_rollups,
        fact.task_id.clone(),
        fact.task_title.clone(),
        fact.project_id.clone(),
        fact.project_name.clone(),
        &evidence_key,
        piece,
        true,
        shared,
    );
    add_project_rollup(
        project_rollups,
        project_evidence,
        fact.project_id.clone(),
        fact.project_name.clone(),
        &evidence_key,
        &piece_key,
        piece,
        true,
        shared,
    );
    if distribution_scopes.insert(piece_key) {
        *app_seconds.entry(fact.segment.app.clone()).or_default() += piece.seconds;
        *category_seconds.entry(category.into()).or_default() += piece.seconds;
    }
}

#[allow(clippy::too_many_arguments)]
fn add_focus_drilldown_piece(
    fact: &WorkLedgerFocusRangeFact,
    piece_index: usize,
    piece: LinkedPiece,
    bucket_id: &str,
    aggregate_range: &ValidatedDateRange,
    raw_rows: &mut Vec<TrendRawRow>,
    ownership: &mut Vec<TrendWorkflowOwnership>,
    task_rollups: &mut BTreeMap<String, (String, String, String, MutableLinkedRollup)>,
    project_rollups: &mut BTreeMap<String, (String, MutableLinkedRollup)>,
    project_evidence: &mut BTreeSet<(String, String)>,
    app_seconds: &mut BTreeMap<String, i64>,
    category_seconds: &mut BTreeMap<String, i64>,
    distribution_scopes: &mut BTreeSet<String>,
) {
    let evidence_key = format!("focus:{}", fact.session_id);
    let piece_key = format!("{evidence_key}:{}", piece.day_index);
    let ownership_id = format!(
        "{bucket_id}.ownership.focus.{}.{}",
        fact.session_id, fact.task_id
    );
    if !ownership
        .iter()
        .any(|item| item.ownership_id == ownership_id)
    {
        ownership.push(TrendWorkflowOwnership {
            ownership_id,
            evidence_kind: TrendRawEvidenceKind::Focus,
            evidence_id: fact.session_id.clone(),
            task_id: fact.task_id.clone(),
            task_title: fact.task_title.clone(),
            project_id: fact.project_id.clone(),
            project_name: fact.project_name.clone(),
            shared: false,
        });
    }
    raw_rows.push(TrendRawRow {
        row_id: format!(
            "{bucket_id}.focus.{}.{}.{}",
            fact.session_id, fact.task_id, piece_index
        ),
        bucket_id: bucket_id.into(),
        evidence_kind: TrendRawEvidenceKind::Focus,
        evidence_id: fact.session_id.clone(),
        date: format_date(aggregate_range.start + Duration::days(piece.day_index as i64)),
        start_time: format_piece_time(piece.started_at_ms, piece.day_start_ms),
        end_time: format_piece_time(piece.ended_at_ms, piece.day_start_ms),
        app: "Focus".into(),
        title_summary: "Focus session".into(),
        category: "focus".into(),
        video_purpose: None,
        meaningful: false,
        meaningful_reason: MeaningfulReason::Excluded,
        task_id: Some(fact.task_id.clone()),
        task_title: Some(fact.task_title.clone()),
        project_id: Some(fact.project_id.clone()),
        project_name: Some(fact.project_name.clone()),
        clipped_duration_seconds: piece.seconds,
        confidence: None,
        review_state: TrendReviewState::Confirmed,
        shared: false,
    });
    add_task_rollup(
        task_rollups,
        fact.task_id.clone(),
        fact.task_title.clone(),
        fact.project_id.clone(),
        fact.project_name.clone(),
        &evidence_key,
        piece,
        false,
        false,
    );
    add_project_rollup(
        project_rollups,
        project_evidence,
        fact.project_id.clone(),
        fact.project_name.clone(),
        &evidence_key,
        &piece_key,
        piece,
        false,
        false,
    );
    if distribution_scopes.insert(piece_key) {
        *app_seconds.entry("Focus".into()).or_default() += piece.seconds;
        *category_seconds.entry("focus".into()).or_default() += piece.seconds;
    }
}

#[allow(clippy::too_many_arguments)]
fn add_task_rollup(
    rollups: &mut BTreeMap<String, (String, String, String, MutableLinkedRollup)>,
    task_id: String,
    task_title: String,
    project_id: String,
    project_name: String,
    evidence_key: &str,
    piece: LinkedPiece,
    activity: bool,
    shared: bool,
) {
    let rollup = &mut rollups
        .entry(task_id)
        .or_insert_with(|| {
            (
                task_title,
                project_id,
                project_name,
                MutableLinkedRollup::default(),
            )
        })
        .3;
    rollup
        .linked_intervals_by_day
        .entry(piece.day_index)
        .or_default()
        .push((piece.started_at_ms, piece.ended_at_ms));
    if activity {
        rollup
            .activity_intervals_by_day
            .entry(piece.day_index)
            .or_default()
            .push((piece.started_at_ms, piece.ended_at_ms));
    } else {
        rollup
            .focus_intervals_by_day
            .entry(piece.day_index)
            .or_default()
            .push((piece.started_at_ms, piece.ended_at_ms));
    }
    rollup.evidence_ids.insert(evidence_key.into());
    if shared {
        rollup.shared_evidence_ids.insert(evidence_key.into());
    }
}

#[allow(clippy::too_many_arguments)]
fn add_project_rollup(
    rollups: &mut BTreeMap<String, (String, MutableLinkedRollup)>,
    project_evidence: &mut BTreeSet<(String, String)>,
    project_id: String,
    project_name: String,
    evidence_key: &str,
    piece_key: &str,
    piece: LinkedPiece,
    activity: bool,
    shared: bool,
) {
    if !project_evidence.insert((project_id.clone(), piece_key.into())) {
        return;
    }
    let rollup = &mut rollups
        .entry(project_id)
        .or_insert_with(|| (project_name, MutableLinkedRollup::default()))
        .1;
    rollup
        .linked_intervals_by_day
        .entry(piece.day_index)
        .or_default()
        .push((piece.started_at_ms, piece.ended_at_ms));
    if activity {
        rollup
            .activity_intervals_by_day
            .entry(piece.day_index)
            .or_default()
            .push((piece.started_at_ms, piece.ended_at_ms));
    } else {
        rollup
            .focus_intervals_by_day
            .entry(piece.day_index)
            .or_default()
            .push((piece.started_at_ms, piece.ended_at_ms));
    }
    rollup.evidence_ids.insert(evidence_key.into());
    if shared {
        rollup.shared_evidence_ids.insert(evidence_key.into());
    }
}

fn rollup_interval_seconds(intervals_by_day: &BTreeMap<usize, Vec<(i64, i64)>>) -> i64 {
    intervals_by_day
        .values()
        .map(|intervals| merged_interval_milliseconds(intervals.clone()) / 1_000)
        .sum()
}

fn distribution_items(values: BTreeMap<String, i64>) -> Vec<TrendDistributionItem> {
    values
        .into_iter()
        .map(|(key, seconds)| TrendDistributionItem {
            label: key.clone(),
            key,
            seconds,
        })
        .collect()
}

fn format_piece_time(timestamp_ms: i64, day_start_ms: i64) -> String {
    let seconds = timestamp_ms.saturating_sub(day_start_ms) / 1_000;
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3_600,
        (seconds % 3_600) / 60,
        seconds % 60
    )
}

fn activity_category_key(category: ActivityCategory) -> &'static str {
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

fn build_summary(
    aggregate: &RangeAggregate,
    buckets: &[TrendBucket],
) -> (TrendWorkbenchSummary, Vec<TrendEvidence>) {
    let totals = total_values(&aggregate.days);
    let (daily_average, average_sample_day_count) = daily_average(&aggregate.days);
    let switches_per_active_hour = (totals.active_seconds > 0)
        .then(|| totals.switch_count as f64 * 3_600.0 / totals.active_seconds as f64);
    let bucket_values = buckets
        .iter()
        .map(|bucket| bucket.values.clone())
        .collect::<Vec<_>>();
    let day_values = aggregate
        .days
        .iter()
        .map(|day| day.values.clone())
        .collect::<Vec<_>>();
    let mean_per_bucket = mean_values(&bucket_values);
    let daily_median = median_values(&day_values);
    let daily_max = max_values(&day_values);
    let daily_sample_stddev = sample_stddev_values(&day_values);
    let daily_coefficient_of_variation =
        coefficient_of_variation_values(&day_values, &daily_sample_stddev);
    let recorded_day_count = recorded_day_count(&aggregate.days);
    let effective_activity_day_count = effective_activity_day_count(&aggregate.days);
    let classified_seconds = aggregate
        .days
        .iter()
        .map(|day| day.classified_seconds)
        .sum();
    let pending_seconds = aggregate.days.iter().map(|day| day.pending_seconds).sum();
    let low_confidence_seconds = aggregate
        .days
        .iter()
        .map(|day| day.low_confidence_seconds)
        .sum();

    let mut evidence = Vec::new();
    let mut evidence_ids = Vec::new();
    for metric in AVAILABLE_TREND_METRICS {
        let id = format!("current.summary.{}", metric_key(metric));
        evidence_ids.push(id.clone());
        evidence.push(TrendEvidence {
            id,
            scope: TrendEvidenceScope::Summary,
            series_kind: TrendBaselineKind::Current,
            bucket_id: None,
            metric: Some(metric),
            value: totals.get(metric),
        });
    }
    for (prefix, values) in [
        ("meanPerBucket.", &mean_per_bucket),
        ("dailyMedian.", &daily_median),
        ("dailyMax.", &daily_max),
        ("dailySampleStddev.", &daily_sample_stddev),
        (
            "dailyCoefficientOfVariation.",
            &daily_coefficient_of_variation,
        ),
    ] {
        for metric in AVAILABLE_TREND_METRICS {
            let id = format!("current.summary.{prefix}{}", metric_key(metric));
            evidence_ids.push(id.clone());
            evidence.push(TrendEvidence {
                id,
                scope: TrendEvidenceScope::Summary,
                series_kind: TrendBaselineKind::Current,
                bucket_id: None,
                metric: Some(metric),
                value: values.get(metric),
            });
        }
    }
    for (suffix, value, metric) in [
        (
            "monitoredSeconds",
            daily_average.monitored_seconds,
            TrendMetric::MonitoredSeconds,
        ),
        (
            "activeSeconds",
            daily_average.active_seconds,
            TrendMetric::ActiveSeconds,
        ),
        (
            "idleSeconds",
            daily_average.idle_seconds,
            TrendMetric::IdleSeconds,
        ),
        (
            "learningSeconds",
            daily_average.learning_seconds,
            TrendMetric::LearningSeconds,
        ),
    ] {
        if let Some(value) = value {
            let id = format!("current.summary.dailyAverage.{suffix}");
            evidence_ids.push(id.clone());
            evidence.push(TrendEvidence {
                id,
                scope: TrendEvidenceScope::Summary,
                series_kind: TrendBaselineKind::Current,
                bucket_id: None,
                metric: Some(metric),
                value,
            });
        }
    }
    if let Some(value) = switches_per_active_hour {
        let id = "current.summary.switchesPerActiveHour".to_string();
        evidence_ids.push(id.clone());
        evidence.push(TrendEvidence {
            id,
            scope: TrendEvidenceScope::Rate,
            series_kind: TrendBaselineKind::Current,
            bucket_id: None,
            metric: None,
            value,
        });
    }
    for (name, value) in [
        ("dataQuality.recordedDayCount", recorded_day_count as f64),
        (
            "dataQuality.averageSampleDayCount",
            average_sample_day_count as f64,
        ),
        (
            "dataQuality.effectiveActivityDayCount",
            effective_activity_day_count as f64,
        ),
        (
            "dataQuality.missingDayCount",
            (aggregate.selection.selected_date_count() - recorded_day_count) as f64,
        ),
        ("dataQuality.classifiedSeconds", classified_seconds as f64),
        (
            "dataQuality.classificationCoverage",
            totals.classification_coverage,
        ),
        (
            "dataQuality.lowConfidenceSeconds",
            low_confidence_seconds as f64,
        ),
        ("dataQuality.pendingSeconds", pending_seconds as f64),
    ] {
        let id = format!("current.summary.{name}");
        evidence_ids.push(id.clone());
        evidence.push(TrendEvidence {
            id,
            scope: TrendEvidenceScope::Summary,
            series_kind: TrendBaselineKind::Current,
            bucket_id: None,
            metric: None,
            value,
        });
    }
    (
        TrendWorkbenchSummary {
            totals,
            daily_average,
            average_sample_day_count,
            switches_per_active_hour,
            mean_per_bucket,
            daily_median,
            daily_max,
            daily_sample_stddev,
            daily_coefficient_of_variation,
            recorded_day_count,
            effective_activity_day_count,
            missing_day_count: aggregate.selection.selected_date_count() - recorded_day_count,
            classified_seconds,
            classification_coverage: ratio(
                classified_seconds,
                aggregate
                    .days
                    .iter()
                    .map(|day| day.values.monitored_seconds)
                    .sum(),
            ),
            low_confidence_seconds,
            pending_seconds,
            evidence_ids,
        },
        evidence,
    )
}

fn daily_average(days: &[DailyFacts]) -> (TrendDailyAverage, usize) {
    // `days` already contains exactly the normalized selected dates. Zero-data
    // dates must remain in the denominator so range and sparse selections use
    // the same calendar-day average.
    let sample_count = days.len();
    let average = |value: fn(&TrendMetricValues) -> i64| {
        (sample_count > 0).then(|| {
            days.iter().map(|day| value(&day.values)).sum::<i64>() as f64 / sample_count as f64
        })
    };
    (
        TrendDailyAverage {
            monitored_seconds: average(|values| values.monitored_seconds),
            active_seconds: average(|values| values.active_seconds),
            idle_seconds: average(|values| values.idle_seconds),
            learning_seconds: average(|values| values.learning_seconds),
        },
        sample_count,
    )
}

fn total_values(days: &[DailyFacts]) -> TrendMetricValues {
    let monitored_seconds = days
        .iter()
        .map(|day| day.values.monitored_seconds)
        .sum::<i64>();
    let classified_seconds = days.iter().map(|day| day.classified_seconds).sum::<i64>();
    TrendMetricValues {
        monitored_seconds,
        active_seconds: days.iter().map(|day| day.values.active_seconds).sum(),
        learning_seconds: days.iter().map(|day| day.values.learning_seconds).sum(),
        idle_seconds: days.iter().map(|day| day.values.idle_seconds).sum(),
        switch_count: days.iter().map(|day| day.values.switch_count).sum(),
        longest_focus_seconds: days
            .iter()
            .map(|day| day.values.longest_focus_seconds)
            .max()
            .unwrap_or_default(),
        classification_coverage: ratio(classified_seconds, monitored_seconds),
        completed_task_count: days.iter().map(|day| day.values.completed_task_count).sum(),
        linked_task_seconds: days.iter().map(|day| day.values.linked_task_seconds).sum(),
    }
}

fn mean_values(values: &[TrendMetricValues]) -> TrendStatisticalMetricValues {
    metric_statistics(values, |items| {
        if items.is_empty() {
            0.0
        } else {
            items.iter().sum::<f64>() / items.len() as f64
        }
    })
}

fn median_values(values: &[TrendMetricValues]) -> TrendStatisticalMetricValues {
    metric_statistics(values, |items| {
        if items.is_empty() {
            return 0.0;
        }
        let mut sorted = items.to_vec();
        sorted.sort_by(f64::total_cmp);
        let middle = sorted.len() / 2;
        if sorted.len() % 2 == 0 {
            (sorted[middle - 1] + sorted[middle]) / 2.0
        } else {
            sorted[middle]
        }
    })
}

fn max_values(values: &[TrendMetricValues]) -> TrendStatisticalMetricValues {
    metric_statistics(values, |items| items.iter().copied().fold(0.0, f64::max))
}

fn sample_stddev_values(values: &[TrendMetricValues]) -> TrendStatisticalMetricValues {
    metric_statistics(values, |items| {
        if items.len() < 2 {
            return 0.0;
        }
        let mean = items.iter().sum::<f64>() / items.len() as f64;
        let variance = items
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / (items.len() - 1) as f64;
        variance.sqrt()
    })
}

fn coefficient_of_variation_values(
    values: &[TrendMetricValues],
    sample_stddev: &TrendStatisticalMetricValues,
) -> TrendStatisticalMetricValues {
    let means = mean_values(values);
    let mut result = TrendStatisticalMetricValues::default();
    for metric in AVAILABLE_TREND_METRICS {
        let mean = means.get(metric);
        result.set(
            metric,
            if mean == 0.0 {
                0.0
            } else {
                sample_stddev.get(metric) / mean.abs()
            },
        );
    }
    result
}

fn metric_statistics(
    values: &[TrendMetricValues],
    statistic: impl Fn(&[f64]) -> f64,
) -> TrendStatisticalMetricValues {
    let mut result = TrendStatisticalMetricValues::default();
    for metric in AVAILABLE_TREND_METRICS {
        let items = values
            .iter()
            .map(|values| values.get(metric))
            .collect::<Vec<_>>();
        result.set(metric, statistic(&items));
    }
    result
}

fn build_bucket_plans(
    selection: &ValidatedDateSelection,
    granularity: TrendGranularity,
) -> Result<Vec<BucketPlan>> {
    if let Some(selected_dates) = &selection.selected_dates {
        let mut groups = BTreeMap::<NaiveDate, Vec<NaiveDate>>::new();
        for date in selected_dates {
            let group_start = match granularity {
                TrendGranularity::Day => *date,
                TrendGranularity::Week => {
                    *date - Duration::days(date.weekday().num_days_from_monday() as i64)
                }
                TrendGranularity::Month => date.with_day(1).ok_or_else(invalid_trend_request)?,
            };
            groups.entry(group_start).or_default().push(*date);
        }
        return groups
            .into_values()
            .map(|dates| {
                let start = *dates.first().ok_or_else(invalid_trend_request)?;
                let end = *dates.last().ok_or_else(invalid_trend_request)?;
                Ok(BucketPlan {
                    envelope: ValidatedDateRange {
                        start,
                        end,
                        day_count: (end - start).num_days() as usize + 1,
                    },
                    dates,
                    selection_mode: TrendSelectionMode::SelectedDates,
                })
            })
            .collect();
    }

    let range = &selection.envelope;
    let mut buckets = Vec::new();
    let mut cursor = range.start;
    while cursor <= range.end {
        let natural_end = match granularity {
            TrendGranularity::Day => cursor,
            TrendGranularity::Week => {
                cursor + Duration::days(6 - cursor.weekday().num_days_from_monday() as i64)
            }
            TrendGranularity::Month => month_end(cursor)?,
        };
        let end = natural_end.min(range.end);
        let day_count = (end - cursor).num_days() as usize + 1;
        buckets.push(BucketPlan {
            envelope: ValidatedDateRange {
                start: cursor,
                end,
                day_count,
            },
            dates: (0..day_count)
                .map(|offset| cursor + Duration::days(offset as i64))
                .collect(),
            selection_mode: TrendSelectionMode::Continuous,
        });
        cursor = end.succ_opt().ok_or_else(invalid_trend_request)?;
    }
    Ok(buckets)
}

fn previous_equal_selection(selection: &ValidatedDateSelection) -> Result<ValidatedDateSelection> {
    if let Some(selected_dates) = &selection.selected_dates {
        let offset = Duration::days(selection.envelope.day_count as i64);
        let mapped = selected_dates
            .iter()
            .map(|date| {
                date.checked_sub_signed(offset)
                    .ok_or_else(invalid_trend_request)
            })
            .collect::<Result<Vec<_>>>()?;
        return sparse_selection_from_dates(mapped);
    }
    Ok(ValidatedDateSelection::continuous(previous_equal_range(
        &selection.envelope,
    )?))
}

fn previous_equal_range(range: &ValidatedDateRange) -> Result<ValidatedDateRange> {
    let end = range.start.pred_opt().ok_or_else(invalid_trend_request)?;
    let start = range
        .start
        .checked_sub_signed(Duration::days(range.day_count as i64))
        .ok_or_else(invalid_trend_request)?;
    Ok(ValidatedDateRange {
        start,
        end,
        day_count: range.day_count,
    })
}

fn previous_month_selection(selection: &ValidatedDateSelection) -> Result<ValidatedDateSelection> {
    if let Some(selected_dates) = &selection.selected_dates {
        let mapped = selected_dates
            .iter()
            .copied()
            .map(subtract_calendar_month)
            .collect::<Result<BTreeSet<_>>>()?
            .into_iter()
            .collect();
        return sparse_selection_from_dates(mapped);
    }
    Ok(ValidatedDateSelection::continuous(previous_month_range(
        &selection.envelope,
    )?))
}

fn previous_month_range(range: &ValidatedDateRange) -> Result<ValidatedDateRange> {
    let start = subtract_calendar_month(range.start)?;
    let end = subtract_calendar_month(range.end)?;
    if start > end {
        return Err(invalid_trend_request());
    }
    Ok(ValidatedDateRange {
        start,
        end,
        day_count: (end - start).num_days() as usize + 1,
    })
}

fn sparse_selection_from_dates(dates: Vec<NaiveDate>) -> Result<ValidatedDateSelection> {
    let start = *dates.first().ok_or_else(invalid_trend_request)?;
    let end = *dates.last().ok_or_else(invalid_trend_request)?;
    Ok(ValidatedDateSelection {
        envelope: ValidatedDateRange {
            start,
            end,
            day_count: (end - start).num_days() as usize + 1,
        },
        selected_dates: Some(dates),
    })
}

fn subtract_calendar_month(date: NaiveDate) -> Result<NaiveDate> {
    let (year, month) = if date.month() == 1 {
        (date.year() - 1, 12)
    } else {
        (date.year(), date.month() - 1)
    };
    let last_day =
        month_end(NaiveDate::from_ymd_opt(year, month, 1).ok_or_else(invalid_trend_request)?)?
            .day();
    NaiveDate::from_ymd_opt(year, month, date.day().min(last_day)).ok_or_else(invalid_trend_request)
}

fn month_end(date: NaiveDate) -> Result<NaiveDate> {
    let (next_year, next_month) = if date.month() == 12 {
        (date.year() + 1, 1)
    } else {
        (date.year(), date.month() + 1)
    };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .and_then(|date| date.pred_opt())
        .ok_or_else(invalid_trend_request)
}

fn validate_date_range(range: &TrendDateRange) -> Result<ValidatedDateRange> {
    let start = parse_date(&range.start_date)?;
    let end = parse_date(&range.end_date)?;
    let day_count = (end - start).num_days() + 1;
    if !(1..=366).contains(&day_count) {
        return Err(invalid_trend_request());
    }
    Ok(ValidatedDateRange {
        start,
        end,
        day_count: day_count as usize,
    })
}

fn validate_date_selection(
    range: &TrendDateRange,
    selected_dates: Option<Vec<String>>,
) -> Result<ValidatedDateSelection> {
    let envelope = validate_date_range(range)?;
    let Some(selected_dates) = selected_dates.filter(|dates| !dates.is_empty()) else {
        return Ok(ValidatedDateSelection::continuous(envelope));
    };
    let normalized = selected_dates
        .into_iter()
        .map(|value| parse_date(&value))
        .collect::<Result<BTreeSet<_>>>()?;
    if normalized.is_empty()
        || normalized
            .iter()
            .any(|date| *date < envelope.start || *date > envelope.end)
    {
        return Err(invalid_trend_request());
    }
    Ok(ValidatedDateSelection {
        envelope,
        selected_dates: Some(normalized.into_iter().collect()),
    })
}

fn day_boundaries(range: &ValidatedDateRange, timezone_offset_minutes: i32) -> Result<Vec<i64>> {
    let mut boundaries = Vec::with_capacity(range.day_count + 1);
    for offset in 0..=range.day_count {
        let date = range
            .start
            .checked_add_signed(Duration::days(offset as i64))
            .ok_or_else(invalid_trend_request)?;
        let utc_midnight_ms = date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(invalid_trend_request)?
            .and_utc()
            .timestamp_millis();
        boundaries.push(
            utc_midnight_ms
                .checked_add(timezone_offset_minutes as i64 * MILLIS_PER_MINUTE)
                .ok_or_else(invalid_trend_request)?,
        );
    }
    Ok(boundaries)
}

fn validate_timezone_offset(timezone_offset_minutes: i32) -> Result<()> {
    if !(-24 * 60..=24 * 60).contains(&timezone_offset_minutes) {
        return Err(invalid_trend_request());
    }
    Ok(())
}

fn default_granularity(day_count: usize) -> TrendGranularity {
    match day_count {
        1..=31 => TrendGranularity::Day,
        32..=120 => TrendGranularity::Week,
        _ => TrendGranularity::Month,
    }
}

fn recorded_day_count(days: &[DailyFacts]) -> usize {
    days.iter()
        .filter(|day| day.values.monitored_seconds > 0 || day.has_task_evidence)
        .count()
}

fn effective_activity_day_count(days: &[DailyFacts]) -> usize {
    days.iter()
        .filter(|day| day.values.active_seconds > 0)
        .count()
}

fn percentage_delta(current: f64, baseline: f64) -> Option<f64> {
    if baseline == 0.0 {
        None
    } else {
        Some(((current - baseline) / baseline) * 100.0)
    }
}

fn ratio(part: i64, whole: i64) -> f64 {
    if whole <= 0 {
        0.0
    } else {
        part.max(0) as f64 / whole as f64
    }
}

fn counts_as_learning(category: ActivityCategory, video_purpose: VideoPurpose) -> bool {
    matches!(
        category,
        ActivityCategory::Research
            | ActivityCategory::TextInput
            | ActivityCategory::CreationDevelopment
    ) || (category == ActivityCategory::VideoInput && video_purpose == VideoPurpose::Learning)
}

fn metric_key(metric: TrendMetric) -> &'static str {
    match metric {
        TrendMetric::MonitoredSeconds => "monitoredSeconds",
        TrendMetric::ActiveSeconds => "activeSeconds",
        TrendMetric::LearningSeconds => "learningSeconds",
        TrendMetric::IdleSeconds => "idleSeconds",
        TrendMetric::SwitchCount => "switchCount",
        TrendMetric::LongestFocusSeconds => "longestFocusSeconds",
        TrendMetric::ClassificationCoverage => "classificationCoverage",
        TrendMetric::CompletedTaskCount => "completedTaskCount",
        TrendMetric::LinkedTaskSeconds => "linkedTaskSeconds",
    }
}

#[cfg(test)]
mod interval_merge_tests {
    use super::merged_interval_milliseconds;

    #[test]
    fn interval_union_merges_full_partial_adjacent_and_duplicate_ranges() {
        assert_eq!(merged_interval_milliseconds(vec![(0, 10), (2, 8)]), 10);
        assert_eq!(merged_interval_milliseconds(vec![(0, 10), (5, 15)]), 15);
        assert_eq!(merged_interval_milliseconds(vec![(0, 10), (10, 20)]), 20);
        assert_eq!(merged_interval_milliseconds(vec![(0, 10), (0, 10)]), 10);
    }

    #[test]
    fn interval_union_preserves_half_open_gaps_and_ignores_empty_ranges() {
        assert_eq!(
            merged_interval_milliseconds(vec![(0, 10), (11, 20), (5, 5), (9, 3)]),
            19
        );
    }
}

fn baseline_key(kind: TrendBaselineKind) -> &'static str {
    match kind {
        TrendBaselineKind::Current => "current",
        TrendBaselineKind::PreviousEqualLength => "previousEqualLength",
        TrendBaselineKind::PreviousMonthSamePeriod => "previousMonthSamePeriod",
        TrendBaselineKind::Custom => "custom",
    }
}

fn summary_evidence_id(kind: TrendBaselineKind, metric: TrendMetric) -> String {
    format!("{}.summary.{}", baseline_key(kind), metric_key(metric))
}

fn parse_date(value: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| invalid_trend_request())
}

fn format_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn invalid_trend_request() -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName("invalid trend workbench request".into())
}
