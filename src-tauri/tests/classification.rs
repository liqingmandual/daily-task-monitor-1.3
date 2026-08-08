use daily_task_monitor_core::ai::{
    AiExecutionErrorKind, AiExecutionMode, AiExecutionSnapshot, AiJob, AiJobStatus,
};
#[cfg(feature = "desktop")]
use daily_task_monitor_core::app::AppSettings;
use daily_task_monitor_core::classifier::{
    AI_AUTO_APPLY_THRESHOLD, AiDisposition, decide_ai_disposition,
};
use daily_task_monitor_core::db::{ActivitySegmentRecord, Database};
#[cfg(feature = "desktop")]
use daily_task_monitor_core::desktop::{
    AiAutomationGate, background_ai_gate_enabled, complete_terminal_ai_job_error,
    enqueue_segment_classification_job, enqueue_segment_classification_job_with_privacy,
    persist_page_classification_job_result,
};
use daily_task_monitor_core::domain::{
    ActivityCategory, Classification, ClassificationSource, VideoPurpose, learning_seconds,
};

fn segment(id: &str) -> ActivitySegmentRecord {
    ActivitySegmentRecord {
        id: id.into(),
        started_at_ms: 1_000,
        ended_at_ms: 2_000,
        app: "Codex".into(),
        app_path: String::new(),
        title: "Task 4 policy".into(),
        category: ActivityCategory::Pending,
        video_purpose: VideoPurpose::Unknown,
        confidence: 0.2,
        source: ClassificationSource::Pending,
        reason: "fixture".into(),
        model_version: "rules-v1".into(),
        needs_review: true,
        inactivity_reason: None,
    }
}

fn execution(mode: AiExecutionMode) -> AiExecutionSnapshot {
    AiExecutionSnapshot {
        execution_mode: mode,
        executor_id: if mode == AiExecutionMode::Codex {
            r"C:\Tools\codex.exe".into()
        } else {
            "openai".into()
        },
        model: if mode == AiExecutionMode::Codex {
            "gpt-5-codex".into()
        } else {
            "gpt-test".into()
        },
        evidence_hash: String::new(),
        created_at_ms: 1_000,
    }
}

fn claim_classification_job(database: &Database, segment_id: &str) -> AiJob {
    let mut execution = execution(AiExecutionMode::ApiKey);
    if let Some(evidence_hash) = database.classification_evidence_hash(segment_id).unwrap() {
        execution.evidence_hash = evidence_hash;
    }
    database
        .enqueue_ai_job_for_subject(
            "classify_segment",
            segment_id,
            &format!(r#"{{"id":"{segment_id}"}}"#),
            1_000,
            &execution,
        )
        .unwrap();
    database.claim_next_due_ai_job(1_000).unwrap().unwrap()
}

fn classification(
    category: ActivityCategory,
    video_purpose: VideoPurpose,
    duration_seconds: i64,
) -> Classification {
    Classification {
        category,
        video_purpose,
        confidence: 0.9,
        source: ClassificationSource::Rule,
        reason: "test fixture".into(),
        model_version: "rules-v1".into(),
        duration_seconds,
        needs_review: false,
    }
}

#[test]
fn learning_time_includes_research_text_creation_and_learning_video() {
    let classifications = vec![
        classification(ActivityCategory::Research, VideoPurpose::Unknown, 60),
        classification(ActivityCategory::TextInput, VideoPurpose::Unknown, 120),
        classification(
            ActivityCategory::CreationDevelopment,
            VideoPurpose::Unknown,
            180,
        ),
        classification(ActivityCategory::VideoInput, VideoPurpose::Learning, 240),
    ];

    assert_eq!(learning_seconds(&classifications), 600);
}

#[test]
fn learning_time_excludes_leisure_video_and_non_learning_categories() {
    let classifications = vec![
        classification(ActivityCategory::VideoInput, VideoPurpose::Leisure, 90),
        classification(ActivityCategory::VideoInput, VideoPurpose::Unknown, 90),
        classification(ActivityCategory::Game, VideoPurpose::Unknown, 90),
        classification(ActivityCategory::Social, VideoPurpose::Unknown, 90),
        classification(ActivityCategory::FileManagement, VideoPurpose::Unknown, 90),
        classification(ActivityCategory::Idle, VideoPurpose::Unknown, 90),
    ];

    assert_eq!(learning_seconds(&classifications), 0);
}

#[test]
fn shared_ai_disposition_uses_the_exact_public_threshold() {
    assert_eq!(AI_AUTO_APPLY_THRESHOLD, 0.85);
    assert_eq!(decide_ai_disposition(0.85, false), AiDisposition::AutoApply);
    assert_eq!(
        decide_ai_disposition(0.849_999, false),
        AiDisposition::Review
    );
}

#[test]
fn manual_ownership_takes_precedence_over_confidence() {
    assert_eq!(decide_ai_disposition(1.0, true), AiDisposition::ManualLock);
}

#[test]
fn non_finite_confidence_requires_review_unless_manually_owned() {
    for confidence in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            decide_ai_disposition(confidence, false),
            AiDisposition::Review
        );
        assert_eq!(
            decide_ai_disposition(confidence, true),
            AiDisposition::ManualLock
        );
    }
}

