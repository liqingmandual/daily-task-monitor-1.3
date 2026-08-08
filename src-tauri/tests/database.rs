use daily_task_monitor_core::ai::{
    AiExecutionErrorKind, AiExecutionMode, AiExecutionSnapshot, AiJobStatus,
};
use daily_task_monitor_core::browser::BrowserVisit;
use daily_task_monitor_core::db::{
    ActivitySegmentRecord, DailyAnalysisRecord, Database, TrendAnalysisRecord,
};
use daily_task_monitor_core::domain::{
    ActivityCategory, ActivityScope, ClassificationSource, VideoPurpose,
};
use rusqlite::Connection;

fn segment(
    id: &str,
    start_ms: i64,
    duration_seconds: i64,
    category: ActivityCategory,
    purpose: VideoPurpose,
) -> ActivitySegmentRecord {
    ActivitySegmentRecord {
        id: id.into(),
        started_at_ms: start_ms,
        ended_at_ms: start_ms + duration_seconds * 1_000,
        app: "fixture-app".into(),
        app_path: String::new(),
        title: "fixture-title".into(),
        category,
        video_purpose: purpose,
        confidence: 0.9,
        source: ClassificationSource::Rule,
        reason: "fixture".into(),
        model_version: "rules-v1".into(),
        needs_review: false,
        inactivity_reason: None,
    }
}

fn snapshot(mode: AiExecutionMode, executor_id: &str, model: &str) -> AiExecutionSnapshot {
    AiExecutionSnapshot {
        execution_mode: mode,
        executor_id: executor_id.into(),
        model: model.into(),
        evidence_hash: String::new(),
        created_at_ms: 1_000,
    }
}

#[test]
fn executable_path_round_trips_with_a_segment() {
    let db = Database::open_in_memory().unwrap();
    let mut item = segment(
        "with-path",
        1_000,
        30,
        ActivityCategory::CreationDevelopment,
        VideoPurpose::Unknown,
    );
    item.app = "ChatGPT".into();
    item.app_path = r"C:\Program Files\WindowsApps\OpenAI.Codex_26.707.3748.0_x64__2p2nqsd0c76g0\app\ChatGPT.exe".into();

    db.insert_segment(&item).unwrap();

    assert_eq!(
        db.list_segments(0, 100_000).unwrap()[0].app_path,
        item.app_path
    );
}

#[test]
fn migration_keeps_legacy_rows_valid_with_an_empty_executable_path() {
    let path = std::env::temp_dir().join(format!(
        "daily-task-monitor-task4-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE activity_segments (
                id TEXT PRIMARY KEY,
                started_at_ms INTEGER NOT NULL,
                ended_at_ms INTEGER NOT NULL,
                app TEXT NOT NULL,
                title TEXT NOT NULL,
                category TEXT NOT NULL,
                video_purpose TEXT NOT NULL,
                confidence REAL NOT NULL,
                classification_source TEXT NOT NULL,
                reason TEXT NOT NULL,
                model_version TEXT NOT NULL,
                needs_review INTEGER NOT NULL DEFAULT 0,
                origin TEXT NOT NULL DEFAULT 'native'
            );
            INSERT INTO activity_segments VALUES (
                'legacy-row', 1000, 2000, 'Codex', 'Legacy title',
                'creation_development', 'unknown', 0.9, 'rule', 'legacy', 'v1', 0, 'legacy'
            );",
            )
            .unwrap();
    }

    let rows = Database::open(&path)
        .unwrap()
        .list_segments(0, 3_000)
        .unwrap();
    assert_eq!(rows[0].app_path, "");
    let _ = std::fs::remove_file(path);
}

#[test]
fn migrations_create_the_core_tables() {
    let db = Database::open_in_memory().unwrap();

    for table in [
        "activity_samples",
        "activity_segments",
        "browser_visits",
        "page_snapshots",
        "classifications",
        "classification_rules",
        "daily_goals",
        "daily_analyses",
        "trend_analyses",
        "focus_sessions",
        "ai_jobs",
        "settings",
    ] {
        assert!(db.has_table(table).unwrap(), "missing table {table}");
    }
}

