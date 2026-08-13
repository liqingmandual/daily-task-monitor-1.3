use daily_task_monitor_core::ai::{
    AiExecutionErrorKind, AiExecutionMode, AiExecutionSnapshot, AiJobStatus, AiProviderConfig,
    ProviderRegistry, validate_provider_endpoint,
};
use daily_task_monitor_core::ai_executor::{
    AiExecutionBackends, AiExecutionErrorKind as ExecutorErrorKind, AiExecutionRequest,
    ApiProviderCredential, execute_ai,
};
use daily_task_monitor_core::db::Database;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

fn queue_test_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "daily-task-monitor-{label}-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn provider(id: &str, priority: i32, has_credential: bool) -> AiProviderConfig {
    AiProviderConfig {
        id: id.into(),
        name: id.into(),
        base_url: format!("https://{id}.example.com/v1"),
        model: "model".into(),
        enabled: true,
        auto_safe: true,
        priority,
        has_credential,
    }
}

fn snapshot(
    mode: AiExecutionMode,
    executor_id: &str,
    model: &str,
    evidence_hash: &str,
    created_at_ms: i64,
) -> AiExecutionSnapshot {
    AiExecutionSnapshot {
        execution_mode: mode,
        executor_id: executor_id.into(),
        model: model.into(),
        evidence_hash: evidence_hash.into(),
        created_at_ms,
    }
}

fn spawn_success_provider() -> (String, mpsc::Receiver<()>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let (called_tx, called_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_millis(750);
        while std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut request = [0_u8; 4096];
                    let _ = stream.read(&mut request);
                    let body = r#"{"choices":[{"message":{"content":"fallback"}}]}"#;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                    let _ = called_tx.send(());
                    return;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return,
            }
        }
    });
    (format!("http://{address}"), called_rx, server)
}

#[test]
fn api_execution_never_falls_back_from_the_snapshotted_provider() {
    let (fallback_url, fallback_called, server) = spawn_success_provider();
    let request = AiExecutionRequest {
        job_id: "no-provider-fallback".into(),
        kind: "daily_analysis".into(),
        snapshot: snapshot(
            AiExecutionMode::ApiKey,
            "removed-primary",
            "queued-model",
            "hash",
            1_000,
        ),
        system_prompt: "system".into(),
        minimal_payload_json: "{}".into(),
        timeout_ms: 500,
    };
    let backends = AiExecutionBackends {
        api_providers: vec![ApiProviderCredential {
            provider: AiProviderConfig {
                base_url: fallback_url,
                ..provider("fallback", 0, true)
            },
            api_key: "fallback-key".into(),
        }],
        configured_codex: None,
    };

    let error = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(execute_ai(&request, &backends))
        .expect_err("a missing snapshotted provider must fail without fallback");

    assert_eq!(error.kind, ExecutorErrorKind::Provider);
    assert!(
        fallback_called
            .recv_timeout(Duration::from_millis(100))
            .is_err()
    );
    server.join().unwrap();
}

#[test]
fn provider_registry_selects_configured_providers_by_priority() {
    let registry = ProviderRegistry::new(vec![
        provider("openai", 50, true),
        provider("zhipu", 10, true),
        provider("missing-key", 1, false),
    ]);

    let ids: Vec<_> = registry
        .automatic_candidates()
        .iter()
        .map(|item| item.id.as_str())
        .collect();

    assert_eq!(ids, vec!["zhipu", "openai"]);
}

#[test]
fn queue_deduplicates_jobs_by_content_hash() {
    let db = Database::open_in_memory().unwrap();

    let first = db
        .enqueue_ai_job(
            "classify",
            r#"{"segment":"one"}"#,
            1_000,
            &snapshot(AiExecutionMode::ApiKey, "openai", "gpt-4.1-mini", "", 1_000),
        )
        .unwrap();
    let second = db
        .enqueue_ai_job(
            "classify",
            r#"{"segment":"one"}"#,
            2_000,
            &snapshot(AiExecutionMode::ApiKey, "openai", "gpt-4.1-mini", "", 2_000),
        )
        .unwrap();

    assert_eq!(first, second);
    assert_eq!(db.ai_job_count().unwrap(), 1);
}

