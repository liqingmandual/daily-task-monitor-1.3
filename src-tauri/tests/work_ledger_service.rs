use daily_task_monitor_core::ai::{
    AiExecutionErrorKind, AiExecutionMode, AiExecutionSnapshot, AiJobStatus,
};
use daily_task_monitor_core::ai_review::{
    AiReviewAction, AiReviewEventKind, AiReviewFilter, AiReviewKind, AiReviewRecord,
    AiReviewResolution, AiReviewState,
};
use daily_task_monitor_core::browser::BrowserVisit;
use daily_task_monitor_core::classifier::AI_AUTO_APPLY_THRESHOLD;
use daily_task_monitor_core::db::{ActivitySegmentRecord, Database};
use daily_task_monitor_core::domain::{ActivityCategory, ClassificationSource, VideoPurpose};
use daily_task_monitor_core::work_ledger::{
    AiWorkLedgerSuggestion, EvidenceProvenance, NewProject, NewTask, ProgressOriginKind,
    TaskPriority, TaskStatus, WorkLedgerRepository, WorkLedgerService,
    parse_work_ledger_assignment_job,
};
use rusqlite::Connection;

fn unique_database_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "daily-task-monitor-{label}-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn ai_snapshot() -> AiExecutionSnapshot {
    AiExecutionSnapshot {
        execution_mode: AiExecutionMode::ApiKey,
        executor_id: "openai".into(),
        model: "gpt-test".into(),
        evidence_hash: "evidence-hash".into(),
        created_at_ms: 1_000,
    }
}

fn codex_snapshot() -> AiExecutionSnapshot {
    AiExecutionSnapshot {
        execution_mode: AiExecutionMode::Codex,
        executor_id: r"C:\Tools\codex.exe".into(),
        model: "gpt-5-codex".into(),
        evidence_hash: String::new(),
        created_at_ms: 2_000,
    }
}

fn segment(id: &str, started_at_ms: i64, ended_at_ms: i64) -> ActivitySegmentRecord {
    ActivitySegmentRecord {
        id: id.into(),
        started_at_ms,
        ended_at_ms,
        app: "Codex".into(),
        app_path: String::new(),
        title: "Desktop rewrite".into(),
        category: ActivityCategory::CreationDevelopment,
        video_purpose: VideoPurpose::Unknown,
        confidence: 0.9,
        source: ClassificationSource::Rule,
        reason: "fixture".into(),
        model_version: "rules-v1".into(),
        needs_review: false,
        inactivity_reason: None,
    }
}

