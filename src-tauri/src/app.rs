use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Datelike, Duration, NaiveDate, SecondsFormat, Utc};
use rusqlite::Result;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

use crate::activity_composition::{
    ActivityCompositionSlice, activity_is_meaningful, build_activity_compositions,
};
use crate::ai::{
    AiExecutionMode, AiExecutionSnapshot, AiJob, AiJobStatus, TrendAnalysisAllowedCandidates,
};
use crate::db::{
    ActivitySegmentRecord, DailyAnalysisRecord, DashboardTotals, Database, TrendAnalysisRecord,
    TrendResearchAnalysisRecord, category_key,
};
use crate::domain::{
    ActivityCategory, ActivityCompositions, ActivityScope, TrendBreakdownItem, TrendComparison,
    TrendDataQuality, TrendDay, TrendPayload, TrendRange, TrendSummary, VideoPurpose,
};
use crate::knowledge_graph::{KnowledgeGraphFilters, KnowledgeGraphPayload, build_knowledge_graph};
use crate::trend_analysis::{
    TrendResearchAnalysis, TrendResearchJobPayload, build_trend_research_input,
    build_trend_research_job_payload_scoped, limitations_only_analysis, unavailable_analysis,
};
use crate::trends::{
    TrendBaselineKind, TrendGranularity, TrendMetric, TrendSelectionMode, TrendWorkbenchPayload,
    TrendWorkbenchRequest, TrendWorkbenchResult, get_trend_workbench, get_trend_workbench_scoped,
};
use crate::work_ledger::WorkLedgerRangeRollup;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSnapshot {
    pub totals: DashboardTotals,
    pub timeline: Vec<ActivitySegmentRecord>,
    pub work_ledger: WorkLedgerRangeRollup,
    #[serde(default)]
    pub activity_composition: ActivityCompositions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyAnalysisEvidence {
    pub date: String,
    pub start_ms: i64,
    pub end_ms: i64,
    #[serde(default, skip_serializing_if = "activity_scope_is_all")]
    pub activity_scope: ActivityScope,
    pub goals: String,
    pub expected_output: String,
    pub actual_output: String,
    pub monitored_seconds: i64,
    pub active_seconds: i64,
    pub learning_seconds: i64,
    pub idle_seconds: i64,
    pub switch_count: i64,
    pub longest_focus_seconds: i64,
    pub category_seconds: BTreeMap<String, i64>,
    pub top_apps: Vec<DailyAnalysisApp>,
    pub browser_visit_count: i64,
    pub classification_coverage: f64,
    pub evidence_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyAnalysisApp {
    pub name: String,
    pub seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyAnalysisResult {
    pub portrait: String,
    pub recommendation: String,
    #[serde(default)]
    pub findings: Vec<EvidenceBasedFinding>,
    #[serde(default = "legacy_analysis_protocol")]
    pub protocol_version: u8,
    pub source: String,
    pub evidence_hash: String,
    pub generated_at_ms: i64,
    #[serde(default)]
    pub activity_scope: ActivityScope,
}

fn legacy_analysis_protocol() -> u8 {
    1
}

fn activity_scope_key(scope: ActivityScope) -> &'static str {
    match scope {
        ActivityScope::All => "all",
        ActivityScope::Meaningful => "meaningful",
    }
}

fn activity_scope_is_all(scope: &ActivityScope) -> bool {
    *scope == ActivityScope::All
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisScope {
    Daily,
    Trend,
    Workflow,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceBasedFinding {
    pub observation: String,
    pub hypothesis: String,
    pub validation: String,
    pub action: String,
    pub evidence_ids: Vec<String>,
    pub limitations: Vec<String>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrendAnalysisResult {
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
    #[serde(default)]
    pub activity_scope: ActivityScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrendAnalysisJobPayload {
    #[serde(default)]
    pub activity_scope: ActivityScope,
    pub evidence: TrendPayload,
    pub day_boundaries_ms: Vec<i64>,
    pub comparison_day_boundaries_ms: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default = "default_idle_threshold")]
    pub idle_threshold_minutes: u32,
    #[serde(default = "default_true")]
    pub monitoring_enabled: bool,
    #[serde(default)]
    pub ai_backfill_enabled: bool,
    #[serde(default)]
    pub ai_execution_mode: AiExecutionMode,
    #[serde(default)]
    pub selected_api_provider_id: Option<String>,
    #[serde(default = "default_codex_executable")]
    pub codex_executable: String,
    #[serde(default)]
    pub codex_model: String,
    #[serde(
        default = "default_true",
        rename = "aiAutoResearchAnalysisEnabled",
        alias = "aiAutoTrendAnalysisEnabled"
    )]
    pub ai_auto_trend_analysis_enabled: bool,
    #[serde(default = "default_true")]
    pub ai_auto_classification_enabled: bool,
    #[serde(default = "default_true")]
    pub ai_auto_workflow_assignment_enabled: bool,
    #[serde(default)]
    pub ai_automation_notice_version: u32,
    #[serde(default)]
    pub excluded_apps: Vec<String>,
    #[serde(default)]
    pub excluded_domains: Vec<String>,
    #[serde(default)]
    pub ui_theme: UiTheme,
    #[serde(default)]
    pub experimental_knowledge_graph_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UiTheme {
    ClassicWorkbench,
    MoonGlass,
    SoftPaper,
    BlueprintData,
    KnowledgeSpace,
}

impl Default for UiTheme {
    fn default() -> Self {
        Self::ClassicWorkbench
    }
}

impl<'de> Deserialize<'de> for UiTheme {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(
            match serde_json::Value::deserialize(deserializer)?.as_str() {
                Some("moon-glass") => Self::MoonGlass,
                Some("soft-paper" | "studio-blocks") => Self::SoftPaper,
                Some("blueprint-data" | "signal-console") => Self::BlueprintData,
                Some("knowledge-space") => Self::KnowledgeSpace,
                Some("classic-workbench" | "precision-paper") => Self::ClassicWorkbench,
                _ => Self::ClassicWorkbench,
            },
        )
    }
}

fn default_idle_threshold() -> u32 {
    6
}

fn default_true() -> bool {
    true
}

fn default_codex_executable() -> String {
    "codex".to_string()
}

fn deserialize_optional_nullable_string<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

fn normalize_codex_executable(value: String) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return default_codex_executable();
    }
    if let Some(rest) = trimmed.strip_prefix('"') {
        if let Some(end) = rest.find('"') {
            let quoted = rest[..end].trim();
            return if quoted.is_empty() {
                default_codex_executable()
            } else {
                quoted.to_string()
            };
        }
    }
    let without_flags = trimmed
        .split_once(" --")
        .map(|(program, _)| program)
        .unwrap_or(trimmed)
        .trim();
    if without_flags.is_empty() {
        default_codex_executable()
    } else {
        without_flags.to_string()
    }
}

fn normalize_codex_model(value: String) -> String {
    value.trim().to_string()
}

impl AppSettings {
    fn normalized(mut self) -> Self {
        self.selected_api_provider_id = self
            .selected_api_provider_id
            .map(|provider_id| provider_id.trim().to_string())
            .filter(|provider_id| !provider_id.is_empty());
        self.codex_executable = normalize_codex_executable(self.codex_executable);
        self.codex_model = normalize_codex_model(self.codex_model);
        self
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            idle_threshold_minutes: 6,
            monitoring_enabled: true,
            ai_backfill_enabled: false,
            ai_execution_mode: AiExecutionMode::default(),
            selected_api_provider_id: None,
            codex_executable: default_codex_executable(),
            codex_model: String::new(),
            ai_auto_trend_analysis_enabled: true,
            ai_auto_classification_enabled: true,
            ai_auto_workflow_assignment_enabled: true,
            ai_automation_notice_version: 0,
            excluded_apps: Vec::new(),
            excluded_domains: Vec::new(),
            ui_theme: UiTheme::default(),
            experimental_knowledge_graph_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub idle_threshold_minutes: Option<u32>,
    pub monitoring_enabled: Option<bool>,
    pub ai_backfill_enabled: Option<bool>,
    pub ai_execution_mode: Option<AiExecutionMode>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable_string")]
    pub selected_api_provider_id: Option<Option<String>>,
    pub codex_executable: Option<String>,
    pub codex_model: Option<String>,
    #[serde(
        rename = "aiAutoResearchAnalysisEnabled",
        alias = "aiAutoTrendAnalysisEnabled"
    )]
    pub ai_auto_trend_analysis_enabled: Option<bool>,
    pub ai_auto_classification_enabled: Option<bool>,
    pub ai_auto_workflow_assignment_enabled: Option<bool>,
    pub ai_automation_notice_version: Option<u32>,
    pub excluded_apps: Option<Vec<String>>,
    pub excluded_domains: Option<Vec<String>>,
    pub ui_theme: Option<UiTheme>,
    pub experimental_knowledge_graph_enabled: Option<bool>,
}

pub struct AppService {
    database: Database,
}