#[test]
fn ai_job_generation_migrates_and_pauses_legacy_rows() {
    let path = std::env::temp_dir().join(format!(
        "daily-task-monitor-ai-generation-migration-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE ai_jobs (
                    id TEXT PRIMARY KEY,
                    content_hash TEXT NOT NULL UNIQUE,
                    kind TEXT NOT NULL,
                    payload_json TEXT NOT NULL,
                    status TEXT NOT NULL,
                    attempts INTEGER NOT NULL DEFAULT 0,
                    next_attempt_at_ms INTEGER NOT NULL,
                    last_error TEXT NOT NULL DEFAULT ''
                );
                INSERT INTO ai_jobs VALUES (
                    'legacy-job', 'legacy-hash', 'trend_analysis', '{}',
                    'pending', 0, 1000, ''
                );",
            )
            .unwrap();
    }

    let db = Database::open(&path).unwrap();
    let migrated = db.get_ai_job("legacy-job").unwrap().unwrap();
    assert_eq!(migrated.generation, 0);
    assert_eq!(migrated.execution.execution_mode, AiExecutionMode::ApiKey);
    assert_eq!(migrated.execution.executor_id, "legacy-provider-registry");
    assert_eq!(migrated.execution.model, "");
    assert_eq!(migrated.execution.evidence_hash, "");
    assert_eq!(migrated.execution.created_at_ms, 1000);
    assert_eq!(migrated.started_at_ms, None);
    assert_eq!(migrated.finished_at_ms, None);
    assert_eq!(migrated.duration_ms, None);
    assert_eq!(migrated.executor_id, None);
    assert_eq!(migrated.model, None);
    assert_eq!(migrated.exit_code, None);
    assert_eq!(migrated.error_kind, None);
    assert_eq!(migrated.status, AiJobStatus::AwaitingReassignment);
    assert!(db.claim_next_due_ai_job(1_000).unwrap().is_none());
    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn ai_job_failure_audit_sanitizes_and_truncates_error_details() {
    let db = Database::open_in_memory().unwrap();
    let id = db
        .enqueue_ai_job(
            "classify_segment",
            "{}",
            1_000,
            &snapshot(AiExecutionMode::ApiKey, "openai", "gpt-4.1-mini"),
        )
        .unwrap();
    let run = db.claim_next_due_ai_job(1_000).unwrap().unwrap();
    let noisy = format!(
        concat!(
            "Authorization: Bearer bearer-secret API-Key=api-secret token: token-secret ",
            "https://user:url-secret@example.com/path?api_key=query-secret&safe=value ",
            "{{\"prompt\":\"prompt-secret\",\"evidence\":{{\"text\":\"evidence-secret\"}}}} ",
            "stderr: raw-cli-secret {}"
        ),
        "x".repeat(1_200)
    );

    assert!(
        db.fail_ai_job_generation(
            &id,
            run.generation,
            1_750,
            &noisy,
            AiExecutionErrorKind::Provider,
            Some("openai"),
            Some("gpt-4.1-mini"),
            Some(7),
        )
        .unwrap()
    );
    let failed = db.get_ai_job(&id).unwrap().unwrap();

    assert_eq!(failed.status, AiJobStatus::Pending);
    assert_eq!(failed.started_at_ms, Some(1_000));
    assert_eq!(failed.finished_at_ms, Some(1_750));
    assert_eq!(failed.duration_ms, Some(750));
    assert_eq!(failed.executor_id, Some("openai".into()));
    assert_eq!(failed.model, Some("gpt-4.1-mini".into()));
    assert_eq!(failed.exit_code, Some(7));
    assert_eq!(failed.error_kind, Some(AiExecutionErrorKind::Provider));
    assert!(failed.last_error.len() <= 1_000);
    for secret in [
        "bearer-secret",
        "api-secret",
        "token-secret",
        "user",
        "url-secret",
        "query-secret",
        "prompt-secret",
        "evidence-secret",
        "raw-cli-secret",
    ] {
        assert!(
            !failed.last_error.contains(secret),
            "diagnostic leaked {secret}: {}",
            failed.last_error
        );
    }
}