#[test]
fn game_evidence_is_removed_from_workflows_and_never_reappears_unassigned() {
    let database = Database::open_in_memory().unwrap();
    let mut game = segment("game-session", 1_000, 61_000);
    game.app = "Steam".into();
    game.title = "Game session".into();
    database.insert_segment(&game).unwrap();

    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    service
        .create_project(NewProject {
            id: "study".into(),
            name: "学习雅思".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    service
        .create_task(NewTask {
            id: "vocabulary".into(),
            project_id: "study".into(),
            title: "背诵雅思词汇".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();
    database
        .assign_work_ledger_activity(
            "vocabulary",
            "game-session",
            EvidenceProvenance::Manual,
            1.0,
            "legacy manual link",
            2_000,
        )
        .unwrap();
    database
        .save_manual_classification(
            "game-session",
            ActivityCategory::Game,
            VideoPurpose::Unknown,
            "correct historical classification",
        )
        .unwrap();

    let snapshot = service.get_work_ledger(0, 70_000, None).unwrap();
    assert!(snapshot.linked_evidence.is_empty());
    assert!(snapshot.unassigned_evidence.is_empty());
    assert!(
        service
            .list_activity_links("vocabulary")
            .unwrap()
            .is_empty()
    );
    assert!(
        !service
            .assign_activity(
                "vocabulary",
                "game-session",
                EvidenceProvenance::Manual,
                1.0,
                "manual retry",
                3_000,
            )
            .unwrap()
    );
}

#[test]
fn standalone_file_management_evidence_cannot_start_a_workflow() {
    let database = Database::open_in_memory().unwrap();
    let mut file = segment("download-cleanup", 1_000, 61_000);
    file.app = "Explorer".into();
    file.title = "Downloads".into();
    file.category = ActivityCategory::FileManagement;
    database.insert_segment(&file).unwrap();

    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let snapshot = service.get_work_ledger(0, 70_000, None).unwrap();
    assert!(
        snapshot.unassigned_evidence.is_empty(),
        "context-only file activity must not create a workflow candidate by itself"
    );
}

fn two_generation_workflow_reviews(
    database: &Database,
) -> (
    WorkLedgerService<'_>,
    AiReviewRecord,
    AiReviewRecord,
    AiWorkLedgerSuggestion,
) {
    database
        .insert_segment(&segment("generation-segment", 1_000, 2_000))
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(database));
    let project = service
        .create_project(NewProject {
            id: "generation-project".into(),
            name: "Generation review".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    for task_id in ["old-task", "new-task"] {
        service
            .create_task(NewTask {
                id: task_id.into(),
                project_id: project.id.clone(),
                title: format!("{task_id} unrelated"),
                priority: TaskPriority::Medium,
                expected_output: String::new(),
                due_date: None,
                created_at_ms: 1_000,
            })
            .unwrap();
    }

    service
        .get_work_ledger_with_ai_queue(0, 3_000, None, Some(&ai_snapshot()))
        .unwrap();
    let first_job = database.claim_next_due_ai_job(i64::MAX).unwrap().unwrap();
    let first_queued = parse_work_ledger_assignment_job(&first_job.payload_json).unwrap();
    assert!(
        service
            .complete_ai_assignment_suggestion_if_current(
                &first_job.id,
                first_job.generation,
                &first_queued.evidence.kind,
                &first_queued.evidence.id,
                &first_queued.evidence_hash,
                "old-task",
                0.7,
                "openai",
                "gpt-test",
                first_queued.start_ms,
                first_queued.end_ms,
                2_000,
                100,
                true,
            )
            .unwrap()
    );

    let mut second_execution = codex_snapshot();
    second_execution.evidence_hash = first_job.execution.evidence_hash.clone();
    database
        .force_enqueue_ai_job_for_subject(
            "work_ledger_assignment",
            &first_job.execution.evidence_hash,
            &first_job.payload_json,
            3_000,
            &second_execution,
        )
        .unwrap();
    let second_job = database.claim_next_due_ai_job(i64::MAX).unwrap().unwrap();
    assert_eq!(second_job.generation, first_job.generation + 1);
    assert_eq!(second_job.execution.execution_mode, AiExecutionMode::Codex);
    let second_queued = parse_work_ledger_assignment_job(&second_job.payload_json).unwrap();
    assert!(
        service
            .complete_ai_assignment_suggestion_if_current(
                &second_job.id,
                second_job.generation,
                &second_queued.evidence.kind,
                &second_queued.evidence.id,
                &second_queued.evidence_hash,
                "new-task",
                0.7,
                "codex",
                "gpt-5-codex",
                second_queued.start_ms,
                second_queued.end_ms,
                4_000,
                100,
                true,
            )
            .unwrap()
    );

    let reviews = database
        .list_ai_reviews(&AiReviewFilter {
            kinds: vec![AiReviewKind::WorkflowAssignment],
            subject_id: Some("generation-segment".into()),
            ..AiReviewFilter::default()
        })
        .unwrap();
    assert_eq!(reviews.len(), 2);
    let review_for_task = |task_id: &str| {
        reviews
            .iter()
            .find(|review| {
                serde_json::from_str::<serde_json::Value>(&review.proposed_json).unwrap()["taskId"]
                    == task_id
            })
            .unwrap()
            .clone()
    };
    let old_review = review_for_task("old-task");
    let new_review = review_for_task("new-task");
    assert_eq!(old_review.state, AiReviewState::Pending);
    assert_eq!(new_review.state, AiReviewState::Pending);
    assert_ne!(
        old_review.execution.generation,
        new_review.execution.generation
    );
    assert_ne!(
        old_review.execution.execution_mode,
        new_review.execution.execution_mode
    );
    assert_ne!(old_review.execution.model, new_review.execution.model);

    let suggestions = database.list_work_ledger_ai_suggestions().unwrap();
    assert_eq!(suggestions.len(), 1);
    let suggestion = suggestions.into_iter().next().unwrap();
    assert_eq!(suggestion.task_id, "new-task");
    (service, old_review, new_review, suggestion)
}

#[test]
fn service_transitions_tasks_tracks_completion_and_rolls_up_linked_duration() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("segment-1", 1_000, 4_000))
        .unwrap();
    database
        .insert_segment(&segment("segment-2", 4_000, 9_000))
        .unwrap();
    let repository = WorkLedgerRepository::new(&database);
    let service = WorkLedgerService::new(repository);
    let project = service
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Desktop rewrite".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    let task = service
        .create_task(NewTask {
            id: "task-1".into(),
            project_id: project.id.clone(),
            title: "Work ledger".into(),
            priority: TaskPriority::Medium,
            expected_output: "Domain and migration".into(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();

    service
        .assign_activity(
            &task.id,
            "segment-1",
            EvidenceProvenance::Rule,
            0.8,
            "title rule",
            2_000,
        )
        .unwrap();
    service
        .assign_activity(
            &task.id,
            "segment-2",
            EvidenceProvenance::Manual,
            1.0,
            "confirmed",
            2_000,
        )
        .unwrap();
    let in_progress = service
        .transition_task_status(&task.id, TaskStatus::InProgress, 3_000)
        .unwrap();
    assert_eq!(in_progress.completed_at_ms, None);
    let completed = service
        .transition_task_status(&task.id, TaskStatus::Completed, 10_000)
        .unwrap();
    assert_eq!(completed.completed_at_ms, Some(10_000));
    assert_eq!(service.task_duration_seconds(&task.id).unwrap(), 8);
    assert_eq!(service.project_duration_seconds(&project.id).unwrap(), 8);
}

#[test]
fn project_rollup_counts_a_shared_segment_once_across_tasks() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("shared-segment", 1_000, 6_000))
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Desktop rewrite".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    for task_id in ["task-1", "task-2"] {
        service
            .create_task(NewTask {
                id: task_id.into(),
                project_id: project.id.clone(),
                title: task_id.into(),
                priority: TaskPriority::Medium,
                expected_output: String::new(),
                due_date: None,
                created_at_ms: 1_000,
            })
            .unwrap();
        database
            .assign_work_ledger_activity(
                task_id,
                "shared-segment",
                EvidenceProvenance::Manual,
                1.0,
                "shared work",
                2_000,
            )
            .unwrap();
    }

    assert_eq!(service.project_duration_seconds(&project.id).unwrap(), 5);
}

#[test]
fn project_insight_separates_shared_time_and_zero_fills_the_selected_range() {
    let database = Database::open_in_memory().unwrap();
    for segment_id in ["research-segment", "vocabulary-segment"] {
        database
            .insert_segment(&segment(segment_id, 1_000, 6_001_000))
            .unwrap();
    }
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-insight".into(),
            name: "学习雅思".into(),
            color: "#3182ce".into(),
            description: "搜索与词汇学习".into(),
            created_at_ms: 0,
        })
        .unwrap();
    for (task_id, segment_id) in [
        ("research", "research-segment"),
        ("vocabulary", "vocabulary-segment"),
    ] {
        service
            .create_task(NewTask {
                id: task_id.into(),
                project_id: project.id.clone(),
                title: task_id.into(),
                priority: TaskPriority::Medium,
                expected_output: String::new(),
                due_date: None,
                created_at_ms: 0,
            })
            .unwrap();
        database
            .assign_work_ledger_activity(
                task_id,
                segment_id,
                EvidenceProvenance::Manual,
                1.0,
                "shared evidence",
                1,
            )
            .unwrap();
    }

    let insight = service
        .project_time_insight(&project.id, 57_600_000, 7)
        .unwrap();

    assert_eq!(insight.summary.lifecycle_total_seconds, 6_000);
    assert_eq!(insight.shared_seconds, 6_000);
    assert_eq!(
        insight
            .task_contributions
            .iter()
            .map(|task| task.invested_seconds)
            .sum::<i64>(),
        0
    );
    assert_eq!(insight.daily_points.len(), 7);
    assert_eq!(
        insight
            .daily_points
            .iter()
            .map(|point| point.invested_seconds)
            .sum::<i64>(),
        6_000
    );
}

#[test]
fn range_rollup_clips_activity_deduplicates_by_scope_and_keeps_archived_history() {
    let path = unique_database_path("authoritative-range-rollup");
    {
        let database = Database::open(&path).unwrap();
        database
            .insert_segment(&segment("shared-segment", 0, 6_000))
            .unwrap();
        database
            .insert_segment(&segment("task-one-only", 4_000, 7_000))
            .unwrap();
        database
            .insert_browser_visit(
                "visit-in-range",
                "Chrome",
                "Default",
                &BrowserVisit {
                    url: "https://example.com/in".into(),
                    title: "In range".into(),
                    visited_at_ms: 1_000,
                },
                "example.com",
            )
            .unwrap();
        database
            .insert_browser_visit(
                "visit-at-end",
                "Chrome",
                "Default",
                &BrowserVisit {
                    url: "https://example.com/end".into(),
                    title: "At end".into(),
                    visited_at_ms: 5_000,
                },
                "example.com",
            )
            .unwrap();
        let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
        let project = service
            .create_project(NewProject {
                id: "archived-project".into(),
                name: "Historical project".into(),
                color: "#3182ce".into(),
                description: String::new(),
                created_at_ms: 100,
            })
            .unwrap();
        for task_id in ["task-1", "task-2"] {
            service
                .create_task(NewTask {
                    id: task_id.into(),
                    project_id: project.id.clone(),
                    title: task_id.into(),
                    priority: TaskPriority::Medium,
                    expected_output: String::new(),
                    due_date: None,
                    created_at_ms: 100,
                })
                .unwrap();
        }
        service
            .assign_activity(
                "task-1",
                "shared-segment",
                EvidenceProvenance::Manual,
                1.0,
                "confirmed",
                500,
            )
            .unwrap();
        service
            .assign_activity(
                "task-1",
                "task-one-only",
                EvidenceProvenance::Manual,
                1.0,
                "confirmed",
                500,
            )
            .unwrap();
        service
            .assign_browser_visit(
                "task-1",
                "visit-in-range",
                EvidenceProvenance::Manual,
                1.0,
                "confirmed",
                1_000,
            )
            .unwrap();
        service
            .assign_browser_visit(
                "task-1",
                "visit-at-end",
                EvidenceProvenance::Manual,
                1.0,
                "confirmed",
                5_000,
            )
            .unwrap();
        service
            .add_progress_entry(daily_task_monitor_core::work_ledger::NewProgressEntry {
                id: "progress-in-range".into(),
                task_id: "task-1".into(),
                note: "Progress".into(),
                created_at_ms: 2_000,
            })
            .unwrap();
        database
            .start_focus_session_for_task(
                "focus-in-range",
                "2026-07-13",
                "Focus",
                25,
                500,
                Some("task-1"),
            )
            .unwrap();
        database
            .complete_focus_session("focus-in-range", 3_500, "Outcome")
            .unwrap();
        service.archive_project(&project.id, 8_000).unwrap();
    }

    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             DROP INDEX idx_task_activity_links_unique_segment;
             INSERT INTO task_activity_links(
                task_id, activity_segment_id, provenance, confidence, reason, created_at_ms
             ) VALUES ('task-2', 'shared-segment', 'manual', 1.0, 'legacy shared link', 600);",
        )
        .unwrap();
    drop(connection);

    let database = Database::open(&path).unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let rollup = service.range_rollup(1_000, 5_000).unwrap();
    assert_eq!(rollup.start_ms, 1_000);
    assert_eq!(rollup.end_ms, 5_000);
    assert_eq!(rollup.projects.len(), 1);
    assert_eq!(rollup.tasks.len(), 2);
    let project = &rollup.projects[0];
    assert_eq!(project.project_id, "archived-project");
    assert_eq!(project.project_name, "Historical project");
    assert_eq!(project.project_status.as_str(), "archived");
    assert_eq!(project.invested_seconds, 5);
    assert_eq!(project.activity_segment_count, 2);
    assert_eq!(project.browser_visit_count, 1);
    assert_eq!(project.progress_count, 2);
    assert_eq!(project.focus_session_count, 1);
    assert_eq!(project.focus_seconds, 2);

    let task_one = rollup
        .tasks
        .iter()
        .find(|task| task.task_id == "task-1")
        .unwrap();
    assert_eq!(task_one.invested_seconds, 5);
    assert_eq!(task_one.activity_segment_count, 2);
    assert_eq!(task_one.browser_visit_count, 1);
    assert_eq!(task_one.progress_count, 2);
    assert_eq!(task_one.focus_session_count, 1);
    assert_eq!(task_one.focus_seconds, 2);
    let task_two = rollup
        .tasks
        .iter()
        .find(|task| task.task_id == "task-2")
        .unwrap();
    assert_eq!(task_two.invested_seconds, 4);
    assert_eq!(task_two.activity_segment_count, 1);
    assert_eq!(task_two.browser_visit_count, 0);
    assert_eq!(task_two.progress_count, 0);
    assert_eq!(task_two.focus_seconds, 0);
    assert!(service.range_rollup(5_000, 5_000).is_err());
    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn reopening_a_completed_task_clears_its_completion_timestamp() {
    let database = Database::open_in_memory().unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Desktop rewrite".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    let task = service
        .create_task(NewTask {
            id: "task-1".into(),
            project_id: project.id,
            title: "Reopen me".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();

    let completed = service
        .transition_task_status(&task.id, TaskStatus::Completed, 2_000)
        .unwrap();
    assert_eq!(completed.completed_at_ms, Some(2_000));
    let reopened = service
        .transition_task_status(&task.id, TaskStatus::InProgress, 3_000)
        .unwrap();
    assert_eq!(reopened.completed_at_ms, None);
}

#[test]
fn confirmed_goal_rows_link_or_create_tasks_atomically_and_idempotently() {
    let database = Database::open_in_memory().unwrap();
    database
        .save_daily_goal(
            "2026-07-13",
            "Link existing task\nCreate a new task",
            "A checked result",
            "",
        )
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Desktop rewrite".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    let existing = service
        .create_task(NewTask {
            id: "existing-task".into(),
            project_id: project.id.clone(),
            title: "Existing".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();

    let linked = service
        .confirm_daily_goal_task(
            "goal-row-stable-existing",
            "2026-07-13",
            "Link existing task",
            &existing.id,
            None,
            2_000,
        )
        .unwrap();
    assert_eq!(linked.task.id, existing.id);
    assert!(!linked.task_created);

    let retry = service
        .confirm_daily_goal_task(
            "goal-row-stable-existing",
            "2026-07-13",
            "Link existing task",
            "ignored-on-retry",
            Some(NewTask {
                id: "ignored-on-retry".into(),
                project_id: project.id.clone(),
                title: "Must not be created".into(),
                priority: TaskPriority::Urgent,
                expected_output: String::new(),
                due_date: None,
                created_at_ms: 3_000,
            }),
            3_000,
        )
        .unwrap();
    assert_eq!(retry.task.id, existing.id);
    assert!(!retry.task_created);
    assert!(service.get_task("ignored-on-retry").unwrap().is_none());

    let created = service
        .confirm_daily_goal_task(
            "goal-row-stable-new",
            "2026-07-13",
            "Create a new task",
            "created-task",
            Some(NewTask {
                id: "created-task".into(),
                project_id: project.id.clone(),
                title: "Create a new task".into(),
                priority: TaskPriority::High,
                expected_output: "A checked result".into(),
                due_date: Some("2026-07-13".into()),
                created_at_ms: 4_000,
            }),
            4_000,
        )
        .unwrap();
    assert!(created.task_created);
    assert_eq!(created.task.id, "created-task");
    assert_eq!(
        service
            .list_daily_goal_task_links("2026-07-13")
            .unwrap()
            .len(),
        2
    );

    assert!(
        service
            .confirm_daily_goal_task(
                "2",
                "2026-07-13",
                "Create a new task",
                "row-number-task",
                Some(NewTask {
                    id: "row-number-task".into(),
                    project_id: project.id,
                    title: "Must not be created".into(),
                    priority: TaskPriority::Medium,
                    expected_output: String::new(),
                    due_date: None,
                    created_at_ms: 5_000,
                }),
                5_000,
            )
            .is_err()
    );
    assert!(service.get_task("row-number-task").unwrap().is_none());
}

#[test]
fn linked_focus_completion_is_idempotent_and_records_one_provenance_progress() {
    let database = Database::open_in_memory().unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-focus".into(),
            name: "Focus".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    let task = service
        .create_task(NewTask {
            id: "task-focus".into(),
            project_id: project.id,
            title: "Focus task".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();

    database
        .start_focus_session_for_task(
            "focus-linked",
            "2026-07-13",
            "Focus task",
            25,
            1_000,
            Some(&task.id),
        )
        .unwrap();
    assert!(
        database
            .complete_focus_session("focus-linked", 61_000, "Shipped the slice")
            .unwrap()
    );
    assert!(
        !database
            .complete_focus_session("focus-linked", 61_000, "Shipped the slice")
            .unwrap()
    );
    assert!(
        !database
            .complete_focus_session("focus-linked", 90_000, "Must not replace the outcome")
            .unwrap()
    );

    let focus = database.list_focus_sessions(0, 100_000).unwrap();
    assert_eq!(focus[0].task_id.as_deref(), Some(task.id.as_str()));
    assert_eq!(focus[0].ended_at_ms, Some(61_000));
    assert_eq!(focus[0].outcome, "Shipped the slice");
    let progress = service.list_progress_entries(&task.id).unwrap();
    assert_eq!(progress.len(), 1);
    assert_eq!(progress[0].note, "Shipped the slice");
    assert_eq!(progress[0].origin_kind, ProgressOriginKind::FocusOutcome);
    assert_eq!(progress[0].source_id.as_deref(), Some("focus-linked"));
    assert_eq!(progress[0].source_date.as_deref(), Some("2026-07-13"));

    database
        .start_focus_session("focus-legacy", "2026-07-13", "Legacy", 25, 2_000)
        .unwrap();
    assert_eq!(
        database.list_focus_sessions(0, 100_000).unwrap()[1].task_id,
        None
    );
}

#[test]
fn confirmed_daily_actual_output_retries_without_duplicate_progress() {
    let database = Database::open_in_memory().unwrap();
    database
        .save_daily_goal(
            "2026-07-13",
            "Ship ledger",
            "Working backend",
            "Finished the atomic backend",
        )
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-actual".into(),
            name: "Actual".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    let task = service
        .create_task(NewTask {
            id: "task-actual".into(),
            project_id: project.id,
            title: "Ship ledger".into(),
            priority: TaskPriority::High,
            expected_output: "Working backend".into(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();

    let first = service
        .record_daily_actual_output_progress(&task.id, "2026-07-13", 2_000)
        .unwrap()
        .unwrap();
    let retry = service
        .record_daily_actual_output_progress(&task.id, "2026-07-13", 3_000)
        .unwrap()
        .unwrap();
    assert_eq!(retry.id, first.id);
    let progress = service.list_progress_entries(&task.id).unwrap();
    assert_eq!(progress.len(), 1);
    assert_eq!(progress[0].note, "Finished the atomic backend");
    assert_eq!(
        progress[0].origin_kind,
        ProgressOriginKind::DailyActualOutput
    );
    assert_eq!(progress[0].source_id.as_deref(), Some("2026-07-13"));
    assert_eq!(progress[0].source_date.as_deref(), Some("2026-07-13"));
}

#[test]
fn manual_assignment_replaces_an_ai_suggestion_without_touching_activity_classification() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("segment-1", 1_000, 2_000))
        .unwrap();
    let repository = WorkLedgerRepository::new(&database);
    let service = WorkLedgerService::new(repository);
    let project = service
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Desktop rewrite".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    let task = service
        .create_task(NewTask {
            id: "task-1".into(),
            project_id: project.id,
            title: "Work ledger".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();

    service
        .assign_activity(
            &task.id,
            "segment-1",
            EvidenceProvenance::Ai,
            0.5,
            "model suggestion",
            2_000,
        )
        .unwrap();
    service
        .assign_activity(
            &task.id,
            "segment-1",
            EvidenceProvenance::Manual,
            1.0,
            "confirmed by user",
            3_000,
        )
        .unwrap();

    let link = service
        .list_activity_links(&task.id)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(link.provenance, EvidenceProvenance::Manual);
    assert_eq!(link.reason, "confirmed by user");
    assert_eq!(
        database.list_segments(0, 3_000).unwrap()[0].category,
        ActivityCategory::CreationDevelopment
    );
}

#[test]
fn ledger_query_clips_activity_evidence_and_keeps_source_metadata() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("linked-segment", 1_000, 6_000))
        .unwrap();
    database
        .insert_segment(&segment("unassigned-segment", 3_000, 4_000))
        .unwrap();
    database
        .insert_browser_visit(
            "visit-1",
            "Chrome",
            "Default",
            &BrowserVisit {
                url: "https://docs.rs/work-ledger".into(),
                title: "Work ledger API".into(),
                visited_at_ms: 4_000,
            },
            "docs.rs",
        )
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Desktop rewrite".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    let task = service
        .create_task(NewTask {
            id: "task-1".into(),
            project_id: project.id,
            title: "Work ledger commands".into(),
            priority: TaskPriority::Medium,
            expected_output: "Query contract".into(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();
    service
        .assign_activity(
            &task.id,
            "linked-segment",
            EvidenceProvenance::Manual,
            1.0,
            "confirmed",
            2_000,
        )
        .unwrap();
    service
        .add_progress_entry(daily_task_monitor_core::work_ledger::NewProgressEntry {
            id: "progress-1".into(),
            task_id: task.id,
            note: "Command shape agreed".into(),
            created_at_ms: 3_000,
        })
        .unwrap();

    let ledger = service.get_work_ledger(2_000, 6_000, None).unwrap();

    assert_eq!(ledger.projects.len(), 1);
    assert_eq!(ledger.tasks.len(), 1);
    assert_eq!(ledger.progress.len(), 1);
    assert_eq!(ledger.linked_evidence.len(), 1);
    assert_eq!(ledger.unassigned_evidence.len(), 2);
    assert_eq!(ledger.summary.linked_evidence_count, 1);
    assert_eq!(ledger.summary.unassigned_evidence_count, 2);
    let linked = &ledger.linked_evidence[0];
    assert_eq!(linked.evidence.id, "linked-segment");
    assert_eq!(linked.evidence.duration_seconds, 4);
    assert_eq!(linked.evidence.application, "Codex");
    assert_eq!(linked.evidence.title, "Desktop rewrite");
    assert_eq!(linked.evidence.classification_source, "rule");
    assert_eq!(linked.evidence.classification_reason, "fixture");
    assert!((linked.evidence.classification_confidence.unwrap() - 0.9).abs() < 0.000_001);
    let browser = ledger
        .unassigned_evidence
        .iter()
        .find(|evidence| evidence.kind == "browser")
        .unwrap();
    assert_eq!(browser.application, "Chrome");
    assert_eq!(browser.title, "Work ledger API");
    assert_eq!(browser.domain, "docs.rs");
    assert_eq!(browser.duration_seconds, 0);
    assert_eq!(browser.classification_source, "unclassified");
    assert_eq!(browser.classification_confidence, None);
}

#[test]
fn local_suggestions_are_deterministic_and_learn_from_manual_evidence() {
    let database = Database::open_in_memory().unwrap();
    database
        .save_daily_goal(
            "2026-07-13",
            "Document the work ledger command bridge",
            "Documentation",
            "",
        )
        .unwrap();
    database
        .start_focus_session(
            "focus-1",
            "2026-07-13",
            "Write work ledger documentation",
            25,
            1_000,
        )
        .unwrap();
    database
        .insert_segment(&ActivitySegmentRecord {
            app: "VS Code".into(),
            title: "Work ledger documentation".into(),
            ..segment("manual-segment", 1_000, 2_000)
        })
        .unwrap();
    database
        .insert_segment(&ActivitySegmentRecord {
            app: "VS Code".into(),
            title: "Work ledger command reference".into(),
            ..segment("candidate-segment", 3_000, 4_000)
        })
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Desktop rewrite".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    let docs = service
        .create_task(NewTask {
            id: "docs".into(),
            project_id: project.id.clone(),
            title: "Work ledger documentation".into(),
            priority: TaskPriority::Medium,
            expected_output: "Command reference".into(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();
    service
        .create_task(NewTask {
            id: "other".into(),
            project_id: project.id,
            title: "Release checklist".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();
    service
        .assign_activity(
            &docs.id,
            "manual-segment",
            EvidenceProvenance::Manual,
            1.0,
            "confirmed documentation work",
            2_000,
        )
        .unwrap();

    let first = service.get_work_ledger(0, 5_000, None).unwrap();
    let second = service.get_work_ledger(0, 5_000, None).unwrap();
    let suggestion = first
        .suggestions
        .iter()
        .find(|suggestion| suggestion.evidence_id == "candidate-segment")
        .unwrap();

    assert_eq!(suggestion.task_id, "docs");
    assert!(suggestion.confidence >= 0.70);
    assert!(suggestion.can_auto_apply);
    assert_eq!(first.suggestions, second.suggestions);
}

#[test]
fn ambiguous_unassigned_evidence_queues_one_deduplicated_optional_ai_job_but_linked_evidence_does_not()
 {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("linked-segment", 1_000, 2_000))
        .unwrap();
    database
        .insert_segment(&segment("ambiguous-segment", 3_000, 4_000))
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Test project".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    let task = service
        .create_task(NewTask {
            id: "task-1".into(),
            project_id: project.id,
            title: "Unrelated task".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();
    service
        .assign_activity(
            &task.id,
            "linked-segment",
            EvidenceProvenance::Manual,
            1.0,
            "confirmed",
            2_000,
        )
        .unwrap();

    let execution = ai_snapshot();
    let first = service
        .get_work_ledger_with_ai_queue(0, 5_000, None, Some(&execution))
        .unwrap();
    assert_eq!(database.ai_job_count().unwrap(), 1);
    assert_eq!(first.ambiguous_evidence_hashes.len(), 1);
    let queued = database.next_due_ai_job(i64::MAX).unwrap().unwrap();
    assert_eq!(queued.kind, "work_ledger_assignment");
    assert_eq!(queued.execution.execution_mode, AiExecutionMode::ApiKey);
    assert_eq!(queued.execution.executor_id, "openai");
    assert_eq!(queued.execution.model, "gpt-test");
    assert!(
        queued
            .payload_json
            .contains(&first.ambiguous_evidence_hashes[0])
    );

    let second = service
        .get_work_ledger_with_ai_queue(0, 5_000, None, Some(&execution))
        .unwrap();
    assert_eq!(database.ai_job_count().unwrap(), 1);
    assert_eq!(
        first.ambiguous_evidence_hashes,
        second.ambiguous_evidence_hashes
    );
}

#[test]
fn explicit_ai_suggestion_queue_reports_only_newly_inserted_jobs() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("ambiguous-segment", 1_000, 2_000))
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-count".into(),
            name: "Count project".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    service
        .create_task(NewTask {
            id: "task-count".into(),
            project_id: project.id,
            title: "Unrelated task".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();
    let execution = ai_snapshot();

    assert_eq!(
        service
            .queue_workflow_ai_suggestions(0, 5_000, &execution)
            .unwrap(),
        1
    );
    assert_eq!(
        service
            .queue_workflow_ai_suggestions(0, 5_000, &execution)
            .unwrap(),
        0
    );
    assert_eq!(database.ai_job_count().unwrap(), 1);
}

#[test]
fn codex_work_ledger_snapshot_is_preserved_when_ambiguous_evidence_is_queued() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("ambiguous-segment", 1_000, 2_000))
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-codex".into(),
            name: "Codex project".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    service
        .create_task(NewTask {
            id: "task-codex".into(),
            project_id: project.id,
            title: "Unrelated task".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();
    let execution = codex_snapshot();

    let snapshot = service
        .get_work_ledger_with_ai_queue(0, 5_000, None, Some(&execution))
        .unwrap();

    assert_eq!(snapshot.ambiguous_evidence_hashes.len(), 1);
    let queued = database.next_due_ai_job(i64::MAX).unwrap().unwrap();
    assert_eq!(queued.execution.execution_mode, AiExecutionMode::Codex);
    assert_eq!(queued.execution.executor_id, r"C:\Tools\codex.exe");
    assert_eq!(queued.execution.model, "gpt-5-codex");
    assert!(queued.execution.evidence_hash.starts_with("episode-"));
    let payload = daily_task_monitor_core::work_ledger::parse_work_ledger_assignment_job(
        &queued.payload_json,
    )
    .unwrap();
    assert_eq!(payload.protocol_version, 2);
    assert_eq!(payload.fragments.len(), 1);
    assert_eq!(
        payload.fragments[0].evidence_hash,
        snapshot.ambiguous_evidence_hashes[0]
    );
}

#[test]
fn ambiguous_evidence_without_task_candidates_does_not_queue_ai_assignment() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("ambiguous-segment", 1_000, 2_000))
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));

    let snapshot = service.get_work_ledger(0, 5_000, None).unwrap();

    assert_eq!(snapshot.summary.task_count, 0);
    assert_eq!(snapshot.ambiguous_evidence_hashes.len(), 1);
    assert_eq!(database.ai_job_count().unwrap(), 0);
}

#[test]
fn invalid_queued_ai_assignment_is_completed_instead_of_retried() {
    let database = Database::open_in_memory().unwrap();
    let payload = serde_json::json!({
        "evidence": {
            "kind": "activity",
            "id": "segment-1",
            "startedAtMs": 1_000,
            "endedAtMs": 2_000,
            "app": "ChatGPT",
            "title": "Daily analysis",
            "url": "",
            "domain": "",
            "category": "creationDevelopment",
            "classificationSource": "pending",
            "classificationReason": "fixture",
            "classificationConfidence": null,
            "evidenceHash": "evidence-hash"
        },
        "evidenceHash": "evidence-hash",
        "startMs": 0,
        "endMs": 5_000,
        "tasks": []
    })
    .to_string();
    database
        .enqueue_ai_job_for_subject(
            "work_ledger_assignment",
            "segment-1",
            &payload,
            1_000,
            &ai_snapshot(),
        )
        .unwrap();
    let job = database.claim_next_due_ai_job(1_000).unwrap().unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));

    assert!(
        service
            .complete_invalid_ai_assignment_job(&job, "validator", "queued-payload-v1", Some(0),)
            .unwrap()
    );
    let completed = database.get_ai_job(&job.id).unwrap().unwrap();
    assert_eq!(completed.status, AiJobStatus::Complete);
    assert_eq!(completed.attempts, 0);
    assert_eq!(completed.executor_id, Some("validator".into()));
    assert_eq!(completed.model, Some("queued-payload-v1".into()));
    assert_eq!(completed.exit_code, Some(0));
    assert_eq!(completed.error_kind, Some(AiExecutionErrorKind::InvalidJob));
    assert!(
        completed
            .last_error
            .contains("Queued work ledger assignment"),
        "{}",
        completed.last_error
    );
}

#[test]
fn superseded_invalid_queued_ai_assignment_retires_the_old_generation() {
    let database = Database::open_in_memory().unwrap();
    let invalid_payload = serde_json::json!({
        "evidence": null,
        "evidenceHash": "obsolete-evidence",
        "startMs": 0,
        "endMs": 5_000,
        "tasks": []
    })
    .to_string();
    let old_id = database
        .enqueue_ai_job_for_subject(
            "work_ledger_assignment",
            "segment-obsolete",
            &invalid_payload,
            1_000,
            &ai_snapshot(),
        )
        .unwrap();
    let old_run = database.claim_next_due_ai_job(1_000).unwrap().unwrap();
    let current_id = database
        .force_enqueue_ai_job_for_subject(
            "work_ledger_assignment",
            "segment-obsolete",
            &invalid_payload,
            2_000,
            &codex_snapshot(),
        )
        .unwrap();
    let current_run = database.claim_next_due_ai_job(2_000).unwrap().unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));

    assert!(
        !service
            .complete_invalid_ai_assignment_job(
                &old_run,
                "validator",
                "queued-payload-v1",
                Some(0),
            )
            .unwrap()
    );

    let retired = database.get_ai_job(&old_id).unwrap().unwrap();
    assert_eq!(retired.status, AiJobStatus::Complete);
    assert_eq!(retired.attempts, old_run.attempts);
    assert!(
        retired
            .last_error
            .to_ascii_lowercase()
            .contains("superseded")
    );
    assert_eq!(retired.error_kind, None);
    assert_eq!(
        database.get_ai_job(&current_id).unwrap().unwrap(),
        current_run
    );
}

#[test]
fn work_ledger_assignment_consumption_errors_map_to_audit_kinds() {
    use daily_task_monitor_core::work_ledger::WorkLedgerAssignmentConsumptionError;

    assert_eq!(
        WorkLedgerAssignmentConsumptionError::InvalidJob("bad payload".into()).ai_error_kind(),
        AiExecutionErrorKind::InvalidJob
    );
    assert_eq!(
        WorkLedgerAssignmentConsumptionError::InvalidResponse("bad response".into())
            .ai_error_kind(),
        AiExecutionErrorKind::InvalidResponse
    );
    assert_eq!(
        WorkLedgerAssignmentConsumptionError::Persistence("database locked".into()).ai_error_kind(),
        AiExecutionErrorKind::Persistence
    );
}

#[test]
fn manual_assignment_is_global_and_can_safely_move_evidence_between_tasks() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("segment-1", 1_000, 2_000))
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Desktop rewrite".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    for task_id in ["task-1", "task-2"] {
        service
            .create_task(NewTask {
                id: task_id.into(),
                project_id: project.id.clone(),
                title: task_id.into(),
                priority: TaskPriority::Medium,
                expected_output: String::new(),
                due_date: None,
                created_at_ms: 1_000,
            })
            .unwrap();
    }

    assert!(
        service
            .assign_activity(
                "task-1",
                "segment-1",
                EvidenceProvenance::Ai,
                0.8,
                "ai",
                2_000
            )
            .unwrap()
    );
    assert!(
        !service
            .assign_activity(
                "task-2",
                "segment-1",
                EvidenceProvenance::Rule,
                0.9,
                "rule",
                2_500
            )
            .unwrap()
    );
    assert!(service.list_activity_links("task-2").unwrap().is_empty());
    assert!(
        service
            .assign_activity(
                "task-2",
                "segment-1",
                EvidenceProvenance::Manual,
                1.0,
                "confirmed",
                3_000
            )
            .unwrap()
    );
    assert!(service.list_activity_links("task-1").unwrap().is_empty());
    let task_two_link = service
        .list_activity_links("task-2")
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(task_two_link.provenance, EvidenceProvenance::Manual);
    assert!(
        !service
            .assign_activity(
                "task-1",
                "segment-1",
                EvidenceProvenance::Rule,
                0.99,
                "rule",
                4_000
            )
            .unwrap()
    );

    assert!(
        service
            .assign_activity(
                "task-1",
                "segment-1",
                EvidenceProvenance::Manual,
                1.0,
                "moved",
                5_000
            )
            .unwrap()
    );
    assert!(service.list_activity_links("task-2").unwrap().is_empty());
    let task_one_link = service
        .list_activity_links("task-1")
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(task_one_link.provenance, EvidenceProvenance::Manual);
    assert_eq!(task_one_link.reason, "moved");
}

#[test]
fn focus_sessions_use_a_half_open_range_at_the_lower_boundary() {
    let database = Database::open_in_memory().unwrap();
    database
        .start_focus_session("ends-at-start", "2026-07-13", "outside", 25, 500)
        .unwrap();
    database
        .complete_focus_session("ends-at-start", 1_000, "done")
        .unwrap();
    database
        .start_focus_session("overlaps-start", "2026-07-13", "inside", 25, 900)
        .unwrap();
    database
        .complete_focus_session("overlaps-start", 1_001, "done")
        .unwrap();

    let sessions = database.list_focus_sessions(1_000, 2_000).unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "overlaps-start");
}

#[test]
fn suggested_assignment_uses_only_a_current_server_generated_suggestion() {
    let database = Database::open_in_memory().unwrap();
    database
        .save_daily_goal("2026-07-13", "ledger", "", "")
        .unwrap();
    database
        .insert_segment(&ActivitySegmentRecord {
            app: String::new(),
            title: "ledger".into(),
            ..segment("manual-segment", 1_000, 2_000)
        })
        .unwrap();
    database
        .insert_segment(&ActivitySegmentRecord {
            app: String::new(),
            title: "ledger".into(),
            ..segment("candidate-segment", 2_000, 3_000)
        })
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Desktop rewrite".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    let task = service
        .create_task(NewTask {
            id: "task-1".into(),
            project_id: project.id,
            title: "ledger".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();
    let other_task = service
        .create_task(NewTask {
            id: "task-2".into(),
            project_id: "project-1".into(),
            title: "other".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();
    service
        .assign_activity(
            &task.id,
            "manual-segment",
            EvidenceProvenance::Manual,
            1.0,
            "confirmed",
            2_000,
        )
        .unwrap();
    let suggestion = service
        .get_work_ledger(0, 4_000, None)
        .unwrap()
        .suggestions
        .into_iter()
        .find(|item| item.evidence_id == "candidate-segment")
        .unwrap();

    assert!(
        !service
            .apply_suggested_assignment(
                &other_task.id,
                "activity",
                "candidate-segment",
                &suggestion.evidence_hash,
                "local",
                0,
                4_000,
                0.9,
                "arbitrary client task",
                2_000,
            )
            .unwrap()
    );
    assert!(
        !service
            .apply_suggested_assignment(
                &task.id,
                "activity",
                "candidate-segment",
                "stale",
                "local",
                0,
                4_000,
                0.0,
                "arbitrary client confidence",
                2_000,
            )
            .unwrap()
    );
    assert!(
        service
            .apply_suggested_assignment(
                &task.id,
                "activity",
                "candidate-segment",
                &suggestion.evidence_hash,
                "local",
                0,
                4_000,
                0.0,
                "arbitrary client confidence",
                2_000,
            )
            .unwrap()
    );
    service
        .assign_activity(
            &other_task.id,
            "candidate-segment",
            EvidenceProvenance::Manual,
            1.0,
            "confirmed",
            3_000,
        )
        .unwrap();
    assert!(
        !service
            .apply_suggested_assignment(
                &task.id,
                "activity",
                "candidate-segment",
                &suggestion.evidence_hash,
                "local",
                0,
                4_000,
                0.95,
                "later local suggestion",
                4_000,
            )
            .unwrap()
    );
    assert_eq!(
        service.list_activity_links(&other_task.id).unwrap()[0].provenance,
        EvidenceProvenance::Manual
    );
}

#[test]
fn low_confidence_ai_completion_is_audited_without_creating_a_confirmation_step() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("candidate-segment", 1_000, 2_000))
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Desktop rewrite".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    service
        .create_task(NewTask {
            id: "task-1".into(),
            project_id: project.id,
            title: "Unrelated task".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();

    service
        .get_work_ledger_with_ai_queue(0, 3_000, None, Some(&ai_snapshot()))
        .unwrap();
    let job = database.claim_next_due_ai_job(i64::MAX).unwrap().unwrap();
    let finished_at_ms = job.started_at_ms.unwrap().saturating_add(777);

    assert!(
        service
            .consume_ai_assignment_job_response(
                &job,
                r#"{"taskId":"task-1","confidence":0.82}"#,
                "openai",
                "gpt-test",
                finished_at_ms,
                777,
            )
            .unwrap()
    );
    assert!(service.list_activity_links("task-1").unwrap().is_empty());
    let completed = database.get_ai_job(&job.id).unwrap().unwrap();
    assert_eq!(completed.status, AiJobStatus::Complete);
    assert_eq!(completed.executor_id, Some("openai".into()));
    assert_eq!(completed.model, Some("gpt-test".into()));
    assert_eq!(completed.exit_code, Some(0));
    assert_eq!(completed.finished_at_ms, Some(finished_at_ms));
    assert_eq!(completed.duration_ms, Some(777));
    let persisted = service.get_work_ledger(0, 3_000, None).unwrap();
    assert!(
        persisted
            .suggestions
            .iter()
            .all(|suggestion| suggestion.source != "ai")
    );
    assert!(
        database
            .list_ai_reviews(&AiReviewFilter {
                kinds: vec![AiReviewKind::WorkflowAssignment],
                subject_id: Some("candidate-segment".into()),
                ..AiReviewFilter::default()
            })
            .unwrap()
            .is_empty()
    );
    assert!(
        database
            .list_work_ledger_ai_suggestions()
            .unwrap()
            .is_empty()
    );
    let segment = database.list_segments(0, 3_000).unwrap().remove(0);
    assert_eq!(segment.source, ClassificationSource::Rule);
    assert_eq!(segment.reason, "fixture");
}

#[test]
fn legacy_ai_suggestion_apply_rejections_have_no_side_effects() {
    for rejection in ["no-pending-review", "stale-evidence", "manual-owner"] {
        let database = Database::open_in_memory().unwrap();
        database
            .insert_segment(&segment("candidate-segment", 1_000, 2_000))
            .unwrap();
        let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
        let project = service
            .create_project(NewProject {
                id: "project-1".into(),
                name: "Desktop rewrite".into(),
                color: "#3182ce".into(),
                description: String::new(),
                created_at_ms: 1_000,
            })
            .unwrap();
        service
            .create_task(NewTask {
                id: "task-1".into(),
                project_id: project.id,
                title: "Unrelated task".into(),
                priority: TaskPriority::Medium,
                expected_output: String::new(),
                due_date: None,
                created_at_ms: 1_000,
            })
            .unwrap();
        service
            .get_work_ledger_with_ai_queue(0, 3_000, None, Some(&ai_snapshot()))
            .unwrap();
        let job = database.claim_next_due_ai_job(i64::MAX).unwrap().unwrap();
        let queued = parse_work_ledger_assignment_job(&job.payload_json).unwrap();
        assert!(
            service
                .complete_ai_assignment_suggestion_if_current(
                    &job.id,
                    job.generation,
                    &queued.evidence.kind,
                    &queued.evidence.id,
                    &queued.evidence_hash,
                    "task-1",
                    0.82,
                    "openai",
                    "gpt-test",
                    queued.start_ms,
                    queued.end_ms,
                    2_000,
                    100,
                    true,
                )
                .unwrap()
        );
        let suggestion = database
            .list_work_ledger_ai_suggestions()
            .unwrap()
            .remove(0);
        let review = database
            .list_ai_reviews(&AiReviewFilter {
                kinds: vec![AiReviewKind::WorkflowAssignment],
                subject_id: Some("candidate-segment".into()),
                ..AiReviewFilter::default()
            })
            .unwrap()
            .remove(0);

        match rejection {
            "no-pending-review" => {
                database
                    .resolve_ai_review(&daily_task_monitor_core::ai_review::AiReviewResolution {
                        review_ids: vec![review.id.clone()],
                        action: daily_task_monitor_core::ai_review::AiReviewAction::Ignore,
                        changed_json: None,
                        evidence_hashes: std::collections::BTreeMap::from([(
                            review.id.clone(),
                            review.evidence_hash.clone(),
                        )]),
                        resolved_at_ms: 3_000,
                    })
                    .unwrap();
            }
            "stale-evidence" => {
                database
                    .save_manual_classification(
                        "candidate-segment",
                        ActivityCategory::Research,
                        VideoPurpose::Unknown,
                        "changed after queueing",
                    )
                    .unwrap();
            }
            "manual-owner" => database
                .claim_manual_field_ownership(
                    AiReviewKind::WorkflowAssignment,
                    "candidate-segment",
                    "user",
                    3_000,
                )
                .unwrap(),
            _ => unreachable!(),
        }

        let result = service.apply_suggested_assignment(
            &suggestion.task_id,
            &suggestion.evidence_kind,
            &suggestion.evidence_id,
            &suggestion.evidence_hash,
            "ai",
            0,
            3_000,
            suggestion.confidence,
            &suggestion.reason,
            4_000,
        );
        if rejection == "manual-owner" {
            assert!(result.is_err(), "{rejection} must reject the legacy apply");
        } else {
            assert!(!result.unwrap(), "{rejection} must reject the legacy apply");
        }
        assert!(service.list_activity_links("task-1").unwrap().is_empty());
        assert_eq!(
            database.list_work_ledger_ai_suggestions().unwrap().len(),
            usize::from(rejection != "no-pending-review")
        );
        let persisted_review = database.get_ai_review(&review.id).unwrap().unwrap();
        if rejection == "no-pending-review" {
            assert_eq!(persisted_review.state, AiReviewState::Dismissed);
            assert_eq!(
                database.list_ai_review_events(&review.id).unwrap(),
                vec![AiReviewEventKind::Generated, AiReviewEventKind::Ignored]
            );
        } else {
            assert_eq!(persisted_review.state, AiReviewState::Pending);
            assert_eq!(
                database.list_ai_review_events(&review.id).unwrap(),
                vec![AiReviewEventKind::Generated]
            );
        }
    }
}

#[test]
fn batch_accept_rejects_multiple_workflow_reviews_for_one_subject() {
    let database = Database::open_in_memory().unwrap();
    let (service, old_review, current_review, suggestion) =
        two_generation_workflow_reviews(&database);

    let result = database.resolve_ai_review(&AiReviewResolution {
        review_ids: vec![old_review.id.clone(), current_review.id.clone()],
        action: AiReviewAction::Accept,
        changed_json: None,
        evidence_hashes: std::collections::BTreeMap::from([
            (old_review.id.clone(), old_review.evidence_hash.clone()),
            (
                current_review.id.clone(),
                current_review.evidence_hash.clone(),
            ),
        ]),
        resolved_at_ms: 5_000,
    });

    let error = result.unwrap_err().to_string();
    assert!(error.contains("distinct subjects"), "{error}");
    for review in [&old_review, &current_review] {
        assert_eq!(
            database.get_ai_review(&review.id).unwrap().unwrap().state,
            AiReviewState::Pending
        );
        assert_eq!(
            database.list_ai_review_events(&review.id).unwrap(),
            vec![AiReviewEventKind::Generated]
        );
    }
    assert!(service.list_activity_links("old-task").unwrap().is_empty());
    assert!(service.list_activity_links("new-task").unwrap().is_empty());
    assert!(
        !database
            .has_manual_field_ownership(AiReviewKind::WorkflowAssignment, "generation-segment",)
            .unwrap()
    );
    let retained = database.list_work_ledger_ai_suggestions().unwrap();
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].evidence_kind, suggestion.evidence_kind);
    assert_eq!(retained[0].evidence_id, suggestion.evidence_id);
    assert_eq!(retained[0].evidence_hash, suggestion.evidence_hash);
    assert_eq!(retained[0].task_id, suggestion.task_id);
}

#[test]
fn legacy_apply_accepts_only_the_review_bound_to_the_current_generation_suggestion() {
    let path = unique_database_path("workflow-review-binding");
    let database = Database::open(&path).unwrap();
    let (service, old_review, new_review, suggestion) = two_generation_workflow_reviews(&database);
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE ai_review_records SET created_at_ms=999999 WHERE id=?1",
            [&old_review.id],
        )
        .unwrap();
    drop(connection);

    assert!(
        service
            .apply_suggested_assignment(
                &suggestion.task_id,
                &suggestion.evidence_kind,
                &suggestion.evidence_id,
                &suggestion.evidence_hash,
                "ai",
                0,
                3_000,
                suggestion.confidence,
                &suggestion.reason,
                5_000,
            )
            .unwrap()
    );

    assert!(service.list_activity_links("old-task").unwrap().is_empty());
    let links = service.list_activity_links("new-task").unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].provenance, EvidenceProvenance::Manual);
    assert_eq!(
        database
            .get_ai_review(&old_review.id)
            .unwrap()
            .unwrap()
            .state,
        AiReviewState::Pending
    );
    assert_eq!(
        database
            .get_ai_review(&new_review.id)
            .unwrap()
            .unwrap()
            .state,
        AiReviewState::ManualOverride
    );
    assert_eq!(
        database.list_ai_review_events(&old_review.id).unwrap(),
        vec![AiReviewEventKind::Generated]
    );
    assert_eq!(
        database.list_ai_review_events(&new_review.id).unwrap(),
        vec![AiReviewEventKind::Generated, AiReviewEventKind::Accepted]
    );
    assert!(
        database
            .list_work_ledger_ai_suggestions()
            .unwrap()
            .is_empty()
    );
    assert!(
        database
            .has_manual_field_ownership(AiReviewKind::WorkflowAssignment, "generation-segment",)
            .unwrap()
    );

    database
        .resolve_ai_review(&AiReviewResolution {
            review_ids: vec![old_review.id.clone()],
            action: AiReviewAction::Ignore,
            changed_json: None,
            evidence_hashes: std::collections::BTreeMap::from([(
                old_review.id.clone(),
                old_review.evidence_hash.clone(),
            )]),
            resolved_at_ms: 6_000,
        })
        .unwrap();
    assert_eq!(
        database
            .get_ai_review(&old_review.id)
            .unwrap()
            .unwrap()
            .state,
        AiReviewState::Dismissed
    );
    assert_eq!(
        database.list_ai_review_events(&old_review.id).unwrap(),
        vec![AiReviewEventKind::Generated, AiReviewEventKind::Ignored]
    );
    assert!(service.list_activity_links("old-task").unwrap().is_empty());
    assert_eq!(service.list_activity_links("new-task").unwrap(), links);
    assert!(
        database
            .has_manual_field_ownership(AiReviewKind::WorkflowAssignment, "generation-segment",)
            .unwrap()
    );
    drop(service);
    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn workflow_review_can_be_ignored_after_a_direct_manual_assignment() {
    let database = Database::open_in_memory().unwrap();
    let (service, _old_review, current_review, _suggestion) =
        two_generation_workflow_reviews(&database);

    assert!(
        service
            .assign_activity(
                "old-task",
                "generation-segment",
                EvidenceProvenance::Manual,
                1.0,
                "direct user assignment",
                5_000,
            )
            .unwrap()
    );
    let manual_links = service.list_activity_links("old-task").unwrap();
    assert_eq!(manual_links.len(), 1);
    assert!(service.list_activity_links("new-task").unwrap().is_empty());
    let retained_suggestions = database.list_work_ledger_ai_suggestions().unwrap();
    assert_eq!(retained_suggestions.len(), 1);
    assert_eq!(retained_suggestions[0].task_id, "new-task");
    assert!(
        database
            .has_manual_field_ownership(AiReviewKind::WorkflowAssignment, "generation-segment",)
            .unwrap()
    );

    database
        .resolve_ai_review(&AiReviewResolution {
            review_ids: vec![current_review.id.clone()],
            action: AiReviewAction::Ignore,
            changed_json: None,
            evidence_hashes: std::collections::BTreeMap::from([(
                current_review.id.clone(),
                current_review.evidence_hash.clone(),
            )]),
            resolved_at_ms: 6_000,
        })
        .unwrap();

    assert_eq!(
        database
            .get_ai_review(&current_review.id)
            .unwrap()
            .unwrap()
            .state,
        AiReviewState::Dismissed
    );
    assert_eq!(
        database.list_ai_review_events(&current_review.id).unwrap(),
        vec![AiReviewEventKind::Generated, AiReviewEventKind::Ignored]
    );
    assert_eq!(
        service.list_activity_links("old-task").unwrap(),
        manual_links
    );
    assert!(service.list_activity_links("new-task").unwrap().is_empty());
    assert!(
        database
            .list_work_ledger_ai_suggestions()
            .unwrap()
            .is_empty()
    );
    assert!(
        database
            .has_manual_field_ownership(AiReviewKind::WorkflowAssignment, "generation-segment",)
            .unwrap()
    );
}

#[test]
fn legacy_apply_does_not_fall_back_to_an_older_pending_review() {
    let path = unique_database_path("workflow-review-no-fallback");
    let database = Database::open(&path).unwrap();
    let (service, old_review, new_review, suggestion) = two_generation_workflow_reviews(&database);
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE ai_review_records SET state='dismissed', resolved_at_ms=5000 WHERE id=?1",
            [&new_review.id],
        )
        .unwrap();
    drop(connection);

    let result = service.apply_suggested_assignment(
        &suggestion.task_id,
        &suggestion.evidence_kind,
        &suggestion.evidence_id,
        &suggestion.evidence_hash,
        "ai",
        0,
        3_000,
        suggestion.confidence,
        &suggestion.reason,
        6_000,
    );
    assert!(result.is_err() || !result.unwrap());
    assert!(service.list_activity_links("old-task").unwrap().is_empty());
    assert!(service.list_activity_links("new-task").unwrap().is_empty());
    assert_eq!(
        database
            .get_ai_review(&old_review.id)
            .unwrap()
            .unwrap()
            .state,
        AiReviewState::Pending
    );
    assert_eq!(
        database
            .get_ai_review(&new_review.id)
            .unwrap()
            .unwrap()
            .state,
        AiReviewState::Dismissed
    );
    drop(service);
    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn workflow_ignore_deletes_only_the_suggestion_bound_to_that_review() {
    let database = Database::open_in_memory().unwrap();
    let (service, old_review, new_review, suggestion) = two_generation_workflow_reviews(&database);

    database
        .resolve_ai_review(&AiReviewResolution {
            review_ids: vec![old_review.id.clone()],
            action: AiReviewAction::Ignore,
            changed_json: None,
            evidence_hashes: std::collections::BTreeMap::from([(
                old_review.id.clone(),
                old_review.evidence_hash.clone(),
            )]),
            resolved_at_ms: 5_000,
        })
        .unwrap();
    let retained = database.list_work_ledger_ai_suggestions().unwrap();
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].task_id, "new-task");

    database
        .resolve_ai_review(&AiReviewResolution {
            review_ids: vec![new_review.id.clone()],
            action: AiReviewAction::Ignore,
            changed_json: None,
            evidence_hashes: std::collections::BTreeMap::from([(
                new_review.id.clone(),
                new_review.evidence_hash.clone(),
            )]),
            resolved_at_ms: 6_000,
        })
        .unwrap();
    assert!(
        database
            .list_work_ledger_ai_suggestions()
            .unwrap()
            .is_empty()
    );
    assert!(
        service
            .get_work_ledger(0, 3_000, None)
            .unwrap()
            .suggestions
            .iter()
            .all(|candidate| candidate.source != "ai")
    );
    assert!(
        !service
            .apply_suggested_assignment(
                &suggestion.task_id,
                &suggestion.evidence_kind,
                &suggestion.evidence_id,
                &suggestion.evidence_hash,
                "ai",
                0,
                3_000,
                suggestion.confidence,
                &suggestion.reason,
                7_000,
            )
            .unwrap()
    );
    assert!(service.list_activity_links("old-task").unwrap().is_empty());
    assert!(service.list_activity_links("new-task").unwrap().is_empty());
}

#[test]
fn ai_worker_rejects_malformed_or_unknown_task_results_without_side_effects() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("candidate-segment", 1_000, 2_000))
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Desktop rewrite".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    service
        .create_task(NewTask {
            id: "task-1".into(),
            project_id: project.id,
            title: "Unrelated task".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();

    for response in ["not json", r#"{"taskId":"unknown-task","confidence":0.82}"#] {
        service
            .get_work_ledger_with_ai_queue(0, 3_000, None, Some(&ai_snapshot()))
            .unwrap();
        let job = database.claim_next_due_ai_job(i64::MAX).unwrap().unwrap();
        assert!(
            service
                .consume_ai_assignment_job_response(&job, response, "openai", "gpt-test", 0, 0)
                .is_err()
        );
        assert!(
            database
                .list_work_ledger_ai_suggestions()
                .unwrap()
                .is_empty()
        );
        assert!(service.list_activity_links("task-1").unwrap().is_empty());
        let segment = database.list_segments(0, 3_000).unwrap().remove(0);
        assert_eq!(segment.source, ClassificationSource::Rule);
        assert_eq!(segment.reason, "fixture");
        database
            .fail_ai_job_generation(
                &job.id,
                job.generation,
                3_000,
                "rejected provider result",
                AiExecutionErrorKind::Provider,
                Some("openai"),
                Some("gpt-test"),
                None,
            )
            .unwrap();
        let retried = database.get_ai_job(&job.id).unwrap().unwrap();
        assert_eq!(
            retried.status,
            daily_task_monitor_core::ai::AiJobStatus::Pending
        );
        assert!(retried.attempts > 0);
    }
}

#[test]
fn ai_worker_discards_stale_evidence_without_persisting_a_suggestion() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("candidate-segment", 1_000, 2_000))
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Desktop rewrite".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    service
        .create_task(NewTask {
            id: "task-1".into(),
            project_id: project.id,
            title: "Unrelated task".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();
    service
        .get_work_ledger_with_ai_queue(0, 3_000, None, Some(&ai_snapshot()))
        .unwrap();
    let job = database.claim_next_due_ai_job(i64::MAX).unwrap().unwrap();
    let finished_at_ms = job.started_at_ms.unwrap().saturating_add(1_234);
    let mut changed = segment("candidate-segment", 1_000, 2_500);
    changed.title = "Changed after queue".into();
    database.upsert_native_segment(&changed).unwrap();

    assert!(
        !service
            .consume_ai_assignment_job_response(
                &job,
                r#"{"taskId":"task-1","confidence":1.0}"#,
                "openai",
                "gpt-test",
                finished_at_ms,
                1_234,
            )
            .unwrap()
    );
    assert!(
        database
            .list_work_ledger_ai_suggestions()
            .unwrap()
            .is_empty()
    );
    let completed = database.get_ai_job(&job.id).unwrap().unwrap();
    assert_eq!(completed.status, AiJobStatus::Complete);
    assert_eq!(completed.executor_id, Some("openai".into()));
    assert_eq!(completed.model, Some("gpt-test".into()));
    assert_eq!(completed.exit_code, Some(0));
    assert_eq!(completed.finished_at_ms, Some(finished_at_ms));
    assert_eq!(completed.duration_ms, Some(1_234));
}

#[test]
fn ai_worker_discards_manually_owned_evidence_without_replacing_ownership() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("candidate-segment", 1_000, 2_000))
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Desktop rewrite".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    let task = service
        .create_task(NewTask {
            id: "task-1".into(),
            project_id: project.id,
            title: "Unrelated task".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();

    service
        .get_work_ledger_with_ai_queue(0, 3_000, None, Some(&ai_snapshot()))
        .unwrap();
    let job = database.claim_next_due_ai_job(i64::MAX).unwrap().unwrap();
    let finished_at_ms = job.started_at_ms.unwrap().saturating_add(222);
    service
        .assign_activity(
            &task.id,
            "candidate-segment",
            EvidenceProvenance::Manual,
            1.0,
            "confirmed",
            3_500,
        )
        .unwrap();
    assert!(
        !service
            .consume_ai_assignment_job_response(
                &job,
                r#"{"taskId":"task-1","confidence":1.0}"#,
                "openai",
                "gpt-test",
                finished_at_ms,
                222,
            )
            .unwrap()
    );
    assert!(
        database
            .list_work_ledger_ai_suggestions()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        service.list_activity_links(&task.id).unwrap()[0].provenance,
        EvidenceProvenance::Manual
    );
    let completed = database.get_ai_job(&job.id).unwrap().unwrap();
    assert_eq!(completed.status, AiJobStatus::Complete);
    assert_eq!(completed.executor_id, Some("openai".into()));
    assert_eq!(completed.model, Some("gpt-test".into()));
    assert_eq!(completed.exit_code, Some(0));
    assert_eq!(completed.finished_at_ms, Some(finished_at_ms));
    assert_eq!(completed.duration_ms, Some(222));
}

#[test]
fn below_threshold_ai_result_stays_unassigned_without_confirmation() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("candidate-segment", 1_000, 2_000))
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Desktop rewrite".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    service
        .create_task(NewTask {
            id: "task-1".into(),
            project_id: project.id,
            title: "Unrelated task".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();
    service
        .get_work_ledger_with_ai_queue(0, 3_000, None, Some(&ai_snapshot()))
        .unwrap();
    let job = database.claim_next_due_ai_job(i64::MAX).unwrap().unwrap();

    assert!(
        service
            .consume_ai_assignment_job_response(
                &job,
                r#"{"taskId":"task-1","confidence":0.849999}"#,
                "openai",
                "gpt-test",
                job.started_at_ms.unwrap(),
                0,
            )
            .unwrap()
    );
    assert!(service.list_activity_links("task-1").unwrap().is_empty());
    let persisted = service.get_work_ledger(0, 3_000, None).unwrap();
    assert!(
        persisted
            .suggestions
            .iter()
            .filter(|suggestion| suggestion.source == "ai")
            .next()
            .is_none()
    );
}

#[test]
fn exact_threshold_ai_result_is_automatically_assigned() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("candidate-segment", 1_000, 2_000))
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Desktop rewrite".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    service
        .create_task(NewTask {
            id: "task-1".into(),
            project_id: project.id,
            title: "Unrelated task".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();
    service
        .get_work_ledger_with_ai_queue(0, 3_000, None, Some(&ai_snapshot()))
        .unwrap();
    let job = database.claim_next_due_ai_job(i64::MAX).unwrap().unwrap();

    assert!(
        service
            .consume_ai_assignment_job_response(
                &job,
                &format!(r#"{{"taskId":"task-1","confidence":{AI_AUTO_APPLY_THRESHOLD}}}"#),
                "openai",
                "gpt-test",
                job.started_at_ms.unwrap(),
                0,
            )
            .unwrap()
    );

    let links = service.list_activity_links("task-1").unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].provenance, EvidenceProvenance::Ai);
    assert!(
        service
            .get_work_ledger(0, 3_000, None)
            .unwrap()
            .suggestions
            .iter()
            .all(|suggestion| suggestion.source != "ai")
    );
}

#[test]
fn high_confidence_ai_match_with_a_close_runner_up_stays_unassigned() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("candidate-segment", 1_000, 2_000))
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Desktop rewrite".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    for (id, title) in [("task-1", "Candidate one"), ("task-2", "Candidate two")] {
        service
            .create_task(NewTask {
                id: id.into(),
                project_id: project.id.clone(),
                title: title.into(),
                priority: TaskPriority::Medium,
                expected_output: String::new(),
                due_date: None,
                created_at_ms: 1_000,
            })
            .unwrap();
    }
    service
        .get_work_ledger_with_ai_queue(0, 3_000, None, Some(&ai_snapshot()))
        .unwrap();
    let job = database.claim_next_due_ai_job(i64::MAX).unwrap().unwrap();
    let response = r#"{"decision":"match_existing","taskId":"task-1","projectId":null,"suggestedTitle":null,"confidence":0.95,"alternativeTaskId":"task-2","alternativeConfidence":0.85,"reasonCode":"title_overlap"}"#;

    assert!(
        service
            .consume_ai_assignment_job_response(
                &job,
                response,
                "openai",
                "gpt-test",
                job.started_at_ms.unwrap(),
                0,
            )
            .unwrap()
    );
    assert!(service.list_activity_links("task-1").unwrap().is_empty());
    assert!(service.list_activity_links("task-2").unwrap().is_empty());
    let snapshot = service.get_work_ledger(0, 3_000, None).unwrap();
    assert!(
        snapshot
            .suggestions
            .iter()
            .all(|suggestion| suggestion.source != "ai")
    );
}

