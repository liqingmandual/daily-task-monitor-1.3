use std::collections::BTreeMap;

use daily_task_monitor_core::ai::{
    AiExecutionErrorKind, AiExecutionMode, AiExecutionSnapshot, AiJobStatus,
};
use daily_task_monitor_core::ai_review::{
    AiExecutionAuditView, AiReviewAction, AiReviewDraft, AiReviewEventKind, AiReviewFilter,
    AiReviewKind, AiReviewResolution, AiReviewState,
};
use daily_task_monitor_core::browser::BrowserVisit;
use daily_task_monitor_core::db::{ActivitySegmentRecord, Database};
use daily_task_monitor_core::domain::{ActivityCategory, ClassificationSource, VideoPurpose};
use daily_task_monitor_core::work_ledger::{
    EvidenceProvenance, NewProject, NewTask, TaskPriority, WorkLedgerRepository, WorkLedgerService,
};
use serde_json::json;
use sha2::{Digest, Sha256};

fn execution(mode: AiExecutionMode, evidence_hash: &str, generation: i64) -> AiExecutionAuditView {
    AiExecutionAuditView {
        execution_mode: Some(mode),
        executor_id: Some(match mode {
            AiExecutionMode::ApiKey => "openai".into(),
            AiExecutionMode::Codex => "codex".into(),
        }),
        model: Some("review-test-model".into()),
        evidence_hash: evidence_hash.into(),
        generation,
        created_at_ms: 1_000,
        started_at_ms: Some(1_100),
        finished_at_ms: Some(1_200),
        duration_ms: Some(100),
        exit_code: Some(0),
        error_kind: None,
        diagnostic: String::new(),
    }
}

fn segment(id: &str) -> ActivitySegmentRecord {
    ActivitySegmentRecord {
        id: id.into(),
        started_at_ms: 1_000,
        ended_at_ms: 2_000,
        app: "Code".into(),
        app_path: String::new(),
        title: "Review implementation".into(),
        category: ActivityCategory::Pending,
        video_purpose: VideoPurpose::Unknown,
        confidence: 0.0,
        source: ClassificationSource::Rule,
        reason: "fixture".into(),
        model_version: "rule-v1".into(),
        needs_review: true,
        inactivity_reason: None,
    }
}

fn classification_json(
    category: ActivityCategory,
    video_purpose: VideoPurpose,
    confidence: f64,
    reason: &str,
) -> String {
    serde_json::to_string(&json!({
        "category": category,
        "videoPurpose": video_purpose,
        "confidence": confidence,
        "reason": reason,
        "modelVersion": "review-test-model",
    }))
    .unwrap()
}

fn classification_evidence_hash(id: &str) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("{id}\nCode\n\nReview implementation\n1000\n2000").as_bytes())
    )
}

fn classification_draft(
    id: &str,
    subject_id: &str,
    confidence: f64,
    _evidence_hash: &str,
) -> AiReviewDraft {
    let evidence_hash = classification_evidence_hash(subject_id);
    AiReviewDraft {
        id: id.into(),
        job_id: None,
        kind: AiReviewKind::Classification,
        subject_id: subject_id.into(),
        before_json: serde_json::to_string(&json!({
            "category": ActivityCategory::Pending,
            "videoPurpose": VideoPurpose::Unknown,
            "confidence": 0.0,
            "reason": "fixture",
            "modelVersion": "rule-v1",
        }))
        .unwrap(),
        proposed_json: classification_json(
            ActivityCategory::Research,
            VideoPurpose::Unknown,
            confidence,
            "bounded AI result",
        ),
        confidence: Some(confidence),
        evidence_summary: "Code: Review implementation".into(),
        evidence_hash: evidence_hash.clone(),
        execution: execution(AiExecutionMode::ApiKey, &evidence_hash, 0),
        created_at_ms: 2_000,
        execution_error: None,
    }
}

fn create_ledger_task<'a>(db: &'a Database, task_id: &str) -> WorkLedgerService<'a> {
    let service = WorkLedgerService::new(WorkLedgerRepository::new(db));
    let project = service
        .create_project(NewProject {
            id: format!("project-{task_id}"),
            name: "Review project".into(),
            color: "#3182ce".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    service
        .create_task(NewTask {
            id: task_id.into(),
            project_id: project.id,
            title: "Unrelated task".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_000,
        })
        .unwrap();
    service
}

#[test]
fn migration_creates_review_event_and_manual_ownership_tables_idempotently() {
    let path = std::env::temp_dir().join(format!(
        "daily-task-monitor-ai-review-migration-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch("PRAGMA user_version = 5;")
            .unwrap();
    }

    Database::open(&path).unwrap();
    Database::open(&path).unwrap();

    let connection = rusqlite::Connection::open(&path).unwrap();
    for table in [
        "ai_review_records",
        "ai_review_events",
        "manual_field_ownership",
        "work_ledger_ai_suggestions",
    ] {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists, "missing {table}");
    }
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        11
    );
    drop(connection);
    let _ = std::fs::remove_file(path);
}