#[test]
fn ai_job_failure_audit_redacts_structural_prompt_evidence_and_url_secrets() {
    let db = Database::open_in_memory().unwrap();
    let id = db
        .enqueue_ai_job(
            "trend_analysis",
            "{}",
            1_000,
            &snapshot(AiExecutionMode::ApiKey, "openai", "gpt-4.1-mini"),
        )
        .unwrap();
    let run = db.claim_next_due_ai_job(1_000).unwrap().unwrap();
    let noisy = concat!(
        "payload: unquoted payload secret; prompt: unquoted multiword prompt secret evidence: unquoted evidence secret ",
        r#"json {"prompt":"escaped \"quote prompt secret\" tail","evidence":"escaped evidence secret"} "#,
        r#"malformed prompt="missing close prompt secret evidence={missing close evidence secret "#,
        "url https://user:url-password@example.com/private/token/path?api_key=query-secret#frag-secret ",
        "bad-url https://example.com/%zz/private-fragment-secret#bad-fragment"
    );

    assert!(
        db.fail_ai_job_generation(
            &id,
            run.generation,
            1_500,
            noisy,
            AiExecutionErrorKind::InvalidResponse,
            Some("openai"),
            Some("gpt-4.1-mini"),
            None,
        )
        .unwrap()
    );
    let failed = db.get_ai_job(&id).unwrap().unwrap();

    assert_eq!(
        failed.error_kind,
        Some(AiExecutionErrorKind::InvalidResponse)
    );
    assert!(failed.last_error.len() <= 1_000);
    assert!(
        failed.last_error.contains("[redacted]") || failed.last_error.contains("[redacted-url]")
    );
    for secret in [
        "unquoted multiword prompt secret",
        "unquoted evidence secret",
        "unquoted payload secret",
        "escaped",
        "quote prompt secret",
        "escaped evidence secret",
        "missing close prompt secret",
        "missing close evidence secret",
        "url-password",
        "private/token/path",
        "query-secret",
        "frag-secret",
        "private-fragment-secret",
        "bad-fragment",
    ] {
        assert!(
            !failed.last_error.contains(secret),
            "diagnostic leaked {secret}: {}",
            failed.last_error
        );
    }
}

#[test]
fn ai_job_failure_audit_redacts_case_insensitive_and_malformed_http_urls() {
    let db = Database::open_in_memory().unwrap();
    let id = db
        .enqueue_ai_job(
            "trend_analysis",
            "{}",
            1_000,
            &snapshot(AiExecutionMode::ApiKey, "openai", "gpt-4.1-mini"),
        )
        .unwrap();
    let run = db.claim_next_due_ai_job(1_000).unwrap().unwrap();
    let diagnostic = format!(
        concat!(
            "upper HTTPS://upper-user:upper-password@upper.example/private/path?api_key=upper-query#upper-fragment ",
            "mixed HtTpS://mixed-user:mixed-password@mixed.example/secret?token=mixed-query#mixed-fragment ",
            "one-slash https:/slash-user:slash-password@slash.example/private?key=slash-query#slash-fragment ",
            "no-slash hTTp:colon-user:colon-password@colon.example/private?key=colon-query#colon-fragment ",
            "tail {}"
        ),
        "x".repeat(1_200)
    );

    assert!(
        db.fail_ai_job_generation(
            &id,
            run.generation,
            1_500,
            &diagnostic,
            AiExecutionErrorKind::Provider,
            Some("openai"),
            Some("gpt-4.1-mini"),
            None,
        )
        .unwrap()
    );
    let failed = db.get_ai_job(&id).unwrap().unwrap();

    assert!(failed.last_error.len() <= 1_000);
    for marker in ["upper", "mixed", "one-slash", "no-slash"] {
        assert!(
            failed
                .last_error
                .contains(&format!("{marker} [redacted-url]")),
            "diagnostic did not redact {marker} URL: {}",
            failed.last_error
        );
    }
    assert!(failed.last_error.contains("tail "));
    for secret in [
        "upper-user",
        "upper-password",
        "upper.example",
        "upper-query",
        "upper-fragment",
        "mixed-user",
        "mixed-password",
        "mixed.example",
        "mixed-query",
        "mixed-fragment",
        "slash-user",
        "slash-password",
        "slash.example",
        "slash-query",
        "slash-fragment",
        "colon-user",
        "colon-password",
        "colon.example",
        "colon-query",
        "colon-fragment",
        "/private",
    ] {
        assert!(
            !failed.last_error.contains(secret),
            "diagnostic leaked {secret}: {}",
            failed.last_error
        );
    }
}