#[test]
fn workflow_queue_bounds_large_real_world_episodes_before_persisting_the_job() {
    let database = Database::open_in_memory().unwrap();
    for index in 0..150 {
        let start_ms = 1_000 + index * 46_000;
        database
            .insert_segment(&ActivitySegmentRecord {
                app: "Microsoft Edge".into(),
                title: "雅思备考资料 - 搜索".into(),
                ..segment(
                    &format!("ielts-browser-{index}"),
                    start_ms,
                    start_ms + 45_000,
                )
            })
            .unwrap();
    }
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));

    assert_eq!(
        service
            .queue_workflow_ai_suggestions(0, 8_000_000, &ai_snapshot())
            .unwrap(),
        1
    );
    let job = database.claim_next_due_ai_job(i64::MAX).unwrap().unwrap();
    let payload = parse_work_ledger_assignment_job(&job.payload_json).unwrap();

    assert_eq!(payload.protocol_version, 2);
    assert_eq!(payload.fragments.len(), 64);
}

#[test]
fn related_browser_and_pdf_work_auto_create_one_workflow_with_two_tasks() {
    let database = Database::open_in_memory().unwrap();
    for (id, app, title, start_ms) in [
        (
            "ielts-browser",
            "Microsoft Edge",
            "雅思考试备考资料与报名要求",
            1_000,
        ),
        ("unrelated-code", "Codex", "实现桌面窗口状态同步", 701_000),
        (
            "ielts-pdf",
            "PDF Reader",
            "IELTS vocabulary 雅思核心词汇.pdf",
            1_301_000,
        ),
    ] {
        database
            .insert_segment(&ActivitySegmentRecord {
                app: app.into(),
                title: title.into(),
                ..segment(
                    id,
                    start_ms,
                    start_ms
                        + if id == "unrelated-code" {
                            300_000
                        } else {
                            600_000
                        },
                )
            })
            .unwrap();
    }
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    assert_eq!(
        service
            .queue_workflow_ai_suggestions(0, 2_000_000, &ai_snapshot())
            .unwrap(),
        1
    );
    let job = database.claim_next_due_ai_job(i64::MAX).unwrap().unwrap();
    let payload: serde_json::Value = serde_json::from_str(&job.payload_json).unwrap();
    let cluster_ids = payload["candidateClusters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["clusterId"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(cluster_ids.len(), 2);
    let response = serde_json::json!({
        "decision": "draft_new_workflow",
        "taskId": null,
        "projectId": null,
        "workflowName": "学习雅思",
        "description": "准备雅思考试并积累核心词汇",
        "tasks": [
            {
                "key": "research",
                "title": "检索雅思备考资料",
                "expectedOutput": "整理考试要求与备考资料",
                "clusterIds": [cluster_ids[0]]
            },
            {
                "key": "vocabulary",
                "title": "背诵雅思词汇",
                "expectedOutput": "完成核心词汇复习",
                "clusterIds": [cluster_ids[1]]
            }
        ],
        "confidence": 0.93,
        "reasonCode": "shared_goal"
    })
    .to_string();

    assert!(
        service
            .consume_ai_assignment_job_response(
                &job, &response, "openai", "gpt-test", 2_000_000, 120,
            )
            .unwrap()
    );
    let projects = service.list_projects(false).unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].name, "学习雅思");
    let reviews = database
        .list_ai_reviews(&AiReviewFilter {
            states: vec![AiReviewState::AutoApplied],
            ..AiReviewFilter::default()
        })
        .unwrap();
    assert_eq!(reviews.len(), 1);
    assert_eq!(reviews[0].kind, AiReviewKind::ProjectDraft);
    let proposal: serde_json::Value = serde_json::from_str(&reviews[0].proposed_json).unwrap();
    assert_eq!(proposal["name"], "学习雅思");
    assert_eq!(proposal["tasks"].as_array().unwrap().len(), 2);

    let tasks = service.list_tasks(&projects[0].id).unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(
        tasks
            .iter()
            .map(|task| service.list_activity_links(&task.id).unwrap().len())
            .sum::<usize>(),
        2
    );
    assert!(
        service
            .get_work_ledger(0, 2_000_000, None)
            .unwrap()
            .unassigned_evidence
            .iter()
            .any(|evidence| evidence.id == "unrelated-code")
    );
    assert_eq!(
        database
            .get_ai_review(&reviews[0].id)
            .unwrap()
            .unwrap()
            .state,
        AiReviewState::AutoApplied
    );
    let cancellation = service.cancel_ai_task(&tasks[0].id, 2_200_000).unwrap();
    assert_eq!(cancellation.task.status, TaskStatus::Cancelled);
    assert_eq!(cancellation.released_evidence_count, 1);
    assert_eq!(cancellation.dismissed_cluster_count, 1);
    assert!(!cancellation.project_archived);
    assert!(
        service
            .list_activity_links(&tasks[0].id)
            .unwrap()
            .is_empty()
    );
    assert!(service.list_browser_links(&tasks[0].id).unwrap().is_empty());
}