#[test]
fn queue_identity_includes_execution_snapshot() {
    let db = Database::open_in_memory().unwrap();
    let api = snapshot(
        AiExecutionMode::ApiKey,
        "openai",
        "gpt-4.1-mini",
        "hash-a",
        1_000,
    );
    let codex = snapshot(
        AiExecutionMode::Codex,
        "codex",
        "gpt-5-codex",
        "hash-a",
        2_000,
    );

    let first = db
        .enqueue_ai_job_for_subject("daily_analysis", "2026-07-14", "{}", 1_000, &api)
        .unwrap();
    let second = db
        .enqueue_ai_job_for_subject("daily_analysis", "2026-07-14", "{}", 2_000, &codex)
        .unwrap();

    assert_ne!(first, second);
    assert_eq!(db.ai_job_count().unwrap(), 2);
    assert_eq!(db.get_ai_job(&first).unwrap().unwrap().execution, api);
    assert_eq!(db.get_ai_job(&second).unwrap().unwrap().execution, codex);
}

#[test]
fn retry_keeps_snapshot_and_retry_current_creates_new_generation_snapshot() {
    let db = Database::open_in_memory().unwrap();
    let api = snapshot(
        AiExecutionMode::ApiKey,
        "openai",
        "gpt-4.1-mini",
        "hash-a",
        1_000,
    );
    let codex = snapshot(
        AiExecutionMode::Codex,
        "codex",
        "gpt-5-codex",
        "hash-a",
        2_000,
    );
    let id = db
        .enqueue_ai_job_for_subject("trend_analysis", "range:hash-a", "{}", 1_000, &api)
        .unwrap();

    let run = db.claim_next_due_ai_job(1_000).unwrap().unwrap();
    assert_eq!(run.execution, api);
    assert!(
        db.fail_ai_job_generation(
            &run.id,
            run.generation,
            2_000,
            "temporary failure",
            AiExecutionErrorKind::Provider,
            None,
            None,
            Some(1),
        )
        .unwrap()
    );
    let retry = db.claim_next_due_ai_job(62_000).unwrap().unwrap();
    assert_eq!(retry.id, id);
    assert_eq!(retry.generation, run.generation);
    assert_eq!(retry.execution, api);
    let historical_before_retry_current = db.get_ai_job(&id).unwrap().unwrap();

    let forced = db
        .force_enqueue_ai_job_for_subject("trend_analysis", "range:hash-a", "{}", 63_000, &codex)
        .unwrap();
    assert_ne!(forced, id);
    assert_eq!(
        db.get_ai_job(&id).unwrap().unwrap(),
        historical_before_retry_current
    );
    let next = db.get_ai_job(&forced).unwrap().unwrap();
    assert_eq!(next.generation, run.generation + 1);
    assert_eq!(next.execution, codex);
    assert_eq!(db.ai_job_count().unwrap(), 2);
}

#[test]
fn failed_jobs_use_exponential_backoff() {
    let db = Database::open_in_memory().unwrap();
    let id = db
        .enqueue_ai_job(
            "classify",
            "{}",
            1_000,
            &snapshot(AiExecutionMode::ApiKey, "openai", "gpt-4.1-mini", "", 1_000),
        )
        .unwrap();

    let first_run = db.claim_next_due_ai_job(1_000).unwrap().unwrap();
    db.fail_ai_job_generation(
        &id,
        first_run.generation,
        2_000,
        "offline",
        AiExecutionErrorKind::Provider,
        Some("openai"),
        Some("gpt-4.1-mini"),
        None,
    )
    .unwrap();
    let first = db.get_ai_job(&id).unwrap().unwrap();
    assert_eq!(first.status, AiJobStatus::Pending);
    assert_eq!(first.attempts, 1);
    assert_eq!(first.next_attempt_at_ms, 62_000);

    let second_run = db.claim_next_due_ai_job(62_000).unwrap().unwrap();
    db.fail_ai_job_generation(
        &id,
        second_run.generation,
        62_000,
        "still offline",
        AiExecutionErrorKind::Provider,
        Some("openai"),
        Some("gpt-4.1-mini"),
        None,
    )
    .unwrap();
    let second = db.get_ai_job(&id).unwrap().unwrap();
    assert_eq!(second.attempts, 2);
    assert_eq!(second.next_attempt_at_ms, 182_000);
}