#[test]
fn forced_generation_invalidates_old_worker_success_and_failure() {
    let db = Database::open_in_memory().unwrap();
    let id = db
        .enqueue_ai_job_for_subject(
            "trend_analysis",
            "same-hash",
            "{}",
            1_000,
            &snapshot(AiExecutionMode::ApiKey, "openai", "gpt-4.1-mini"),
        )
        .unwrap();
    let old_run = db.claim_next_due_ai_job(1_000).unwrap().unwrap();
    assert_eq!(old_run.id, id);
    assert_eq!(old_run.status, AiJobStatus::Running);

    let forced = db
        .force_enqueue_ai_job_for_subject(
            "trend_analysis",
            "same-hash",
            "{}",
            2_000,
            &snapshot(AiExecutionMode::Codex, "codex", "gpt-5-codex"),
        )
        .unwrap();
    assert_ne!(forced, id);
    assert_eq!(db.get_ai_job(&id).unwrap().unwrap(), old_run);
    let pending = db.get_ai_job(&forced).unwrap().unwrap();
    assert_eq!(pending.generation, old_run.generation + 1);
    assert_eq!(pending.status, AiJobStatus::Pending);

    assert!(
        !db.complete_ai_job_generation_audit(
            &id,
            old_run.generation,
            3_000,
            Some("openai"),
            Some("gpt-test"),
            Some(0),
        )
        .unwrap()
    );
    let retired = db.get_ai_job(&id).unwrap().unwrap();
    assert_eq!(retired.status, AiJobStatus::Complete);
    assert_eq!(retired.attempts, old_run.attempts);
    assert!(
        retired
            .last_error
            .to_ascii_lowercase()
            .contains("superseded")
    );
    let still_pending = db.get_ai_job(&forced).unwrap().unwrap();
    assert_eq!(still_pending.generation, pending.generation);
    assert_eq!(still_pending.status, AiJobStatus::Pending);
    assert_eq!(still_pending.attempts, 0);
    assert_eq!(still_pending.last_error, "");

    let new_run = db.claim_next_due_ai_job(2_000).unwrap().unwrap();
    assert_eq!(new_run.generation, pending.generation);
    assert!(
        db.complete_ai_job_generation_audit(
            &forced,
            new_run.generation,
            4_000,
            Some("codex"),
            Some("gpt-5-codex"),
            Some(0),
        )
        .unwrap()
    );
    assert_eq!(
        db.get_ai_job(&forced).unwrap().unwrap().status,
        AiJobStatus::Complete
    );
}