#[test]
fn legacy_create_new_response_is_audited_without_creating_an_inbox_project() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&ActivitySegmentRecord {
            app: "Codex".into(),
            title: "实现任务洞察".into(),
            ..segment("cluster-segment", 1_000, 1_801_000)
        })
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    assert_eq!(
        service
            .queue_workflow_ai_suggestions(0, 2_000_000, &ai_snapshot())
            .unwrap(),
        1
    );
    let job = database.claim_next_due_ai_job(i64::MAX).unwrap().unwrap();
    let response = r#"{"decision":"create_new","taskId":null,"projectId":null,"suggestedTitle":"实现任务洞察","confidence":0.93,"alternativeTaskId":null,"alternativeConfidence":null,"reasonCode":"new_work_cluster"}"#;

    assert!(
        service
            .consume_ai_assignment_job_response(
                &job, response, "openai", "gpt-test", 2_000_000, 120,
            )
            .unwrap()
    );
    assert!(service.list_projects(false).unwrap().is_empty());
    assert!(service.list_tasks("ai-task-inbox").unwrap().is_empty());
}

#[test]
fn two_independent_similar_fragments_queue_together_without_creating_an_inbox() {
    let database = Database::open_in_memory().unwrap();
    for (id, start_ms) in [("cluster-a", 1_000), ("cluster-b", 1_301_000)] {
        database
            .insert_segment(&ActivitySegmentRecord {
                app: "Codex".into(),
                title: "实现任务洞察".into(),
                ..segment(id, start_ms, start_ms + 600_000)
            })
            .unwrap();
    }
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    assert_eq!(
        service
            .queue_workflow_ai_suggestions(0, 2_000_000, &ai_snapshot())
            .unwrap(),
        1
    );
    let response = r#"{"decision":"create_new","taskId":null,"projectId":null,"suggestedTitle":"实现任务洞察","confidence":0.93,"alternativeTaskId":null,"alternativeConfidence":null,"reasonCode":"new_work_cluster"}"#;
    let job = database.claim_next_due_ai_job(i64::MAX).unwrap().unwrap();
    assert!(
        service
            .consume_ai_assignment_job_response(
                &job, response, "openai", "gpt-test", 2_000_000, 120,
            )
            .unwrap()
    );
    assert!(service.list_projects(false).unwrap().is_empty());
}