#[test]
fn due_jobs_transition_from_pending_to_running_to_complete() {
    let db = Database::open_in_memory().unwrap();
    let execution = snapshot(
        AiExecutionMode::ApiKey,
        "openai",
        "gpt-4.1-mini",
        "segment-hash",
        1_000,
    );
    let id = db
        .enqueue_ai_job("classify_segment", "{}", 1_000, &execution)
        .unwrap();

    assert!(db.next_due_ai_job(999).unwrap().is_none());
    let job = db.next_due_ai_job(1_000).unwrap().unwrap();
    assert_eq!(job.id, id);
    assert_eq!(job.execution, execution);
    db.mark_ai_job_running(&id, 1_500).unwrap();
    assert!(db.next_due_ai_job(2_000).unwrap().is_none());
    db.complete_ai_job_generation_audit(
        &id,
        0,
        2_250,
        Some("openai"),
        Some("gpt-4.1-mini"),
        Some(0),
    )
    .unwrap();

    let completed = db.get_ai_job(&id).unwrap().unwrap();
    assert_eq!(completed.status, AiJobStatus::Complete);
    assert_eq!(completed.started_at_ms, Some(1_500));
    assert_eq!(completed.finished_at_ms, Some(2_250));
    assert_eq!(completed.duration_ms, Some(750));
    assert_eq!(completed.executor_id, Some("openai".into()));
    assert_eq!(completed.model, Some("gpt-4.1-mini".into()));
    assert_eq!(completed.exit_code, Some(0));
    assert_eq!(completed.error_kind, None);
    assert_eq!(db.pending_ai_job_count().unwrap(), 0);
}

#[test]
fn analysis_jobs_are_claimed_before_older_bulk_classification_jobs() {
    let db = Database::open_in_memory().unwrap();
    let execution = snapshot(AiExecutionMode::ApiKey, "openai", "gpt-4.1-mini", "", 1_000);
    db.enqueue_ai_job_for_subject("classify_segment", "segment-1", "{}", 1_000, &execution)
        .unwrap();
    db.enqueue_ai_job_for_subject("daily_analysis", "2026-07-13", "{}", 2_000, &execution)
        .unwrap();

    let claimed = db.claim_next_due_ai_job(2_000).unwrap().unwrap();

    assert_eq!(claimed.kind, "daily_analysis");
}

#[test]
fn workflow_recognition_is_claimed_before_analysis_and_bulk_classification() {
    let db = Database::open_in_memory().unwrap();
    let execution = snapshot(
        AiExecutionMode::Codex,
        "codex",
        "cli-default",
        "evidence",
        1_000,
    );
    let classification = db
        .enqueue_ai_job("classify_segment", "{}", 1_000, &execution)
        .unwrap();
    let analysis = db
        .enqueue_ai_job("daily_analysis", "{}", 2_000, &execution)
        .unwrap();
    let workflow = db
        .enqueue_ai_job("work_ledger_assignment", "{}", 3_000, &execution)
        .unwrap();

    assert_eq!(
        db.claim_next_due_ai_job(3_000).unwrap().unwrap().id,
        workflow
    );
    assert_eq!(
        db.claim_next_due_ai_job(3_000).unwrap().unwrap().id,
        analysis
    );
    assert_eq!(
        db.claim_next_due_ai_job(3_000).unwrap().unwrap().id,
        classification
    );
}

#[test]
fn unavailable_versioned_codex_jobs_are_quarantined_without_touching_current_jobs() {
    let db = Database::open_in_memory().unwrap();
    let missing = snapshot(
        AiExecutionMode::Codex,
        r#"C:\missing-codex-version\codex.exe"#,
        "cli-default",
        "old",
        1_000,
    );
    let portable = snapshot(
        AiExecutionMode::Codex,
        "codex",
        "cli-default",
        "current",
        2_000,
    );
    let stale_id = db
        .enqueue_ai_job("classify_segment", "{}", 1_000, &missing)
        .unwrap();
    let current_id = db
        .enqueue_ai_job("work_ledger_assignment", "{}", 2_000, &portable)
        .unwrap();

    assert_eq!(db.quarantine_unavailable_codex_jobs(3_000).unwrap(), 1);
    assert_eq!(
        db.get_ai_job(&stale_id).unwrap().unwrap().status,
        AiJobStatus::AwaitingReassignment
    );
    assert_eq!(
        db.get_ai_job(&current_id).unwrap().unwrap().status,
        AiJobStatus::Pending
    );
}