#[test]
fn superseded_terminal_error_completion_retires_only_the_exact_old_generation() {
    let db = Database::open_in_memory().unwrap();
    let old_id = db
        .enqueue_ai_job_for_subject(
            "classify_page",
            "terminal-error",
            "{}",
            1_000,
            &snapshot(AiExecutionMode::ApiKey, "openai", "gpt-test"),
        )
        .unwrap();
    let old_run = db.claim_next_due_ai_job(1_000).unwrap().unwrap();
    let current_id = db
        .force_enqueue_ai_job_for_subject(
            "classify_page",
            "terminal-error",
            r#"{"visitId":"current"}"#,
            2_000,
            &snapshot(AiExecutionMode::Codex, "codex", "gpt-5-codex"),
        )
        .unwrap();
    let current_run = db.claim_next_due_ai_job(2_000).unwrap().unwrap();

    assert!(
        !db.complete_ai_job_generation_error(
            &old_id,
            old_run.generation,
            3_000,
            "invalid obsolete payload",
            AiExecutionErrorKind::InvalidJob,
            Some("openai"),
            Some("gpt-test"),
            None,
        )
        .unwrap()
    );

    let retired = db.get_ai_job(&old_id).unwrap().unwrap();
    assert_eq!(retired.status, AiJobStatus::Complete);
    assert_eq!(retired.attempts, old_run.attempts);
    assert!(
        retired
            .last_error
            .to_ascii_lowercase()
            .contains("superseded")
    );
    assert_eq!(retired.error_kind, None);
    assert_eq!(db.get_ai_job(&current_id).unwrap().unwrap(), current_run);
}

#[test]
fn trend_analysis_and_generation_complete_commit_atomically() {
    let db = Database::open_in_memory().unwrap();
    let id = db
        .enqueue_ai_job_for_subject(
            "trend_analysis",
            "atomic-hash",
            "{}",
            1_000,
            &snapshot(AiExecutionMode::ApiKey, "openai", "gpt-4.1-mini"),
        )
        .unwrap();
    let old_run = db.claim_next_due_ai_job(1_000).unwrap().unwrap();
    let analysis = TrendAnalysisRecord {
        range_start: "2026-07-06".into(),
        range_end: "2026-07-12".into(),
        evidence_hash: "atomic-hash".into(),
        summary: "Summary".into(),
        observations: vec!["Observation".into()],
        suggestions: vec!["Suggestion".into()],
        source: "openai".into(),
        model: "gpt-test".into(),
        confidence: 0.8,
        generated_at_ms: 3_000,
    };

    let forced = db
        .force_enqueue_ai_job_for_subject(
            "trend_analysis",
            "atomic-hash",
            "{}",
            2_000,
            &snapshot(AiExecutionMode::Codex, "codex", "gpt-5-codex"),
        )
        .unwrap();
    assert!(
        !db.complete_trend_analysis_job_generation(&id, old_run.generation, &analysis)
            .unwrap()
    );
    assert!(
        db.get_trend_analysis(
            &analysis.range_start,
            &analysis.range_end,
            &analysis.evidence_hash,
        )
        .unwrap()
        .is_none()
    );
    let pending = db.get_ai_job(&forced).unwrap().unwrap();
    assert_eq!(pending.generation, old_run.generation + 1);
    assert_eq!(pending.status, AiJobStatus::Pending);

    let current_run = db.claim_next_due_ai_job(2_000).unwrap().unwrap();
    assert!(
        db.complete_trend_analysis_job_generation(&forced, current_run.generation, &analysis)
            .unwrap()
    );
    assert_eq!(
        db.get_trend_analysis(
            &analysis.range_start,
            &analysis.range_end,
            &analysis.evidence_hash,
        )
        .unwrap(),
        Some(analysis)
    );
    assert_eq!(
        db.get_ai_job(&forced).unwrap().unwrap().status,
        AiJobStatus::Complete
    );
}

