use std::{
    collections::{HashMap, HashSet},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ai::{
    AiExecutionErrorKind, AiExecutionSnapshot, AiJob, WorkLedgerDecisionKind,
    parse_work_ledger_decision_response,
};
use crate::ai_review::{AiReviewFilter, AiReviewKind, AiReviewState};
use crate::classifier::{AiDisposition, decide_ai_disposition};
use crate::db::{ActivitySegmentRecord, BrowserVisitRecord};

use super::domain::{
    AiTaskCancellation, AiWorkLedgerSuggestion, ConfirmedDailyGoalTask, DailyGoalTaskLink,
    EvidenceLink, EvidenceProvenance, NewProgressEntry, NewProject, NewTask, ProgressEntry,
    Project, ProjectDraftProposal, ProjectDraftTaskProposal, ProjectDraftView, ProjectTimeInsight,
    ProjectUpdate, Task, TaskStatus, TaskTimeInsight, TaskTimeSummary, TaskUpdate,
    WorkLedgerRangeRollup,
};
use super::repository::WorkLedgerRepository;

const LOCAL_AUTO_APPLY_CONFIDENCE: f64 = 0.85;

fn is_auto_applicable(confidence: f64) -> bool {
    confidence.is_finite() && confidence >= LOCAL_AUTO_APPLY_CONFIDENCE
}

#[cfg(test)]
mod tests {
    use super::{WorkLedgerEvidence, group_work_episodes, is_auto_applicable};

    #[test]
    fn confidence_at_the_auto_apply_threshold_is_accepted() {
        assert!(is_auto_applicable(0.85));
        assert!(!is_auto_applicable(0.849_999));
    }

    fn evidence(
        id: &str,
        occurred_at_ms: i64,
        duration_seconds: i64,
        kind: &str,
    ) -> WorkLedgerEvidence {
        WorkLedgerEvidence {
            kind: kind.into(),
            id: id.into(),
            occurred_at_ms,
            duration_seconds,
            application: "Codex".into(),
            title: "实现任务洞察".into(),
            domain: String::new(),
            category: "creation_development".into(),
            workflow_eligibility: "initiator".into(),
            eligibility_reason: "development activity".into(),
            classification_source: "rule".into(),
            classification_reason: String::new(),
            classification_confidence: Some(0.9),
            evidence_hash: format!("hash-{id}"),
        }
    }

    #[test]
    fn work_episode_groups_a_ten_minute_gap_and_does_not_count_browser_as_time() {
        let episodes = group_work_episodes(&[
            evidence("a", 0, 600, "activity"),
            evidence("browser", 1_000_000, 0, "browser"),
            evidence("b", 1_601_000, 300, "activity"),
        ]);
        assert_eq!(episodes.len(), 2);
        assert_eq!(episodes[0].fragments.len(), 2);
        assert_eq!(episodes[0].duration_seconds, 600);
        assert_eq!(episodes[1].duration_seconds, 300);
        assert!(episodes[0].episode_key.starts_with("episode-"));
        assert_eq!(episodes[0].cluster_key, episodes[1].cluster_key);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkLedgerEvidence {
    pub kind: String,
    pub id: String,
    pub occurred_at_ms: i64,
    pub duration_seconds: i64,
    pub application: String,
    pub title: String,
    pub domain: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub workflow_eligibility: String,
    #[serde(default)]
    pub eligibility_reason: String,
    pub classification_source: String,
    pub classification_reason: String,
    pub classification_confidence: Option<f64>,
    pub evidence_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkLedgerAssignmentTaskCandidate {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub expected_output: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkLedgerCandidateCluster {
    pub cluster_id: String,
    pub episode_keys: Vec<String>,
    pub evidence_count: usize,
    pub duration_seconds: i64,
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkLedgerAssignmentJobPayload {
    #[serde(default = "legacy_assignment_protocol_version")]
    pub protocol_version: u8,
    pub evidence: WorkLedgerEvidence,
    pub evidence_hash: String,
    #[serde(default)]
    pub episode_key: Option<String>,
    #[serde(default)]
    pub cluster_key: Option<String>,
    #[serde(default)]
    pub fragments: Vec<WorkLedgerEvidence>,
    #[serde(default)]
    pub context_hints: Vec<String>,
    #[serde(default)]
    pub candidate_clusters: Vec<WorkLedgerCandidateCluster>,
    pub start_ms: i64,
    pub end_ms: i64,
    pub tasks: Vec<WorkLedgerAssignmentTaskCandidate>,
}

const fn legacy_assignment_protocol_version() -> u8 {
    1
}

pub fn parse_work_ledger_assignment_job(
    payload_json: &str,
) -> std::result::Result<WorkLedgerAssignmentJobPayload, String> {
    let payload: WorkLedgerAssignmentJobPayload =
        serde_json::from_str(payload_json).map_err(|error| error.to_string())?;
    let v2_valid = payload.protocol_version == 2
        && payload
            .episode_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty())
        && payload
            .cluster_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty())
        && !payload.fragments.is_empty()
        && payload.fragments.len() <= 64
        && payload.candidate_clusters.len() <= 12
        && payload.fragments[0].evidence_hash == payload.evidence_hash;
    if payload.end_ms <= payload.start_ms
        || payload.evidence_hash != payload.evidence.evidence_hash
        || !matches!(payload.evidence.kind.as_str(), "activity" | "browser")
        || !matches!(payload.protocol_version, 1 | 2)
        || (payload.protocol_version == 1 && payload.tasks.is_empty())
        || (payload.protocol_version == 2 && !v2_valid)
        || payload.tasks.iter().any(|task| task.id.trim().is_empty())
        || payload
            .fragments
            .iter()
            .any(|fragment| !matches!(fragment.kind.as_str(), "activity" | "browser"))
        || payload.candidate_clusters.iter().any(|cluster| {
            cluster.cluster_id.trim().is_empty()
                || cluster.episode_keys.is_empty()
                || cluster.evidence_count == 0
                || cluster.ended_at_ms < cluster.started_at_ms
        })
    {
        return Err("Queued work ledger assignment payload is invalid".into());
    }
    let mut task_ids = HashSet::new();
    if payload
        .tasks
        .iter()
        .any(|task| !task_ids.insert(task.id.as_str()))
    {
        return Err("Queued work ledger assignment candidates must be unique".into());
    }
    let mut cluster_ids = HashSet::new();
    if payload
        .candidate_clusters
        .iter()
        .any(|cluster| !cluster_ids.insert(cluster.cluster_id.as_str()))
    {
        return Err("Queued workflow candidate clusters must be unique".into());
    }
    Ok(payload)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkLedgerAssignmentConsumptionError {
    InvalidJob(String),
    InvalidResponse(String),
    Persistence(String),
}

impl WorkLedgerAssignmentConsumptionError {
    pub fn ai_error_kind(&self) -> AiExecutionErrorKind {
        match self {
            Self::InvalidJob(_) => AiExecutionErrorKind::InvalidJob,
            Self::InvalidResponse(_) => AiExecutionErrorKind::InvalidResponse,
            Self::Persistence(_) => AiExecutionErrorKind::Persistence,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedWorkLedgerEvidence {
    pub task_id: String,
    pub provenance: EvidenceProvenance,
    pub assignment_confidence: f64,
    pub assignment_reason: String,
    pub assigned_at_ms: i64,
    pub evidence: WorkLedgerEvidence,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkLedgerSuggestion {
    pub evidence_kind: String,
    pub evidence_id: String,
    pub task_id: String,
    pub confidence: f64,
    pub reason: String,
    pub evidence_hash: String,
    pub source: String,
    pub can_auto_apply: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkLedgerSummary {
    pub project_count: usize,
    pub task_count: usize,
    pub progress_count: usize,
    pub linked_evidence_count: usize,
    pub unassigned_evidence_count: usize,
    pub local_suggestion_count: usize,
    pub ambiguous_evidence_count: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowAnalysisQueueResult {
    pub queued_count: usize,
    pub reused_count: usize,
    pub job_ids: Vec<String>,
}

impl PartialEq<usize> for WorkflowAnalysisQueueResult {
    fn eq(&self, other: &usize) -> bool {
        self.queued_count == *other
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkLedgerSnapshot {
    pub projects: Vec<Project>,
    pub tasks: Vec<Task>,
    pub progress: Vec<ProgressEntry>,
    pub linked_evidence: Vec<LinkedWorkLedgerEvidence>,
    pub unassigned_evidence: Vec<WorkLedgerEvidence>,
    pub suggestions: Vec<WorkLedgerSuggestion>,
    pub ambiguous_evidence_hashes: Vec<String>,
    pub project_drafts: Vec<ProjectDraftView>,
    pub task_summaries: Vec<TaskTimeSummary>,
    pub summary: WorkLedgerSummary,
}

pub struct WorkLedgerService<'a> {
    repository: WorkLedgerRepository<'a>,
}

impl<'a> WorkLedgerService<'a> {
    pub fn new(repository: WorkLedgerRepository<'a>) -> Self {
        Self { repository }
    }

    pub fn create_project(&self, project: NewProject) -> Result<Project> {
        self.repository.create_project(project)
    }

    pub fn get_project(&self, id: &str) -> Result<Option<Project>> {
        self.repository.get_project(id)
    }

    pub fn list_projects(&self, include_archived: bool) -> Result<Vec<Project>> {
        self.repository.list_projects(include_archived)
    }

    pub fn update_project(
        &self,
        id: &str,
        update: ProjectUpdate,
        updated_at_ms: i64,
    ) -> Result<bool> {
        self.repository.update_project(id, update, updated_at_ms)
    }

    pub fn archive_project(&self, id: &str, archived_at_ms: i64) -> Result<bool> {
        self.repository.archive_project(id, archived_at_ms)
    }

    pub fn delete_project(&self, id: &str) -> Result<bool> {
        self.repository.delete_project(id)
    }

    pub fn create_task(&self, task: NewTask) -> Result<Task> {
        self.repository.create_task(task)
    }

    pub fn get_task(&self, id: &str) -> Result<Option<Task>> {
        self.repository.get_task(id)
    }

    pub fn list_tasks(&self, project_id: &str) -> Result<Vec<Task>> {
        self.repository.list_tasks(project_id)
    }

    pub fn update_task(&self, id: &str, update: TaskUpdate, updated_at_ms: i64) -> Result<bool> {
        self.repository.update_task(id, update, updated_at_ms)
    }

    pub fn transition_task_status(
        &self,
        id: &str,
        status: TaskStatus,
        updated_at_ms: i64,
    ) -> Result<Task> {
        self.repository
            .transition_task_status(id, status, updated_at_ms)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    }

    pub fn cancel_ai_task(&self, id: &str, cancelled_at_ms: i64) -> Result<AiTaskCancellation> {
        self.repository
            .database()
            .cancel_work_ledger_ai_task(id, cancelled_at_ms)
    }

    pub fn delete_task(&self, id: &str) -> Result<bool> {
        self.repository.delete_task(id)
    }

    pub fn assign_activity(
        &self,
        task_id: &str,
        activity_segment_id: &str,
        provenance: EvidenceProvenance,
        confidence: f64,
        reason: &str,
        created_at_ms: i64,
    ) -> Result<bool> {
        if self
            .repository
            .database()
            .work_ledger_evidence_is_excluded("activity", activity_segment_id)?
        {
            return Ok(false);
        }
        if provenance == EvidenceProvenance::Manual {
            let assigned = self.repository.database().assign_manual_workflow_review(
                task_id,
                "activity",
                activity_segment_id,
                confidence,
                reason,
                created_at_ms,
            )?;
            if assigned {
                adjust_manual_match_profile(
                    self.repository.database(),
                    task_id,
                    "activity",
                    activity_segment_id,
                    1.0,
                    created_at_ms,
                )?;
            }
            return Ok(assigned);
        }
        self.repository.assign_activity(
            task_id,
            activity_segment_id,
            provenance,
            confidence,
            reason,
            created_at_ms,
        )
    }

    pub fn remove_activity(&self, task_id: &str, activity_segment_id: &str) -> Result<bool> {
        let removed = self.repository.database().remove_manual_workflow_review(
            task_id,
            "activity",
            activity_segment_id,
        )?;
        if removed {
            adjust_manual_match_profile(
                self.repository.database(),
                task_id,
                "activity",
                activity_segment_id,
                -1.0,
                now_ms(),
            )?;
        }
        Ok(removed)
    }

    pub fn list_activity_links(&self, task_id: &str) -> Result<Vec<EvidenceLink>> {
        self.repository.list_activity_links(task_id)
    }

    pub fn assign_browser_visit(
        &self,
        task_id: &str,
        browser_visit_id: &str,
        provenance: EvidenceProvenance,
        confidence: f64,
        reason: &str,
        created_at_ms: i64,
    ) -> Result<bool> {
        if self
            .repository
            .database()
            .work_ledger_evidence_is_excluded("browser", browser_visit_id)?
        {
            return Ok(false);
        }
        if provenance == EvidenceProvenance::Manual {
            let assigned = self.repository.database().assign_manual_workflow_review(
                task_id,
                "browser",
                browser_visit_id,
                confidence,
                reason,
                created_at_ms,
            )?;
            if assigned {
                adjust_manual_match_profile(
                    self.repository.database(),
                    task_id,
                    "browser",
                    browser_visit_id,
                    1.0,
                    created_at_ms,
                )?;
            }
            return Ok(assigned);
        }
        self.repository.assign_browser_visit(
            task_id,
            browser_visit_id,
            provenance,
            confidence,
            reason,
            created_at_ms,
        )
    }

    pub fn remove_browser_visit(&self, task_id: &str, browser_visit_id: &str) -> Result<bool> {
        let removed = self.repository.database().remove_manual_workflow_review(
            task_id,
            "browser",
            browser_visit_id,
        )?;
        if removed {
            adjust_manual_match_profile(
                self.repository.database(),
                task_id,
                "browser",
                browser_visit_id,
                -1.0,
                now_ms(),
            )?;
        }
        Ok(removed)
    }

    pub fn list_browser_links(&self, task_id: &str) -> Result<Vec<EvidenceLink>> {
        self.repository.list_browser_links(task_id)
    }

    pub fn add_progress_entry(&self, entry: NewProgressEntry) -> Result<ProgressEntry> {
        self.repository.add_progress_entry(entry)
    }

    pub fn list_progress_entries(&self, task_id: &str) -> Result<Vec<ProgressEntry>> {
        self.repository.list_progress_entries(task_id)
    }

    pub fn delete_progress_entry(&self, id: &str) -> Result<bool> {
        self.repository.delete_progress_entry(id)
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
        self.repository.confirm_daily_goal_task(
            goal_row_id,
            goal_date,
            goal_text,
            task_id,
            new_task,
            confirmed_at_ms,
        )
    }

    pub fn list_daily_goal_task_links(&self, goal_date: &str) -> Result<Vec<DailyGoalTaskLink>> {
        self.repository.list_daily_goal_task_links(goal_date)
    }

    pub fn record_daily_actual_output_progress(
        &self,
        task_id: &str,
        date: &str,
        created_at_ms: i64,
    ) -> Result<Option<ProgressEntry>> {
        self.repository
            .record_daily_actual_output_progress(task_id, date, created_at_ms)
    }

    pub fn task_duration_seconds(&self, task_id: &str) -> Result<i64> {
        self.repository.task_duration_seconds(task_id)
    }

    pub fn project_duration_seconds(&self, project_id: &str) -> Result<i64> {
        self.repository.project_duration_seconds(project_id)
    }

    pub fn range_rollup(&self, start_ms: i64, end_ms: i64) -> Result<WorkLedgerRangeRollup> {
        self.repository.range_rollup(start_ms, end_ms)
    }

    pub fn task_time_insight(&self, task_id: &str, end_ms: i64) -> Result<TaskTimeInsight> {
        self.repository.task_time_insight(task_id, end_ms)
    }

    pub fn project_time_insight(
        &self,
        project_id: &str,
        end_ms: i64,
        range_days: i64,
    ) -> Result<ProjectTimeInsight> {
        self.repository
            .project_time_insight(project_id, end_ms, range_days)
    }

    pub fn merge_tasks(
        &self,
        source_task_id: &str,
        target_task_id: &str,
        updated_at_ms: i64,
    ) -> Result<bool> {
        self.repository
            .merge_tasks(source_task_id, target_task_id, updated_at_ms)
    }

    pub fn get_work_ledger(
        &self,
        start_ms: i64,
        end_ms: i64,
        project_id: Option<&str>,
    ) -> Result<WorkLedgerSnapshot> {
        self.get_work_ledger_with_ai_queue(start_ms, end_ms, project_id, None)
    }

    pub fn get_work_ledger_with_ai_queue(
        &self,
        start_ms: i64,
        end_ms: i64,
        project_id: Option<&str>,
        execution: Option<&AiExecutionSnapshot>,
    ) -> Result<WorkLedgerSnapshot> {
        self.build_work_ledger_with_ai_queue(start_ms, end_ms, project_id, execution)
            .map(|(snapshot, _)| snapshot)
    }

    pub fn queue_workflow_ai_suggestions(
        &self,
        _start_ms: i64,
        end_ms: i64,
        execution: &AiExecutionSnapshot,
    ) -> Result<WorkflowAnalysisQueueResult> {
        let analysis_start_ms = end_ms.saturating_sub(30 * 24 * 60 * 60 * 1_000);
        self.build_work_ledger_with_ai_queue(analysis_start_ms, end_ms, None, Some(execution))
            .map(|(_, result)| result)
    }

    fn build_work_ledger_with_ai_queue(
        &self,
        start_ms: i64,
        end_ms: i64,
        project_id: Option<&str>,
        execution: Option<&AiExecutionSnapshot>,
    ) -> Result<(WorkLedgerSnapshot, WorkflowAnalysisQueueResult)> {
        if end_ms <= start_ms {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let database = self.repository.database();
        database.cleanup_ineligible_workflow_links(now_ms())?;
        let mut all_projects = self.repository.list_projects(true)?;
        all_projects.sort_by(|left, right| left.id.cmp(&right.id));
        let visible_projects: Vec<_> = all_projects
            .iter()
            .filter(|project| project_id.is_none_or(|id| project.id == id))
            .filter(|project| {
                project.status != super::domain::ProjectStatus::Archived || project_id.is_some()
            })
            .cloned()
            .collect();

        let mut all_tasks = Vec::new();
        for project in &all_projects {
            all_tasks.extend(self.repository.list_tasks(&project.id)?);
        }
        all_tasks.sort_by(|left, right| left.id.cmp(&right.id));
        let visible_project_ids: HashSet<_> = visible_projects
            .iter()
            .map(|project| project.id.as_str())
            .collect();
        let visible_tasks: Vec<_> = all_tasks
            .iter()
            .filter(|task| visible_project_ids.contains(task.project_id.as_str()))
            .cloned()
            .collect();

        let mut evidence_by_key = HashMap::new();
        for segment in database.list_clipped_segments(start_ms, end_ms)? {
            let evidence = work_ledger_evidence_from_activity(segment);
            if !database.work_ledger_evidence_is_excluded(&evidence.kind, &evidence.id)? {
                evidence_by_key.insert(evidence_key(&evidence.kind, &evidence.id), evidence);
            }
        }
        for visit in database.list_browser_visits(start_ms, end_ms)? {
            let evidence = work_ledger_evidence_from_browser(visit);
            if !database.work_ledger_evidence_is_excluded(&evidence.kind, &evidence.id)? {
                evidence_by_key.insert(evidence_key(&evidence.kind, &evidence.id), evidence);
            }
        }

        let mut assigned_keys = HashSet::new();
        let mut manual_profiles: HashMap<String, HashSet<String>> = HashMap::new();
        let mut visible_links = Vec::new();
        let visible_task_ids: HashSet<_> =
            visible_tasks.iter().map(|task| task.id.as_str()).collect();
        for task in &all_tasks {
            for (kind, links) in [
                ("activity", self.repository.list_activity_links(&task.id)?),
                ("browser", self.repository.list_browser_links(&task.id)?),
            ] {
                for link in links {
                    let key = evidence_key(kind, &link.evidence_id);
                    assigned_keys.insert(key.clone());
                    let Some(evidence) = evidence_by_key.get(&key) else {
                        continue;
                    };
                    if link.provenance == EvidenceProvenance::Manual {
                        manual_profiles
                            .entry(task.id.clone())
                            .or_default()
                            .extend(evidence_tokens(evidence));
                    }
                    if visible_task_ids.contains(task.id.as_str()) {
                        visible_links.push(LinkedWorkLedgerEvidence {
                            task_id: link.task_id,
                            provenance: link.provenance,
                            assignment_confidence: link.confidence,
                            assignment_reason: link.reason,
                            assigned_at_ms: link.created_at_ms,
                            evidence: evidence.clone(),
                        });
                    }
                }
            }
            manual_profiles
                .entry(task.id.clone())
                .or_default()
                .extend(database.list_task_match_profile_keys(&task.id)?);
        }
        visible_links.sort_by(|left, right| {
            left.evidence
                .occurred_at_ms
                .cmp(&right.evidence.occurred_at_ms)
                .then_with(|| left.evidence.kind.cmp(&right.evidence.kind))
                .then_with(|| left.evidence.id.cmp(&right.evidence.id))
                .then_with(|| left.task_id.cmp(&right.task_id))
        });

        let mut progress = Vec::new();
        for task in &visible_tasks {
            progress.extend(self.repository.list_progress_entries(&task.id)?);
        }
        progress.sort_by(|left, right| {
            left.created_at_ms
                .cmp(&right.created_at_ms)
                .then_with(|| left.id.cmp(&right.id))
        });

        let mut unassigned_evidence: Vec<_> = evidence_by_key
            .into_iter()
            .filter_map(|(key, evidence)| (!assigned_keys.contains(&key)).then_some(evidence))
            .collect();
        unassigned_evidence.sort_by(|left, right| {
            left.occurred_at_ms
                .cmp(&right.occurred_at_ms)
                .then_with(|| left.kind.cmp(&right.kind))
                .then_with(|| left.id.cmp(&right.id))
        });

        let mut context_tokens = HashSet::new();
        for goal in database.list_daily_goals()? {
            context_tokens.extend(tokens(&format!(
                "{} {} {}",
                goal.goals, goal.expected_output, goal.actual_output
            )));
        }
        for focus in database.list_focus_sessions(start_ms, end_ms)? {
            context_tokens.extend(tokens(&format!("{} {}", focus.goal_text, focus.outcome)));
        }
        let initiators = unassigned_evidence
            .iter()
            .filter(|evidence| evidence.workflow_eligibility == "initiator")
            .cloned()
            .collect::<Vec<_>>();
        unassigned_evidence.retain(|evidence| {
            evidence.workflow_eligibility == "initiator"
                || (evidence.workflow_eligibility == "context"
                    && context_evidence_is_connected(evidence, &initiators, &context_tokens))
        });

        let (suggestions, ambiguous_evidence_hashes) = build_local_suggestions(
            &unassigned_evidence,
            &visible_tasks,
            &visible_projects,
            &manual_profiles,
            &context_tokens,
        );
        let mut suggestions = suggestions;
        let local_evidence: HashSet<_> = suggestions
            .iter()
            .map(|suggestion| evidence_key(&suggestion.evidence_kind, &suggestion.evidence_id))
            .collect();
        let visible_task_ids: HashSet<_> =
            visible_tasks.iter().map(|task| task.id.as_str()).collect();
        let unassigned_by_key: HashMap<_, _> = unassigned_evidence
            .iter()
            .map(|evidence| (evidence_key(&evidence.kind, &evidence.id), evidence))
            .collect();
        for suggestion in self.repository.list_ai_suggestions()? {
            let key = evidence_key(&suggestion.evidence_kind, &suggestion.evidence_id);
            let Some(evidence) = unassigned_by_key.get(&key) else {
                continue;
            };
            if local_evidence.contains(&key)
                || !visible_task_ids.contains(suggestion.task_id.as_str())
                || suggestion.evidence_hash != evidence.evidence_hash
            {
                continue;
            }
            suggestions.push(WorkLedgerSuggestion {
                evidence_kind: suggestion.evidence_kind,
                evidence_id: suggestion.evidence_id,
                task_id: suggestion.task_id,
                confidence: suggestion.confidence,
                reason: suggestion.reason,
                evidence_hash: suggestion.evidence_hash,
                source: "ai".into(),
                can_auto_apply: false,
            });
        }
        suggestions.sort_by(|left, right| {
            left.evidence_kind
                .cmp(&right.evidence_kind)
                .then_with(|| left.evidence_id.cmp(&right.evidence_id))
                .then_with(|| left.source.cmp(&right.source))
                .then_with(|| left.task_id.cmp(&right.task_id))
        });
        let queue_result = if let Some(execution) = execution {
            queue_ambiguous_evidence(
                database,
                &unassigned_evidence,
                &ambiguous_evidence_hashes,
                &visible_tasks,
                &context_tokens,
                start_ms,
                end_ms,
                execution,
            )?
        } else {
            WorkflowAnalysisQueueResult::default()
        };
        let task_summaries = visible_tasks
            .iter()
            .map(|task| {
                self.task_time_insight(&task.id, end_ms)
                    .map(|insight| insight.summary)
            })
            .collect::<Result<Vec<_>>>()?;
        let project_drafts = database
            .list_ai_reviews(&AiReviewFilter {
                states: vec![AiReviewState::Pending],
                kinds: vec![AiReviewKind::ProjectDraft],
                ..AiReviewFilter::default()
            })?
            .into_iter()
            .filter_map(|review| {
                let proposal =
                    serde_json::from_str::<ProjectDraftProposal>(&review.proposed_json).ok()?;
                if project_id
                    .is_some_and(|selected| proposal.target_project_id.as_deref() != Some(selected))
                {
                    return None;
                }
                let (evidence_count, accumulated_seconds) =
                    database.project_draft_metrics(&review.id).ok()?;
                Some(ProjectDraftView {
                    review_id: review.id,
                    subject_id: review.subject_id,
                    proposal,
                    evidence_count,
                    accumulated_seconds,
                    created_at_ms: review.created_at_ms,
                })
            })
            .collect();
        let summary = WorkLedgerSummary {
            project_count: visible_projects.len(),
            task_count: visible_tasks.len(),
            progress_count: progress.len(),
            linked_evidence_count: visible_links.len(),
            unassigned_evidence_count: unassigned_evidence.len(),
            local_suggestion_count: suggestions
                .iter()
                .filter(|suggestion| suggestion.source == "local")
                .count(),
            ambiguous_evidence_count: ambiguous_evidence_hashes.len(),
        };

        Ok((
            WorkLedgerSnapshot {
                projects: visible_projects,
                tasks: visible_tasks,
                progress,
                linked_evidence: visible_links,
                unassigned_evidence,
                suggestions,
                ambiguous_evidence_hashes,
                project_drafts,
                task_summaries,
                summary,
            },
            queue_result,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_suggested_assignment(
        &self,
        task_id: &str,
        evidence_kind: &str,
        evidence_id: &str,
        evidence_hash: &str,
        source: &str,
        start_ms: i64,
        end_ms: i64,
        _confidence: f64,
        _reason: &str,
        created_at_ms: i64,
    ) -> Result<bool> {
        let snapshot = self.get_work_ledger(start_ms, end_ms, None)?;
        let Some(suggestion) = snapshot.suggestions.iter().find(|suggestion| {
            suggestion.source == source
                && suggestion.task_id == task_id
                && suggestion.evidence_kind == evidence_kind
                && suggestion.evidence_id == evidence_id
                && suggestion.evidence_hash == evidence_hash
        }) else {
            return Ok(false);
        };
        if suggestion.source == "local"
            && (!suggestion.can_auto_apply || !is_auto_applicable(suggestion.confidence))
        {
            return Ok(false);
        }
        if suggestion.source == "ai" {
            return self
                .repository
                .database()
                .accept_pending_work_ledger_ai_suggestion(
                    task_id,
                    evidence_kind,
                    evidence_id,
                    evidence_hash,
                    created_at_ms,
                );
        }
        match evidence_kind {
            "activity" => self.assign_activity(
                task_id,
                evidence_id,
                EvidenceProvenance::Rule,
                suggestion.confidence,
                &suggestion.reason,
                created_at_ms,
            ),
            "browser" => self.assign_browser_visit(
                task_id,
                evidence_id,
                EvidenceProvenance::Rule,
                suggestion.confidence,
                &suggestion.reason,
                created_at_ms,
            ),
            _ => Ok(false),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_ai_assignment_suggestion_if_current(
        &self,
        job_id: &str,
        generation: i64,
        evidence_kind: &str,
        evidence_id: &str,
        evidence_hash: &str,
        task_id: &str,
        confidence: f64,
        provider_id: &str,
        model: &str,
        start_ms: i64,
        end_ms: i64,
        finished_at_ms: i64,
        duration_ms: i64,
        force_review: bool,
    ) -> Result<bool> {
        if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) {
            return Ok(false);
        }
        let snapshot = self.get_work_ledger(start_ms, end_ms, None)?;
        let current = snapshot.unassigned_evidence.iter().any(|evidence| {
            evidence.kind == evidence_kind
                && evidence.id == evidence_id
                && evidence.evidence_hash == evidence_hash
        });
        let task_is_current = snapshot.tasks.iter().any(|task| task.id == task_id);
        if !current || !task_is_current {
            self.repository
                .database()
                .complete_ai_job_generation_audit_with_duration(
                    job_id,
                    generation,
                    finished_at_ms,
                    duration_ms,
                    Some(provider_id),
                    Some(model),
                    Some(0),
                )?;
            return Ok(false);
        }
        match decide_ai_disposition(confidence, false) {
            AiDisposition::AutoApply | AiDisposition::Review => {}
            AiDisposition::ManualLock => return Ok(false),
        }
        self.repository.complete_ai_suggestion_job(
            job_id,
            generation,
            &AiWorkLedgerSuggestion {
                evidence_kind: evidence_kind.into(),
                evidence_id: evidence_id.into(),
                evidence_hash: evidence_hash.into(),
                task_id: task_id.into(),
                confidence,
                reason: "AI suggestion from bounded queued work-ledger evidence".into(),
                provider_id: provider_id.into(),
                model: model.into(),
                created_at_ms: now_ms(),
            },
            finished_at_ms,
            duration_ms,
            force_review,
        )
    }

    pub fn consume_ai_assignment_job_response(
        &self,
        job: &AiJob,
        response_content: &str,
        provider_id: &str,
        model: &str,
        finished_at_ms: i64,
        duration_ms: i64,
    ) -> std::result::Result<bool, WorkLedgerAssignmentConsumptionError> {
        if job.kind != "work_ledger_assignment" {
            return Err(WorkLedgerAssignmentConsumptionError::InvalidJob(format!(
                "Unsupported AI job kind: {}",
                job.kind
            )));
        }
        let queued = parse_work_ledger_assignment_job(&job.payload_json)
            .map_err(WorkLedgerAssignmentConsumptionError::InvalidJob)?;
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
        let decision = parse_work_ledger_decision_response(
            response_content,
            &allowed_task_ids,
            &allowed_project_ids,
        )
        .map_err(WorkLedgerAssignmentConsumptionError::InvalidResponse)?;
        let database = self.repository.database();
        match decision.decision {
            WorkLedgerDecisionKind::MatchExisting => {
                let task_id = decision.task_id.as_deref().ok_or_else(|| {
                    WorkLedgerAssignmentConsumptionError::InvalidResponse(
                        "match_existing omitted taskId".into(),
                    )
                })?;
                let runner_up = decision.alternative_confidence.unwrap_or_default();
                let decisive = is_auto_applicable(decision.confidence)
                    && decision
                        .alternative_confidence
                        .is_none_or(|_| decision.confidence - runner_up >= 0.15);
                if !decisive {
                    return database
                        .complete_ai_job_generation_audit_with_duration(
                            &job.id,
                            job.generation,
                            finished_at_ms,
                            duration_ms,
                            Some(provider_id),
                            Some(model),
                            Some(0),
                        )
                        .map_err(|error| {
                            WorkLedgerAssignmentConsumptionError::Persistence(error.to_string())
                        });
                }
                let completed = self
                    .complete_ai_assignment_suggestion_if_current(
                        &job.id,
                        job.generation,
                        &queued.evidence.kind,
                        &queued.evidence.id,
                        &queued.evidence_hash,
                        task_id,
                        decision.confidence,
                        provider_id,
                        model,
                        queued.start_ms,
                        queued.end_ms,
                        finished_at_ms,
                        duration_ms,
                        false,
                    )
                    .map_err(|error| {
                        WorkLedgerAssignmentConsumptionError::Persistence(error.to_string())
                    })?;
                if completed {
                    for evidence in queued.fragments.iter().skip(1) {
                        let result = match evidence.kind.as_str() {
                            "activity" => database.assign_work_ledger_activity(
                                task_id,
                                &evidence.id,
                                EvidenceProvenance::Ai,
                                decision.confidence,
                                "AI work-fragment match",
                                finished_at_ms,
                            ),
                            "browser" => database.assign_work_ledger_browser_visit(
                                task_id,
                                &evidence.id,
                                EvidenceProvenance::Ai,
                                decision.confidence,
                                "AI work-fragment match",
                                finished_at_ms,
                            ),
                            _ => Ok(false),
                        };
                        result.map_err(|error| {
                            WorkLedgerAssignmentConsumptionError::Persistence(error.to_string())
                        })?;
                    }
                }
                Ok(completed)
            }
            WorkLedgerDecisionKind::CreateNew => {
                // Legacy responses are retained for audit compatibility only. New AI-created
                // projects and tasks must go through an explicit project_draft review.
                if matches!(queued.protocol_version, 1 | 2) {
                    return database
                        .complete_ai_job_generation_audit_with_duration(
                            &job.id,
                            job.generation,
                            finished_at_ms,
                            duration_ms,
                            Some(provider_id),
                            Some(model),
                            Some(0),
                        )
                        .map_err(|error| {
                            WorkLedgerAssignmentConsumptionError::Persistence(error.to_string())
                        });
                }
                let cluster_key = queued.cluster_key.as_deref().ok_or_else(|| {
                    WorkLedgerAssignmentConsumptionError::InvalidJob(
                        "Version two job omitted clusterKey".into(),
                    )
                })?;
                let duration_seconds = queued
                    .fragments
                    .iter()
                    .filter(|item| item.kind == "activity")
                    .map(|item| item.duration_seconds.max(0))
                    .sum();
                let first_seen = queued
                    .fragments
                    .iter()
                    .map(|item| item.occurred_at_ms)
                    .min()
                    .unwrap_or(queued.start_ms);
                let last_seen = queued
                    .fragments
                    .iter()
                    .map(|item| {
                        item.occurred_at_ms
                            .saturating_add(item.duration_seconds.max(0).saturating_mul(1_000))
                    })
                    .max()
                    .unwrap_or(first_seen);
                let (accumulated, episode_count, status, existing_task_id) = database
                    .record_work_episode_cluster(
                        cluster_key,
                        cluster_key.trim_start_matches("cluster-"),
                        duration_seconds,
                        first_seen,
                        last_seen,
                    )
                    .map_err(|error| {
                        WorkLedgerAssignmentConsumptionError::Persistence(error.to_string())
                    })?;
                if status != "collecting" {
                    if let Some(task_id) = existing_task_id {
                        for evidence in &queued.fragments {
                            match evidence.kind.as_str() {
                                "activity" => database.assign_work_ledger_activity(
                                    &task_id,
                                    &evidence.id,
                                    EvidenceProvenance::Ai,
                                    decision.confidence,
                                    "Existing AI work-fragment cluster",
                                    finished_at_ms,
                                ),
                                "browser" => database.assign_work_ledger_browser_visit(
                                    &task_id,
                                    &evidence.id,
                                    EvidenceProvenance::Ai,
                                    decision.confidence,
                                    "Existing AI work-fragment cluster",
                                    finished_at_ms,
                                ),
                                _ => Ok(false),
                            }
                            .map_err(|error| {
                                WorkLedgerAssignmentConsumptionError::Persistence(error.to_string())
                            })?;
                        }
                        return database
                            .complete_ai_job_generation_audit_with_duration(
                                &job.id,
                                job.generation,
                                finished_at_ms,
                                duration_ms,
                                Some(provider_id),
                                Some(model),
                                Some(0),
                            )
                            .map_err(|error| {
                                WorkLedgerAssignmentConsumptionError::Persistence(error.to_string())
                            });
                    }
                    return Ok(false);
                }
                if accumulated < 30 * 60 && episode_count < 2 {
                    return database
                        .complete_ai_job_generation_audit_with_duration(
                            &job.id,
                            job.generation,
                            finished_at_ms,
                            duration_ms,
                            Some(provider_id),
                            Some(model),
                            Some(0),
                        )
                        .map_err(|error| {
                            WorkLedgerAssignmentConsumptionError::Persistence(error.to_string())
                        });
                }
                let digest = cluster_key.trim_start_matches("cluster-");
                let task_id = format!("ai-task-{}", digest.chars().take(24).collect::<String>());
                let task = database
                    .create_ai_provisional_work_ledger_task(
                        &task_id,
                        decision.project_id.as_deref(),
                        cluster_key,
                        decision
                            .suggested_title
                            .as_deref()
                            .unwrap_or("AI 暂定任务")
                            .trim(),
                        decision.confidence,
                        finished_at_ms,
                    )
                    .map_err(|error| {
                        WorkLedgerAssignmentConsumptionError::Persistence(error.to_string())
                    })?;
                for evidence in &queued.fragments {
                    let result = match evidence.kind.as_str() {
                        "activity" => database.assign_work_ledger_activity(
                            &task.id,
                            &evidence.id,
                            EvidenceProvenance::Ai,
                            decision.confidence,
                            "AI-created work-fragment cluster",
                            finished_at_ms,
                        ),
                        "browser" => database.assign_work_ledger_browser_visit(
                            &task.id,
                            &evidence.id,
                            EvidenceProvenance::Ai,
                            decision.confidence,
                            "AI-created work-fragment cluster",
                            finished_at_ms,
                        ),
                        _ => Ok(false),
                    };
                    result.map_err(|error| {
                        WorkLedgerAssignmentConsumptionError::Persistence(error.to_string())
                    })?;
                }
                database
                    .complete_ai_job_generation_audit_with_duration(
                        &job.id,
                        job.generation,
                        finished_at_ms,
                        duration_ms,
                        Some(provider_id),
                        Some(model),
                        Some(0),
                    )
                    .map_err(|error| {
                        WorkLedgerAssignmentConsumptionError::Persistence(error.to_string())
                    })
            }
            WorkLedgerDecisionKind::DraftExistingWorkflow
            | WorkLedgerDecisionKind::DraftNewWorkflow => {
                if queued.protocol_version != 2 {
                    return Err(WorkLedgerAssignmentConsumptionError::InvalidResponse(
                        "Workflow drafts require a version two job".into(),
                    ));
                }
                let allowed_cluster_ids = queued
                    .candidate_clusters
                    .iter()
                    .map(|cluster| cluster.cluster_id.as_str())
                    .collect::<HashSet<_>>();
                let mut used_cluster_ids = HashSet::new();
                for task in &decision.tasks {
                    for cluster_id in &task.cluster_ids {
                        if !allowed_cluster_ids.contains(cluster_id.as_str())
                            || !used_cluster_ids.insert(cluster_id.as_str())
                        {
                            return Err(WorkLedgerAssignmentConsumptionError::InvalidResponse(
                                "Workflow draft referenced an unknown or duplicate candidate cluster"
                                    .into(),
                            ));
                        }
                    }
                }
                if used_cluster_ids.len() != allowed_cluster_ids.len() {
                    return Err(WorkLedgerAssignmentConsumptionError::InvalidResponse(
                        "Workflow draft must account for every candidate cluster".into(),
                    ));
                }
                let accumulated_seconds = queued
                    .candidate_clusters
                    .iter()
                    .map(|cluster| cluster.duration_seconds.max(0))
                    .sum::<i64>();
                let episode_count = queued
                    .candidate_clusters
                    .iter()
                    .map(|cluster| cluster.episode_keys.len())
                    .sum::<usize>();
                if accumulated_seconds < 30 * 60 && episode_count < 2 {
                    return database
                        .complete_ai_job_generation_audit_with_duration(
                            &job.id,
                            job.generation,
                            finished_at_ms,
                            duration_ms,
                            Some(provider_id),
                            Some(model),
                            Some(0),
                        )
                        .map_err(|error| {
                            WorkLedgerAssignmentConsumptionError::Persistence(error.to_string())
                        });
                }
                let target_project_id = match decision.decision {
                    WorkLedgerDecisionKind::DraftExistingWorkflow => decision.project_id.clone(),
                    WorkLedgerDecisionKind::DraftNewWorkflow => None,
                    _ => unreachable!(),
                };
                let proposal = ProjectDraftProposal {
                    target_project_id,
                    name: decision.workflow_name.unwrap_or_default().trim().to_string(),
                    description: decision.description.unwrap_or_default().trim().to_string(),
                    tasks: decision
                        .tasks
                        .into_iter()
                        .map(|task| ProjectDraftTaskProposal {
                            semantic_reason: format!(
                                "该任务由 {} 个相互一致的候选工作片段支持。",
                                task.cluster_ids.len()
                            ),
                            key: task.key.trim().to_string(),
                            title: task.title.trim().to_string(),
                            expected_output: task.expected_output.trim().to_string(),
                            cluster_ids: task.cluster_ids,
                        })
                        .collect(),
                    confidence: decision.confidence,
                    reason_code: decision.reason_code.as_str().into(),
                    semantic_reason: match decision.reason_code.as_str() {
                        "shared_goal" => {
                            "这些任务的主题与预期产出指向同一个可持续目标；任务边界按活动方式与产出类型区分。"
                                .into()
                        }
                        "goal_context" | "focus_context" => {
                            "已有目标或专注记录为这些任务提供了共同目标线索。".into()
                        }
                        _ => "候选片段在主题、应用类型或预期产出上形成了可复核的共同目标。"
                            .into(),
                    },
                    data_limitations: vec![
                        "AI 仅接收脱敏后的代表证据；完整时长、次数与关联由本地数据库复核。"
                            .into(),
                        "草稿只表示待确认的语义归类，不证明任务质量、动机或能力。".into(),
                    ],
                };
                if !is_auto_applicable(proposal.confidence) {
                    return database
                        .complete_ai_job_generation_audit_with_duration(
                            &job.id,
                            job.generation,
                            finished_at_ms,
                            duration_ms,
                            Some(provider_id),
                            Some(model),
                            Some(0),
                        )
                        .map_err(|error| {
                            WorkLedgerAssignmentConsumptionError::Persistence(error.to_string())
                        });
                }
                database
                    .complete_project_draft_job(
                        &job.id,
                        job.generation,
                        queued
                            .episode_key
                            .as_deref()
                            .unwrap_or(&queued.evidence_hash),
                        &proposal,
                        &queued
                            .candidate_clusters
                            .iter()
                            .map(|cluster| {
                                (cluster.cluster_id.clone(), cluster.episode_keys.clone())
                            })
                            .collect::<Vec<_>>(),
                        provider_id,
                        model,
                        finished_at_ms,
                        duration_ms,
                        true,
                    )
                    .map_err(|error| {
                        WorkLedgerAssignmentConsumptionError::Persistence(error.to_string())
                    })
            }
            WorkLedgerDecisionKind::Unassigned => database
                .complete_ai_job_generation_audit_with_duration(
                    &job.id,
                    job.generation,
                    finished_at_ms,
                    duration_ms,
                    Some(provider_id),
                    Some(model),
                    Some(0),
                )
                .map_err(|error| {
                    WorkLedgerAssignmentConsumptionError::Persistence(error.to_string())
                }),
        }
    }

    pub fn complete_invalid_ai_assignment_job(
        &self,
        job: &AiJob,
        executor_id: &str,
        model: &str,
        exit_code: Option<i32>,
    ) -> Result<bool> {
        if job.kind != "work_ledger_assignment"
            || parse_work_ledger_assignment_job(&job.payload_json).is_ok()
        {
            return Ok(false);
        }
        self.repository.database().complete_ai_job_generation_error(
            &job.id,
            job.generation,
            now_ms(),
            "Queued work ledger assignment payload is invalid",
            AiExecutionErrorKind::InvalidJob,
            Some(executor_id),
            Some(model),
            exit_code,
        )
    }
}

fn queue_ambiguous_evidence(
    database: &crate::db::Database,
    unassigned_evidence: &[WorkLedgerEvidence],
    ambiguous_evidence_hashes: &[String],
    tasks: &[Task],
    context_tokens: &HashSet<String>,
    start_ms: i64,
    end_ms: i64,
    execution: &AiExecutionSnapshot,
) -> Result<WorkflowAnalysisQueueResult> {
    let ambiguous: HashSet<_> = ambiguous_evidence_hashes
        .iter()
        .map(String::as_str)
        .collect();
    let task_context: Vec<WorkLedgerAssignmentTaskCandidate> = tasks
        .iter()
        .map(|task| WorkLedgerAssignmentTaskCandidate {
            id: task.id.clone(),
            project_id: task.project_id.clone(),
            title: task.title.clone(),
            expected_output: task.expected_output.clone(),
        })
        .collect();
    let mut result = WorkflowAnalysisQueueResult::default();
    let ambiguous_items = unassigned_evidence
        .iter()
        .filter(|evidence| ambiguous.contains(evidence.evidence_hash.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    for batch in group_semantic_episode_batches(group_work_episodes(&ambiguous_items)) {
        let batch_duration_seconds = batch
            .iter()
            .map(|episode| episode.duration_seconds.max(0))
            .sum::<i64>();
        if task_context.is_empty() && batch.len() < 2 && batch_duration_seconds < 30 * 60 {
            continue;
        }
        for episode in &batch {
            database.record_work_episode_members(
                &episode.episode_key,
                &episode.cluster_key,
                &episode.signature_hash,
                episode.duration_seconds,
                episode.started_at_ms,
                episode.ended_at_ms,
                &episode.fragments,
            )?;
        }
        let mut fragments = sample_episode_fragments(&batch, 64);
        fragments.sort_by(|left, right| {
            left.occurred_at_ms
                .cmp(&right.occurred_at_ms)
                .then_with(|| left.kind.cmp(&right.kind))
                .then_with(|| left.id.cmp(&right.id))
        });
        let evidence = fragments[0].clone();
        let batch_key = if batch.len() == 1 {
            batch[0].episode_key.clone()
        } else {
            format!(
                "workflow-batch-{:x}",
                Sha256::digest(
                    batch
                        .iter()
                        .map(|episode| episode.episode_key.as_str())
                        .collect::<Vec<_>>()
                        .join("|")
                        .as_bytes()
                )
            )
        };
        let batch_cluster_key = format!(
            "workflow-cluster-{:x}",
            Sha256::digest(
                batch
                    .iter()
                    .map(|episode| episode.cluster_key.as_str())
                    .collect::<Vec<_>>()
                    .join("|")
                    .as_bytes()
            )
        );
        let payload_json = serde_json::to_string(&WorkLedgerAssignmentJobPayload {
            protocol_version: 2,
            evidence: evidence.clone(),
            evidence_hash: evidence.evidence_hash.clone(),
            episode_key: Some(batch_key.clone()),
            cluster_key: Some(batch_cluster_key),
            fragments,
            context_hints: {
                let mut hints = context_tokens.iter().cloned().collect::<Vec<_>>();
                hints.sort();
                hints.truncate(12);
                hints
            },
            candidate_clusters: candidate_clusters_for_batch(&batch),
            start_ms,
            end_ms,
            tasks: task_context.clone(),
        })
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let queued_at_ms = now_ms();
        let mut execution = execution.clone();
        execution.evidence_hash = batch_key.clone();
        execution.created_at_ms = queued_at_ms;
        let (job_id, inserted) = database.enqueue_ai_job_for_subject_with_outcome(
            "work_ledger_assignment",
            &batch_key,
            &payload_json,
            queued_at_ms,
            &execution,
        )?;
        result.job_ids.push(job_id);
        if inserted {
            result.queued_count += 1;
        } else {
            result.reused_count += 1;
        }
    }
    Ok(result)
}

fn candidate_clusters_for_batch(batch: &[WorkEpisode]) -> Vec<WorkLedgerCandidateCluster> {
    let mut candidates: Vec<WorkLedgerCandidateCluster> = Vec::new();
    for episode in batch {
        if let Some(candidate) = candidates
            .iter_mut()
            .find(|candidate| candidate.cluster_id == episode.cluster_key)
        {
            candidate.episode_keys.push(episode.episode_key.clone());
            candidate.evidence_count = candidate
                .evidence_count
                .saturating_add(episode.fragments.len());
            candidate.duration_seconds = candidate
                .duration_seconds
                .saturating_add(episode.duration_seconds.max(0));
            candidate.started_at_ms = candidate.started_at_ms.min(episode.started_at_ms);
            candidate.ended_at_ms = candidate.ended_at_ms.max(episode.ended_at_ms);
        } else {
            candidates.push(WorkLedgerCandidateCluster {
                cluster_id: episode.cluster_key.clone(),
                episode_keys: vec![episode.episode_key.clone()],
                evidence_count: episode.fragments.len(),
                duration_seconds: episode.duration_seconds.max(0),
                started_at_ms: episode.started_at_ms,
                ended_at_ms: episode.ended_at_ms,
            });
        }
    }
    candidates.sort_by(|left, right| left.cluster_id.cmp(&right.cluster_id));
    candidates
}

fn sample_episode_fragments(episodes: &[WorkEpisode], limit: usize) -> Vec<WorkLedgerEvidence> {
    let mut sampled = Vec::with_capacity(limit);
    let mut indexes = vec![0_usize; episodes.len()];
    while sampled.len() < limit {
        let mut added = false;
        for (episode, index) in episodes.iter().zip(indexes.iter_mut()) {
            let Some(fragment) = episode.fragments.get(*index) else {
                continue;
            };
            sampled.push(fragment.clone());
            *index += 1;
            added = true;
            if sampled.len() == limit {
                break;
            }
        }
        if !added {
            break;
        }
    }
    sampled
}

#[derive(Debug, Clone)]
struct WorkEpisode {
    episode_key: String,
    cluster_key: String,
    signature_hash: String,
    fragments: Vec<WorkLedgerEvidence>,
    duration_seconds: i64,
    started_at_ms: i64,
    ended_at_ms: i64,
}

fn group_semantic_episode_batches(episodes: Vec<WorkEpisode>) -> Vec<Vec<WorkEpisode>> {
    let mut batches: Vec<(HashSet<String>, Vec<WorkEpisode>)> = Vec::new();
    for episode in episodes {
        let episode_tokens = episode
            .fragments
            .iter()
            .flat_map(|fragment| semantic_goal_tokens(&fragment.title, &fragment.domain))
            .collect::<HashSet<_>>();
        let matching = batches
            .iter()
            .position(|(tokens, items)| items.len() < 12 && !tokens.is_disjoint(&episode_tokens));
        if let Some(index) = matching {
            batches[index].0.extend(episode_tokens);
            batches[index].1.push(episode);
        } else {
            batches.push((episode_tokens, vec![episode]));
        }
    }
    batches.into_iter().map(|(_, episodes)| episodes).collect()
}

fn semantic_goal_tokens(title: &str, domain: &str) -> HashSet<String> {
    let mut result = HashSet::new();
    for token in format!("{title} {domain}")
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.chars().count() >= 2)
    {
        let characters = token.chars().collect::<Vec<_>>();
        if characters
            .iter()
            .any(|character| matches!(*character as u32, 0x3400..=0x4dbf | 0x4e00..=0x9fff))
        {
            for pair in characters.windows(2) {
                result.insert(pair.iter().collect());
            }
        } else if characters.len() >= 3 {
            result.insert(token.to_string());
        }
    }
    result
}

fn group_work_episodes(evidence: &[WorkLedgerEvidence]) -> Vec<WorkEpisode> {
    let mut ordered = evidence.to_vec();
    ordered.sort_by_key(|item| item.occurred_at_ms);
    let mut groups: Vec<Vec<WorkLedgerEvidence>> = Vec::new();
    for item in ordered {
        let should_join = groups
            .last()
            .and_then(|group| group.last())
            .is_some_and(|previous| {
                let previous_end = previous
                    .occurred_at_ms
                    .saturating_add(previous.duration_seconds.max(0).saturating_mul(1_000));
                let within_gap =
                    item.occurred_at_ms.saturating_sub(previous_end) <= 10 * 60 * 1_000;
                let previous_tokens = semantic_goal_tokens(&previous.title, &previous.domain);
                let item_tokens = semantic_goal_tokens(&item.title, &item.domain);
                within_gap
                    && (!previous_tokens.is_disjoint(&item_tokens)
                        || (!previous.domain.is_empty()
                            && previous.domain.eq_ignore_ascii_case(&item.domain)))
            });
        if should_join {
            groups.last_mut().expect("group exists").push(item);
        } else {
            groups.push(vec![item]);
        }
    }
    groups
        .into_iter()
        .map(|fragments| {
            let started_at_ms = fragments
                .first()
                .map(|item| item.occurred_at_ms)
                .unwrap_or(0);
            let ended_at_ms = fragments
                .iter()
                .map(|item| {
                    item.occurred_at_ms
                        .saturating_add(item.duration_seconds.max(0).saturating_mul(1_000))
                })
                .max()
                .unwrap_or(started_at_ms);
            let duration_seconds = fragments
                .iter()
                .filter(|item| item.kind == "activity")
                .map(|item| item.duration_seconds.max(0))
                .sum();
            let episode_key = format!(
                "episode-{:x}",
                Sha256::digest(
                    fragments
                        .iter()
                        .flat_map(|item| [&item.kind, &item.id, &item.evidence_hash])
                        .flat_map(|value| value.as_bytes().iter().copied())
                        .collect::<Vec<_>>()
                )
            );
            let mut signature_parts = fragments
                .iter()
                .flat_map(|item| {
                    let mut values = vec![
                        item.application.trim().to_ascii_lowercase(),
                        item.domain.trim().to_ascii_lowercase(),
                    ];
                    values.extend(evidence_tokens(item).into_iter().take(6));
                    values
                })
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>();
            signature_parts.sort();
            signature_parts.dedup();
            let signature_hash =
                format!("{:x}", Sha256::digest(signature_parts.join("|").as_bytes()));
            WorkEpisode {
                cluster_key: format!("cluster-{signature_hash}"),
                signature_hash,
                episode_key,
                fragments,
                duration_seconds,
                started_at_ms,
                ended_at_ms,
            }
        })
        .collect()
}

fn build_local_suggestions(
    evidence: &[WorkLedgerEvidence],
    tasks: &[Task],
    projects: &[Project],
    manual_profiles: &HashMap<String, HashSet<String>>,
    context_tokens: &HashSet<String>,
) -> (Vec<WorkLedgerSuggestion>, Vec<String>) {
    let project_names: HashMap<_, _> = projects
        .iter()
        .map(|project| (project.id.as_str(), project.name.as_str()))
        .collect();
    let mut suggestions = Vec::new();
    let mut ambiguous = Vec::new();
    for item in evidence {
        let evidence_tokens = evidence_tokens(item);
        let mut profile_evidence_tokens = evidence_tokens.clone();
        profile_evidence_tokens.extend(
            evidence_tokens
                .iter()
                .map(|token| hashed_profile_token(token)),
        );
        let mut candidates: Vec<_> = tasks
            .iter()
            .map(|task| {
                let project_name = project_names
                    .get(task.project_id.as_str())
                    .copied()
                    .unwrap_or("");
                let task_tokens = tokens(&format!(
                    "{} {} {}",
                    task.title, task.expected_output, project_name
                ));
                let title_score = overlap_score(&evidence_tokens, &task_tokens) * 0.50;
                let context_score = overlap_score(&task_tokens, context_tokens) * 0.15;
                let manual_score = manual_profiles
                    .get(&task.id)
                    .map(|profile| overlap_score(&profile_evidence_tokens, profile) * 0.50)
                    .unwrap_or_default();
                (task, (title_score + context_score + manual_score).min(0.95))
            })
            .collect();
        candidates.sort_by(|(left_task, left_score), (right_task, right_score)| {
            right_score
                .total_cmp(left_score)
                .then_with(|| left_task.id.cmp(&right_task.id))
        });
        let Some((task, confidence)) = candidates.first() else {
            ambiguous.push(item.evidence_hash.clone());
            continue;
        };
        let runner_up = candidates
            .get(1)
            .map(|(_, score)| *score)
            .unwrap_or_default();
        if !is_auto_applicable(*confidence) || (*confidence - runner_up) < 0.15 {
            ambiguous.push(item.evidence_hash.clone());
            continue;
        }
        suggestions.push(WorkLedgerSuggestion {
            evidence_kind: item.kind.clone(),
            evidence_id: item.id.clone(),
            task_id: task.id.clone(),
            confidence: *confidence,
            reason: "Deterministic match from task, goal/focus, and manual evidence keywords"
                .into(),
            evidence_hash: item.evidence_hash.clone(),
            source: "local".into(),
            can_auto_apply: is_auto_applicable(*confidence),
        });
    }
    (suggestions, ambiguous)
}

fn evidence_key(kind: &str, id: &str) -> (String, String) {
    (kind.to_string(), id.to_string())
}

fn hashed_profile_token(token: &str) -> String {
    format!("profile-{:x}", Sha256::digest(token.as_bytes()))
}

fn adjust_manual_match_profile(
    database: &crate::db::Database,
    task_id: &str,
    evidence_kind: &str,
    evidence_id: &str,
    delta: f64,
    updated_at_ms: i64,
) -> Result<()> {
    let Some(profile_text) =
        database.work_ledger_evidence_profile_text(evidence_kind, evidence_id)?
    else {
        return Ok(());
    };
    for token in tokens(&profile_text) {
        database.adjust_task_match_profile(
            &hashed_profile_token(&token),
            task_id,
            delta,
            updated_at_ms,
        )?;
    }
    Ok(())
}

pub(crate) fn work_ledger_evidence_from_activity(
    segment: ActivitySegmentRecord,
) -> WorkLedgerEvidence {
    let evidence_hash = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                "activity",
                &segment.id,
                segment.started_at_ms,
                segment.ended_at_ms,
                &segment.app,
                &segment.app_path,
                &segment.title,
                segment.category,
                segment.video_purpose,
                segment.confidence,
                segment.source,
                &segment.reason,
                &segment.model_version,
                segment.needs_review,
            ))
            .expect("work ledger evidence serializes"),
        )
    );
    WorkLedgerEvidence {
        kind: "activity".into(),
        id: segment.id,
        occurred_at_ms: segment.started_at_ms,
        duration_seconds: segment
            .ended_at_ms
            .saturating_sub(segment.started_at_ms)
            .max(0)
            / 1_000,
        application: segment.app,
        title: segment.title,
        domain: String::new(),
        category: activity_category_key(segment.category).into(),
        workflow_eligibility: activity_workflow_eligibility(
            segment.category,
            segment.video_purpose,
        )
        .0
        .into(),
        eligibility_reason: activity_workflow_eligibility(segment.category, segment.video_purpose)
            .1
            .into(),
        classification_source: format!("{:?}", segment.source).to_ascii_lowercase(),
        classification_reason: segment.reason,
        classification_confidence: Some(segment.confidence.into()),
        evidence_hash,
    }
}

pub(crate) fn work_ledger_evidence_from_browser(visit: BrowserVisitRecord) -> WorkLedgerEvidence {
    let evidence_hash = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                "browser",
                &visit.id,
                &visit.browser,
                &visit.profile,
                visit.visited_at_ms,
                &visit.url,
                &visit.domain,
                &visit.title,
            ))
            .expect("work ledger evidence serializes"),
        )
    );
    WorkLedgerEvidence {
        kind: "browser".into(),
        id: visit.id,
        occurred_at_ms: visit.visited_at_ms,
        duration_seconds: 0,
        application: visit.browser,
        title: visit.title,
        domain: visit.domain,
        category: "browser".into(),
        workflow_eligibility: "initiator".into(),
        eligibility_reason: "search or web research activity".into(),
        classification_source: "unclassified".into(),
        classification_reason: String::new(),
        classification_confidence: None,
        evidence_hash,
    }
}

fn activity_category_key(category: crate::domain::ActivityCategory) -> &'static str {
    use crate::domain::ActivityCategory;
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

fn activity_workflow_eligibility(
    category: crate::domain::ActivityCategory,
    video_purpose: crate::domain::VideoPurpose,
) -> (&'static str, &'static str) {
    use crate::domain::{ActivityCategory, VideoPurpose};
    match (category, video_purpose) {
        (ActivityCategory::Idle | ActivityCategory::Game, _)
        | (ActivityCategory::VideoInput, VideoPurpose::Leisure) => {
            ("excluded", "inactivity, game, or leisure evidence")
        }
        (
            ActivityCategory::Research
            | ActivityCategory::TextInput
            | ActivityCategory::CreationDevelopment,
            _,
        )
        | (ActivityCategory::VideoInput, VideoPurpose::Learning) => (
            "initiator",
            "meaningful development, research, or learning activity",
        ),
        _ => (
            "context",
            "supporting evidence requires a nearby goal-related task",
        ),
    }
}

fn context_evidence_is_connected(
    evidence: &WorkLedgerEvidence,
    initiators: &[WorkLedgerEvidence],
    context_tokens: &HashSet<String>,
) -> bool {
    let evidence_tokens = evidence_tokens(evidence);
    if !evidence_tokens.is_disjoint(context_tokens) {
        return true;
    }
    let evidence_end_ms = evidence
        .occurred_at_ms
        .saturating_add(evidence.duration_seconds.max(0).saturating_mul(1_000));
    initiators.iter().any(|initiator| {
        let initiator_end_ms = initiator
            .occurred_at_ms
            .saturating_add(initiator.duration_seconds.max(0).saturating_mul(1_000));
        evidence.occurred_at_ms.saturating_sub(initiator_end_ms) <= 15 * 60 * 1_000
            && initiator.occurred_at_ms.saturating_sub(evidence_end_ms) <= 15 * 60 * 1_000
    })
}

fn evidence_tokens(evidence: &WorkLedgerEvidence) -> HashSet<String> {
    tokens(&format!(
        "{} {} {}",
        evidence.application, evidence.title, evidence.domain
    ))
}

fn tokens(value: &str) -> HashSet<String> {
    value
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.chars().count() >= 3)
        .map(ToOwned::to_owned)
        .collect()
}

fn overlap_score(left: &HashSet<String>, right: &HashSet<String>) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let overlap = left.intersection(right).count() as f64;
    overlap / left.len().min(right.len()) as f64
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}