#[test]
#[cfg(feature = "desktop")]
fn each_background_gate_reads_only_its_own_setting() {
    for mask in 0_u8..8 {
        let settings = AppSettings {
            ai_auto_trend_analysis_enabled: mask & 0b001 != 0,
            ai_auto_classification_enabled: mask & 0b010 != 0,
            ai_auto_workflow_assignment_enabled: mask & 0b100 != 0,
            ..AppSettings::default()
        };

        assert_eq!(
            background_ai_gate_enabled(&settings, AiAutomationGate::TrendAnalysis),
            mask & 0b001 != 0,
        );
        assert_eq!(
            background_ai_gate_enabled(&settings, AiAutomationGate::Classification),
            mask & 0b010 != 0,
        );
        assert_eq!(
            background_ai_gate_enabled(&settings, AiAutomationGate::WorkflowAssignment),
            mask & 0b100 != 0,
        );
    }
}

#[test]
fn classification_application_uses_the_shared_threshold_boundary() {
    let database = Database::open_in_memory().unwrap();
    database.insert_segment(&segment("review")).unwrap();
    database.insert_segment(&segment("apply")).unwrap();

    assert!(
        database
            .apply_ai_classification(
                "review",
                ActivityCategory::Research,
                VideoPurpose::Unknown,
                0.849_999,
                "review",
                "test-model",
            )
            .unwrap()
    );
    assert!(
        database
            .apply_ai_classification(
                "apply",
                ActivityCategory::Research,
                VideoPurpose::Unknown,
                AI_AUTO_APPLY_THRESHOLD as f32,
                "apply",
                "test-model",
            )
            .unwrap()
    );

    let stored = database.list_segments(0, 3_000).unwrap();
    assert!(
        stored
            .iter()
            .find(|item| item.id == "review")
            .unwrap()
            .needs_review
    );
    assert!(
        !stored
            .iter()
            .find(|item| item.id == "apply")
            .unwrap()
            .needs_review
    );
}

#[test]
fn manual_classification_is_never_overwritten_by_ai() {
    let database = Database::open_in_memory().unwrap();
    database.insert_segment(&segment("manual")).unwrap();
    database
        .save_manual_classification(
            "manual",
            ActivityCategory::Social,
            VideoPurpose::Unknown,
            "user decision",
        )
        .unwrap();

    assert!(
        !database
            .apply_ai_classification(
                "manual",
                ActivityCategory::Research,
                VideoPurpose::Unknown,
                1.0,
                "AI decision",
                "test-model",
            )
            .unwrap()
    );
    let stored = database.list_segments(0, 3_000).unwrap().remove(0);
    assert_eq!(stored.category, ActivityCategory::Social);
    assert_eq!(stored.source, ClassificationSource::Manual);
    assert_eq!(stored.reason, "user decision");
}