#[test]
fn retry_between_daily_execution_and_completion_cannot_persist_old_result() {
    let db = Database::open_in_memory().unwrap();
    let id = db
        .enqueue_ai_job_for_subject(
            "daily_analysis",
            "2026-07-14:daily-hash",
            "{}",
            1_000,
            &snapshot(AiExecutionMode::ApiKey, "openai", "gpt-4.1-mini"),
        )
        .unwrap();
    let old_run = db.claim_next_due_ai_job(1_000).unwrap().unwrap();
    let analysis = DailyAnalysisRecord {
        date: "2026-07-14".into(),
        evidence_hash: "daily-hash".into(),
        portrait: "Old portrait".into(),
        recommendation: "Old recommendation".into(),
        findings_json: "[]".into(),
        protocol_version: 1,
        source: "ai".into(),
        generated_at_ms: 3_000,
    };

    db.force_enqueue_ai_job_for_subject(
        "daily_analysis",
        "2026-07-14:daily-hash",
        "{}",
        2_000,
        &snapshot(AiExecutionMode::Codex, "codex", "gpt-5-codex"),
    )
    .unwrap();

    assert!(
        !db.complete_daily_analysis_job_generation(
            &id,
            old_run.generation,
            &analysis,
            "openai",
            "gpt-4.1-mini",
            Some(0),
        )
        .unwrap()
    );
    assert!(db.get_daily_analysis(&analysis.date).unwrap().is_none());
}

#[test]
fn retry_between_classification_execution_and_completion_cannot_mutate_segment() {
    let db = Database::open_in_memory().unwrap();
    db.insert_segment(&segment(
        "race-segment",
        1_000,
        30,
        ActivityCategory::Pending,
        VideoPurpose::Unknown,
    ))
    .unwrap();
    let id = db
        .enqueue_ai_job_for_subject(
            "classify_segment",
            "race-segment",
            r#"{"id":"race-segment"}"#,
            1_000,
            &snapshot(AiExecutionMode::ApiKey, "openai", "gpt-4.1-mini"),
        )
        .unwrap();
    let old_run = db.claim_next_due_ai_job(1_000).unwrap().unwrap();

    db.force_enqueue_ai_job_for_subject(
        "classify_segment",
        "race-segment",
        r#"{"id":"race-segment"}"#,
        2_000,
        &snapshot(AiExecutionMode::Codex, "codex", "gpt-5-codex"),
    )
    .unwrap();

    assert!(
        !db.complete_segment_classification_job_generation(
            &id,
            old_run.generation,
            "race-segment",
            ActivityCategory::Research,
            VideoPurpose::Unknown,
            0.9,
            "stale AI result",
            "openai/gpt-4.1-mini",
            "openai",
            "gpt-4.1-mini",
            Some(0),
            3_000,
        )
        .unwrap()
    );
    let stored = db.list_segments(0, 100_000).unwrap().remove(0);
    assert_eq!(stored.category, ActivityCategory::Pending);
    assert_eq!(stored.source, ClassificationSource::Rule);
}