#[test]
fn v6_migration_rolls_back_everything_when_a_schema_conflict_occurs() {
    let path = std::env::temp_dir().join(format!(
        "daily-task-monitor-ai-review-v6-rollback-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "
            PRAGMA user_version = 5;
            CREATE TABLE work_ledger_ai_suggestions (
                evidence_kind TEXT NOT NULL,
                evidence_id TEXT NOT NULL
            );
            INSERT INTO work_ledger_ai_suggestions VALUES ('activity', 'legacy-suggestion');
            CREATE TABLE ai_review_events (id INTEGER PRIMARY KEY);
            ",
        )
        .unwrap();
    drop(connection);

    assert!(Database::open(&path).is_err());

    let connection = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        5
    );
    let records_created: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='ai_review_records')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!records_created);
    let suggestion_columns = connection
        .prepare("PRAGMA table_info(work_ledger_ai_suggestions)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(suggestion_columns, ["evidence_kind", "evidence_id"]);
    assert_eq!(
        connection
            .query_row(
                "SELECT evidence_id FROM work_ledger_ai_suggestions",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "legacy-suggestion"
    );
    drop(connection);
    let _ = std::fs::remove_file(path);
}

#[test]
fn confidence_boundary_creates_review_before_applying_or_waiting() {
    let db = Database::open_in_memory().unwrap();
    db.insert_segment(&segment("low")).unwrap();
    db.insert_segment(&segment("high")).unwrap();

    let low = db
        .create_ai_review(&classification_draft(
            "review-low",
            "low",
            0.849_999,
            "low-hash",
        ))
        .unwrap();
    assert_eq!(low.state, AiReviewState::Pending);
    assert_eq!(
        db.get_segment("low").unwrap().unwrap().category,
        ActivityCategory::Pending
    );

    let high = db
        .create_ai_review(&classification_draft(
            "review-high",
            "high",
            0.85,
            "high-hash",
        ))
        .unwrap();
    assert_eq!(high.state, AiReviewState::AutoApplied);
    assert_eq!(high.applied_json, Some(high.proposed_json.clone()));
    let applied = db.get_segment("high").unwrap().unwrap();
    assert_eq!(applied.category, ActivityCategory::Research);
    assert_eq!(applied.source, ClassificationSource::Ai);

    assert_eq!(
        db.list_ai_review_events("review-high").unwrap(),
        vec![AiReviewEventKind::Generated, AiReviewEventKind::AutoApplied]
    );
}

#[test]
fn pending_reviews_support_accept_change_ignore_and_reject_illegal_transitions() {
    let db = Database::open_in_memory().unwrap();
    for id in ["accept", "change", "ignore"] {
        db.insert_segment(&segment(id)).unwrap();
        db.create_ai_review(&classification_draft(
            &format!("review-{id}"),
            id,
            0.6,
            &format!("hash-{id}"),
        ))
        .unwrap();
    }

    let accepted = db
        .resolve_ai_review(&AiReviewResolution {
            review_ids: vec!["review-accept".into()],
            action: AiReviewAction::Accept,
            changed_json: None,
            evidence_hashes: BTreeMap::from([(
                "review-accept".into(),
                classification_evidence_hash("accept"),
            )]),
            resolved_at_ms: 3_000,
        })
        .unwrap();
    assert_eq!(accepted[0].state, AiReviewState::ManualOverride);
    assert_eq!(
        db.get_segment("accept").unwrap().unwrap().category,
        ActivityCategory::Research
    );

    let changed_json = classification_json(
        ActivityCategory::CreationDevelopment,
        VideoPurpose::Unknown,
        1.0,
        "chosen by user",
    );
    let changed = db
        .resolve_ai_review(&AiReviewResolution {
            review_ids: vec!["review-change".into()],
            action: AiReviewAction::Change,
            changed_json: Some(changed_json.clone()),
            evidence_hashes: BTreeMap::from([(
                "review-change".into(),
                classification_evidence_hash("change"),
            )]),
            resolved_at_ms: 3_100,
        })
        .unwrap();
    assert_eq!(
        changed[0].applied_json.as_deref(),
        Some(changed_json.as_str())
    );
    assert_eq!(
        db.get_segment("change").unwrap().unwrap().category,
        ActivityCategory::CreationDevelopment
    );

    let stale_ignore = db.resolve_ai_review(&AiReviewResolution {
        review_ids: vec!["review-ignore".into()],
        action: AiReviewAction::Ignore,
        changed_json: None,
        evidence_hashes: BTreeMap::from([("review-ignore".into(), "stale-client-hash".into())]),
        resolved_at_ms: 3_150,
    });
    assert!(stale_ignore.is_err());
    assert_eq!(
        db.get_ai_review("review-ignore").unwrap().unwrap().state,
        AiReviewState::Pending
    );

    let ignored = db
        .resolve_ai_review(&AiReviewResolution {
            review_ids: vec!["review-ignore".into()],
            action: AiReviewAction::Ignore,
            changed_json: None,
            evidence_hashes: BTreeMap::from([(
                "review-ignore".into(),
                classification_evidence_hash("ignore"),
            )]),
            resolved_at_ms: 3_200,
        })
        .unwrap();
    assert_eq!(ignored[0].state, AiReviewState::Dismissed);
    assert_eq!(
        db.get_segment("ignore").unwrap().unwrap().category,
        ActivityCategory::Pending
    );

    let illegal = db.resolve_ai_review(&AiReviewResolution {
        review_ids: vec!["review-ignore".into()],
        action: AiReviewAction::Accept,
        changed_json: None,
        evidence_hashes: BTreeMap::from([(
            "review-ignore".into(),
            classification_evidence_hash("ignore"),
        )]),
        resolved_at_ms: 3_300,
    });
    assert!(illegal.is_err());
}

#[test]
fn resolve_rejects_classification_when_any_formal_field_changed() {
    let db = Database::open_in_memory().unwrap();
    db.insert_segment(&segment("formal-change")).unwrap();
    let review = db
        .create_ai_review(&classification_draft(
            "review-formal-change",
            "formal-change",
            0.6,
            "ignored",
        ))
        .unwrap();
    let mut changed = segment("formal-change");
    changed.confidence = 0.2;
    changed.reason = "changed reason".into();
    changed.model_version = "changed-model".into();
    db.upsert_native_segment(&changed).unwrap();

    let result = db.resolve_ai_review(&AiReviewResolution {
        review_ids: vec![review.id.clone()],
        action: AiReviewAction::Accept,
        changed_json: None,
        evidence_hashes: BTreeMap::from([(review.id, review.evidence_hash)]),
        resolved_at_ms: 3_000,
    });
    assert!(result.is_err());
}

#[test]
fn resolve_rejects_classification_when_server_evidence_changes() {
    let db = Database::open_in_memory().unwrap();
    db.insert_segment(&segment("evidence-change")).unwrap();
    let review = db
        .create_ai_review(&classification_draft(
            "review-evidence-change",
            "evidence-change",
            0.6,
            "ignored",
        ))
        .unwrap();
    let mut changed = segment("evidence-change");
    changed.title = "Sensitive replacement title".into();
    db.upsert_native_segment(&changed).unwrap();

    let result = db.resolve_ai_review(&AiReviewResolution {
        review_ids: vec![review.id.clone()],
        action: AiReviewAction::Accept,
        changed_json: None,
        evidence_hashes: BTreeMap::from([(review.id, review.evidence_hash)]),
        resolved_at_ms: 3_000,
    });
    assert!(result.is_err());
}

#[test]
fn reverting_auto_apply_restores_before_value_and_establishes_manual_ownership() {
    let db = Database::open_in_memory().unwrap();
    db.insert_segment(&segment("revert")).unwrap();
    let review = db
        .create_ai_review(&classification_draft(
            "review-revert",
            "revert",
            0.95,
            "hash",
        ))
        .unwrap();
    assert_eq!(review.state, AiReviewState::AutoApplied);

    let reverted = db.revert_ai_auto_apply("review-revert", 3_000).unwrap();
    assert_eq!(reverted.state, AiReviewState::Reverted);
    let restored = db.get_segment("revert").unwrap().unwrap();
    assert_eq!(restored.category, ActivityCategory::Pending);
    assert_eq!(restored.source, ClassificationSource::Manual);
    assert!(
        db.has_manual_field_ownership(AiReviewKind::Classification, "revert")
            .unwrap()
    );

    let later = db
        .create_ai_review(&classification_draft(
            "review-later",
            "revert",
            0.99,
            "later-hash",
        ))
        .unwrap();
    assert_eq!(later.state, AiReviewState::Pending);
    assert_eq!(
        db.get_segment("revert").unwrap().unwrap().category,
        ActivityCategory::Pending
    );
}

#[test]
fn batch_accept_rejects_duplicate_review_ids_without_side_effects() {
    let db = Database::open_in_memory().unwrap();
    db.insert_segment(&segment("duplicate-review-id")).unwrap();
    let review = db
        .create_ai_review(&classification_draft(
            "review-duplicate-id",
            "duplicate-review-id",
            0.7,
            "unused",
        ))
        .unwrap();
    let before = db.get_segment("duplicate-review-id").unwrap().unwrap();

    let result = db.resolve_ai_review(&AiReviewResolution {
        review_ids: vec![review.id.clone(), review.id.clone()],
        action: AiReviewAction::Accept,
        changed_json: None,
        evidence_hashes: BTreeMap::from([(review.id.clone(), review.evidence_hash.clone())]),
        resolved_at_ms: 3_000,
    });

    let error = result.unwrap_err().to_string();
    assert!(error.contains("duplicate review ID"), "{error}");
    assert_eq!(
        db.get_ai_review(&review.id).unwrap().unwrap().state,
        AiReviewState::Pending
    );
    let after = db.get_segment("duplicate-review-id").unwrap().unwrap();
    assert_eq!(after.category, before.category);
    assert_eq!(after.video_purpose, before.video_purpose);
    assert_eq!(after.confidence, before.confidence);
    assert_eq!(after.source, before.source);
    assert_eq!(after.reason, before.reason);
    assert_eq!(after.model_version, before.model_version);
    assert_eq!(after.needs_review, before.needs_review);
    assert_eq!(
        db.list_ai_review_events(&review.id).unwrap(),
        vec![AiReviewEventKind::Generated]
    );
    assert!(
        !db.has_manual_field_ownership(AiReviewKind::Classification, "duplicate-review-id")
            .unwrap()
    );
    assert!(db.list_work_ledger_ai_suggestions().unwrap().is_empty());
}

#[test]
fn batch_accept_rejects_multiple_reviews_for_one_classification_subject() {
    let db = Database::open_in_memory().unwrap();
    db.insert_segment(&segment("duplicate-classification-subject"))
        .unwrap();
    let first = db
        .create_ai_review(&classification_draft(
            "review-classification-generation-1",
            "duplicate-classification-subject",
            0.7,
            "unused",
        ))
        .unwrap();
    let mut second_draft = classification_draft(
        "review-classification-generation-2",
        "duplicate-classification-subject",
        0.8,
        "unused",
    );
    second_draft.proposed_json = classification_json(
        ActivityCategory::CreationDevelopment,
        VideoPurpose::Unknown,
        0.8,
        "second generation proposal",
    );
    second_draft.execution.generation = 1;
    let second = db.create_ai_review(&second_draft).unwrap();
    let before = db
        .get_segment("duplicate-classification-subject")
        .unwrap()
        .unwrap();

    let result = db.resolve_ai_review(&AiReviewResolution {
        review_ids: vec![first.id.clone(), second.id.clone()],
        action: AiReviewAction::Accept,
        changed_json: None,
        evidence_hashes: BTreeMap::from([
            (first.id.clone(), first.evidence_hash.clone()),
            (second.id.clone(), second.evidence_hash.clone()),
        ]),
        resolved_at_ms: 3_000,
    });

    let error = result.unwrap_err().to_string();
    assert!(error.contains("distinct subjects"), "{error}");
    for review in [&first, &second] {
        assert_eq!(
            db.get_ai_review(&review.id).unwrap().unwrap().state,
            AiReviewState::Pending
        );
        assert_eq!(
            db.list_ai_review_events(&review.id).unwrap(),
            vec![AiReviewEventKind::Generated]
        );
    }
    let after = db
        .get_segment("duplicate-classification-subject")
        .unwrap()
        .unwrap();
    assert_eq!(after.category, before.category);
    assert_eq!(after.video_purpose, before.video_purpose);
    assert_eq!(after.confidence, before.confidence);
    assert_eq!(after.source, before.source);
    assert_eq!(after.reason, before.reason);
    assert_eq!(after.model_version, before.model_version);
    assert_eq!(after.needs_review, before.needs_review);
    assert!(
        !db.has_manual_field_ownership(
            AiReviewKind::Classification,
            "duplicate-classification-subject",
        )
        .unwrap()
    );
    assert!(db.list_work_ledger_ai_suggestions().unwrap().is_empty());
}

#[test]
fn batch_accept_requires_one_kind_current_hashes_and_no_manual_owner() {
    let db = Database::open_in_memory().unwrap();
    for id in ["one", "two", "owned"] {
        db.insert_segment(&segment(id)).unwrap();
        db.create_ai_review(&classification_draft(
            &format!("review-{id}"),
            id,
            0.7,
            &format!("hash-{id}"),
        ))
        .unwrap();
    }
    db.claim_manual_field_ownership(AiReviewKind::Classification, "owned", "user", 2_500)
        .unwrap();

    let batch_ignore = db.resolve_ai_review(&AiReviewResolution {
        review_ids: vec!["review-one".into(), "review-two".into()],
        action: AiReviewAction::Ignore,
        changed_json: None,
        evidence_hashes: BTreeMap::from([
            ("review-one".into(), classification_evidence_hash("one")),
            ("review-two".into(), classification_evidence_hash("two")),
        ]),
        resolved_at_ms: 2_900,
    });
    assert!(batch_ignore.is_err());

    let stale = db.resolve_ai_review(&AiReviewResolution {
        review_ids: vec!["review-one".into(), "review-two".into()],
        action: AiReviewAction::Accept,
        changed_json: None,
        evidence_hashes: BTreeMap::from([
            ("review-one".into(), "stale".into()),
            ("review-two".into(), classification_evidence_hash("two")),
        ]),
        resolved_at_ms: 3_000,
    });
    assert!(stale.is_err());
    assert!(
        db.list_ai_reviews(&AiReviewFilter::default())
            .unwrap()
            .iter()
            .all(|r| {
                r.id != "review-one" && r.id != "review-two" || r.state == AiReviewState::Pending
            })
    );

    let owned = db.resolve_ai_review(&AiReviewResolution {
        review_ids: vec!["review-owned".into()],
        action: AiReviewAction::Accept,
        changed_json: None,
        evidence_hashes: BTreeMap::from([(
            "review-owned".into(),
            classification_evidence_hash("owned"),
        )]),
        resolved_at_ms: 3_100,
    });
    assert!(owned.is_err());

    let accepted = db
        .resolve_ai_review(&AiReviewResolution {
            review_ids: vec!["review-one".into(), "review-two".into()],
            action: AiReviewAction::Accept,
            changed_json: None,
            evidence_hashes: BTreeMap::from([
                ("review-one".into(), classification_evidence_hash("one")),
                ("review-two".into(), classification_evidence_hash("two")),
            ]),
            resolved_at_ms: 3_200,
        })
        .unwrap();
    assert_eq!(accepted.len(), 2);
    assert!(
        accepted
            .iter()
            .all(|review| review.state == AiReviewState::ManualOverride)
    );
}

#[test]
fn retry_keeps_execution_error_record_and_creates_a_new_job_generation() {
    let db = Database::open_in_memory().unwrap();
    db.insert_segment(&segment("failed")).unwrap();
    let queued = AiExecutionSnapshot {
        execution_mode: AiExecutionMode::ApiKey,
        executor_id: "openai".into(),
        model: "old-model".into(),
        evidence_hash: "failed-hash".into(),
        created_at_ms: 1_000,
    };
    let job_id = db
        .enqueue_ai_job_for_subject(
            "classify_segment",
            "failed",
            r#"{"id":"failed"}"#,
            1_000,
            &queued,
        )
        .unwrap();
    let job = db.claim_next_due_ai_job(1_000).unwrap().unwrap();
    assert_eq!(job.id, job_id);
    db.fail_ai_job_generation(
        &job.id,
        job.generation,
        1_200,
        "provider failed",
        AiExecutionErrorKind::Provider,
        Some("openai"),
        Some("old-model"),
        Some(1),
    )
    .unwrap();

    let failures = db
        .list_ai_reviews(&AiReviewFilter {
            states: vec![AiReviewState::ExecutionError],
            subject_id: Some("failed".into()),
            ..AiReviewFilter::default()
        })
        .unwrap();
    assert_eq!(failures.len(), 1);
    let error_review = failures[0].clone();
    assert_eq!(error_review.state, AiReviewState::ExecutionError);
    assert!(error_review.proposed_json.is_empty());

    let current = AiExecutionSnapshot {
        execution_mode: AiExecutionMode::Codex,
        executor_id: "codex".into(),
        model: "current-model".into(),
        evidence_hash: "failed-hash".into(),
        created_at_ms: 2_000,
    };
    assert!(
        db.retry_ai_review(&error_review.id, Some(&current), 2_000)
            .unwrap()
    );
    let retained = db
        .list_ai_reviews(&AiReviewFilter {
            states: vec![AiReviewState::ExecutionError],
            ..AiReviewFilter::default()
        })
        .unwrap();
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].id, error_review.id);
    assert_eq!(retained[0].state, AiReviewState::ExecutionError);
    assert_eq!(
        db.list_ai_review_events(&error_review.id).unwrap(),
        vec![
            AiReviewEventKind::Generated,
            AiReviewEventKind::Failed,
            AiReviewEventKind::Retried,
        ]
    );
    let next = db.claim_next_due_ai_job(2_000).unwrap().unwrap();
    assert_eq!(next.generation, job.generation + 1);
    assert_eq!(next.execution.execution_mode, AiExecutionMode::Codex);
    assert_eq!(next.status, AiJobStatus::Running);
}