#[test]
fn manual_ownership_completes_the_classification_job_without_applying_or_requeueing_it() {
    let database = Database::open_in_memory().unwrap();
    database.insert_segment(&segment("manual-race")).unwrap();
    let job = claim_classification_job(&database, "manual-race");
    database
        .save_manual_classification(
            "manual-race",
            ActivityCategory::Social,
            VideoPurpose::Unknown,
            "user decision after queueing",
        )
        .unwrap();

    assert!(
        database
            .complete_segment_classification_job_generation(
                &job.id,
                job.generation,
                "manual-race",
                ActivityCategory::Research,
                VideoPurpose::Unknown,
                0.99,
                "AI decision",
                "test-model",
                "openai",
                "gpt-test",
                Some(0),
                2_000,
            )
            .unwrap()
    );

    let stored = database.get_segment("manual-race").unwrap().unwrap();
    assert_eq!(stored.category, ActivityCategory::Social);
    assert_eq!(stored.source, ClassificationSource::Manual);
    let persisted_job = database.get_ai_job(&job.id).unwrap().unwrap();
    assert_eq!(persisted_job.status, AiJobStatus::Complete);
    assert_eq!(persisted_job.finished_at_ms, Some(2_000));
    assert_eq!(persisted_job.executor_id, Some("openai".into()));
    assert_eq!(persisted_job.model, Some("gpt-test".into()));
    assert_eq!(persisted_job.exit_code, Some(0));
    assert!(database.claim_next_due_ai_job(i64::MAX).unwrap().is_none());
}