impl AppService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn get_dashboard(&self, start_ms: i64, end_ms: i64) -> Result<DashboardSnapshot> {
        let timeline = self.database.list_segments(start_ms, end_ms)?;
        let clipped_segments = self.database.list_clipped_segments(start_ms, end_ms)?;
        let work_ledger_facts = self
            .database
            .load_work_ledger_range_facts(start_ms, end_ms)?;
        let linked_activity_ids = work_ledger_facts
            .activities
            .iter()
            .map(|fact| fact.segment.id.as_str())
            .collect::<BTreeSet<_>>();
        let activity_composition =
            build_activity_compositions(clipped_segments.iter().map(|segment| {
                ActivityCompositionSlice {
                    scope_key: segment.id.clone(),
                    category: segment.category,
                    video_purpose: segment.video_purpose,
                    seconds: segment.ended_at_ms.saturating_sub(segment.started_at_ms) / 1_000,
                    workflow_linked: linked_activity_ids.contains(segment.id.as_str()),
                }
            }));
        Ok(DashboardSnapshot {
            totals: self.database.dashboard_totals(start_ms, end_ms)?,
            timeline,
            work_ledger: self.database.work_ledger_range_rollup(start_ms, end_ms)?,
            activity_composition,
        })
    }

    pub fn get_timeline(&self, start_ms: i64, end_ms: i64) -> Result<Vec<ActivitySegmentRecord>> {
        self.database.list_segments(start_ms, end_ms)
    }

    pub fn get_trends(
        &self,
        start_date: &str,
        end_date: &str,
        day_boundaries_ms: Vec<i64>,
        comparison_start_date: &str,
        comparison_end_date: &str,
        comparison_day_boundaries_ms: Vec<i64>,
    ) -> Result<TrendPayload> {
        self.get_trends_scoped(
            start_date,
            end_date,
            day_boundaries_ms,
            comparison_start_date,
            comparison_end_date,
            comparison_day_boundaries_ms,
            ActivityScope::All,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn get_trends_scoped(
        &self,
        start_date: &str,
        end_date: &str,
        day_boundaries_ms: Vec<i64>,
        comparison_start_date: &str,
        comparison_end_date: &str,
        comparison_day_boundaries_ms: Vec<i64>,
        activity_scope: ActivityScope,
    ) -> Result<TrendPayload> {
        let range = trend_range(start_date, end_date, &day_boundaries_ms)?;
        let previous_range = trend_range(
            comparison_start_date,
            comparison_end_date,
            &comparison_day_boundaries_ms,
        )?;
        if day_boundaries_ms.len() != comparison_day_boundaries_ms.len()
            || comparison_day_boundaries_ms.last() != day_boundaries_ms.first()
            || !ranges_are_calendar_adjacent(&previous_range, &range)
        {
            return Err(invalid_trend_range());
        }
        let mut current = aggregate_trend_range(
            range.clone(),
            self.scoped_trend_segments(range.start_ms, range.end_ms, activity_scope)?,
            &day_boundaries_ms,
        )?;
        apply_completed_task_counts(
            &mut current,
            self.database
                .list_completed_task_timestamps(range.start_ms, range.end_ms)?,
            &day_boundaries_ms,
        );
        let mut previous = aggregate_trend_range(
            previous_range.clone(),
            self.scoped_trend_segments(
                previous_range.start_ms,
                previous_range.end_ms,
                activity_scope,
            )?,
            &comparison_day_boundaries_ms,
        )?;
        apply_completed_task_counts(
            &mut previous,
            self.database
                .list_completed_task_timestamps(previous_range.start_ms, previous_range.end_ms)?,
            &comparison_day_boundaries_ms,
        );
        let comparison = TrendComparison {
            previous_range,
            day_count: previous.range.day_count,
            previous_monitored_seconds: previous.summary.monitored_seconds,
            previous_active_seconds: previous.summary.active_seconds,
            previous_idle_seconds: previous.summary.idle_seconds,
            previous_learning_seconds: previous.summary.learning_seconds,
            previous_switch_count: previous.summary.switch_count,
            previous_longest_focus_seconds: previous.summary.longest_focus_seconds,
            previous_completed_task_count: previous.summary.completed_task_count,
            previous_learning_ratio: previous.summary.learning_ratio,
            previous_switches_per_active_hour: previous.summary.switches_per_active_hour,
            previous_classification_coverage: previous.quality.classification_coverage,
            previous_category_breakdown: previous.summary.category_breakdown.clone(),
            previous_app_breakdown: previous.summary.app_breakdown.clone(),
            monitored_seconds_delta_percent: percentage_delta(
                current.summary.monitored_seconds as f64,
                previous.summary.monitored_seconds as f64,
            ),
            active_seconds_delta_percent: percentage_delta(
                current.summary.active_seconds as f64,
                previous.summary.active_seconds as f64,
            ),
            idle_seconds_delta_percent: percentage_delta(
                current.summary.idle_seconds as f64,
                previous.summary.idle_seconds as f64,
            ),
            learning_seconds_delta_percent: percentage_delta(
                current.summary.learning_seconds as f64,
                previous.summary.learning_seconds as f64,
            ),
            switch_count_delta_percent: percentage_delta(
                current.summary.switch_count as f64,
                previous.summary.switch_count as f64,
            ),
            longest_focus_seconds_delta_percent: percentage_delta(
                current.summary.longest_focus_seconds as f64,
                previous.summary.longest_focus_seconds as f64,
            ),
            completed_task_count_delta_percent: percentage_delta(
                current.summary.completed_task_count as f64,
                previous.summary.completed_task_count as f64,
            ),
            learning_ratio_delta_percent: percentage_delta(
                current.summary.learning_ratio,
                previous.summary.learning_ratio,
            ),
            switches_per_active_hour_delta_percent: percentage_delta(
                current.summary.switches_per_active_hour,
                previous.summary.switches_per_active_hour,
            ),
            classification_coverage_delta_percent: percentage_delta(
                current.quality.classification_coverage,
                previous.quality.classification_coverage,
            ),
        };
        let mut payload = TrendPayload {
            range: current.range,
            days: current.days,
            summary: current.summary,
            comparison,
            quality: current.quality,
            work_ledger: self
                .database
                .work_ledger_range_rollup(range.start_ms, range.end_ms)?,
            evidence_hash: String::new(),
        };
        let canonical = serde_json::to_vec(&TrendEvidenceHashInput {
            activity_scope,
            range: &payload.range,
            days: &payload.days,
            summary: &payload.summary,
            comparison: &payload.comparison,
            quality: &payload.quality,
            evidence_hash: &payload.evidence_hash,
        })
        .expect("TrendPayload hash evidence always serializes");
        payload.evidence_hash = format!("{:x}", Sha256::digest(canonical));
        Ok(payload)
    }

    fn scoped_trend_segments(
        &self,
        start_ms: i64,
        end_ms: i64,
        activity_scope: ActivityScope,
    ) -> Result<Vec<ActivitySegmentRecord>> {
        let mut segments = self.database.list_clipped_segments(start_ms, end_ms)?;
        if activity_scope == ActivityScope::All {
            return Ok(segments);
        }
        let linked_activity_ids = self
            .database
            .load_work_ledger_range_facts(start_ms, end_ms)?
            .activities
            .into_iter()
            .map(|fact| fact.segment.id)
            .collect::<BTreeSet<_>>();
        segments.retain(|segment| {
            activity_is_meaningful(
                segment.category,
                segment.video_purpose,
                linked_activity_ids.contains(&segment.id),
            )
        });
        Ok(segments)
    }

    pub fn get_trend_workbench(
        &self,
        request: TrendWorkbenchRequest,
    ) -> TrendWorkbenchResult<TrendWorkbenchPayload> {
        get_trend_workbench(&self.database, request)
    }

    pub fn get_trend_workbench_scoped(
        &self,
        request: TrendWorkbenchRequest,
        activity_scope: ActivityScope,
    ) -> TrendWorkbenchResult<TrendWorkbenchPayload> {
        get_trend_workbench_scoped(&self.database, request, activity_scope)
    }

    pub fn get_trend_research_analysis(
        &self,
        request: TrendWorkbenchRequest,
    ) -> std::result::Result<TrendResearchAnalysis, String> {
        self.get_trend_research_analysis_scoped(request, ActivityScope::All)
    }

    pub fn get_trend_research_analysis_scoped(
        &self,
        request: TrendWorkbenchRequest,
        activity_scope: ActivityScope,
    ) -> std::result::Result<TrendResearchAnalysis, String> {
        let range_start = request.start_date.clone();
        let range_end = request.end_date.clone();
        let workbench = self
            .get_trend_workbench_scoped(request, activity_scope)
            .map_err(|error| error.to_string())?;
        let input = build_trend_research_input(&workbench);
        if input.effective_activity_day_count < 3 {
            let mut analysis = limitations_only_analysis(&input);
            analysis.activity_scope = activity_scope;
            return Ok(analysis);
        }
        if let Some(saved) = self
            .database
            .get_trend_research_analysis(&range_start, &range_end, &input.evidence_hash)
            .map_err(|error| error.to_string())?
        {
            let mut analysis = saved.analysis;
            analysis.activity_scope = activity_scope;
            return Ok(analysis);
        }
        let mut analysis = unavailable_analysis(&input);
        analysis.activity_scope = activity_scope;
        Ok(analysis)
    }

    pub fn queue_trend_research_analysis(
        &self,
        request: TrendWorkbenchRequest,
        now_ms: i64,
        execution: Option<AiExecutionSnapshot>,
        force: bool,
    ) -> std::result::Result<Option<String>, String> {
        self.queue_trend_research_analysis_scoped(
            request,
            ActivityScope::All,
            now_ms,
            execution,
            force,
        )
    }

    pub fn queue_trend_research_analysis_scoped(
        &self,
        request: TrendWorkbenchRequest,
        activity_scope: ActivityScope,
        now_ms: i64,
        execution: Option<AiExecutionSnapshot>,
        force: bool,
    ) -> std::result::Result<Option<String>, String> {
        let Some(execution) = execution else {
            return Ok(None);
        };
        let workbench = self
            .get_trend_workbench_scoped(request.clone(), activity_scope)
            .map_err(|error| error.to_string())?;
        let queued = build_trend_research_job_payload_scoped(request, &workbench, activity_scope);
        if queued.input.effective_activity_day_count < 3 {
            return Ok(None);
        }
        if execution.evidence_hash != queued.input.evidence_hash {
            return Err("Trend research execution snapshot does not match current evidence".into());
        }
        let payload = serde_json::to_string(&queued)
            .map_err(|error| format!("Could not serialize trend research job: {error}"))?;
        let subject_key = format!(
            "{}:{}:{}:{}",
            queued.request.start_date,
            queued.request.end_date,
            activity_scope_key(activity_scope),
            queued.input.evidence_hash
        );
        let result = if force {
            self.database.force_enqueue_ai_job_for_subject(
                "trend_research_analysis",
                &subject_key,
                &payload,
                now_ms,
                &execution,
            )
        } else {
            self.database.enqueue_ai_job_for_subject(
                "trend_research_analysis",
                &subject_key,
                &payload,
                now_ms,
                &execution,
            )
        };
        result.map(Some).map_err(|error| error.to_string())
    }

    pub fn complete_ai_trend_research_job(
        &self,
        job_id: &str,
        generation: i64,
        queued: &TrendResearchJobPayload,
        analysis: &TrendResearchAnalysis,
    ) -> std::result::Result<bool, String> {
        let current = self
            .get_trend_workbench_scoped(queued.request.clone(), queued.activity_scope)
            .map_err(|error| error.to_string())?;
        if current.evidence_hash != queued.input.evidence_hash
            || analysis.evidence_hash != queued.input.evidence_hash
        {
            return Ok(false);
        }
        let mut scoped_analysis = analysis.clone();
        scoped_analysis.activity_scope = queued.activity_scope;
        self.database
            .complete_trend_research_analysis_job_generation(
                job_id,
                generation,
                &TrendResearchAnalysisRecord {
                    range_start: queued.request.start_date.clone(),
                    range_end: queued.request.end_date.clone(),
                    analysis: scoped_analysis,
                },
            )
            .map_err(|error| error.to_string())
    }

    pub fn get_settings(&self) -> Result<AppSettings> {
        let Some(value) = self.database.get_setting_json("app")? else {
            return Ok(AppSettings::default());
        };
        let mut value = serde_json::from_str::<serde_json::Value>(&value).unwrap_or_default();
        if let Some(object) = value.as_object_mut() {
            if !object.contains_key("aiExecutionMode")
                && object
                    .get("aiBackend")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|backend| {
                        matches!(
                            backend.trim().to_ascii_lowercase().as_str(),
                            "codex" | "codex-cli" | "codex_cli"
                        )
                    })
            {
                object.insert("aiExecutionMode".into(), serde_json::json!("codex"));
            }
            if !object.contains_key("codexExecutable") {
                if let Some(path) = object
                    .get("codexExecutablePath")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                {
                    object.insert("codexExecutable".into(), serde_json::json!(path));
                }
            }
        }
        Ok(serde_json::from_value::<AppSettings>(value)
            .unwrap_or_default()
            .normalized())
    }

    pub fn update_settings(&self, patch: SettingsPatch) -> Result<AppSettings> {
        let mut settings = self.get_settings()?;
        if let Some(value) = patch.idle_threshold_minutes {
            settings.idle_threshold_minutes = value.clamp(1, 60);
        }
        if let Some(value) = patch.monitoring_enabled {
            settings.monitoring_enabled = value;
        }
        if let Some(value) = patch.ai_backfill_enabled {
            settings.ai_backfill_enabled = value;
        }
        if let Some(value) = patch.ai_execution_mode {
            settings.ai_execution_mode = value;
        }
        if let Some(value) = patch.selected_api_provider_id {
            settings.selected_api_provider_id = value
                .map(|provider_id| provider_id.trim().to_string())
                .filter(|provider_id| !provider_id.is_empty());
        }
        if let Some(value) = patch.codex_executable {
            settings.codex_executable = normalize_codex_executable(value);
        }
        if let Some(value) = patch.codex_model {
            settings.codex_model = normalize_codex_model(value);
        }
        if let Some(value) = patch.ai_auto_trend_analysis_enabled {
            settings.ai_auto_trend_analysis_enabled = value;
        }
        if let Some(value) = patch.ai_auto_classification_enabled {
            settings.ai_auto_classification_enabled = value;
        }
        if let Some(value) = patch.ai_auto_workflow_assignment_enabled {
            settings.ai_auto_workflow_assignment_enabled = value;
        }
        if let Some(value) = patch.ai_automation_notice_version {
            settings.ai_automation_notice_version = value;
        }
        if let Some(value) = patch.excluded_apps {
            settings.excluded_apps = normalize_exclusions(value);
        }
        if let Some(value) = patch.excluded_domains {
            settings.excluded_domains = normalize_exclusions(value);
        }
        if let Some(value) = patch.ui_theme {
            settings.ui_theme = value;
        }
        if let Some(value) = patch.experimental_knowledge_graph_enabled {
            settings.experimental_knowledge_graph_enabled = value;
        }
        let value = serde_json::to_string(&settings).expect("AppSettings always serializes");
        self.database.set_setting_json("app", &value)?;
        Ok(settings)
    }

    pub fn save_manual_classification(
        &self,
        segment_id: &str,
        category: ActivityCategory,
        video_purpose: VideoPurpose,
        reason: &str,
    ) -> Result<bool> {
        let changed = self.database.save_manual_classification(
            segment_id,
            category,
            video_purpose,
            reason,
        )?;
        if changed {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            self.database.save_manual_rule_for_segment(
                segment_id,
                category,
                video_purpose,
                now_ms,
            )?;
        }
        Ok(changed)
    }

    pub fn database(&self) -> &Database {
        &self.database
    }

    pub fn get_knowledge_graph(
        &self,
        start_ms: i64,
        end_ms: i64,
        filters: KnowledgeGraphFilters,
    ) -> Result<KnowledgeGraphPayload> {
        let settings = self.get_settings()?;
        Ok(build_knowledge_graph(
            self.database.list_segments(start_ms, end_ms)?,
            self.database.list_browser_visits(start_ms, end_ms)?,
            start_ms,
            end_ms,
            &filters,
            &settings.excluded_apps,
            &settings.excluded_domains,
        ))
    }

    pub fn get_daily_analysis(
        &self,
        date: &str,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<DailyAnalysisResult> {
        self.get_daily_analysis_scoped(date, start_ms, end_ms, ActivityScope::All)
    }

    pub fn get_daily_analysis_scoped(
        &self,
        date: &str,
        start_ms: i64,
        end_ms: i64,
        activity_scope: ActivityScope,
    ) -> Result<DailyAnalysisResult> {
        let evidence =
            self.build_daily_analysis_evidence_scoped(date, start_ms, end_ms, activity_scope)?;
        if let Some(saved) = self
            .database
            .get_daily_analysis_scoped(date, activity_scope)?
            && saved.evidence_hash == evidence.evidence_hash
        {
            return Ok(DailyAnalysisResult {
                portrait: saved.portrait,
                recommendation: saved.recommendation,
                findings: serde_json::from_str(&saved.findings_json).unwrap_or_default(),
                protocol_version: saved.protocol_version.clamp(1, u8::MAX as i64) as u8,
                source: saved.source,
                evidence_hash: saved.evidence_hash,
                generated_at_ms: saved.generated_at_ms,
                activity_scope,
            });
        }
        Ok(build_local_daily_analysis(&evidence))
    }

    pub fn get_trend_analysis(
        &self,
        evidence: &TrendPayload,
        generated_at_ms: i64,
    ) -> Result<TrendAnalysisResult> {
        self.get_trend_analysis_scoped(evidence, generated_at_ms, ActivityScope::All)
    }

    pub fn get_trend_analysis_scoped(
        &self,
        evidence: &TrendPayload,
        generated_at_ms: i64,
        activity_scope: ActivityScope,
    ) -> Result<TrendAnalysisResult> {
        if let Some(saved) = self.database.get_trend_analysis(
            &evidence.range.start_date,
            &evidence.range.end_date,
            &evidence.evidence_hash,
        )? {
            return Ok(trend_analysis_result(saved, activity_scope));
        }
        let local = build_local_trend_analysis_scoped(evidence, generated_at_ms, activity_scope);
        self.database.save_trend_analysis(&TrendAnalysisRecord {
            range_start: local.range_start.clone(),
            range_end: local.range_end.clone(),
            evidence_hash: local.evidence_hash.clone(),
            summary: local.summary.clone(),
            observations: local.observations.clone(),
            suggestions: local.suggestions.clone(),
            source: local.source.clone(),
            model: local.model.clone(),
            confidence: local.confidence,
            generated_at_ms: local.generated_at_ms,
        })?;
        Ok(local)
    }

    pub fn queue_trend_analysis(
        &self,
        evidence: &TrendPayload,
        day_boundaries_ms: Vec<i64>,
        comparison_day_boundaries_ms: Vec<i64>,
        now_ms: i64,
        execution: Option<&AiExecutionSnapshot>,
        force: bool,
    ) -> Result<Option<String>> {
        self.queue_trend_analysis_scoped(
            evidence,
            day_boundaries_ms,
            comparison_day_boundaries_ms,
            now_ms,
            execution,
            force,
            ActivityScope::All,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn queue_trend_analysis_scoped(
        &self,
        evidence: &TrendPayload,
        day_boundaries_ms: Vec<i64>,
        comparison_day_boundaries_ms: Vec<i64>,
        now_ms: i64,
        execution: Option<&AiExecutionSnapshot>,
        force: bool,
        activity_scope: ActivityScope,
    ) -> Result<Option<String>> {
        let Some(execution) = execution else {
            return Ok(None);
        };
        let mut execution = execution.clone();
        execution.evidence_hash = evidence.evidence_hash.clone();
        execution.created_at_ms = now_ms;
        let payload = serde_json::to_string(&TrendAnalysisQueuePayload {
            activity_scope,
            evidence: TrendEvidenceHashInput {
                activity_scope,
                range: &evidence.range,
                days: &evidence.days,
                summary: &evidence.summary,
                comparison: &evidence.comparison,
                quality: &evidence.quality,
                evidence_hash: &evidence.evidence_hash,
            },
            day_boundaries_ms,
            comparison_day_boundaries_ms,
        })
        .expect("trend analysis job payload serializes");
        let subject_key = format!(
            "{}:{}:{}:{}",
            activity_scope_key(activity_scope),
            evidence.range.start_date,
            evidence.range.end_date,
            evidence.evidence_hash
        );
        let result = if force {
            self.database.force_enqueue_ai_job_for_subject(
                "trend_analysis",
                &subject_key,
                &payload,
                now_ms,
                &execution,
            )
        } else {
            self.database.enqueue_ai_job_for_subject(
                "trend_analysis",
                &subject_key,
                &payload,
                now_ms,
                &execution,
            )
        };
        result.map(Some)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn save_ai_trend_analysis_if_current(
        &self,
        queued: &TrendAnalysisJobPayload,
        summary: &str,
        observations: &[String],
        suggestions: &[String],
        source: &str,
        model: &str,
        confidence: f64,
        generated_at_ms: i64,
    ) -> Result<bool> {
        let Some(current_evidence) = self.current_trend_evidence_if_matching(queued)? else {
            return Ok(false);
        };
        self.database.save_trend_analysis(&TrendAnalysisRecord {
            range_start: current_evidence.range.start_date.clone(),
            range_end: current_evidence.range.end_date.clone(),
            evidence_hash: queued.evidence.evidence_hash.clone(),
            summary: summary.to_string(),
            observations: observations.to_vec(),
            suggestions: suggestions.to_vec(),
            source: source.to_string(),
            model: model.to_string(),
            confidence: confidence.clamp(0.0, 1.0),
            generated_at_ms,
        })?;
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_ai_trend_analysis_job(
        &self,
        job_id: &str,
        generation: i64,
        queued: &TrendAnalysisJobPayload,
        summary: &str,
        observations: &[String],
        suggestions: &[String],
        source: &str,
        model: &str,
        confidence: f64,
        generated_at_ms: i64,
    ) -> Result<bool> {
        let Some(current_evidence) = self.current_trend_evidence_if_matching(queued)? else {
            return Ok(false);
        };

        self.database.complete_trend_analysis_job_generation(
            job_id,
            generation,
            &TrendAnalysisRecord {
                range_start: current_evidence.range.start_date,
                range_end: current_evidence.range.end_date,
                evidence_hash: queued.evidence.evidence_hash.clone(),
                summary: summary.to_string(),
                observations: observations.to_vec(),
                suggestions: suggestions.to_vec(),
                source: source.to_string(),
                model: model.to_string(),
                confidence: confidence.clamp(0.0, 1.0),
                generated_at_ms,
            },
        )
    }

    fn current_trend_evidence_if_matching(
        &self,
        queued: &TrendAnalysisJobPayload,
    ) -> Result<Option<TrendPayload>> {
        let completed = &queued.evidence;
        let current = self.get_trends_scoped(
            &completed.range.start_date,
            &completed.range.end_date,
            queued.day_boundaries_ms.clone(),
            &completed.comparison.previous_range.start_date,
            &completed.comparison.previous_range.end_date,
            queued.comparison_day_boundaries_ms.clone(),
            queued.activity_scope,
        )?;
        Ok((current.evidence_hash == completed.evidence_hash).then_some(current))
    }

    pub fn ai_job_reassignment_evidence_hash(
        &self,
        job: &AiJob,
    ) -> std::result::Result<Option<String>, String> {
        if job.status != AiJobStatus::AwaitingReassignment {
            return Ok(None);
        }
        let payload: serde_json::Value = match serde_json::from_str(&job.payload_json) {
            Ok(payload) => payload,
            Err(_) => return Ok(None),
        };
        match job.kind.as_str() {
            "classify_segment" => {
                let Some(segment_id) = payload.get("id").and_then(serde_json::Value::as_str) else {
                    return Ok(None);
                };
                let current = self
                    .database
                    .classification_evidence_hash(segment_id)
                    .map_err(|error| error.to_string())?;
                Ok(current.filter(|hash| {
                    job.execution.evidence_hash.is_empty() || hash == &job.execution.evidence_hash
                }))
            }
            "classify_page" => {
                let Some(visit_id) = payload.get("visitId").and_then(serde_json::Value::as_str)
                else {
                    return Ok(None);
                };
                let expected_matches = job.execution.evidence_hash.is_empty()
                    || job.execution.evidence_hash == visit_id;
                let payload_is_current = self
                    .database
                    .page_classification_payload_is_current(&job.payload_json)
                    .map_err(|error| error.to_string())?;
                Ok((expected_matches && payload_is_current).then(|| visit_id.to_string()))
            }
            "work_ledger_assignment" => {
                let Some(evidence_hash) = payload
                    .get("evidenceHash")
                    .and_then(serde_json::Value::as_str)
                else {
                    return Ok(None);
                };
                let expected_matches = job.execution.evidence_hash.is_empty()
                    || job.execution.evidence_hash == evidence_hash;
                let payload_is_current = self
                    .database
                    .work_ledger_assignment_evidence_is_current(&job.payload_json)
                    .map_err(|error| error.to_string())?;
                Ok((expected_matches && payload_is_current).then(|| evidence_hash.to_string()))
            }
            "daily_analysis" => {
                let Some(date) = payload.get("date").and_then(serde_json::Value::as_str) else {
                    return Ok(None);
                };
                let Some(evidence_hash) = payload
                    .get("evidenceHash")
                    .and_then(serde_json::Value::as_str)
                else {
                    return Ok(None);
                };
                let Some(start_ms) = payload.get("startMs").and_then(serde_json::Value::as_i64)
                else {
                    return Ok(None);
                };
                let Some(end_ms) = payload.get("endMs").and_then(serde_json::Value::as_i64) else {
                    return Ok(None);
                };
                let activity_scope = match payload.get("activityScope") {
                    Some(scope) => match serde_json::from_value::<ActivityScope>(scope.clone()) {
                        Ok(scope) => scope,
                        Err(_) => return Ok(None),
                    },
                    None => ActivityScope::All,
                };
                if start_ms >= end_ms
                    || (!job.execution.evidence_hash.is_empty()
                        && evidence_hash != job.execution.evidence_hash)
                {
                    return Ok(None);
                }
                let current = self
                    .build_daily_analysis_evidence_scoped(date, start_ms, end_ms, activity_scope)
                    .map_err(|error| error.to_string())?;
                Ok((current.evidence_hash == evidence_hash).then(|| evidence_hash.to_string()))
            }
            "trend_analysis" => {
                let queued: TrendAnalysisJobPayload = match serde_json::from_value(payload) {
                    Ok(queued) => queued,
                    Err(_) => return Ok(None),
                };
                if !job.execution.evidence_hash.is_empty()
                    && queued.evidence.evidence_hash != job.execution.evidence_hash
                {
                    return Ok(None);
                }
                self.current_trend_evidence_if_matching(&queued)
                    .map(|current| current.map(|current| current.evidence_hash))
                    .map_err(|error| error.to_string())
            }
            "trend_research_analysis" => {
                let queued: TrendResearchJobPayload = match serde_json::from_value(payload) {
                    Ok(queued) => queued,
                    Err(_) => return Ok(None),
                };
                if !job.execution.evidence_hash.is_empty()
                    && queued.input.evidence_hash != job.execution.evidence_hash
                {
                    return Ok(None);
                }
                let workbench = self
                    .get_trend_workbench_scoped(queued.request, queued.activity_scope)
                    .map_err(|error| error.to_string())?;
                let current_hash = build_trend_research_input(&workbench).evidence_hash;
                Ok((current_hash == queued.input.evidence_hash).then_some(current_hash))
            }
            _ => Ok(None),
        }
    }

    pub fn ai_job_reassignment_is_current(&self, job: &AiJob) -> std::result::Result<bool, String> {
        self.ai_job_reassignment_evidence_hash(job)
            .map(|hash| hash.is_some())
    }

    pub fn queue_daily_analysis(
        &self,
        date: &str,
        start_ms: i64,
        end_ms: i64,
        now_ms: i64,
        execution: Option<&AiExecutionSnapshot>,
    ) -> Result<Option<String>> {
        self.queue_daily_analysis_scoped(
            date,
            start_ms,
            end_ms,
            ActivityScope::All,
            now_ms,
            execution,
        )
    }

    pub fn queue_daily_analysis_scoped(
        &self,
        date: &str,
        start_ms: i64,
        end_ms: i64,
        activity_scope: ActivityScope,
        now_ms: i64,
        execution: Option<&AiExecutionSnapshot>,
    ) -> Result<Option<String>> {
        let Some(execution) = execution else {
            return Ok(None);
        };
        let evidence =
            self.build_daily_analysis_evidence_scoped(date, start_ms, end_ms, activity_scope)?;
        let mut execution = execution.clone();
        execution.evidence_hash = evidence.evidence_hash.clone();
        execution.created_at_ms = now_ms;
        let mut payload =
            serde_json::to_value(&evidence).expect("daily analysis evidence serializes");
        payload["activityScope"] = serde_json::json!(activity_scope);
        let payload =
            serde_json::to_string(&payload).expect("daily analysis queue payload serializes");
        self.database
            .enqueue_ai_job_for_subject(
                "daily_analysis",
                &format!(
                    "{}:{}:{}",
                    date,
                    activity_scope_key(activity_scope),
                    evidence.evidence_hash
                ),
                &payload,
                now_ms,
                &execution,
            )
            .map(Some)
    }

    pub fn save_ai_daily_analysis_if_current(
        &self,
        date: &str,
        start_ms: i64,
        end_ms: i64,
        evidence_hash: &str,
        portrait: &str,
        recommendation: &str,
        findings: &[EvidenceBasedFinding],
        generated_at_ms: i64,
    ) -> Result<bool> {
        let current = self.build_daily_analysis_evidence(date, start_ms, end_ms)?;
        if current.evidence_hash != evidence_hash {
            let existing = self.database.get_daily_analysis(date)?;
            let can_replace_stale = existing.as_ref().is_none_or(|saved| {
                saved.evidence_hash != current.evidence_hash
                    && saved.generated_at_ms < generated_at_ms
            });
            if !can_replace_stale {
                return Ok(false);
            }
        }
        self.database.save_daily_analysis(&DailyAnalysisRecord {
            date: date.to_string(),
            evidence_hash: evidence_hash.to_string(),
            portrait: portrait.to_string(),
            recommendation: recommendation.to_string(),
            findings_json: serde_json::to_string(findings)
                .expect("evidence-based daily findings serialize"),
            protocol_version: 2,
            source: "ai".to_string(),
            generated_at_ms,
        })?;
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_ai_daily_analysis_job(
        &self,
        job_id: &str,
        generation: i64,
        date: &str,
        start_ms: i64,
        end_ms: i64,
        evidence_hash: &str,
        portrait: &str,
        recommendation: &str,
        findings: &[EvidenceBasedFinding],
        executor_id: &str,
        model: &str,
        exit_code: Option<i32>,
        generated_at_ms: i64,
    ) -> Result<bool> {
        self.complete_ai_daily_analysis_job_scoped(
            job_id,
            generation,
            date,
            start_ms,
            end_ms,
            ActivityScope::All,
            evidence_hash,
            portrait,
            recommendation,
            findings,
            executor_id,
            model,
            exit_code,
            generated_at_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_ai_daily_analysis_job_scoped(
        &self,
        job_id: &str,
        generation: i64,
        date: &str,
        start_ms: i64,
        end_ms: i64,
        activity_scope: ActivityScope,
        evidence_hash: &str,
        portrait: &str,
        recommendation: &str,
        findings: &[EvidenceBasedFinding],
        executor_id: &str,
        model: &str,
        exit_code: Option<i32>,
        generated_at_ms: i64,
    ) -> Result<bool> {
        let current =
            self.build_daily_analysis_evidence_scoped(date, start_ms, end_ms, activity_scope)?;
        if current.evidence_hash != evidence_hash {
            return self.database.complete_ai_job_generation_audit(
                job_id,
                generation,
                generated_at_ms,
                Some(executor_id),
                Some(model),
                exit_code,
            );
        }
        self.database.complete_daily_analysis_job_generation_scoped(
            job_id,
            generation,
            activity_scope,
            &DailyAnalysisRecord {
                date: date.to_string(),
                evidence_hash: evidence_hash.to_string(),
                portrait: portrait.to_string(),
                recommendation: recommendation.to_string(),
                findings_json: serde_json::to_string(findings)
                    .expect("evidence-based daily findings serialize"),
                protocol_version: 2,
                source: "ai".to_string(),
                generated_at_ms,
            },
            executor_id,
            model,
            exit_code,
        )
    }

    fn build_daily_analysis_evidence(
        &self,
        date: &str,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<DailyAnalysisEvidence> {
        self.build_daily_analysis_evidence_scoped(date, start_ms, end_ms, ActivityScope::All)
    }

    fn build_daily_analysis_evidence_scoped(
        &self,
        date: &str,
        start_ms: i64,
        end_ms: i64,
        activity_scope: ActivityScope,
    ) -> Result<DailyAnalysisEvidence> {
        let goal = self.database.get_daily_goal(date)?;
        let all_segments = self.database.list_segments(start_ms, end_ms)?;
        let linked_activity_ids = if activity_scope == ActivityScope::Meaningful {
            self.database
                .load_work_ledger_range_facts(start_ms, end_ms)?
                .activities
                .into_iter()
                .map(|fact| fact.segment.id)
                .collect::<BTreeSet<_>>()
        } else {
            BTreeSet::new()
        };
        let segments: Vec<_> = all_segments
            .into_iter()
            .filter(|segment| {
                activity_scope == ActivityScope::All
                    || activity_is_meaningful(
                        segment.category,
                        segment.video_purpose,
                        linked_activity_ids.contains(&segment.id),
                    )
            })
            .collect();
        let totals = if activity_scope == ActivityScope::All {
            self.database.dashboard_totals(start_ms, end_ms)?
        } else {
            let mut totals = DashboardTotals::default();
            for segment in &segments {
                let seconds = ((segment.ended_at_ms.min(end_ms)
                    - segment.started_at_ms.max(start_ms))
                    / 1_000)
                    .max(0);
                totals.monitored_seconds += seconds;
                totals.active_seconds += seconds;
                if counts_as_learning(segment.category, segment.video_purpose) {
                    totals.learning_seconds += seconds;
                }
                *totals
                    .category_seconds
                    .entry(category_key(segment.category).to_string())
                    .or_default() += seconds;
            }
            totals
        };
        let mut app_seconds = BTreeMap::<String, i64>::new();
        let mut longest_focus_seconds = 0_i64;
        let mut classified_seconds = 0_i64;
        for segment in &segments {
            let seconds = ((segment.ended_at_ms.min(end_ms) - segment.started_at_ms.max(start_ms))
                / 1_000)
                .max(0);
            if segment.category != ActivityCategory::Idle {
                *app_seconds.entry(segment.app.clone()).or_default() += seconds;
                longest_focus_seconds = longest_focus_seconds.max(seconds);
            }
            if segment.category != ActivityCategory::Pending {
                classified_seconds += seconds;
            }
        }
        let mut top_apps: Vec<_> = app_seconds
            .into_iter()
            .map(|(name, seconds)| DailyAnalysisApp { name, seconds })
            .collect();
        top_apps.sort_by(|left, right| {
            right
                .seconds
                .cmp(&left.seconds)
                .then_with(|| left.name.cmp(&right.name))
        });
        top_apps.truncate(5);
        let mut evidence = DailyAnalysisEvidence {
            date: date.to_string(),
            start_ms,
            end_ms,
            activity_scope,
            goals: goal.goals,
            expected_output: goal.expected_output,
            actual_output: goal.actual_output,
            monitored_seconds: totals.monitored_seconds,
            active_seconds: totals.active_seconds,
            learning_seconds: totals.learning_seconds,
            idle_seconds: totals.idle_seconds,
            switch_count: segments.len().saturating_sub(1) as i64,
            longest_focus_seconds,
            category_seconds: totals.category_seconds,
            top_apps,
            browser_visit_count: if activity_scope == ActivityScope::All {
                self.database.list_browser_visits(start_ms, end_ms)?.len() as i64
            } else {
                0
            },
            classification_coverage: if totals.monitored_seconds > 0 {
                classified_seconds as f64 / totals.monitored_seconds as f64
            } else {
                0.0
            },
            evidence_hash: String::new(),
        };
        let canonical = serde_json::to_vec(&evidence).expect("daily analysis evidence serializes");
        evidence.evidence_hash = format!("{:x}", Sha256::digest(canonical));
        Ok(evidence)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrendEvidenceHashInput<'a> {
    #[serde(skip_serializing_if = "activity_scope_is_all")]
    activity_scope: ActivityScope,
    range: &'a TrendRange,
    days: &'a [TrendDay],
    summary: &'a TrendSummary,
    comparison: &'a TrendComparison,
    quality: &'a TrendDataQuality,
    evidence_hash: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrendAnalysisQueuePayload<'a> {
    #[serde(skip_serializing_if = "activity_scope_is_all")]
    activity_scope: ActivityScope,
    evidence: TrendEvidenceHashInput<'a>,
    day_boundaries_ms: Vec<i64>,
    comparison_day_boundaries_ms: Vec<i64>,
}

const FOCUS_DAY_SECONDS: i64 = 25 * 60;

#[derive(Clone, Default)]
struct TrendDayAccumulator {
    monitored_seconds: i64,
    active_seconds: i64,
    idle_seconds: i64,
    learning_seconds: i64,
    classified_seconds: i64,
    pending_seconds: i64,
    low_confidence_seconds: i64,
    segment_count: usize,
    longest_focus_seconds: i64,
    category_seconds: BTreeMap<String, i64>,
    app_seconds: BTreeMap<String, i64>,
}

struct TrendAggregate {
    range: TrendRange,
    days: Vec<TrendDay>,
    summary: TrendSummary,
    quality: TrendDataQuality,
}

fn trend_range(start_date: &str, end_date: &str, boundaries_ms: &[i64]) -> Result<TrendRange> {
    if !(2..=367).contains(&boundaries_ms.len())
        || boundaries_ms
            .windows(2)
            .any(|window| window[0] >= window[1])
    {
        return Err(invalid_trend_range());
    }
    let start_date =
        NaiveDate::parse_from_str(start_date, "%Y-%m-%d").map_err(|_| invalid_trend_range())?;
    let end_date =
        NaiveDate::parse_from_str(end_date, "%Y-%m-%d").map_err(|_| invalid_trend_range())?;
    let day_count = (end_date - start_date).num_days() + 1;
    if !(1..=366).contains(&day_count) || day_count as usize + 1 != boundaries_ms.len() {
        return Err(invalid_trend_range());
    }
    Ok(TrendRange {
        start_ms: boundaries_ms[0],
        end_ms: *boundaries_ms.last().expect("validated boundary length"),
        start_date: start_date.format("%Y-%m-%d").to_string(),
        end_date: end_date.format("%Y-%m-%d").to_string(),
        day_count: day_count as usize,
    })
}

fn ranges_are_calendar_adjacent(previous: &TrendRange, current: &TrendRange) -> bool {
    let previous_end = NaiveDate::parse_from_str(&previous.end_date, "%Y-%m-%d").ok();
    let current_start = NaiveDate::parse_from_str(&current.start_date, "%Y-%m-%d").ok();
    previous_end.and_then(|date| date.succ_opt()) == current_start
}

fn aggregate_trend_range(
    range: TrendRange,
    segments: Vec<ActivitySegmentRecord>,
    boundaries_ms: &[i64],
) -> Result<TrendAggregate> {
    let first_date = NaiveDate::parse_from_str(&range.start_date, "%Y-%m-%d")
        .map_err(|_| invalid_trend_range())?;
    let mut accumulators = vec![TrendDayAccumulator::default(); range.day_count];

    for segment in segments {
        let mut cursor_ms = segment.started_at_ms;
        while cursor_ms < segment.ended_at_ms {
            let boundary_index = boundaries_ms
                .partition_point(|boundary| *boundary <= cursor_ms)
                .saturating_sub(1);
            let Some(next_boundary_ms) = boundaries_ms.get(boundary_index + 1) else {
                return Err(invalid_trend_range());
            };
            let piece_end_ms = segment.ended_at_ms.min(*next_boundary_ms);
            if piece_end_ms <= cursor_ms {
                return Err(invalid_trend_range());
            }
            let duration_seconds = (piece_end_ms - cursor_ms) / 1_000;
            if duration_seconds > 0 {
                let day = accumulators
                    .get_mut(boundary_index)
                    .ok_or_else(invalid_trend_range)?;
                day.monitored_seconds += duration_seconds;
                day.segment_count += 1;
                *day.category_seconds
                    .entry(category_key(segment.category).to_string())
                    .or_default() += duration_seconds;
                if segment.category == ActivityCategory::Idle {
                    day.idle_seconds += duration_seconds;
                } else {
                    day.active_seconds += duration_seconds;
                    day.longest_focus_seconds = day.longest_focus_seconds.max(duration_seconds);
                    *day.app_seconds.entry(segment.app.clone()).or_default() += duration_seconds;
                }
                if counts_as_learning(segment.category, segment.video_purpose) {
                    day.learning_seconds += duration_seconds;
                }
                if segment.category == ActivityCategory::Pending {
                    day.pending_seconds += duration_seconds;
                } else {
                    day.classified_seconds += duration_seconds;
                }
                if segment.confidence < 0.7 {
                    day.low_confidence_seconds += duration_seconds;
                }
            }
            cursor_ms = piece_end_ms;
        }
    }

    let mut category_seconds = BTreeMap::<String, i64>::new();
    let mut app_seconds = BTreeMap::<String, i64>::new();
    let total_monitored_seconds = accumulators
        .iter()
        .map(|day| day.monitored_seconds)
        .sum::<i64>();
    let total_active_seconds = accumulators
        .iter()
        .map(|day| day.active_seconds)
        .sum::<i64>();
    for day in &accumulators {
        merge_seconds(&mut category_seconds, &day.category_seconds);
        merge_seconds(&mut app_seconds, &day.app_seconds);
    }

    let days = accumulators
        .iter()
        .enumerate()
        .map(|(index, day)| {
            let date = first_date + Duration::days(index as i64);
            TrendDay {
                date: date.format("%Y-%m-%d").to_string(),
                label: format!("{}/{}", date.month(), date.day()),
                monitored_seconds: day.monitored_seconds,
                active_seconds: day.active_seconds,
                idle_seconds: day.idle_seconds,
                learning_seconds: day.learning_seconds,
                switch_count: day.segment_count.saturating_sub(1) as i64,
                longest_focus_seconds: day.longest_focus_seconds,
                completed_task_count: 0,
                classification_coverage: ratio(day.classified_seconds, day.monitored_seconds),
                top_category: breakdown(&day.category_seconds, day.monitored_seconds)
                    .into_iter()
                    .next(),
                top_app: breakdown(&day.app_seconds, day.active_seconds)
                    .into_iter()
                    .next(),
            }
        })
        .collect::<Vec<_>>();
    let day_count = range.day_count as f64;
    let summary = TrendSummary {
        monitored_seconds: days.iter().map(|day| day.monitored_seconds).sum(),
        active_seconds: days.iter().map(|day| day.active_seconds).sum(),
        idle_seconds: days.iter().map(|day| day.idle_seconds).sum(),
        learning_seconds: days.iter().map(|day| day.learning_seconds).sum(),
        switch_count: days.iter().map(|day| day.switch_count).sum(),
        longest_focus_seconds: days
            .iter()
            .map(|day| day.longest_focus_seconds)
            .max()
            .unwrap_or_default(),
        completed_task_count: 0,
        average_monitored_seconds: days.iter().map(|day| day.monitored_seconds).sum::<i64>() as f64
            / day_count,
        average_active_seconds: days.iter().map(|day| day.active_seconds).sum::<i64>() as f64
            / day_count,
        average_idle_seconds: days.iter().map(|day| day.idle_seconds).sum::<i64>() as f64
            / day_count,
        average_learning_seconds: days.iter().map(|day| day.learning_seconds).sum::<i64>() as f64
            / day_count,
        average_switch_count: days.iter().map(|day| day.switch_count).sum::<i64>() as f64
            / day_count,
        learning_ratio: ratio(
            days.iter().map(|day| day.learning_seconds).sum::<i64>(),
            days.iter().map(|day| day.active_seconds).sum::<i64>(),
        ),
        switches_per_active_hour: switches_per_active_hour(&days),
        productive_day_count: days.iter().filter(|day| day.active_seconds > 0).count(),
        focus_day_count: days
            .iter()
            .filter(|day| day.longest_focus_seconds >= FOCUS_DAY_SECONDS)
            .count(),
        category_breakdown: breakdown(&category_seconds, total_monitored_seconds),
        app_breakdown: breakdown(&app_seconds, total_active_seconds),
    };
    let classified_seconds = accumulators
        .iter()
        .map(|day| day.classified_seconds)
        .sum::<i64>();
    let quality = TrendDataQuality {
        recorded_day_count: days.iter().filter(|day| day.monitored_seconds > 0).count(),
        missing_day_count: days.iter().filter(|day| day.monitored_seconds == 0).count(),
        classified_seconds,
        pending_seconds: accumulators.iter().map(|day| day.pending_seconds).sum(),
        low_confidence_seconds: accumulators
            .iter()
            .map(|day| day.low_confidence_seconds)
            .sum(),
        classification_coverage: ratio(classified_seconds, total_monitored_seconds),
    };
    Ok(TrendAggregate {
        range,
        days,
        summary,
        quality,
    })
}

fn apply_completed_task_counts(
    aggregate: &mut TrendAggregate,
    completion_timestamps: Vec<i64>,
    boundaries_ms: &[i64],
) {
    for completed_at_ms in completion_timestamps {
        let day_index = boundaries_ms
            .partition_point(|boundary| *boundary <= completed_at_ms)
            .saturating_sub(1);
        if let Some(day) = aggregate.days.get_mut(day_index) {
            day.completed_task_count += 1;
        }
    }
    aggregate.summary.completed_task_count = aggregate
        .days
        .iter()
        .map(|day| day.completed_task_count)
        .sum();
}

fn breakdown(values: &BTreeMap<String, i64>, total_seconds: i64) -> Vec<TrendBreakdownItem> {
    let mut items = values
        .iter()
        .map(|(name, seconds)| TrendBreakdownItem {
            name: name.clone(),
            seconds: *seconds,
            share: ratio(*seconds, total_seconds),
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .seconds
            .cmp(&left.seconds)
            .then_with(|| left.name.cmp(&right.name))
    });
    items
}

fn merge_seconds(target: &mut BTreeMap<String, i64>, source: &BTreeMap<String, i64>) {
    for (name, seconds) in source {
        *target.entry(name.clone()).or_default() += seconds;
    }
}

fn ratio(part: i64, whole: i64) -> f64 {
    if whole <= 0 {
        0.0
    } else {
        part.max(0) as f64 / whole as f64
    }
}

fn percentage_delta(current: f64, previous: f64) -> Option<f64> {
    if previous == 0.0 {
        None
    } else {
        Some(((current - previous) / previous) * 100.0)
    }
}

fn switches_per_active_hour(days: &[TrendDay]) -> f64 {
    let active_seconds = days.iter().map(|day| day.active_seconds).sum::<i64>();
    if active_seconds <= 0 {
        0.0
    } else {
        days.iter().map(|day| day.switch_count).sum::<i64>() as f64
            / (active_seconds as f64 / 3_600.0)
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

fn invalid_trend_range() -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName("invalid trend range".into())
}

fn build_local_daily_analysis(evidence: &DailyAnalysisEvidence) -> DailyAnalysisResult {
    let learning_share = percentage(evidence.learning_seconds, evidence.active_seconds);
    let creation_share = percentage(
        evidence
            .category_seconds
            .get(category_key(ActivityCategory::CreationDevelopment))
            .copied()
            .unwrap_or_default(),
        evidence.active_seconds,
    );
    let switches_per_hour = if evidence.active_seconds > 0 {
        evidence.switch_count as f64 / (evidence.active_seconds as f64 / 3_600.0)
    } else {
        0.0
    };
    let goal = if evidence.goals.trim().is_empty() {
        "今日尚未填写明确目标。".to_string()
    } else {
        format!("今日目标是“{}”。", evidence.goals.trim())
    };
    let top_app = evidence.top_apps.first().map_or_else(
        || "应用记录仍不足。".to_string(),
        |app| {
            format!(
                "主要应用为 {}（{}）。",
                app.name,
                format_duration(app.seconds)
            )
        },
    );
    let portrait = format!(
        "{goal}共记录 {} 活跃时间，其中学习活动占 {learning_share}%，创作开发占 {creation_share}%。{top_app}",
        format_duration(evidence.active_seconds),
    );
    let mut recommendations = vec![if switches_per_hour >= 12.0 {
        format!("当前每小时约切换 {switches_per_hour:.1} 次，可安排一段连续任务以降低切换负荷")
    } else {
        format!("当前每小时约切换 {switches_per_hour:.1} 次，继续保留较长的连续任务段")
    }];
    if evidence.classification_coverage < 0.8 {
        recommendations.push(format!(
            "分类覆盖率为 {}%，建议优先复核低置信记录",
            (evidence.classification_coverage * 100.0).round() as i64
        ));
    }
    if evidence.actual_output.trim().is_empty() {
        recommendations.push("补写一条可核验的实际产出，使目标与时间投入可以对照".to_string());
    } else {
        recommendations.push(format!(
            "已记录实际产出“{}”，可继续核对它与目标的完成程度",
            evidence.actual_output.trim()
        ));
    }
    DailyAnalysisResult {
        portrait,
        recommendation: format!("{}。", recommendations.join("；")),
        findings: vec![
            EvidenceBasedFinding {
                observation: format!(
                    "今日记录 {} 活跃时间，学习活动占 {learning_share}%，创作开发占 {creation_share}%。",
                    format_duration(evidence.active_seconds)
                ),
                hypothesis: if evidence.active_seconds == 0 {
                    "可能是当天尚未形成可分析的活跃记录，而不是没有投入。".to_string()
                } else {
                    "这些构成可能反映了今天在输入学习与产出活动之间的实际分配。".to_string()
                },
                validation: "与前 7 天及同星期的活动构成比较，并核对当天目标与实际产出。"
                    .to_string(),
                action: "明天保留一个目标明确的连续任务段，并在结束时记录可核验产出。".to_string(),
                evidence_ids: vec![
                    "metric:active_seconds".to_string(),
                    "metric:learning_seconds".to_string(),
                    "metric:creation_seconds".to_string(),
                ],
                limitations: vec![
                    "本地统计只能说明时间构成，不能单独证明原因或产出质量。".to_string(),
                ],
                confidence: if evidence.active_seconds > 0 {
                    0.78
                } else {
                    0.35
                },
            },
            EvidenceBasedFinding {
                observation: format!(
                    "活跃期间每小时约切换 {switches_per_hour:.1} 次，分类覆盖率为 {}%。",
                    (evidence.classification_coverage * 100.0).round() as i64
                ),
                hypothesis: if switches_per_hour >= 12.0 {
                    "较高的切换频率可能与任务被打断或并行查找资料有关。".to_string()
                } else {
                    "当前切换频率可能允许形成较稳定的连续工作片段。".to_string()
                },
                validation:
                    "选择一个相似任务做 45 分钟单任务实验，比较切换次数、完成产出和主观阻力。"
                        .to_string(),
                action: if switches_per_hour >= 12.0 {
                    "下一次工作前关闭无关窗口，并把临时查找项记录到待办而非立即切换。".to_string()
                } else {
                    "继续保留较长任务段，并记录任务结束时是否产生预期产出。".to_string()
                },
                evidence_ids: vec![
                    "metric:switch_count".to_string(),
                    "metric:classification_coverage".to_string(),
                ],
                limitations: vec![
                    "窗口切换可能是任务本身需要，不能直接等同于注意力分散。".to_string(),
                ],
                confidence: if evidence.classification_coverage >= 0.8 {
                    0.72
                } else {
                    0.52
                },
            },
        ],
        protocol_version: 2,
        source: "local".to_string(),
        evidence_hash: evidence.evidence_hash.clone(),
        generated_at_ms: 0,
        activity_scope: evidence.activity_scope,
    }
}

fn trend_analysis_result(
    record: TrendAnalysisRecord,
    activity_scope: ActivityScope,
) -> TrendAnalysisResult {
    TrendAnalysisResult {
        range_start: record.range_start,
        range_end: record.range_end,
        evidence_hash: record.evidence_hash,
        summary: record.summary,
        observations: record.observations,
        suggestions: record.suggestions,
        source: record.source,
        model: record.model,
        confidence: record.confidence,
        generated_at_ms: record.generated_at_ms,
        activity_scope,
    }
}

pub fn trend_analysis_allowed_candidates(
    evidence: &TrendPayload,
) -> TrendAnalysisAllowedCandidates {
    const CHANGE_THRESHOLD_PERCENT: f64 = 8.0;

    let summary = if evidence.quality.recorded_day_count == 0 {
        "本区间记录显示可用活动证据有限"
    } else {
        "本区间记录显示活动分布可供复核"
    };
    let mut observations = vec!["记录覆盖情况可继续复核".to_string()];
    if evidence
        .comparison
        .active_seconds_delta_percent
        .is_some_and(|delta| delta.is_finite() && delta.abs() >= CHANGE_THRESHOLD_PERCENT)
    {
        observations.push("活动记录总量有所变化".to_string());
    } else if evidence.summary.active_seconds > 0 {
        observations.push("当前区间存在有效活动记录".to_string());
    }

    if evidence.comparison.previous_category_breakdown.is_empty() {
        if !evidence.summary.category_breakdown.is_empty() {
            observations.push("当前区间分类结构已有记录".to_string());
        }
    } else if trend_breakdowns_differ(
        &evidence.summary.category_breakdown,
        &evidence.comparison.previous_category_breakdown,
    ) {
        observations.push("分类差异有所变化".to_string());
    }

    if evidence.comparison.previous_app_breakdown.is_empty() {
        if !evidence.summary.app_breakdown.is_empty() {
            observations.push("当前区间应用结构已有记录".to_string());
        }
    } else if trend_breakdowns_differ(
        &evidence.summary.app_breakdown,
        &evidence.comparison.previous_app_breakdown,
    ) {
        observations.push("应用分布有所变化".to_string());
    }

    let mut suggestions = vec![
        "建议尝试固定一段连续任务".to_string(),
        "下一周期可继续观察活动分布".to_string(),
    ];
    if evidence.quality.classification_coverage < 0.9 {
        suggestions.push("建议尝试优先复核分类记录".to_string());
    }

    TrendAnalysisAllowedCandidates {
        summaries: vec![summary.to_string()],
        observations,
        suggestions,
    }
}

fn trend_breakdowns_differ(
    current: &[TrendBreakdownItem],
    previous: &[TrendBreakdownItem],
) -> bool {
    const SHARE_EPSILON: f64 = 1e-9;

    let mut by_name = BTreeMap::new();
    for item in current {
        by_name.entry(item.name.as_str()).or_insert((None, None)).0 = Some(item);
    }
    for item in previous {
        by_name.entry(item.name.as_str()).or_insert((None, None)).1 = Some(item);
    }

    by_name
        .values()
        .any(|(current, previous)| match (current, previous) {
            (Some(current), Some(previous)) => {
                current.seconds != previous.seconds
                    || (current.share - previous.share).abs() > SHARE_EPSILON
            }
            _ => true,
        })
}

fn build_local_trend_analysis_scoped(
    evidence: &TrendPayload,
    generated_at_ms: i64,
    activity_scope: ActivityScope,
) -> TrendAnalysisResult {
    let allowed = trend_analysis_allowed_candidates(evidence);
    TrendAnalysisResult {
        range_start: evidence.range.start_date.clone(),
        range_end: evidence.range.end_date.clone(),
        evidence_hash: evidence.evidence_hash.clone(),
        summary: allowed.summaries[0].clone(),
        observations: allowed.observations,
        suggestions: allowed.suggestions,
        source: "local".to_string(),
        model: "deterministic-v1".to_string(),
        confidence: evidence.quality.classification_coverage.clamp(0.0, 1.0),
        generated_at_ms,
        activity_scope,
    }
}

pub fn render_daily_markdown(date: &str, dashboard: &DashboardSnapshot) -> String {
    let totals = &dashboard.totals;
    let mut output = format!(
        "---\ndate: {date}\ntype: daily-review\ntags:\n  - daily-task-monitor\n---\n\n# {date} 每日复盘\n\n> [!summary] 今日概览\n> 总监测 **{} 分钟** · 活跃 **{} 分钟** · 学习 **{} 分钟** · 不活跃 **{} 分钟**\n\n## 时间结构\n\n| 指标 | 时长 |\n| --- | ---: |\n| 总监测 | {} 分钟 |\n| 活跃 | {} 分钟 |\n| 学习 | {} 分钟 |\n| 不活跃 | {} 分钟 |\n",
        totals.monitored_seconds / 60,
        totals.active_seconds / 60,
        totals.learning_seconds / 60,
        totals.idle_seconds / 60,
        totals.monitored_seconds / 60,
        totals.active_seconds / 60,
        totals.learning_seconds / 60,
        totals.idle_seconds / 60,
    );
    push_work_ledger_markdown(&mut output, &dashboard.work_ledger);
    output.push_str("\n## 今日活动\n\n");
    for segment in dashboard.timeline.iter().take(30) {
        output.push_str(&format!(
            "- **{}** · {} · {}\n",
            markdown_table_text(&segment.app),
            markdown_table_text(&segment.title),
            format_ledger_duration((segment.ended_at_ms - segment.started_at_ms).max(0) / 1_000),
        ));
    }
    output.push_str("\n## 今日复盘\n\n- 实际完成：\n- 保持的策略：\n- 明天调整：\n");
    output
}

pub fn render_trend_markdown(
    evidence: &TrendPayload,
    analysis: &TrendAnalysisResult,
    generated_at_ms: i64,
) -> String {
    let generated_at = DateTime::<Utc>::from_timestamp_millis(generated_at_ms)
        .unwrap_or(DateTime::UNIX_EPOCH)
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    let mut output = format!(
        "---\ntype: trend-analysis\nrange_start: \"{}\"\nrange_end: \"{}\"\ngenerated_at: \"{}\"\nevidence_hash: \"{}\"\nconfidence: {:.3}\n---\n\n# 趋势分析 {} 至 {}\n\n> [!summary] 趋势摘要\n> {}\n\n> [!info] 数据质量\n> 记录日 {} 天，缺失日 {} 天，分类覆盖率 {:.1}%，待分类 {}，低置信度 {}。\n\n",
        evidence.range.start_date,
        evidence.range.end_date,
        generated_at,
        evidence.evidence_hash,
        analysis.confidence,
        evidence.range.start_date,
        evidence.range.end_date,
        markdown_callout_text(&analysis.summary),
        evidence.quality.recorded_day_count,
        evidence.quality.missing_day_count,
        evidence.quality.classification_coverage * 100.0,
        format_duration(evidence.quality.pending_seconds),
        format_duration(evidence.quality.low_confidence_seconds),
    );

    output.push_str(
        "## KPI\n\n| 指标 | 当前周期 | 上一周期 | 变化 |\n| --- | ---: | ---: | ---: |\n",
    );
    push_kpi_row(
        &mut output,
        "活跃时间",
        &format_duration(evidence.summary.active_seconds),
        &format_duration(evidence.comparison.previous_active_seconds),
        evidence.comparison.active_seconds_delta_percent,
    );
    push_kpi_row(
        &mut output,
        "学习时间",
        &format_duration(evidence.summary.learning_seconds),
        &format_duration(evidence.comparison.previous_learning_seconds),
        evidence.comparison.learning_seconds_delta_percent,
    );
    push_kpi_row(
        &mut output,
        "不活跃时间",
        &format_duration(evidence.summary.idle_seconds),
        &format_duration(evidence.comparison.previous_idle_seconds),
        evidence.comparison.idle_seconds_delta_percent,
    );
    push_kpi_row(
        &mut output,
        "切换次数",
        &evidence.summary.switch_count.to_string(),
        &evidence.comparison.previous_switch_count.to_string(),
        evidence.comparison.switch_count_delta_percent,
    );

    output.push_str("\n## 每日数据\n\n| 日期 | 活跃 | 学习 | 不活跃 | 切换 | 最长专注 |\n| --- | ---: | ---: | ---: | ---: | ---: |\n");
    for day in &evidence.days {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            markdown_table_text(&day.date),
            format_duration(day.active_seconds),
            format_duration(day.learning_seconds),
            format_duration(day.idle_seconds),
            day.switch_count,
            format_duration(day.longest_focus_seconds),
        ));
    }

    push_work_ledger_markdown(&mut output, &evidence.work_ledger);

    push_breakdown_section(
        &mut output,
        "分类变化",
        &evidence.summary.category_breakdown,
        &evidence.comparison.previous_category_breakdown,
    );
    push_breakdown_section(
        &mut output,
        "应用变化",
        &evidence.summary.app_breakdown,
        &evidence.comparison.previous_app_breakdown,
    );
    push_text_section(&mut output, "观察", &analysis.observations);
    push_text_section(&mut output, "建议", &analysis.suggestions);
    output
}

pub fn render_trend_markdown_with_workbench(
    evidence: &TrendPayload,
    analysis: &TrendAnalysisResult,
    workbench: &TrendWorkbenchPayload,
    generated_at_ms: i64,
) -> String {
    if workbench.range.selection_mode == TrendSelectionMode::SelectedDates {
        return render_sparse_trend_markdown(workbench, generated_at_ms);
    }
    let mut output = render_trend_markdown(evidence, analysis, generated_at_ms);
    let granularity = match workbench.granularity {
        TrendGranularity::Day => "日",
        TrendGranularity::Week => "周",
        TrendGranularity::Month => "月",
    };
    let metric = trend_metric_markdown_label(workbench.metric);

    output.push_str(&format!(
        "\n## 粒度\n\n| 配置 | 值 |\n| --- | --- |\n| 当前粒度 | {granularity} |\n| 当前指标 | {metric} |\n"
    ));
    output.push_str(
        "\n## 比较基准\n\n| 基准 | 区间 | 有效 / 缺失天 | 指标值 |\n| --- | --- | ---: | ---: |\n",
    );
    for baseline in &workbench.baselines {
        let label = match baseline.kind {
            TrendBaselineKind::Current => "当前区间",
            TrendBaselineKind::PreviousEqualLength => "上一等长区间",
            TrendBaselineKind::PreviousMonthSamePeriod => "上月同期",
            TrendBaselineKind::Custom => "自定义基准",
        };
        let value = baseline
            .value
            .filter(|_| baseline.is_valid)
            .map(|value| format_trend_metric_markdown_value(workbench.metric, value))
            .unwrap_or_else(|| "无有效采样".to_string());
        output.push_str(&format!(
            "| {label} | {} 至 {} | {} / {} | {value} |\n",
            markdown_table_text(&baseline.range.start_date),
            markdown_table_text(&baseline.range.end_date),
            baseline.recorded_day_count,
            baseline.missing_day_count,
        ));
    }

    output.push_str(&format!(
        "\n## 任务统计\n\n| 指标 | 值 |\n| --- | ---: |\n| 已完成任务 | {} |\n| 关联任务时长 | {} |\n",
        workbench.summary.totals.completed_task_count,
        format_duration(workbench.summary.totals.linked_task_seconds),
    ));
    append_trend_overview_markdown(&mut output, workbench);
    output.push_str(&format!(
        "\n## 数据质量\n\n| 指标 | 值 |\n| --- | ---: |\n| 记录日 | {} |\n| 有效活动日 | {} |\n| 缺失日 | {} |\n| 分类覆盖率 | {:.1}% |\n| 待分类 | {} |\n| 低置信度 | {} |\n",
        workbench.summary.recorded_day_count,
        workbench.summary.effective_activity_day_count,
        workbench.summary.missing_day_count,
        workbench.summary.classification_coverage * 100.0,
        format_duration(workbench.summary.pending_seconds),
        format_duration(workbench.summary.low_confidence_seconds),
    ));
    output.push_str(&format!(
        "\n## Evidence ID\n\n- 旧版趋势证据：`{}`\n- 工作台证据：`{}`\n",
        markdown_table_text(&evidence.evidence_hash),
        markdown_table_text(&workbench.evidence_hash),
    ));
    if workbench.summary.evidence_ids.is_empty() {
        output.push_str("- 原始证据：无\n");
    } else {
        output.push_str("- 原始证据：");
        for (index, evidence_id) in workbench.summary.evidence_ids.iter().enumerate() {
            if index > 0 {
                output.push_str("、");
            }
            output.push('`');
            output.push_str(&markdown_table_text(evidence_id));
            output.push('`');
        }
        output.push('\n');
    }
    output
}

fn render_sparse_trend_markdown(workbench: &TrendWorkbenchPayload, generated_at_ms: i64) -> String {
    let generated_at = DateTime::<Utc>::from_timestamp_millis(generated_at_ms)
        .unwrap_or(DateTime::UNIX_EPOCH)
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    let selected_dates = format!(
        "[{}]",
        workbench
            .range
            .selected_dates
            .iter()
            .map(|date| serde_json::to_string(date)
                .expect("normalized selected dates always serialize"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut output = format!(
        "---\ntype: trend-analysis\nrange_start: \"{}\"\nrange_end: \"{}\"\nselection_mode: selectedDates\nselected_dates: {}\nselected_date_count: {}\nenvelope_day_count: {}\ngenerated_at: \"{}\"\nevidence_hash: \"{}\"\n---\n\n# Sparse trend analysis {} to {}\n\n## Summary\n\n| Metric | Value |\n| --- | ---: |\n| Monitored seconds | {} |\n| Active seconds | {} |\n| Learning seconds | {} |\n| Idle seconds | {} |\n| Completed tasks | {} |\n| Linked task seconds | {} |\n| Recorded selected dates | {} |\n| Missing selected dates | {} |\n\n## Selected-date buckets\n\n| Start | End | Selected dates | Value | Recorded / missing |\n| --- | --- | --- | ---: | ---: |\n",
        workbench.range.start_date,
        workbench.range.end_date,
        selected_dates,
        workbench.range.selected_date_count,
        workbench.range.envelope_day_count,
        generated_at,
        workbench.evidence_hash,
        workbench.range.start_date,
        workbench.range.end_date,
        workbench.summary.totals.monitored_seconds,
        workbench.summary.totals.active_seconds,
        workbench.summary.totals.learning_seconds,
        workbench.summary.totals.idle_seconds,
        workbench.summary.totals.completed_task_count,
        workbench.summary.totals.linked_task_seconds,
        workbench.summary.recorded_day_count,
        workbench.summary.missing_day_count,
    );
    append_trend_overview_markdown(&mut output, workbench);
    for bucket in &workbench.buckets {
        let dates = bucket.selected_dates.join(", ");
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} / {} |\n",
            markdown_table_text(&bucket.start_date),
            markdown_table_text(&bucket.end_date),
            markdown_table_text(&dates),
            format_trend_metric_markdown_value(
                workbench.metric,
                bucket.values.get(workbench.metric),
            ),
            bucket.recorded_day_count,
            bucket.missing_day_count,
        ));
    }
    output.push_str(
        "\n## Baselines\n\n| Baseline | Envelope | Selected dates | Value | Recorded / missing |\n| --- | --- | --- | ---: | ---: |\n",
    );
    for baseline in &workbench.baselines {
        let dates = if baseline.selection_mode == TrendSelectionMode::SelectedDates {
            baseline.selected_dates.join(", ")
        } else {
            "continuous".into()
        };
        let value = baseline
            .value
            .filter(|_| baseline.is_valid)
            .map(|value| format_trend_metric_markdown_value(workbench.metric, value))
            .unwrap_or_else(|| "unavailable".into());
        output.push_str(&format!(
            "| {:?} | {} to {} | {} | {} | {} / {} |\n",
            baseline.kind,
            markdown_table_text(&baseline.range.start_date),
            markdown_table_text(&baseline.range.end_date),
            markdown_table_text(&dates),
            value,
            baseline.recorded_day_count,
            baseline.missing_day_count,
        ));
    }
    output
}

fn append_trend_overview_markdown(output: &mut String, workbench: &TrendWorkbenchPayload) {
    let average = &workbench.summary.daily_average;
    let rate = workbench
        .summary
        .switches_per_active_hour
        .map(|value| format!("{value:.1} 次/活跃小时"))
        .unwrap_or_else(|| "无活跃记录".to_string());
    let daily = |value: Option<f64>| {
        value
            .map(|value| format_duration(value.round() as i64))
            .unwrap_or_else(|| "无有效采样".to_string())
    };
    output.push_str(&format!(
        "\n## 区间总览\n\n| 指标 | 值 |\n| --- | ---: |\n| 总监测 | {} |\n| 总活跃 | {} |\n| 总不活跃 | {} |\n| 总学习 | {} |\n| 切换负荷 | {rate} |\n| 最长专注 | {} |\n\n## 每日平均\n\n基于 {} 个有效采样日。\n\n| 指标 | 值 |\n| --- | ---: |\n| 日均监测 | {} |\n| 日均活跃 | {} |\n| 日均不活跃 | {} |\n| 日均学习 | {} |\n",
        format_duration(workbench.summary.totals.monitored_seconds),
        format_duration(workbench.summary.totals.active_seconds),
        format_duration(workbench.summary.totals.idle_seconds),
        format_duration(workbench.summary.totals.learning_seconds),
        format_duration(workbench.summary.totals.longest_focus_seconds),
        workbench.summary.average_sample_day_count,
        daily(average.monitored_seconds),
        daily(average.active_seconds),
        daily(average.idle_seconds),
        daily(average.learning_seconds),
    ));
}

fn trend_metric_markdown_label(metric: TrendMetric) -> &'static str {
    match metric {
        TrendMetric::MonitoredSeconds => "监测时长",
        TrendMetric::ActiveSeconds => "活跃时长",
        TrendMetric::LearningSeconds => "学习时长",
        TrendMetric::IdleSeconds => "不活跃时长",
        TrendMetric::SwitchCount => "切换次数",
        TrendMetric::LongestFocusSeconds => "最长专注",
        TrendMetric::ClassificationCoverage => "分类覆盖率",
        TrendMetric::CompletedTaskCount => "已完成任务",
        TrendMetric::LinkedTaskSeconds => "关联任务时长",
    }
}

fn format_trend_metric_markdown_value(metric: TrendMetric, value: f64) -> String {
    match metric {
        TrendMetric::MonitoredSeconds
        | TrendMetric::ActiveSeconds
        | TrendMetric::LearningSeconds
        | TrendMetric::IdleSeconds
        | TrendMetric::LongestFocusSeconds
        | TrendMetric::LinkedTaskSeconds => format_duration(value.round() as i64),
        TrendMetric::ClassificationCoverage => format!("{:.1}%", value * 100.0),
        TrendMetric::SwitchCount | TrendMetric::CompletedTaskCount => {
            (value.round() as i64).to_string()
        }
    }
}

fn push_work_ledger_markdown(output: &mut String, rollup: &WorkLedgerRangeRollup) {
    output.push_str(
        "\n## 项目投入\n\n| 项目 | 状态 | 投入 | Focus | Activity | Browser | Progress |\n| --- | --- | ---: | ---: | ---: | ---: | ---: |\n",
    );
    if rollup.projects.is_empty() {
        output.push_str("| 无 | - | 0 秒 | 0 秒 | 0 | 0 | 0 |\n");
    } else {
        for project in &rollup.projects {
            output.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} |\n",
                markdown_table_text(&project.project_name),
                project.project_status.as_str(),
                format_ledger_duration(project.invested_seconds),
                format_ledger_duration(project.focus_seconds),
                project.activity_segment_count,
                project.browser_visit_count,
                project.progress_count,
            ));
        }
    }
    output.push_str(
        "\n## 任务投入\n\n| 任务 | 状态 | 投入 | Focus | Activity | Browser | Progress |\n| --- | --- | ---: | ---: | ---: | ---: | ---: |\n",
    );
    if rollup.tasks.is_empty() {
        output.push_str("| 无 | - | 0 秒 | 0 秒 | 0 | 0 | 0 |\n");
    } else {
        for task in &rollup.tasks {
            output.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} |\n",
                markdown_table_text(&task.task_title),
                task.task_status.as_str(),
                format_ledger_duration(task.invested_seconds),
                format_ledger_duration(task.focus_seconds),
                task.activity_segment_count,
                task.browser_visit_count,
                task.progress_count,
            ));
        }
    }
}

fn format_ledger_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds < 60 {
        format!("{seconds} 秒")
    } else {
        format_duration(seconds)
    }
}

fn push_kpi_row(
    output: &mut String,
    label: &str,
    current: &str,
    previous: &str,
    delta: Option<f64>,
) {
    let delta = delta
        .map(|value| format!("{value:+.1}%"))
        .unwrap_or_else(|| "无可比基数".to_string());
    output.push_str(&format!(
        "| {label} | {} | {} | {delta} |\n",
        markdown_table_text(current),
        markdown_table_text(previous),
    ));
}

fn push_breakdown_section(
    output: &mut String,
    heading: &str,
    current: &[TrendBreakdownItem],
    previous: &[TrendBreakdownItem],
) {
    let mut rows = BTreeMap::<String, (i64, i64)>::new();
    for item in current {
        rows.entry(item.name.clone()).or_default().0 = item.seconds;
    }
    for item in previous {
        rows.entry(item.name.clone()).or_default().1 = item.seconds;
    }
    output.push_str(&format!(
        "\n## {heading}\n\n| 项目 | 当前周期 | 上一周期 |\n| --- | ---: | ---: |\n"
    ));
    if rows.is_empty() {
        output.push_str("| 暂无数据 | 0分钟 | 0分钟 |\n");
    } else {
        for (name, (current_seconds, previous_seconds)) in rows {
            output.push_str(&format!(
                "| {} | {} | {} |\n",
                markdown_table_text(&name),
                format_duration(current_seconds),
                format_duration(previous_seconds),
            ));
        }
    }
}

fn push_text_section(output: &mut String, heading: &str, items: &[String]) {
    output.push_str(&format!("\n## {heading}\n\n"));
    for item in items {
        output.push_str(&format!("- {}\n", item.replace(['\r', '\n'], " ").trim()));
    }
}

fn markdown_callout_text(value: &str) -> String {
    value.replace('\r', "").replace('\n', "\n> ")
}

fn markdown_table_text(value: &str) -> String {
    value.replace(['\r', '\n'], " ").replace('|', "\\|")
}

fn percentage(part: i64, whole: i64) -> i64 {
    if whole <= 0 {
        0
    } else {
        ((part.max(0) as f64 / whole as f64) * 100.0).round() as i64
    }
}

fn format_duration(seconds: i64) -> String {
    let minutes = ((seconds.max(0) as f64) / 60.0).round() as i64;
    if minutes >= 60 {
        format!("{} 小时 {} 分钟", minutes / 60, minutes % 60)
    } else {
        format!("{minutes} 分钟")
    }
}

fn normalize_exclusions(values: Vec<String>) -> Vec<String> {
    let mut values: Vec<String> = values
        .into_iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect();
    values.sort();
    values.dedup();
    values
}