#[test]
fn classification_completion_persists_review_before_disposition_and_failure_has_no_suggestion() {
    let db = Database::open_in_memory().unwrap();
    for id in ["low-result", "high-result", "failed-result"] {
        db.insert_segment(&segment(id)).unwrap();
    }

    for (subject, confidence) in [("low-result", 0.849_999_f32), ("high-result", 0.85)] {
        let snapshot = AiExecutionSnapshot {
            execution_mode: AiExecutionMode::ApiKey,
            executor_id: "openai".into(),
            model: "review-test-model".into(),
            evidence_hash: db.classification_evidence_hash(subject).unwrap().unwrap(),
            created_at_ms: 1_000,
        };
        let job_id = db
            .enqueue_ai_job_for_subject(
                "classify_segment",
                subject,
                &format!(r#"{{"id":"{subject}"}}"#),
                1_000,
                &snapshot,
            )
            .unwrap();
        let job = db.claim_next_due_ai_job(1_000).unwrap().unwrap();
        assert_eq!(job.id, job_id);
        assert!(
            db.complete_segment_classification_job_generation(
                &job.id,
                job.generation,
                subject,
                ActivityCategory::Research,
                VideoPurpose::Unknown,
                confidence,
                "bounded AI result",
                "openai/review-test-model",
                "openai",
                "review-test-model",
                Some(0),
                1_200,
            )
            .unwrap()
        );
    }

    let low = db
        .list_ai_reviews(&AiReviewFilter {
            subject_id: Some("low-result".into()),
            ..AiReviewFilter::default()
        })
        .unwrap();
    assert_eq!(low.len(), 1);
    assert_eq!(low[0].state, AiReviewState::Pending);
    assert_eq!(
        db.get_segment("low-result").unwrap().unwrap().category,
        ActivityCategory::Pending
    );
    let high = db
        .list_ai_reviews(&AiReviewFilter {
            subject_id: Some("high-result".into()),
            ..AiReviewFilter::default()
        })
        .unwrap();
    assert_eq!(high.len(), 1);
    assert_eq!(high[0].state, AiReviewState::AutoApplied);
    assert_eq!(
        db.get_segment("high-result").unwrap().unwrap().category,
        ActivityCategory::Research
    );

    let failed_snapshot = AiExecutionSnapshot {
        execution_mode: AiExecutionMode::Codex,
        executor_id: "codex".into(),
        model: "review-test-model".into(),
        evidence_hash: "hash-failed".into(),
        created_at_ms: 2_000,
    };
    db.enqueue_ai_job_for_subject(
        "classify_segment",
        "failed-result",
        r#"{"id":"failed-result"}"#,
        2_000,
        &failed_snapshot,
    )
    .unwrap();
    let failed_job = db.claim_next_due_ai_job(2_000).unwrap().unwrap();
    db.fail_ai_job_generation(
        &failed_job.id,
        failed_job.generation,
        2_200,
        "Codex failed",
        AiExecutionErrorKind::Codex,
        Some("codex"),
        Some("review-test-model"),
        Some(1),
    )
    .unwrap();
    let failures = db
        .list_ai_reviews(&AiReviewFilter {
            states: vec![AiReviewState::ExecutionError],
            subject_id: Some("failed-result".into()),
            ..AiReviewFilter::default()
        })
        .unwrap();
    assert_eq!(failures.len(), 1);
    assert!(failures[0].proposed_json.is_empty());
}

#[test]
fn manual_ownership_records_blocked_classification_before_completion_consumes_it() {
    let db = Database::open_in_memory().unwrap();
    db.insert_segment(&segment("manual-result")).unwrap();
    let snapshot = AiExecutionSnapshot {
        execution_mode: AiExecutionMode::ApiKey,
        executor_id: "openai".into(),
        model: "review-test-model".into(),
        evidence_hash: db
            .classification_evidence_hash("manual-result")
            .unwrap()
            .unwrap(),
        created_at_ms: 1_000,
    };
    let job_id = db
        .enqueue_ai_job_for_subject(
            "classify_segment",
            "manual-result",
            r#"{"id":"manual-result"}"#,
            1_000,
            &snapshot,
        )
        .unwrap();
    let job = db.claim_next_due_ai_job(1_000).unwrap().unwrap();
    assert_eq!(job.id, job_id);
    db.save_manual_classification(
        "manual-result",
        ActivityCategory::Social,
        VideoPurpose::Unknown,
        "manual decision",
    )
    .unwrap();

    assert!(
        db.complete_segment_classification_job_generation(
            &job.id,
            job.generation,
            "manual-result",
            ActivityCategory::Research,
            VideoPurpose::Unknown,
            0.99,
            "AI result after manual decision",
            "openai/review-test-model",
            "openai",
            "review-test-model",
            Some(0),
            1_200,
        )
        .unwrap()
    );

    let persisted_job = db.get_ai_job(&job.id).unwrap().unwrap();
    assert_eq!(persisted_job.status, AiJobStatus::Complete);
    let reviews = db
        .list_ai_reviews(&AiReviewFilter {
            subject_id: Some("manual-result".into()),
            ..AiReviewFilter::default()
        })
        .unwrap();
    assert_eq!(reviews.len(), 1);
    assert_eq!(reviews[0].state, AiReviewState::ManualOverride);
    assert!(db.claim_next_due_ai_job(i64::MAX).unwrap().is_none());
    assert_eq!(
        db.get_segment("manual-result").unwrap().unwrap().category,
        ActivityCategory::Social
    );
}

#[test]
fn workflow_result_auto_applies_only_at_high_confidence() {
    for (label, confidence, should_apply) in [("low", 0.849_999, false), ("high", 0.85, true)] {
        let db = Database::open_in_memory().unwrap();
        let subject = format!("{label}-workflow-segment");
        let task_id = format!("{label}-task");
        let mut workflow_segment = segment(&subject);
        workflow_segment.category = ActivityCategory::Research;
        workflow_segment.source = ClassificationSource::Rule;
        workflow_segment.confidence = 0.9;
        workflow_segment.needs_review = false;
        db.insert_segment(&workflow_segment).unwrap();
        let service = create_ledger_task(&db, &task_id);
        service
            .get_work_ledger_with_ai_queue(
                0,
                3_000,
                None,
                Some(&AiExecutionSnapshot {
                    execution_mode: AiExecutionMode::ApiKey,
                    executor_id: "openai".into(),
                    model: "review-test-model".into(),
                    evidence_hash: String::new(),
                    created_at_ms: 1_000,
                }),
            )
            .unwrap();
        let job = db.claim_next_due_ai_job(i64::MAX).unwrap().unwrap();
        assert!(
            service
                .consume_ai_assignment_job_response(
                    &job,
                    &format!(r#"{{"taskId":"{task_id}","confidence":{confidence}}}"#),
                    "openai",
                    "review-test-model",
                    2_000,
                    100,
                )
                .unwrap()
        );
        let reviews = db
            .list_ai_reviews(&AiReviewFilter {
                kinds: vec![AiReviewKind::WorkflowAssignment],
                subject_id: Some(subject.clone()),
                ..AiReviewFilter::default()
            })
            .unwrap();
        if should_apply {
            assert_eq!(reviews.len(), 1);
            assert_eq!(reviews[0].state, AiReviewState::AutoApplied);
        } else {
            assert!(
                reviews.is_empty(),
                "sub-threshold workflow evidence stays unassigned without a confirmation review"
            );
        }
        assert_eq!(
            !service.list_activity_links(&task_id).unwrap().is_empty(),
            should_apply
        );
    }
}

#[test]
fn workflow_auto_apply_rejects_activity_or_browser_evidence_changed_after_queueing() {
    for mutation in [
        "activity-title",
        "activity-classification",
        "browser-domain",
    ] {
        let evidence_kind = if mutation.starts_with("activity") {
            "activity"
        } else {
            "browser"
        };
        let path = std::env::temp_dir().join(format!(
            "daily-task-monitor-workflow-evidence-{mutation}-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db = Database::open(&path).unwrap();
        let evidence_id = format!("{evidence_kind}-evidence");
        if evidence_kind == "activity" {
            let mut workflow_segment = segment(&evidence_id);
            workflow_segment.category = ActivityCategory::Research;
            workflow_segment.source = ClassificationSource::Rule;
            workflow_segment.confidence = 0.9;
            workflow_segment.needs_review = false;
            db.insert_segment(&workflow_segment).unwrap();
        } else {
            db.insert_browser_visit(
                &evidence_id,
                "Chrome",
                "Default",
                &BrowserVisit {
                    visited_at_ms: 1_500,
                    url: "https://example.com/path".into(),
                    title: "Queued browser evidence".into(),
                },
                "example.com",
            )
            .unwrap();
        }
        let service = create_ledger_task(&db, &format!("{evidence_kind}-task"));
        service
            .get_work_ledger_with_ai_queue(
                0,
                3_000,
                None,
                Some(&AiExecutionSnapshot {
                    execution_mode: AiExecutionMode::ApiKey,
                    executor_id: "openai".into(),
                    model: "review-test-model".into(),
                    evidence_hash: String::new(),
                    created_at_ms: 1_000,
                }),
            )
            .unwrap();
        let job = db.claim_next_due_ai_job(i64::MAX).unwrap().unwrap();
        let connection = rusqlite::Connection::open(&path).unwrap();
        match mutation {
            "activity-title" => {
                connection
                    .execute(
                        "UPDATE activity_segments SET title='changed activity evidence' WHERE id=?1",
                        [&evidence_id],
                    )
                    .unwrap();
            }
            "activity-classification" => {
                connection
                    .execute(
                        "UPDATE activity_segments SET category='text_input' WHERE id=?1",
                        [&evidence_id],
                    )
                    .unwrap();
            }
            "browser-domain" => {
                connection
                    .execute(
                        "UPDATE browser_visits SET domain='changed.example.com' WHERE id=?1",
                        [&evidence_id],
                    )
                    .unwrap();
            }
            _ => unreachable!(),
        }
        drop(connection);

        assert!(
            !service
                .consume_ai_assignment_job_response(
                    &job,
                    &format!(r#"{{"taskId":"{evidence_kind}-task","confidence":0.9}}"#),
                    "openai",
                    "review-test-model",
                    2_000,
                    100,
                )
                .unwrap(),
            "mutation {mutation} must reject automatic workflow assignment"
        );
        assert!(
            db.list_ai_reviews(&AiReviewFilter {
                kinds: vec![AiReviewKind::WorkflowAssignment],
                subject_id: Some(evidence_id.clone()),
                ..AiReviewFilter::default()
            })
            .unwrap()
            .is_empty()
        );
        assert!(
            service
                .list_activity_links(&format!("{evidence_kind}-task"))
                .unwrap()
                .is_empty()
        );
        assert!(
            service
                .list_browser_links(&format!("{evidence_kind}-task"))
                .unwrap()
                .is_empty()
        );
        drop(db);
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn direct_manual_classification_and_workflow_changes_append_manual_override_reviews() {
    let db = Database::open_in_memory().unwrap();
    db.insert_segment(&segment("manual-classification"))
        .unwrap();
    assert!(
        db.save_manual_classification(
            "manual-classification",
            ActivityCategory::Social,
            VideoPurpose::Unknown,
            "user correction",
        )
        .unwrap()
    );

    db.insert_segment(&segment("manual-workflow")).unwrap();
    let service = create_ledger_task(&db, "manual-task");
    assert!(
        service
            .assign_activity(
                "manual-task",
                "manual-workflow",
                EvidenceProvenance::Manual,
                1.0,
                "user assignment",
                3_000,
            )
            .unwrap()
    );
    assert!(
        service
            .remove_activity("manual-task", "manual-workflow")
            .unwrap()
    );

    let manual = db
        .list_ai_reviews(&AiReviewFilter {
            states: vec![AiReviewState::ManualOverride],
            ..AiReviewFilter::default()
        })
        .unwrap();
    assert_eq!(manual.len(), 3);
    assert!(
        manual
            .iter()
            .any(|review| review.kind == AiReviewKind::Classification)
    );
    assert!(
        manual
            .iter()
            .any(|review| review.kind == AiReviewKind::WorkflowAssignment)
    );
    assert!(
        db.has_manual_field_ownership(AiReviewKind::Classification, "manual-classification")
            .unwrap()
    );
    assert!(
        db.has_manual_field_ownership(AiReviewKind::WorkflowAssignment, "manual-workflow")
            .unwrap()
    );
}

#[test]
fn classification_review_can_be_ignored_after_a_direct_manual_override() {
    let db = Database::open_in_memory().unwrap();
    db.insert_segment(&segment("stale-manual-classification"))
        .unwrap();
    let review = db
        .create_ai_review(&classification_draft(
            "review-stale-manual-classification",
            "stale-manual-classification",
            0.6,
            "unused",
        ))
        .unwrap();

    assert!(
        db.save_manual_classification(
            "stale-manual-classification",
            ActivityCategory::Social,
            VideoPurpose::Unknown,
            "direct user correction",
        )
        .unwrap()
    );
    let manual_value = db
        .get_segment("stale-manual-classification")
        .unwrap()
        .unwrap();
    assert!(
        db.has_manual_field_ownership(AiReviewKind::Classification, "stale-manual-classification",)
            .unwrap()
    );

    let ignored = db
        .resolve_ai_review(&AiReviewResolution {
            review_ids: vec![review.id.clone()],
            action: AiReviewAction::Ignore,
            changed_json: None,
            evidence_hashes: BTreeMap::from([(review.id.clone(), review.evidence_hash.clone())]),
            resolved_at_ms: 3_000,
        })
        .unwrap();

    assert_eq!(ignored[0].state, AiReviewState::Dismissed);
    assert_eq!(
        db.list_ai_review_events(&review.id).unwrap(),
        vec![AiReviewEventKind::Generated, AiReviewEventKind::Ignored]
    );
    let after_ignore = db
        .get_segment("stale-manual-classification")
        .unwrap()
        .unwrap();
    assert_eq!(after_ignore.category, manual_value.category);
    assert_eq!(after_ignore.video_purpose, manual_value.video_purpose);
    assert_eq!(after_ignore.confidence, manual_value.confidence);
    assert_eq!(after_ignore.source, manual_value.source);
    assert_eq!(after_ignore.reason, manual_value.reason);
    assert_eq!(after_ignore.model_version, manual_value.model_version);
    assert_eq!(after_ignore.needs_review, manual_value.needs_review);
    assert!(
        db.has_manual_field_ownership(AiReviewKind::Classification, "stale-manual-classification",)
            .unwrap()
    );
}

#[test]
fn manual_override_audit_has_no_ai_execution_and_no_generated_event() {
    let db = Database::open_in_memory().unwrap();
    db.insert_segment(&segment("manual-audit")).unwrap();
    db.save_manual_classification(
        "manual-audit",
        ActivityCategory::Social,
        VideoPurpose::Unknown,
        "user correction",
    )
    .unwrap();

    let review = db
        .list_ai_reviews(&AiReviewFilter {
            states: vec![AiReviewState::ManualOverride],
            subject_id: Some("manual-audit".into()),
            ..AiReviewFilter::default()
        })
        .unwrap()
        .remove(0);
    let execution = serde_json::to_value(review.execution).unwrap();
    assert_eq!(execution["executionMode"], serde_json::Value::Null);
    assert_eq!(execution["executorId"], serde_json::Value::Null);
    assert_eq!(execution["model"], serde_json::Value::Null);
    assert_eq!(
        db.list_ai_review_events(&review.id).unwrap(),
        vec![AiReviewEventKind::Changed]
    );
}

#[test]
fn classification_evidence_summary_does_not_persist_the_window_title() {
    let db = Database::open_in_memory().unwrap();
    let mut sensitive = segment("private-summary");
    sensitive.title = "Customer Alice <alice@example.com> password reset".into();
    db.insert_segment(&sensitive).unwrap();
    let snapshot = AiExecutionSnapshot {
        execution_mode: AiExecutionMode::ApiKey,
        executor_id: "openai".into(),
        model: "review-test-model".into(),
        evidence_hash: db
            .classification_evidence_hash("private-summary")
            .unwrap()
            .unwrap(),
        created_at_ms: 1_000,
    };
    db.enqueue_ai_job_for_subject(
        "classify_segment",
        "private-summary",
        r#"{"id":"private-summary"}"#,
        1_000,
        &snapshot,
    )
    .unwrap();
    let job = db.claim_next_due_ai_job(1_000).unwrap().unwrap();
    db.complete_segment_classification_job_generation(
        &job.id,
        job.generation,
        "private-summary",
        ActivityCategory::Research,
        VideoPurpose::Unknown,
        0.6,
        "bounded AI result",
        "openai/review-test-model",
        "openai",
        "review-test-model",
        Some(0),
        1_200,
    )
    .unwrap();

    let review = db
        .list_ai_reviews(&AiReviewFilter {
            subject_id: Some("private-summary".into()),
            ..AiReviewFilter::default()
        })
        .unwrap()
        .remove(0);
    assert!(review.evidence_summary.contains("Code"));
    assert!(review.evidence_summary.contains("private-summary"));
    assert!(!review.evidence_summary.contains("Alice"));
    assert!(!review.evidence_summary.contains("password"));
}

#[test]
fn review_enum_columns_reject_invalid_values_and_corrupt_rows_fail_to_parse() {
    let path = std::env::temp_dir().join(format!(
        "daily-task-monitor-ai-review-enums-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let db = Database::open(&path).unwrap();
    db.insert_segment(&segment("enum-review")).unwrap();
    let review = db
        .create_ai_review(&classification_draft(
            "enum-review",
            "enum-review",
            0.6,
            "ignored",
        ))
        .unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    assert!(
        connection
            .execute(
                "UPDATE ai_review_records SET kind='invalid-kind' WHERE id=?1",
                [&review.id],
            )
            .is_err()
    );
    connection
        .execute_batch("PRAGMA ignore_check_constraints = ON;")
        .unwrap();
    connection
        .execute(
            "UPDATE ai_review_records SET kind='invalid-kind' WHERE id=?1",
            [&review.id],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO ai_review_events(review_id, event_kind, created_at_ms) VALUES (?1, 'invalid-event', 3000)",
            [&review.id],
        )
        .unwrap();
    drop(connection);

    assert!(db.list_ai_reviews(&AiReviewFilter::default()).is_err());
    assert!(db.list_ai_review_events(&review.id).is_err());
    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn terminal_ai_execution_error_creates_review_without_a_fake_proposal() {
    let db = Database::open_in_memory().unwrap();
    db.insert_segment(&segment("terminal-error")).unwrap();
    db.enqueue_ai_job_for_subject(
        "classify_segment",
        "terminal-error",
        r#"{"id":"terminal-error"}"#,
        1_000,
        &AiExecutionSnapshot {
            execution_mode: AiExecutionMode::ApiKey,
            executor_id: "openai".into(),
            model: "review-test-model".into(),
            evidence_hash: "terminal-hash".into(),
            created_at_ms: 1_000,
        },
    )
    .unwrap();
    let job = db.claim_next_due_ai_job(1_000).unwrap().unwrap();
    assert!(
        db.complete_ai_job_generation_error(
            &job.id,
            job.generation,
            1_100,
            "invalid bounded response",
            AiExecutionErrorKind::InvalidResponse,
            Some("openai"),
            Some("review-test-model"),
            Some(0),
        )
        .unwrap()
    );

    let reviews = db
        .list_ai_reviews(&AiReviewFilter {
            states: vec![AiReviewState::ExecutionError],
            subject_id: Some("terminal-error".into()),
            ..AiReviewFilter::default()
        })
        .unwrap();
    assert_eq!(reviews.len(), 1);
    assert_eq!(
        reviews[0].execution.error_kind,
        Some(AiExecutionErrorKind::InvalidResponse)
    );
    assert!(reviews[0].proposed_json.is_empty());
}
