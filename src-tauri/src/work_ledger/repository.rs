use rusqlite::Result;

use crate::db::{Database, WorkLedgerRangeFacts};

use super::domain::{
    AiWorkLedgerSuggestion, ConfirmedDailyGoalTask, DailyGoalTaskLink, EvidenceLink,
    EvidenceProvenance, NewProgressEntry, NewProject, NewTask, ProgressEntry, Project,
    ProjectTimeInsight, ProjectUpdate, Task, TaskStatus, TaskTimeInsight, TaskUpdate,
    WorkLedgerRangeRollup,
};

pub struct WorkLedgerRepository<'a> {
    database: &'a Database,
}

impl<'a> WorkLedgerRepository<'a> {
    pub fn new(database: &'a Database) -> Self {
        Self { database }
    }

    pub fn create_project(&self, project: NewProject) -> Result<Project> {
        self.database.create_work_ledger_project(project)
    }

    pub fn get_project(&self, id: &str) -> Result<Option<Project>> {
        self.database.get_work_ledger_project(id)
    }

    pub fn list_projects(&self, include_archived: bool) -> Result<Vec<Project>> {
        self.database.list_work_ledger_projects(include_archived)
    }

    pub fn update_project(
        &self,
        id: &str,
        update: ProjectUpdate,
        updated_at_ms: i64,
    ) -> Result<bool> {
        self.database
            .update_work_ledger_project(id, update, updated_at_ms)
    }

    pub fn archive_project(&self, id: &str, archived_at_ms: i64) -> Result<bool> {
        self.database
            .archive_work_ledger_project(id, archived_at_ms)
    }

    pub fn delete_project(&self, id: &str) -> Result<bool> {
        self.database.delete_work_ledger_project(id)
    }

    pub fn create_task(&self, task: NewTask) -> Result<Task> {
        self.database.create_work_ledger_task(task)
    }

    pub fn get_task(&self, id: &str) -> Result<Option<Task>> {
        self.database.get_work_ledger_task(id)
    }

    pub fn list_tasks(&self, project_id: &str) -> Result<Vec<Task>> {
        self.database.list_work_ledger_tasks(project_id)
    }

    pub fn update_task(&self, id: &str, update: TaskUpdate, updated_at_ms: i64) -> Result<bool> {
        self.database
            .update_work_ledger_task(id, update, updated_at_ms)
    }

    pub fn transition_task_status(
        &self,
        id: &str,
        status: TaskStatus,
        updated_at_ms: i64,
    ) -> Result<Option<Task>> {
        self.database
            .transition_work_ledger_task_status(id, status, updated_at_ms)
    }

    pub fn delete_task(&self, id: &str) -> Result<bool> {
        self.database.delete_work_ledger_task(id)
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
        self.database.assign_work_ledger_activity(
            task_id,
            activity_segment_id,
            provenance,
            confidence,
            reason,
            created_at_ms,
        )
    }

    pub fn remove_activity(&self, task_id: &str, activity_segment_id: &str) -> Result<bool> {
        self.database
            .remove_work_ledger_activity(task_id, activity_segment_id)
    }

    pub fn list_activity_links(&self, task_id: &str) -> Result<Vec<EvidenceLink>> {
        self.database.list_work_ledger_activity_links(task_id)
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
        self.database.assign_work_ledger_browser_visit(
            task_id,
            browser_visit_id,
            provenance,
            confidence,
            reason,
            created_at_ms,
        )
    }

    pub fn remove_browser_visit(&self, task_id: &str, browser_visit_id: &str) -> Result<bool> {
        self.database
            .remove_work_ledger_browser_visit(task_id, browser_visit_id)
    }

    pub fn list_browser_links(&self, task_id: &str) -> Result<Vec<EvidenceLink>> {
        self.database.list_work_ledger_browser_links(task_id)
    }

    pub fn list_ai_suggestions(&self) -> Result<Vec<AiWorkLedgerSuggestion>> {
        self.database.list_work_ledger_ai_suggestions()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_ai_suggestion_job(
        &self,
        job_id: &str,
        generation: i64,
        suggestion: &AiWorkLedgerSuggestion,
        finished_at_ms: i64,
        duration_ms: i64,
        force_review: bool,
    ) -> Result<bool> {
        self.database.complete_work_ledger_ai_suggestion_job(
            job_id,
            generation,
            suggestion,
            finished_at_ms,
            duration_ms,
            force_review,
        )
    }

    pub fn add_progress_entry(&self, entry: NewProgressEntry) -> Result<ProgressEntry> {
        self.database.create_work_ledger_progress_entry(entry)
    }

    pub fn list_progress_entries(&self, task_id: &str) -> Result<Vec<ProgressEntry>> {
        self.database.list_work_ledger_progress_entries(task_id)
    }

    pub fn delete_progress_entry(&self, id: &str) -> Result<bool> {
        self.database.delete_work_ledger_progress_entry(id)
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
        self.database.confirm_daily_goal_task(
            goal_row_id,
            goal_date,
            goal_text,
            task_id,
            new_task,
            confirmed_at_ms,
        )
    }

    pub fn list_daily_goal_task_links(&self, goal_date: &str) -> Result<Vec<DailyGoalTaskLink>> {
        self.database.list_daily_goal_task_links(goal_date)
    }

    pub fn record_daily_actual_output_progress(
        &self,
        task_id: &str,
        date: &str,
        created_at_ms: i64,
    ) -> Result<Option<ProgressEntry>> {
        self.database
            .record_daily_actual_output_progress(task_id, date, created_at_ms)
    }

    pub fn task_duration_seconds(&self, task_id: &str) -> Result<i64> {
        self.database.work_ledger_task_duration_seconds(task_id)
    }

    pub fn project_duration_seconds(&self, project_id: &str) -> Result<i64> {
        self.database
            .work_ledger_project_duration_seconds(project_id)
    }

    pub fn range_rollup(&self, start_ms: i64, end_ms: i64) -> Result<WorkLedgerRangeRollup> {
        self.database.work_ledger_range_rollup(start_ms, end_ms)
    }

    pub fn range_facts(&self, start_ms: i64, end_ms: i64) -> Result<WorkLedgerRangeFacts> {
        self.database.load_work_ledger_range_facts(start_ms, end_ms)
    }

    pub fn task_time_insight(&self, task_id: &str, end_ms: i64) -> Result<TaskTimeInsight> {
        self.database.work_ledger_task_time_insight(task_id, end_ms)
    }

    pub fn project_time_insight(
        &self,
        project_id: &str,
        end_ms: i64,
        range_days: i64,
    ) -> Result<ProjectTimeInsight> {
        self.database
            .work_ledger_project_time_insight(project_id, end_ms, range_days)
    }

    pub fn merge_tasks(
        &self,
        source_task_id: &str,
        target_task_id: &str,
        updated_at_ms: i64,
    ) -> Result<bool> {
        self.database
            .merge_work_ledger_tasks(source_task_id, target_task_id, updated_at_ms)
    }

    pub fn database(&self) -> &'a Database {
        self.database
    }
}