#[test]
fn trend_analysis_migration_is_idempotent_and_lookup_requires_exact_range_and_hash() {
    let path = std::env::temp_dir().join(format!(
        "daily-task-monitor-trend-analysis-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let record = TrendAnalysisRecord {
        range_start: "2026-07-06".into(),
        range_end: "2026-07-12".into(),
        evidence_hash: "exact-hash".into(),
        summary: "Summary".into(),
        observations: vec!["Observation".into()],
        suggestions: vec!["Suggestion".into()],
        source: "openai".into(),
        model: "gpt-test".into(),
        confidence: 0.8,
        generated_at_ms: 10_000,
    };

    {
        let db = Database::open(&path).unwrap();
        db.save_trend_analysis(&record).unwrap();
        assert_eq!(
            db.get_trend_analysis("2026-07-06", "2026-07-12", "exact-hash")
                .unwrap(),
            Some(record.clone())
        );
        assert!(
            db.get_trend_analysis("2026-07-05", "2026-07-12", "exact-hash")
                .unwrap()
                .is_none()
        );
        assert!(
            db.get_trend_analysis("2026-07-06", "2026-07-12", "other-hash")
                .unwrap()
                .is_none()
        );
    }

    let reopened = Database::open(&path).unwrap();
    assert_eq!(
        reopened
            .get_trend_analysis("2026-07-06", "2026-07-12", "exact-hash")
            .unwrap(),
        Some(record)
    );
    drop(reopened);
    let _ = std::fs::remove_file(path);
}

#[test]
fn dashboard_totals_are_derived_from_non_overlapping_segments() {
    let db = Database::open_in_memory().unwrap();
    db.insert_segment(&segment(
        "research",
        1_000,
        120,
        ActivityCategory::Research,
        VideoPurpose::Unknown,
    ))
    .unwrap();
    db.insert_segment(&segment(
        "learning-video",
        121_000,
        180,
        ActivityCategory::VideoInput,
        VideoPurpose::Learning,
    ))
    .unwrap();
    db.insert_segment(&segment(
        "leisure-video",
        301_000,
        60,
        ActivityCategory::VideoInput,
        VideoPurpose::Leisure,
    ))
    .unwrap();
    db.insert_segment(&segment(
        "idle",
        361_000,
        240,
        ActivityCategory::Idle,
        VideoPurpose::Unknown,
    ))
    .unwrap();

    let totals = db.dashboard_totals(0, 1_000_000).unwrap();

    assert_eq!(totals.monitored_seconds, 600);
    assert_eq!(totals.active_seconds, 360);
    assert_eq!(totals.idle_seconds, 240);
    assert_eq!(totals.learning_seconds, 300);
    assert_eq!(totals.category_seconds["video_input"], 240);
}

#[test]
fn inserting_same_segment_id_is_idempotent() {
    let db = Database::open_in_memory().unwrap();
    let item = segment(
        "stable-id",
        1_000,
        30,
        ActivityCategory::CreationDevelopment,
        VideoPurpose::Unknown,
    );

    db.insert_segment(&item).unwrap();
    db.insert_segment(&item).unwrap();

    assert_eq!(db.segment_count().unwrap(), 1);
}

#[test]
fn native_upsert_extends_a_live_segment_without_duplicating_it() {
    let db = Database::open_in_memory().unwrap();
    let mut item = segment(
        "live",
        1_000,
        5,
        ActivityCategory::CreationDevelopment,
        VideoPurpose::Unknown,
    );
    db.upsert_native_segment(&item).unwrap();
    item.ended_at_ms = 16_000;
    db.upsert_native_segment(&item).unwrap();

    let rows = db.list_segments(0, 20_000).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].ended_at_ms, 16_000);
}

#[test]
fn native_upsert_can_shorten_a_provisional_segment_during_idle_backfill() {
    let db = Database::open_in_memory().unwrap();
    let mut item = segment(
        "provisional",
        1_000,
        355,
        ActivityCategory::CreationDevelopment,
        VideoPurpose::Unknown,
    );
    db.upsert_native_segment(&item).unwrap();
    item.ended_at_ms = item.started_at_ms;
    db.upsert_native_segment(&item).unwrap();

    assert_eq!(db.list_segments(0, 400_000).unwrap()[0].ended_at_ms, 1_000);
}

#[test]
fn ai_classification_never_overwrites_a_manual_correction() {
    let db = Database::open_in_memory().unwrap();
    let item = segment(
        "manual-first",
        1_000,
        30,
        ActivityCategory::Pending,
        VideoPurpose::Unknown,
    );
    db.insert_segment(&item).unwrap();
    db.save_manual_classification(
        "manual-first",
        ActivityCategory::Social,
        VideoPurpose::Unknown,
        "User correction",
    )
    .unwrap();

    let changed = db
        .apply_ai_classification(
            "manual-first",
            ActivityCategory::Research,
            VideoPurpose::Unknown,
            0.95,
            "AI suggestion",
            "provider/model",
        )
        .unwrap();

    assert!(!changed);
    assert_eq!(
        db.list_segments(0, 100_000).unwrap()[0].category,
        ActivityCategory::Social
    );
}