#[test]
fn stable_subject_key_deduplicates_a_growing_live_segment() {
    let db = Database::open_in_memory().unwrap();
    let initial = snapshot(
        AiExecutionMode::ApiKey,
        "openai",
        "gpt-4.1-mini",
        "evidence-5s",
        1_000,
    );
    let first = db
        .enqueue_ai_job_for_subject(
            "classify_segment",
            "segment-42",
            r#"{"id":"segment-42","endedAtMs":5000}"#,
            1_000,
            &initial,
        )
        .unwrap();
    let updated = snapshot(
        AiExecutionMode::ApiKey,
        "openai",
        "gpt-4.1-mini",
        "evidence-10s",
        2_000,
    );
    let second = db
        .enqueue_ai_job_for_subject(
            "classify_segment",
            "segment-42",
            r#"{"id":"segment-42","endedAtMs":10000}"#,
            2_000,
            &updated,
        )
        .unwrap();

    assert_eq!(first, second);
    assert_eq!(db.ai_job_count().unwrap(), 1);
    assert!(
        db.get_ai_job(&first)
            .unwrap()
            .unwrap()
            .payload_json
            .contains("10000")
    );
    assert_eq!(
        db.get_ai_job(&first).unwrap().unwrap().execution,
        updated,
        "the pending job must carry the newest evidence snapshot"
    );
}

#[test]
fn v14_migration_discards_only_unattempted_regenerable_ai_backlog() {
    let path = queue_test_path("v14-ai-backlog-cleanup");
    let db = Database::open(&path).unwrap();
    let execution = snapshot(AiExecutionMode::ApiKey, "openai", "gpt-test", "hash", 1_000);
    let derived = db
        .enqueue_ai_job_for_subject("classify_segment", "segment-stale", "{}", 1_000, &execution)
        .unwrap();
    let completed = db
        .enqueue_ai_job_for_subject("classify_page", "visit-complete", "{}", 2_000, &execution)
        .unwrap();
    let custom = db
        .enqueue_ai_job_for_subject("custom_job", "custom", "{}", 3_000, &execution)
        .unwrap();
    drop(db);

    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE ai_jobs SET status='complete' WHERE id=?1",
            [&completed],
        )
        .unwrap();
    connection
        .execute_batch("PRAGMA user_version = 13;")
        .unwrap();
    drop(connection);

    let upgraded = Database::open(&path).unwrap();
    assert!(upgraded.get_ai_job(&derived).unwrap().is_none());
    assert_eq!(
        upgraded.get_ai_job(&completed).unwrap().unwrap().status,
        AiJobStatus::Complete
    );
    assert_eq!(
        upgraded.get_ai_job(&custom).unwrap().unwrap().status,
        AiJobStatus::Pending
    );
    drop(upgraded);
    let _ = std::fs::remove_file(path);
}