#[test]
fn task_insight_unions_activity_and_focus_and_ignores_browser_duration() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("activity", 0, 3_600_000))
        .unwrap();
    database
        .insert_browser_visit(
            "browser-evidence",
            "Chrome",
            "Default",
            &BrowserVisit {
                url: "https://example.com/research".into(),
                title: "Research evidence".into(),
                visited_at_ms: 2_000,
            },
            "example.com",
        )
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "project-insight".into(),
            name: "Insight".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 0,
        })
        .unwrap();
    let task = service
        .create_task(NewTask {
            id: "task-insight".into(),
            project_id: project.id,
            title: "Union time".into(),
            priority: TaskPriority::Medium,
            expected_output: "A trustworthy summary".into(),
            due_date: None,
            created_at_ms: 0,
        })
        .unwrap();
    service
        .assign_activity(
            &task.id,
            "activity",
            EvidenceProvenance::Manual,
            1.0,
            "confirmed",
            1,
        )
        .unwrap();
    service
        .assign_browser_visit(
            &task.id,
            "browser-evidence",
            EvidenceProvenance::Manual,
            1.0,
            "supporting evidence",
            2,
        )
        .unwrap();
    database
        .start_focus_session_for_task(
            "focus",
            "1970-01-01",
            "Union time",
            60,
            1_800_000,
            Some(&task.id),
        )
        .unwrap();
    database
        .complete_focus_session("focus", 5_400_000, "done")
        .unwrap();

    // 1970-01-02 00:00 in Asia/Shanghai, the end of the selected local day.
    let insight = service.task_time_insight(&task.id, 57_600_000).unwrap();
    assert_eq!(insight.summary.lifecycle_total_seconds, 5_400);
    assert_eq!(insight.summary.active_day_count, 1);
    assert_eq!(insight.summary.active_day_average_seconds, 5_400);
    assert_eq!(insight.summary.natural_day_average_seconds, 5_400);
    assert_eq!(insight.focus_seconds, 3_600);
    assert_eq!(insight.daily_points.len(), 1);
    assert_eq!(insight.daily_points[0].invested_seconds, 5_400);
    assert_eq!(insight.longest_continuous_seconds, 5_400);
    assert_eq!(insight.browser_evidence_count, 1);
}