#[test]
fn native_upsert_preserves_an_existing_ai_classification() {
    let db = Database::open_in_memory().unwrap();
    let mut item = segment(
        "ai-stable",
        1_000,
        30,
        ActivityCategory::Pending,
        VideoPurpose::Unknown,
    );
    db.upsert_native_segment(&item).unwrap();
    db.apply_ai_classification(
        "ai-stable",
        ActivityCategory::Research,
        VideoPurpose::Unknown,
        0.91,
        "AI evidence",
        "provider/model",
    )
    .unwrap();

    item.ended_at_ms = 45_000;
    db.upsert_native_segment(&item).unwrap();

    let stored = &db.list_segments(0, 100_000).unwrap()[0];
    assert_eq!(stored.ended_at_ms, 45_000);
    assert_eq!(stored.category, ActivityCategory::Research);
    assert_eq!(
        stored.source,
        daily_task_monitor_core::domain::ClassificationSource::Ai
    );
    assert_eq!(stored.reason, "AI evidence");
}

#[test]
fn ai_cannot_turn_a_locally_active_segment_into_idle_time() {
    let db = Database::open_in_memory().unwrap();
    let item = segment(
        "active-not-idle",
        1_000,
        30,
        ActivityCategory::Pending,
        VideoPurpose::Unknown,
    );
    db.insert_segment(&item).unwrap();

    let changed = db
        .apply_ai_classification(
            "active-not-idle",
            ActivityCategory::Idle,
            VideoPurpose::Unknown,
            0.95,
            "Model guessed idle",
            "provider/model",
        )
        .unwrap();

    assert!(!changed);
    assert_eq!(
        db.list_segments(0, 100_000).unwrap()[0].category,
        ActivityCategory::Pending
    );
}

#[test]
fn recent_browser_context_can_enrich_a_foreground_browser_sample() {
    let db = Database::open_in_memory().unwrap();
    db.insert_browser_visit(
        "visit-context",
        "Chrome",
        "Default",
        &BrowserVisit {
            visited_at_ms: 20_000,
            url: "https://example.com/article".into(),
            title: "Article".into(),
        },
        "example.com",
    )
    .unwrap();

    assert_eq!(
        db.latest_browser_context(19_000, 21_000).unwrap(),
        Some(("example.com".into(), "Article".into()))
    );
}

#[test]
fn trend_range_segment_query_clips_records_to_requested_bounds() {
    let db = Database::open_in_memory().unwrap();
    db.insert_segment(&segment(
        "crossing",
        1_000,
        120,
        ActivityCategory::CreationDevelopment,
        VideoPurpose::Unknown,
    ))
    .unwrap();

    let records = db.list_clipped_segments(31_000, 91_000).unwrap();

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].started_at_ms, 31_000);
    assert_eq!(records[0].ended_at_ms, 91_000);
}

#[test]
fn scoped_daily_analysis_cache_keeps_all_and_meaningful_results_independent() {
    let db = Database::open_in_memory().unwrap();
    let record = |evidence_hash: &str, portrait: &str| DailyAnalysisRecord {
        date: "2026-08-07".into(),
        evidence_hash: evidence_hash.into(),
        portrait: portrait.into(),
        recommendation: "next step".into(),
        findings_json: "[]".into(),
        protocol_version: 2,
        source: "ai".into(),
        generated_at_ms: 1,
    };

    db.save_daily_analysis_scoped(ActivityScope::All, &record("all-hash", "all result"))
        .unwrap();
    db.save_daily_analysis_scoped(
        ActivityScope::Meaningful,
        &record("meaningful-hash", "meaningful result"),
    )
    .unwrap();

    assert_eq!(
        db.get_daily_analysis_scoped("2026-08-07", ActivityScope::All)
            .unwrap()
            .unwrap()
            .portrait,
        "all result"
    );
    assert_eq!(
        db.get_daily_analysis_scoped("2026-08-07", ActivityScope::Meaningful)
            .unwrap()
            .unwrap()
            .portrait,
        "meaningful result"
    );
}