#[test]
fn database_startup_recovers_interrupted_running_jobs() {
    let path = std::env::temp_dir().join(format!(
        "daily-task-monitor-ai-recovery-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let id = {
        let db = Database::open(&path).unwrap();
        let id = db
            .enqueue_ai_job(
                "classify",
                "{}",
                1_000,
                &snapshot(AiExecutionMode::ApiKey, "openai", "gpt-4.1-mini", "", 1_000),
            )
            .unwrap();
        db.mark_ai_job_running(&id, 1_500).unwrap();
        id
    };

    let reopened = Database::open(&path).unwrap();
    assert_eq!(
        reopened.get_ai_job(&id).unwrap().unwrap().status,
        AiJobStatus::Pending
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn v7_migration_pauses_legacy_pending_jobs_without_claiming_them() {
    let path = queue_test_path("legacy-pause");
    let id = {
        let db = Database::open(&path).unwrap();
        db.enqueue_ai_job_for_subject(
            "daily_analysis",
            "2026-07-17",
            "{}",
            1_000,
            &AiExecutionSnapshot::legacy(1_000),
        )
        .unwrap()
    };
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch("PRAGMA user_version = 6;")
            .unwrap();
    }

    let db = Database::open(&path).unwrap();
    assert_eq!(
        db.get_ai_job(&id).unwrap().unwrap().status,
        AiJobStatus::AwaitingReassignment
    );
    assert!(db.claim_next_due_ai_job(10_000).unwrap().is_none());
    assert_eq!(
        db.list_pending_ai_jobs(Some(AiJobStatus::AwaitingReassignment), 500)
            .unwrap()
            .len(),
        1
    );
    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn bulk_reassignment_is_atomic_freezes_current_mode_and_rejects_duplicates() {
    let path = queue_test_path("bulk-reassign");
    let (first, second, active) = {
        let db = Database::open(&path).unwrap();
        let first = db
            .enqueue_ai_job_for_subject(
                "classify_segment",
                "segment-1",
                r#"{"id":"segment-1"}"#,
                1_000,
                &AiExecutionSnapshot::legacy(1_000),
            )
            .unwrap();
        let second = db
            .enqueue_ai_job_for_subject(
                "classify_page",
                "visit-1",
                r#"{"id":"visit-1"}"#,
                1_001,
                &AiExecutionSnapshot::legacy(1_001),
            )
            .unwrap();
        let active = db
            .enqueue_ai_job_for_subject(
                "daily_analysis",
                "2026-07-17",
                "{}",
                1_002,
                &snapshot(
                    AiExecutionMode::ApiKey,
                    "openai",
                    "gpt-current",
                    "active-hash",
                    1_002,
                ),
            )
            .unwrap();
        (first, second, active)
    };
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch("PRAGMA user_version = 6;")
            .unwrap();
    }
    let db = Database::open(&path).unwrap();
    let count_before = db.ai_job_count().unwrap();
    let current = snapshot(
        AiExecutionMode::Codex,
        r#"C:\Tools\codex.exe"#,
        "cli-default",
        "placeholder",
        2_000,
    );

    assert!(
        db.bulk_retry_ai_jobs_with_current_mode(
            &[
                (first.clone(), "validated-first".into()),
                (active, "active-hash".into()),
            ],
            &current,
            2_000,
        )
        .is_err()
    );
    assert_eq!(db.ai_job_count().unwrap(), count_before);
    assert_eq!(
        db.get_ai_job(&first).unwrap().unwrap().status,
        AiJobStatus::AwaitingReassignment
    );

    let created = db
        .bulk_retry_ai_jobs_with_current_mode(
            &[
                (first.clone(), "validated-first".into()),
                (second.clone(), "validated-second".into()),
            ],
            &current,
            3_000,
        )
        .unwrap();
    assert_eq!(created.len(), 2);
    assert!(created.iter().all(|job| job.status == AiJobStatus::Pending));
    assert!(created.iter().all(|job| {
        job.execution.execution_mode == AiExecutionMode::Codex
            && job.execution.executor_id == "codex"
            && job.execution.model == "cli-default"
            && job.execution.created_at_ms == 3_000
    }));
    for record in &created {
        let stored = db.get_ai_job(&record.id).unwrap().unwrap();
        assert_eq!(stored.execution.executor_id, r#"C:\Tools\codex.exe"#);
        assert!(matches!(
            stored.execution.evidence_hash.as_str(),
            "validated-first" | "validated-second"
        ));
    }
    assert_eq!(
        db.get_ai_job(&first).unwrap().unwrap().status,
        AiJobStatus::Complete
    );
    assert_eq!(
        db.get_ai_job(&second).unwrap().unwrap().status,
        AiJobStatus::Complete
    );

    let count_after = db.ai_job_count().unwrap();
    assert!(
        db.bulk_retry_ai_jobs_with_current_mode(
            &[
                (first, "validated-first".into()),
                (second, "validated-second".into()),
            ],
            &current,
            4_000,
        )
        .is_err()
    );
    assert_eq!(db.ai_job_count().unwrap(), count_after);
    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn queue_dto_redacts_codex_paths_and_failure_diagnostics() {
    let db = Database::open_in_memory().unwrap();
    let id = db
        .enqueue_ai_job(
            "daily_analysis",
            "{}",
            1_000,
            &snapshot(
                AiExecutionMode::Codex,
                r#"C:\Users\alice\private\codex.exe"#,
                "cli-default",
                "hash",
                1_000,
            ),
        )
        .unwrap();
    let running = db.claim_next_due_ai_job(1_000).unwrap().unwrap();
    db.fail_ai_job_generation(
        &id,
        running.generation,
        2_000,
        r#"failed C:\Users\alice\private\codex.exe Authorization: Bearer secret"#,
        AiExecutionErrorKind::Codex,
        Some(r#"C:\Users\alice\private\codex.exe"#),
        Some("cli-default"),
        Some(1),
    )
    .unwrap();

    let records = db
        .list_pending_ai_jobs(Some(AiJobStatus::Pending), 10)
        .unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].execution.executor_id, "codex");
    assert_eq!(records[0].executor_id.as_deref(), Some("codex"));
    assert!(!records[0].last_error.contains("alice"));
    assert!(!records[0].last_error.contains("secret"));
}

#[test]
fn custom_provider_requires_https_except_for_loopback() {
    assert!(validate_provider_endpoint("https://api.example.com/v1"));
    assert!(validate_provider_endpoint("http://127.0.0.1:11434/v1"));
    assert!(validate_provider_endpoint("http://localhost:11434/v1"));
    assert!(!validate_provider_endpoint("http://api.example.com/v1"));
    assert!(!validate_provider_endpoint("ftp://example.com/v1"));
}