#[test]
fn merging_tasks_moves_evidence_progress_and_focus_without_duplicate_time() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("merge-segment", 1_000, 4_000))
        .unwrap();
    let service = WorkLedgerService::new(WorkLedgerRepository::new(&database));
    let project = service
        .create_project(NewProject {
            id: "merge-project".into(),
            name: "Merge".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 0,
        })
        .unwrap();
    for task_id in ["target-task", "source-task"] {
        service
            .create_task(NewTask {
                id: task_id.into(),
                project_id: project.id.clone(),
                title: task_id.into(),
                priority: TaskPriority::Medium,
                expected_output: String::new(),
                due_date: None,
                created_at_ms: 0,
            })
            .unwrap();
    }
    service
        .assign_activity(
            "source-task",
            "merge-segment",
            EvidenceProvenance::Manual,
            1.0,
            "confirmed",
            1,
        )
        .unwrap();
    service
        .add_progress_entry(daily_task_monitor_core::work_ledger::NewProgressEntry {
            id: "merge-progress".into(),
            task_id: "source-task".into(),
            note: "moved".into(),
            created_at_ms: 2_000,
        })
        .unwrap();
    database
        .start_focus_session_for_task(
            "merge-focus",
            "1970-01-01",
            "Merge",
            25,
            2_000,
            Some("source-task"),
        )
        .unwrap();
    database
        .complete_focus_session("merge-focus", 5_000, "")
        .unwrap();

    assert!(
        service
            .merge_tasks("source-task", "target-task", 6_000)
            .unwrap()
    );
    assert!(service.get_task("source-task").unwrap().is_none());
    assert_eq!(service.list_activity_links("target-task").unwrap().len(), 1);
    assert_eq!(
        service.list_progress_entries("target-task").unwrap()[0].note,
        "moved"
    );
    assert_eq!(
        service
            .task_time_insight("target-task", 10_000)
            .unwrap()
            .summary
            .lifecycle_total_seconds,
        4
    );
}