#[test]
fn completed_manual_ownership_job_is_not_recovered_after_restart() {
    let path = std::env::temp_dir().join(format!(
        "daily-task-monitor-manual-ownership-recovery-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let job_id;
    {
        let database = Database::open(&path).unwrap();
        database
            .insert_segment(&segment("manual-recovery"))
            .unwrap();
        let job = claim_classification_job(&database, "manual-recovery");
        job_id = job.id.clone();
        database
            .save_manual_classification(
                "manual-recovery",
                ActivityCategory::Social,
                VideoPurpose::Unknown,
                "user decision after queueing",
            )
            .unwrap();
        assert!(
            database
                .complete_segment_classification_job_generation(
                    &job.id,
                    job.generation,
                    "manual-recovery",
                    ActivityCategory::Research,
                    VideoPurpose::Unknown,
                    0.99,
                    "AI decision",
                    "test-model",
                    "openai",
                    "gpt-test",
                    Some(0),
                    2_000,
                )
                .unwrap()
        );
    }

    let reopened = Database::open(&path).unwrap();
    assert_eq!(
        reopened.get_ai_job(&job_id).unwrap().unwrap().status,
        AiJobStatus::Complete
    );
    assert!(reopened.claim_next_due_ai_job(i64::MAX).unwrap().is_none());
    drop(reopened);
    let _ = std::fs::remove_file(path);
}

#[test]
#[cfg(feature = "desktop")]
fn stale_high_confidence_classification_completes_without_applying_or_creating_a_review() {
    for mutation in ["title", "path", "time"] {
        let path = std::env::temp_dir().join(format!(
            "daily-task-monitor-stale-classification-{mutation}-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let database = Database::open(&path).unwrap();
        let queued = segment("stale-high-confidence");
        database.insert_segment(&queued).unwrap();
        enqueue_segment_classification_job(
            &database,
            &queued,
            &execution(AiExecutionMode::ApiKey),
            1_000,
        )
        .unwrap();
        let job = database.claim_next_due_ai_job(1_000).unwrap().unwrap();

        let connection = rusqlite::Connection::open(&path).unwrap();
        let statement = match mutation {
            "title" => "UPDATE activity_segments SET title='changed' WHERE id=?1",
            "path" => "UPDATE activity_segments SET app_path='C:\\changed.exe' WHERE id=?1",
            "time" => "UPDATE activity_segments SET ended_at_ms=2500 WHERE id=?1",
            _ => unreachable!(),
        };
        connection
            .execute(statement, ["stale-high-confidence"])
            .unwrap();
        drop(connection);

        assert!(
            database
                .consume_segment_classification_job_generation(
                    &job.id,
                    job.generation,
                    "stale-high-confidence",
                    ActivityCategory::Research,
                    VideoPurpose::Unknown,
                    0.99,
                    "stale AI decision",
                    "test-model",
                    "openai",
                    "gpt-test",
                    Some(0),
                    2_000,
                )
                .unwrap()
        );
        assert_eq!(
            database
                .get_segment("stale-high-confidence")
                .unwrap()
                .unwrap()
                .category,
            ActivityCategory::Pending
        );
        assert!(
            database
                .list_ai_reviews(&daily_task_monitor_core::ai_review::AiReviewFilter {
                    subject_id: Some("stale-high-confidence".into()),
                    ..Default::default()
                })
                .unwrap()
                .iter()
                .all(|review| review.kind
                    != daily_task_monitor_core::ai_review::AiReviewKind::Classification)
        );
        assert_eq!(
            database.get_ai_job(&job.id).unwrap().unwrap().status,
            AiJobStatus::Complete
        );
        assert!(database.claim_next_due_ai_job(i64::MAX).unwrap().is_none());
        drop(database);
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn classification_job_is_not_completed_when_segment_is_missing() {
    let database = Database::open_in_memory().unwrap();
    let job = claim_classification_job(&database, "missing-segment");

    assert!(
        !database
            .complete_segment_classification_job_generation(
                &job.id,
                job.generation,
                "missing-segment",
                ActivityCategory::Research,
                VideoPurpose::Unknown,
                0.99,
                "AI decision",
                "test-model",
                "openai",
                "gpt-test",
                Some(0),
                2_000,
            )
            .unwrap()
    );

    let persisted_job = database.get_ai_job(&job.id).unwrap().unwrap();
    assert_eq!(persisted_job.generation, job.generation);
    assert_eq!(persisted_job.status, AiJobStatus::Running);
    assert_eq!(persisted_job.finished_at_ms, None);
    assert_eq!(persisted_job.executor_id, None);
    assert_eq!(persisted_job.model, None);
    assert_eq!(persisted_job.exit_code, None);
}

#[test]
fn superseded_running_classification_generation_is_retired_without_touching_current_work() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("superseded-generation"))
        .unwrap();
    let old_run = claim_classification_job(&database, "superseded-generation");

    let current_id = database
        .force_enqueue_ai_job_for_subject(
            "classify_segment",
            "superseded-generation",
            r#"{"id":"superseded-generation"}"#,
            2_000,
            &execution(AiExecutionMode::Codex),
        )
        .unwrap();
    let current_run = database.claim_next_due_ai_job(2_000).unwrap().unwrap();
    assert_eq!(current_run.id, current_id);
    assert_eq!(current_run.generation, old_run.generation + 1);

    assert!(
        !database
            .complete_segment_classification_job_generation(
                &old_run.id,
                old_run.generation,
                "superseded-generation",
                ActivityCategory::Research,
                VideoPurpose::Unknown,
                0.99,
                "stale AI result",
                "openai/gpt-test",
                "openai",
                "gpt-test",
                Some(0),
                3_000,
            )
            .unwrap()
    );
    assert!(
        !database
            .fail_ai_job_generation(
                &old_run.id,
                old_run.generation,
                3_000,
                "Segment classification completion lease or segment is no longer current",
                AiExecutionErrorKind::Persistence,
                Some("openai"),
                Some("gpt-test"),
                Some(0),
            )
            .unwrap()
    );

    let retired = database.get_ai_job(&old_run.id).unwrap().unwrap();
    assert_eq!(retired.status, AiJobStatus::Complete);
    assert_eq!(retired.attempts, old_run.attempts);
    assert!(
        retired
            .last_error
            .to_ascii_lowercase()
            .contains("superseded")
    );
    assert_eq!(retired.finished_at_ms, Some(3_000));
    assert_eq!(retired.executor_id.as_deref(), Some("openai"));
    assert_eq!(retired.model.as_deref(), Some("gpt-test"));
    assert_eq!(retired.exit_code, Some(0));
    assert_eq!(
        database.get_ai_job(&current_id).unwrap().unwrap(),
        current_run
    );

    let stored = database
        .get_segment("superseded-generation")
        .unwrap()
        .unwrap();
    assert_eq!(stored.category, ActivityCategory::Pending);
    assert_eq!(stored.source, ClassificationSource::Pending);
}

#[test]
#[cfg(feature = "desktop")]
fn stale_page_classification_production_completion_retires_the_old_generation() {
    let database = Database::open_in_memory().unwrap();
    let old_id = database
        .enqueue_ai_job_for_subject(
            "classify_page",
            "visit-superseded",
            r#"{"visitId":"visit-superseded"}"#,
            1_000,
            &execution(AiExecutionMode::ApiKey),
        )
        .unwrap();
    let old_run = database.claim_next_due_ai_job(1_000).unwrap().unwrap();
    assert_eq!(old_run.id, old_id);

    let current_id = database
        .force_enqueue_ai_job_for_subject(
            "classify_page",
            "visit-superseded",
            r#"{"visitId":"visit-superseded"}"#,
            2_000,
            &execution(AiExecutionMode::Codex),
        )
        .unwrap();
    let current_run = database.claim_next_due_ai_job(2_000).unwrap().unwrap();
    assert_eq!(current_run.id, current_id);

    let error = persist_page_classification_job_result(
        &database,
        &old_run,
        "visit-superseded",
        r#"{"category":"research"}"#,
        "openai",
        "gpt-test",
        Some(0),
        3_000,
    )
    .unwrap_err();
    assert!(error.contains("no longer current"), "{error}");

    let retired = database.get_ai_job(&old_id).unwrap().unwrap();
    assert_eq!(retired.status, AiJobStatus::Complete);
    assert_eq!(retired.attempts, old_run.attempts);
    assert!(
        retired
            .last_error
            .to_ascii_lowercase()
            .contains("superseded")
    );
    assert_eq!(retired.finished_at_ms, Some(3_000));
    assert_eq!(
        database.get_ai_job(&current_id).unwrap().unwrap(),
        current_run
    );
}

#[test]
#[cfg(feature = "desktop")]
fn superseded_generic_terminal_invalid_job_completion_retires_the_old_generation() {
    let database = Database::open_in_memory().unwrap();
    let old_id = database
        .enqueue_ai_job_for_subject(
            "classify_page",
            "invalid-generic",
            "{}",
            1_000,
            &execution(AiExecutionMode::ApiKey),
        )
        .unwrap();
    let old_run = database.claim_next_due_ai_job(1_000).unwrap().unwrap();
    assert_eq!(old_run.id, old_id);

    let current_id = database
        .force_enqueue_ai_job_for_subject(
            "classify_page",
            "invalid-generic",
            r#"{"visitId":"current"}"#,
            2_000,
            &execution(AiExecutionMode::Codex),
        )
        .unwrap();
    let current_run = database.claim_next_due_ai_job(2_000).unwrap().unwrap();
    assert_eq!(current_run.id, current_id);

    assert!(
        !complete_terminal_ai_job_error(
            &database,
            &old_run,
            3_000,
            "Queued page payload has no visit id",
            AiExecutionErrorKind::InvalidJob,
            Some("openai"),
            Some("gpt-test"),
            None,
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
#[cfg(feature = "desktop")]
fn manual_segment_queue_uses_the_supplied_current_execution_snapshot() {
    let database = Database::open_in_memory().unwrap();
    let candidate = segment("manual-queue");
    database.insert_segment(&candidate).unwrap();
    let snapshot = execution(AiExecutionMode::Codex);

    let job_id =
        enqueue_segment_classification_job(&database, &candidate, &snapshot, 2_000).unwrap();
    let queued = database.get_ai_job(&job_id).unwrap().unwrap();

    assert_eq!(queued.execution.execution_mode, AiExecutionMode::Codex);
    assert_eq!(queued.execution.executor_id, r"C:\Tools\codex.exe");
    assert_eq!(queued.execution.model, "gpt-5-codex");
}

#[test]
#[cfg(feature = "desktop")]
fn manual_segment_queue_rejects_excluded_apps_without_exposing_the_title() {
    let database = Database::open_in_memory().unwrap();
    let mut candidate = segment("private-segment");
    candidate.app = "Private Notes".into();
    candidate.title = "Secret acquisition codename".into();
    database.insert_segment(&candidate).unwrap();

    let error = enqueue_segment_classification_job_with_privacy(
        &database,
        &candidate,
        &["private notes".into()],
        &execution(AiExecutionMode::ApiKey),
        2_000,
    )
    .unwrap_err();

    assert_eq!(error, "Activity segment is excluded from AI classification");
    assert!(!error.contains(&candidate.title));
    assert_eq!(database.ai_job_count().unwrap(), 0);
}
