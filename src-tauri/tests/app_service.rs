use daily_task_monitor_core::ai::{AiExecutionMode, AiExecutionSnapshot, AiJob, AiJobStatus};
use daily_task_monitor_core::app::{
    AppService, SettingsPatch, TrendAnalysisJobPayload, UiTheme, render_daily_markdown,
    render_trend_markdown, render_trend_markdown_with_workbench, trend_analysis_allowed_candidates,
};
use daily_task_monitor_core::browser::BrowserVisit;
use daily_task_monitor_core::classifier::{ActivityEvidence, RuleClassifier};
use daily_task_monitor_core::db::{ActivitySegmentRecord, Database};
use daily_task_monitor_core::domain::{
    ActivityCategory, ActivityScope, ClassificationSource, VideoPurpose,
};
use daily_task_monitor_core::knowledge_graph::{GraphNodeKind, KnowledgeGraphFilters};
use daily_task_monitor_core::trends::{
    TrendDateRange, TrendGranularity, TrendMetric, TrendWorkbenchRequest,
};
use daily_task_monitor_core::work_ledger::{
    EvidenceProvenance, NewProgressEntry, NewProject, NewTask, ProjectUpdate, TaskPriority,
    TaskUpdate, WorkLedgerRepository, WorkLedgerService,
};

fn segment(id: &str, category: ActivityCategory, start: i64, end: i64) -> ActivitySegmentRecord {
    ActivitySegmentRecord {
        id: id.into(),
        started_at_ms: start,
        ended_at_ms: end,
        app: "Codex".into(),
        app_path: String::new(),
        title: "Desktop rewrite".into(),
        category,
        video_purpose: VideoPurpose::Unknown,
        confidence: 0.82,
        source: ClassificationSource::Rule,
        reason: "test".into(),
        model_version: "rules-v1".into(),
        needs_review: false,
        inactivity_reason: None,
    }
}

#[test]
fn reassignment_validation_rejects_changed_or_missing_classification_evidence() {
    let database = Database::open_in_memory().unwrap();
    let original = segment("reassign-segment", ActivityCategory::Pending, 1_000, 2_000);
    database.insert_segment(&original).unwrap();
    let evidence_hash = database
        .classification_evidence_hash(&original.id)
        .unwrap()
        .unwrap();
    let service = AppService::new(database);
    let job = AiJob {
        id: "legacy-job".into(),
        generation: 0,
        kind: "classify_segment".into(),
        payload_json: r#"{"id":"reassign-segment"}"#.into(),
        status: AiJobStatus::AwaitingReassignment,
        attempts: 1,
        next_attempt_at_ms: 3_000,
        last_error: String::new(),
        execution: AiExecutionSnapshot {
            execution_mode: AiExecutionMode::ApiKey,
            executor_id: "legacy-provider-registry".into(),
            model: String::new(),
            evidence_hash,
            created_at_ms: 1_000,
        },
        started_at_ms: None,
        finished_at_ms: None,
        duration_ms: None,
        executor_id: None,
        model: None,
        exit_code: None,
        error_kind: None,
    };

    assert_eq!(
        service
            .ai_job_reassignment_evidence_hash(&job)
            .unwrap()
            .as_deref(),
        Some(job.execution.evidence_hash.as_str())
    );
    let mut legacy = job.clone();
    legacy.execution.evidence_hash.clear();
    assert_eq!(
        service
            .ai_job_reassignment_evidence_hash(&legacy)
            .unwrap()
            .as_deref(),
        Some(job.execution.evidence_hash.as_str())
    );
    let mut changed = original;
    changed.title = "Changed title".into();
    service.database().upsert_native_segment(&changed).unwrap();
    assert!(
        service
            .ai_job_reassignment_evidence_hash(&job)
            .unwrap()
            .is_none()
    );

    let mut malformed = job;
    malformed.payload_json = "{}".into();
    assert!(
        service
            .ai_job_reassignment_evidence_hash(&malformed)
            .unwrap()
            .is_none()
    );
}

fn trend_segment(
    id: &str,
    app: &str,
    category: ActivityCategory,
    video_purpose: VideoPurpose,
    start: i64,
    end: i64,
) -> ActivitySegmentRecord {
    ActivitySegmentRecord {
        app: app.into(),
        video_purpose,
        ..segment(id, category, start, end)
    }
}

fn one_day_trend_payload(service: &AppService) -> daily_task_monitor_core::domain::TrendPayload {
    const DAY_MS: i64 = 86_400_000;
    service
        .get_trends(
            "2026-07-12",
            "2026-07-12",
            vec![0, DAY_MS],
            "2026-07-11",
            "2026-07-11",
            vec![-DAY_MS, 0],
        )
        .unwrap()
}

fn one_day_trend_boundaries() -> (Vec<i64>, Vec<i64>) {
    const DAY_MS: i64 = 86_400_000;
    (vec![0, DAY_MS], vec![-DAY_MS, 0])
}

fn execution_snapshot(provider_id: &str, model: &str, created_at_ms: i64) -> AiExecutionSnapshot {
    AiExecutionSnapshot {
        execution_mode: AiExecutionMode::ApiKey,
        executor_id: provider_id.into(),
        model: model.into(),
        evidence_hash: String::new(),
        created_at_ms,
    }
}

#[test]
fn daily_and_trend_reports_share_rollups_while_renames_leave_trend_hash_stable() {
    const DAY_MS: i64 = 86_400_000;
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&trend_segment(
            "ledger-segment",
            "Codex",
            ActivityCategory::CreationDevelopment,
            VideoPurpose::Unknown,
            1_000,
            5_000,
        ))
        .unwrap();
    database
        .insert_browser_visit(
            "ledger-visit",
            "Chrome",
            "Default",
            &BrowserVisit {
                url: "https://example.com/ledger".into(),
                title: "Ledger evidence".into(),
                visited_at_ms: 2_000,
            },
            "example.com",
        )
        .unwrap();
    {
        let ledger = WorkLedgerService::new(WorkLedgerRepository::new(&database));
        let project = ledger
            .create_project(NewProject {
                id: "project-report".into(),
                name: "Original project name".into(),
                color: "#3182ce".into(),
                description: String::new(),
                created_at_ms: 100,
            })
            .unwrap();
        let task = ledger
            .create_task(NewTask {
                id: "task-report".into(),
                project_id: project.id.clone(),
                title: "Original task title".into(),
                priority: TaskPriority::High,
                expected_output: "Report".into(),
                due_date: None,
                created_at_ms: 100,
            })
            .unwrap();
        ledger
            .assign_activity(
                &task.id,
                "ledger-segment",
                EvidenceProvenance::Manual,
                1.0,
                "confirmed",
                500,
            )
            .unwrap();
        ledger
            .assign_browser_visit(
                &task.id,
                "ledger-visit",
                EvidenceProvenance::Manual,
                1.0,
                "confirmed",
                2_000,
            )
            .unwrap();
        ledger
            .add_progress_entry(NewProgressEntry {
                id: "report-progress".into(),
                task_id: task.id.clone(),
                note: "Progress evidence".into(),
                created_at_ms: 2_500,
            })
            .unwrap();
        database
            .start_focus_session_for_task(
                "report-focus",
                "2026-07-12",
                "Original task title",
                25,
                500,
                Some(&task.id),
            )
            .unwrap();
        database
            .complete_focus_session("report-focus", 3_500, "Focus outcome")
            .unwrap();
        ledger.archive_project(&project.id, 10_000).unwrap();
    }

    let service = AppService::new(database);
    let dashboard = service.get_dashboard(0, DAY_MS).unwrap();
    let daily_project = &dashboard.work_ledger.projects[0];
    assert_eq!(daily_project.project_name, "Original project name");
    assert_eq!(daily_project.invested_seconds, 4);
    assert_eq!(daily_project.focus_seconds, 3);
    assert_eq!(daily_project.activity_segment_count, 1);
    assert_eq!(daily_project.browser_visit_count, 1);
    assert_eq!(daily_project.progress_count, 2);
    assert_eq!(daily_project.focus_session_count, 1);
    let daily_markdown = render_daily_markdown("2026-07-12", &dashboard);
    for expected in [
        "Original project name",
        "Original task title",
        "4 秒",
        "Progress",
        "Focus",
    ] {
        assert!(daily_markdown.contains(expected), "missing {expected}");
    }

    let before = one_day_trend_payload(&service);
    assert_eq!(before.work_ledger.projects[0], *daily_project);
    {
        let ledger = WorkLedgerService::new(WorkLedgerRepository::new(service.database()));
        ledger
            .update_project(
                "project-report",
                ProjectUpdate {
                    name: Some("Renamed project".into()),
                    color: None,
                    description: None,
                },
                20_000,
            )
            .unwrap();
        ledger
            .update_task(
                "task-report",
                TaskUpdate {
                    title: Some("Renamed task".into()),
                    ..TaskUpdate::default()
                },
                20_000,
            )
            .unwrap();
    }
    let after = one_day_trend_payload(&service);
    assert_eq!(after.evidence_hash, before.evidence_hash);
    assert_eq!(
        after.work_ledger.projects[0].project_name,
        "Renamed project"
    );
    assert_eq!(after.work_ledger.tasks[0].task_title, "Renamed task");
    let analysis = service.get_trend_analysis(&after, 30_000).unwrap();
    let trend_markdown = render_trend_markdown(&after, &analysis, 30_000);
    for expected in ["Renamed project", "Renamed task", "Progress", "Focus"] {
        assert!(trend_markdown.contains(expected), "missing {expected}");
    }
}

#[test]
fn trend_range_aggregates_clipped_local_days_and_previous_period() {
    const DAY_MS: i64 = 86_400_000;
    let selected_start = 1_752_076_800_000; // 2025-07-10T00:00:00+08:00
    let selected_end = selected_start + 2 * DAY_MS;
    let database = Database::open_in_memory().unwrap();

    database
        .insert_segment(&trend_segment(
            "previous-active",
            "Terminal",
            ActivityCategory::Social,
            VideoPurpose::Unknown,
            selected_start - DAY_MS - 1_800_000,
            selected_start - DAY_MS,
        ))
        .unwrap();
    database
        .insert_segment(&trend_segment(
            "previous-idle",
            "System",
            ActivityCategory::Idle,
            VideoPurpose::Unknown,
            selected_start - DAY_MS,
            selected_start - DAY_MS + 600_000,
        ))
        .unwrap();
    database
        .insert_segment(&trend_segment(
            "cross-midnight-learning",
            "Codex",
            ActivityCategory::VideoInput,
            VideoPurpose::Learning,
            selected_start + DAY_MS - 1_800_000,
            selected_start + DAY_MS + 1_800_000,
        ))
        .unwrap();
    database
        .insert_segment(&trend_segment(
            "idle",
            "System",
            ActivityCategory::Idle,
            VideoPurpose::Unknown,
            selected_start + 3_600_000,
            selected_start + 5_400_000,
        ))
        .unwrap();
    database
        .insert_segment(&trend_segment(
            "pending-second-app",
            "Browser",
            ActivityCategory::Pending,
            VideoPurpose::Unknown,
            selected_start + DAY_MS + 7_200_000,
            selected_start + DAY_MS + 9_000_000,
        ))
        .unwrap();

    let service = AppService::new(database);
    let payload = service
        .get_trends(
            "2025-07-10",
            "2025-07-11",
            vec![selected_start, selected_start + DAY_MS, selected_end],
            "2025-07-08",
            "2025-07-09",
            vec![
                selected_start - 2 * DAY_MS,
                selected_start - DAY_MS,
                selected_start,
            ],
        )
        .unwrap();

    assert_eq!(payload.range.start_date, "2025-07-10");
    assert_eq!(payload.range.end_date, "2025-07-11");
    assert_eq!(payload.days.len(), 2);
    assert_eq!(payload.days[0].active_seconds, 1_800);
    assert_eq!(payload.days[0].idle_seconds, 1_800);
    assert_eq!(payload.days[1].active_seconds, 3_600);
    assert_eq!(payload.summary.monitored_seconds, 7_200);
    assert_eq!(payload.summary.active_seconds, 5_400);
    assert_eq!(payload.summary.learning_seconds, 3_600);
    assert_eq!(payload.summary.average_monitored_seconds, 3_600.0);
    assert_eq!(payload.summary.average_idle_seconds, 900.0);
    assert!((payload.summary.learning_ratio - (2.0 / 3.0)).abs() < f64::EPSILON);
    assert_eq!(payload.summary.switches_per_active_hour, 4.0 / 3.0);
    assert_eq!(payload.summary.category_breakdown[0].name, "video_input");
    assert_eq!(payload.summary.category_breakdown[0].seconds, 3_600);
    assert_eq!(payload.summary.app_breakdown[0].name, "Codex");
    assert_eq!(payload.summary.app_breakdown[0].seconds, 3_600);
    assert!((payload.quality.classification_coverage - 0.75).abs() < f64::EPSILON);
    assert_eq!(payload.comparison.day_count, 2);
    assert_eq!(payload.comparison.previous_active_seconds, 1_800);
    assert_eq!(payload.comparison.active_seconds_delta_percent, Some(200.0));
    assert_eq!(payload.comparison.learning_seconds_delta_percent, None);
    assert_eq!(payload.comparison.learning_ratio_delta_percent, None);
    assert_eq!(
        payload
            .days
            .iter()
            .map(|day| day.active_seconds)
            .sum::<i64>(),
        payload.summary.active_seconds
    );
    assert_eq!(payload.evidence_hash.len(), 64);
    assert_eq!(
        payload.evidence_hash,
        service
            .get_trends(
                "2025-07-10",
                "2025-07-11",
                vec![selected_start, selected_start + DAY_MS, selected_end],
                "2025-07-08",
                "2025-07-09",
                vec![
                    selected_start - 2 * DAY_MS,
                    selected_start - DAY_MS,
                    selected_start,
                ],
            )
            .unwrap()
            .evidence_hash
    );
}

#[test]
fn trend_range_honors_explicit_23_and_25_hour_day_boundaries() {
    const HOUR_MS: i64 = 3_600_000;
    let anchor = 1_800_000_000_000;
    let current = vec![
        anchor,
        anchor + 24 * HOUR_MS,
        anchor + 47 * HOUR_MS,
        anchor + 71 * HOUR_MS,
    ];
    let comparison = vec![
        anchor - 73 * HOUR_MS,
        anchor - 49 * HOUR_MS,
        anchor - 24 * HOUR_MS,
        anchor,
    ];
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&trend_segment(
            "spring-short-day",
            "Codex",
            ActivityCategory::CreationDevelopment,
            VideoPurpose::Unknown,
            current[1],
            current[2],
        ))
        .unwrap();
    database
        .insert_segment(&trend_segment(
            "fall-long-day",
            "Codex",
            ActivityCategory::CreationDevelopment,
            VideoPurpose::Unknown,
            comparison[1],
            comparison[2],
        ))
        .unwrap();

    let payload = AppService::new(database)
        .get_trends(
            "2025-03-08",
            "2025-03-10",
            current,
            "2025-03-05",
            "2025-03-07",
            comparison,
        )
        .unwrap();

    assert_eq!(payload.days[1].active_seconds, 23 * 3_600);
    assert_eq!(payload.comparison.previous_active_seconds, 25 * 3_600);
    assert_eq!(payload.comparison.day_count, 3);
}

#[test]
fn trend_range_accepts_366_calendar_days() {
    const DAY_MS: i64 = 86_400_000;
    let anchor = 1_704_067_200_000;
    let current = (0..=366).map(|day| anchor + day * DAY_MS).collect();
    let comparison = (0..=366).map(|day| anchor - (366 - day) * DAY_MS).collect();

    let payload = AppService::new(Database::open_in_memory().unwrap())
        .get_trends(
            "2024-01-01",
            "2024-12-31",
            current,
            "2022-12-31",
            "2023-12-31",
            comparison,
        )
        .unwrap();

    assert_eq!(payload.days.len(), 366);
    assert_eq!(payload.range.day_count, 366);
}

#[test]
fn trend_range_rejects_invalid_or_mismatched_boundaries() {
    let service = AppService::new(Database::open_in_memory().unwrap());

    assert!(
        service
            .get_trends(
                "2025-01-01",
                "2025-01-01",
                vec![0, 0],
                "2024-12-31",
                "2024-12-31",
                vec![-1, 0]
            )
            .is_err()
    );
    assert!(
        service
            .get_trends(
                "2025-01-02",
                "2025-01-02",
                vec![10, 20],
                "2024-12-01",
                "2024-12-01",
                vec![0, 10],
            )
            .is_err()
    );
    assert!(
        service
            .get_trends(
                "2025-01-01",
                "2025-01-02",
                vec![0, 1, 2],
                "2024-12-31",
                "2024-12-31",
                vec![-1, 0]
            )
            .is_err()
    );
    assert!(
        service
            .get_trends(
                "2025-01-01",
                "2025-01-01",
                vec![0, 1],
                "2024-12-31",
                "2024-12-31",
                vec![-2, -1]
            )
            .is_err()
    );
    assert!(
        service
            .get_trends(
                "2025-01-01",
                "2025-01-02",
                vec![0, 2, 1],
                "2024-12-30",
                "2024-12-31",
                vec![-2, -1, 0]
            )
            .is_err()
    );
    assert!(
        service
            .get_trends(
                "2025-01-01",
                "2026-01-02",
                (0..=367).collect(),
                "2024-01-01",
                "2024-12-31",
                (-367..=0).collect()
            )
            .is_err()
    );
}

#[test]
fn trend_range_uses_one_integer_second_value_for_every_rollup() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&trend_segment(
            "one-second-after-floor",
            "Codex",
            ActivityCategory::CreationDevelopment,
            VideoPurpose::Unknown,
            10_000,
            11_500,
        ))
        .unwrap();
    database
        .insert_segment(&trend_segment(
            "subsecond",
            "Browser",
            ActivityCategory::Social,
            VideoPurpose::Unknown,
            12_000,
            12_900,
        ))
        .unwrap();

    let payload = AppService::new(database)
        .get_trends(
            "2025-01-02",
            "2025-01-02",
            vec![10_000, 20_000],
            "2025-01-01",
            "2025-01-01",
            vec![0, 10_000],
        )
        .unwrap();

    assert_eq!(payload.summary.monitored_seconds, 1);
    assert_eq!(payload.summary.active_seconds, 1);
    assert_eq!(payload.days[0].monitored_seconds, 1);
    assert_eq!(
        payload
            .summary
            .category_breakdown
            .iter()
            .map(|item| item.seconds)
            .sum::<i64>(),
        1
    );
    assert_eq!(
        payload
            .summary
            .app_breakdown
            .iter()
            .map(|item| item.seconds)
            .sum::<i64>(),
        1
    );
}

#[test]
fn dashboard_snapshot_contains_totals_and_timeline() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment(
            "dev",
            ActivityCategory::CreationDevelopment,
            1_000,
            61_000,
        ))
        .unwrap();
    database
        .insert_segment(&segment("idle", ActivityCategory::Idle, 61_000, 91_000))
        .unwrap();

    let service = AppService::new(database);
    let snapshot = service.get_dashboard(0, 120_000).unwrap();

    assert_eq!(snapshot.totals.monitored_seconds, 90);
    assert_eq!(snapshot.totals.learning_seconds, 60);
    assert_eq!(snapshot.timeline.len(), 2);
    assert_eq!(snapshot.activity_composition.all.total_seconds, 90);
    assert_eq!(snapshot.activity_composition.meaningful.total_seconds, 60);
}

#[test]
fn settings_patch_preserves_unspecified_values() {
    let service = AppService::new(Database::open_in_memory().unwrap());
    service
        .update_settings(SettingsPatch {
            idle_threshold_minutes: Some(9),
            monitoring_enabled: None,
            ai_backfill_enabled: None,
            ai_execution_mode: None,
            selected_api_provider_id: None,
            codex_executable: None,
            codex_model: None,
            ai_auto_trend_analysis_enabled: None,
            ai_auto_classification_enabled: None,
            ai_auto_workflow_assignment_enabled: None,
            ai_automation_notice_version: None,
            excluded_apps: None,
            excluded_domains: None,
            ui_theme: None,
            ui_font: None,
            experimental_knowledge_graph_enabled: None,
        })
        .unwrap();

    let settings = service.get_settings().unwrap();
    assert_eq!(settings.idle_threshold_minutes, 9);
    assert!(settings.monitoring_enabled);
    assert!(!settings.ai_backfill_enabled);
}

#[test]
fn selected_api_provider_setting_uses_camel_case_and_patch_preserves_it() {
    let service = AppService::new(Database::open_in_memory().unwrap());

    service
        .update_settings(SettingsPatch {
            selected_api_provider_id: Some(Some("openai".into())),
            ..SettingsPatch::default()
        })
        .unwrap();
    service
        .update_settings(SettingsPatch {
            idle_threshold_minutes: Some(12),
            ..SettingsPatch::default()
        })
        .unwrap();

    assert_eq!(
        service
            .get_settings()
            .unwrap()
            .selected_api_provider_id
            .as_deref(),
        Some("openai")
    );
    let stored = service.database().get_setting_json("app").unwrap().unwrap();
    assert!(stored.contains(r#""selectedApiProviderId":"openai""#));
    assert!(!stored.contains("selected_api_provider_id"));

    let clear_patch: SettingsPatch =
        serde_json::from_str(r#"{"selectedApiProviderId":null}"#).unwrap();
    assert_eq!(clear_patch.selected_api_provider_id, Some(None));
    service.update_settings(clear_patch).unwrap();
    assert_eq!(
        service.get_settings().unwrap().selected_api_provider_id,
        None
    );
}

#[test]
fn legacy_ai_backend_and_codex_path_preserve_codex_preference() {
    let database = Database::open_in_memory().unwrap();
    database
        .set_setting_json(
            "app",
            r#"{"idleThresholdMinutes":12,"aiBackend":"codex_cli","codexExecutablePath":"C:\\Legacy\\codex.exe"}"#,
        )
        .unwrap();
    let service = AppService::new(database);

    let settings = service.get_settings().unwrap();

    assert_eq!(settings.ai_execution_mode, AiExecutionMode::Codex);
    assert_eq!(settings.codex_executable, r#"C:\Legacy\codex.exe"#);
    assert_eq!(settings.selected_api_provider_id, None);
}

#[test]
fn settings_missing_new_ai_fields_defaults_automation_on_and_codex_path() {
    let database = Database::open_in_memory().unwrap();
    database
        .set_setting_json(
            "app",
            r#"{"idleThresholdMinutes":12,"monitoringEnabled":true,"aiBackfillEnabled":false}"#,
        )
        .unwrap();
    let service = AppService::new(database);

    let settings = service.get_settings().unwrap();

    assert_eq!(settings.idle_threshold_minutes, 12);
    assert!(!settings.ai_backfill_enabled);
    assert!(settings.ai_auto_trend_analysis_enabled);
    assert!(settings.ai_auto_classification_enabled);
    assert!(settings.ai_auto_workflow_assignment_enabled);
    assert_eq!(settings.ai_automation_notice_version, 0);
    assert_eq!(settings.codex_executable, "codex");
    assert_eq!(settings.codex_model, "");
}

#[test]
fn settings_preserves_user_disabled_ai_automation_across_saved_settings() {
    let service = AppService::new(Database::open_in_memory().unwrap());

    service
        .update_settings(SettingsPatch {
            ai_auto_trend_analysis_enabled: Some(false),
            ai_auto_classification_enabled: Some(false),
            ai_auto_workflow_assignment_enabled: Some(false),
            ..SettingsPatch::default()
        })
        .unwrap();
    let saved = service.database().get_setting_json("app").unwrap().unwrap();
    service.database().set_setting_json("app", &saved).unwrap();

    let settings = service.get_settings().unwrap();

    assert!(!settings.ai_auto_trend_analysis_enabled);
    assert!(!settings.ai_auto_classification_enabled);
    assert!(!settings.ai_auto_workflow_assignment_enabled);
}

#[test]
fn settings_patch_persists_codex_and_notice_fields_without_using_backfill_as_master_gate() {
    let service = AppService::new(Database::open_in_memory().unwrap());

    service
        .update_settings(SettingsPatch {
            ai_backfill_enabled: Some(false),
            codex_executable: Some("  C:\\Tools\\codex.exe --model ignored  ".into()),
            codex_model: Some("  gpt-5-codex  ".into()),
            ai_auto_trend_analysis_enabled: Some(true),
            ai_auto_classification_enabled: Some(false),
            ai_auto_workflow_assignment_enabled: Some(true),
            ai_automation_notice_version: Some(1),
            ..SettingsPatch::default()
        })
        .unwrap();

    let settings = service.get_settings().unwrap();

    assert!(!settings.ai_backfill_enabled);
    assert_eq!(settings.codex_executable, "C:\\Tools\\codex.exe");
    assert_eq!(settings.codex_model, "gpt-5-codex");
    assert!(settings.ai_auto_trend_analysis_enabled);
    assert!(!settings.ai_auto_classification_enabled);
    assert!(settings.ai_auto_workflow_assignment_enabled);
    assert_eq!(settings.ai_automation_notice_version, 1);
}

#[test]
fn ui_theme_defaults_to_classic_workbench() {
    let service = AppService::new(Database::open_in_memory().unwrap());

    assert_eq!(
        service.get_settings().unwrap().ui_theme,
        UiTheme::ClassicWorkbench
    );
}

#[test]
fn allowed_ui_theme_patches_are_persisted() {
    let service = AppService::new(Database::open_in_memory().unwrap());
    for (theme, serialized) in [
        (UiTheme::MossNocturne, "moss-nocturne"),
        (UiTheme::ClassicWorkbench, "classic-workbench"),
        (UiTheme::MoonGlass, "moon-glass"),
        (UiTheme::SoftPaper, "soft-paper"),
        (UiTheme::BlueprintData, "blueprint-data"),
        (UiTheme::KnowledgeSpace, "knowledge-space"),
    ] {
        service
            .update_settings(SettingsPatch {
                ui_theme: Some(theme),
                ..SettingsPatch::default()
            })
            .unwrap();

        assert_eq!(service.get_settings().unwrap().ui_theme, theme);
        assert!(
            service
                .database()
                .get_setting_json("app")
                .unwrap()
                .unwrap()
                .contains(&format!(r#""uiTheme":"{serialized}""#))
        );
    }
}

#[test]
fn knowledge_space_experiment_is_opt_in_and_persisted() {
    let service = AppService::new(Database::open_in_memory().unwrap());
    assert!(
        !service
            .get_settings()
            .unwrap()
            .experimental_knowledge_graph_enabled
    );

    service
        .update_settings(SettingsPatch {
            experimental_knowledge_graph_enabled: Some(true),
            ui_theme: Some(UiTheme::KnowledgeSpace),
            ..SettingsPatch::default()
        })
        .unwrap();

    let settings = service.get_settings().unwrap();
    assert!(settings.experimental_knowledge_graph_enabled);
    assert_eq!(settings.ui_theme, UiTheme::KnowledgeSpace);
}

#[test]
fn knowledge_graph_contains_raw_activity_and_browser_visits_and_respects_exclusions() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&ActivitySegmentRecord {
            app: "Chrome".into(),
            app_path: String::new(),
            title: "Knowledge graph research".into(),
            ..segment("research", ActivityCategory::Research, 1_000, 61_000)
        })
        .unwrap();
    database
        .insert_segment(&ActivitySegmentRecord {
            app: "PasswordManager".into(),
            app_path: String::new(),
            title: "Private vault".into(),
            ..segment("private", ActivityCategory::Pending, 62_000, 82_000)
        })
        .unwrap();
    database
        .insert_browser_visit(
            "visit-public",
            "Chrome",
            "Default",
            &BrowserVisit {
                visited_at_ms: 30_000,
                url: "https://example.com/article".into(),
                title: "Graph article".into(),
            },
            "example.com",
        )
        .unwrap();
    database
        .insert_browser_visit(
            "visit-private",
            "Chrome",
            "Default",
            &BrowserVisit {
                visited_at_ms: 40_000,
                url: "https://private.example/secret".into(),
                title: "Secret".into(),
            },
            "private.example",
        )
        .unwrap();

    let service = AppService::new(database);
    service
        .update_settings(SettingsPatch {
            excluded_apps: Some(vec!["PasswordManager".into()]),
            excluded_domains: Some(vec!["private.example".into()]),
            ..SettingsPatch::default()
        })
        .unwrap();

    let graph = service
        .get_knowledge_graph(0, 100_000, KnowledgeGraphFilters::default())
        .unwrap();

    assert!(graph.nodes.iter().any(|node| {
        node.kind == GraphNodeKind::Activity && node.label == "Knowledge graph research"
    }));
    assert!(
        graph.nodes.iter().any(|node| {
            node.kind == GraphNodeKind::BrowserVisit && node.label == "Graph article"
        })
    );
    assert!(
        graph
            .nodes
            .iter()
            .any(|node| { node.kind == GraphNodeKind::Domain && node.label == "example.com" })
    );
    assert!(
        !graph
            .nodes
            .iter()
            .any(|node| { node.label.contains("Private") || node.label.contains("Secret") })
    );
    assert_eq!(graph.counts.get("activity").copied().unwrap_or_default(), 1);
    assert_eq!(
        graph
            .counts
            .get("browser-visit")
            .copied()
            .unwrap_or_default(),
        1
    );
}

#[test]
fn legacy_ui_themes_migrate_to_current_values() {
    for (stored, expected) in [
        ("precision-paper", UiTheme::ClassicWorkbench),
        ("moon-glass", UiTheme::MoonGlass),
        ("signal-console", UiTheme::BlueprintData),
        ("studio-blocks", UiTheme::SoftPaper),
    ] {
        let database = Database::open_in_memory().unwrap();
        database
            .set_setting_json("app", &format!(r#"{{"uiTheme":"{stored}"}}"#))
            .unwrap();
        let service = AppService::new(database);

        assert_eq!(service.get_settings().unwrap().ui_theme, expected);
    }
}

#[test]
fn missing_or_invalid_ui_theme_normalizes_without_resetting_other_settings() {
    for stored in [
        r#"{"idleThresholdMinutes":12}"#,
        r#"{"idleThresholdMinutes":12,"uiTheme":"legacy-theme"}"#,
        r#"{"idleThresholdMinutes":12,"uiTheme":null}"#,
    ] {
        let database = Database::open_in_memory().unwrap();
        database.set_setting_json("app", stored).unwrap();
        let service = AppService::new(database);

        let settings = service.get_settings().unwrap();
        assert_eq!(settings.idle_threshold_minutes, 12);
        assert_eq!(settings.ui_theme, UiTheme::ClassicWorkbench);
    }
}

#[test]
fn privacy_exclusions_are_normalized_and_persisted() {
    let service = AppService::new(Database::open_in_memory().unwrap());
    service
        .update_settings(SettingsPatch {
            excluded_apps: Some(vec!["  SecretApp  ".into(), "".into()]),
            excluded_domains: Some(vec!["Private.Example.com".into()]),
            ..SettingsPatch::default()
        })
        .unwrap();

    let settings = service.get_settings().unwrap();
    assert_eq!(settings.excluded_apps, vec!["secretapp"]);
    assert_eq!(settings.excluded_domains, vec!["private.example.com"]);
}

#[test]
fn manual_learning_video_rule_preserves_its_video_purpose() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment(
            "lecture",
            ActivityCategory::Pending,
            1_000,
            61_000,
        ))
        .unwrap();
    let service = AppService::new(database);
    service
        .save_manual_classification(
            "lecture",
            ActivityCategory::VideoInput,
            VideoPurpose::Learning,
            "Course video",
        )
        .unwrap();

    let classifier = RuleClassifier::new(service.database().list_manual_rules().unwrap());
    let result = classifier.classify(&ActivityEvidence {
        app: "Codex".into(),
        title: "Desktop rewrite".into(),
        domain: String::new(),
        duration_seconds: 120,
        key_presses: 0,
        window_switches: 0,
        media_playing: true,
    });
    assert_eq!(result.category, ActivityCategory::VideoInput);
    assert_eq!(result.video_purpose, VideoPurpose::Learning);
}

#[test]
fn manual_classification_overrides_the_segment_and_marks_manual_source() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment("visit", ActivityCategory::Pending, 1_000, 61_000))
        .unwrap();
    let service = AppService::new(database);

    service
        .save_manual_classification(
            "visit",
            ActivityCategory::Research,
            VideoPurpose::Unknown,
            "User correction",
        )
        .unwrap();

    let snapshot = service.get_dashboard(0, 120_000).unwrap();
    assert_eq!(snapshot.timeline[0].category, ActivityCategory::Research);
    assert_eq!(snapshot.timeline[0].source, ClassificationSource::Manual);
    assert_eq!(snapshot.timeline[0].confidence, 1.0);

    let classifier = RuleClassifier::new(service.database().list_manual_rules().unwrap());
    let reapplied = classifier.classify(&ActivityEvidence {
        app: "Codex".into(),
        title: "Desktop rewrite".into(),
        domain: String::new(),
        duration_seconds: 10,
        key_presses: 0,
        window_switches: 0,
        media_playing: false,
    });
    assert_eq!(reapplied.category, ActivityCategory::Research);
    assert_eq!(reapplied.source, ClassificationSource::Manual);
}

#[test]
fn daily_analysis_is_available_offline_from_structured_evidence() {
    let database = Database::open_in_memory().unwrap();
    database
        .save_daily_goal(
            "2026-07-12",
            "完成桌面版 AI 分析",
            "形成两个分析区块",
            "完成本地算法",
        )
        .unwrap();
    database
        .insert_segment(&segment(
            "dev",
            ActivityCategory::CreationDevelopment,
            1_000,
            3_601_000,
        ))
        .unwrap();

    let service = AppService::new(database);
    let analysis = service
        .get_daily_analysis("2026-07-12", 0, 7_200_000)
        .unwrap();

    assert_eq!(analysis.source, "local");
    assert!(!analysis.evidence_hash.is_empty());
    assert!(analysis.portrait.contains("学习"));
    assert!(analysis.recommendation.contains("切换"));
    assert!(!format!("{}{}", analysis.portrait, analysis.recommendation).contains("心情"));
}

#[test]
fn daily_analysis_queue_deduplicates_the_same_evidence_hash() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment(
            "dev",
            ActivityCategory::CreationDevelopment,
            1_000,
            61_000,
        ))
        .unwrap();
    let service = AppService::new(database);
    let execution = execution_snapshot("openai", "gpt-frozen", 5_000);

    let first = service
        .queue_daily_analysis("2026-07-12", 0, 120_000, 5_000, Some(&execution))
        .unwrap();
    let second = service
        .queue_daily_analysis("2026-07-12", 0, 120_000, 6_000, Some(&execution))
        .unwrap();

    assert_eq!(first, second);
    assert_eq!(service.database().ai_job_count().unwrap(), 1);
}

#[test]
fn daily_analysis_scope_separates_evidence_and_pending_jobs() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment(
            "dev",
            ActivityCategory::CreationDevelopment,
            1_000,
            61_000,
        ))
        .unwrap();
    database
        .insert_segment(&segment("game", ActivityCategory::Game, 61_000, 121_000))
        .unwrap();
    let service = AppService::new(database);
    let execution = execution_snapshot("openai", "gpt-frozen", 5_000);

    let all = service
        .queue_daily_analysis_scoped(
            "2026-07-12",
            0,
            180_000,
            ActivityScope::All,
            5_000,
            Some(&execution),
        )
        .unwrap()
        .unwrap();
    let meaningful = service
        .queue_daily_analysis_scoped(
            "2026-07-12",
            0,
            180_000,
            ActivityScope::Meaningful,
            5_000,
            Some(&execution),
        )
        .unwrap()
        .unwrap();

    assert_ne!(
        all, meaningful,
        "a scope must not reuse the other scope's job"
    );
    let all_job = service.database().get_ai_job(&all).unwrap().unwrap();
    let meaningful_job = service.database().get_ai_job(&meaningful).unwrap().unwrap();
    let all_payload: serde_json::Value = serde_json::from_str(&all_job.payload_json).unwrap();
    let meaningful_payload: serde_json::Value =
        serde_json::from_str(&meaningful_job.payload_json).unwrap();
    assert_eq!(all_payload["activityScope"], "all");
    assert_eq!(meaningful_payload["activityScope"], "meaningful");
    assert_ne!(
        all_payload["evidenceHash"], meaningful_payload["evidenceHash"],
        "excluded activity must change the evidence hash"
    );
}

#[test]
fn daily_learning_scope_excludes_file_and_social_activity_even_when_task_linked() {
    let database = Database::open_in_memory().unwrap();
    database
        .create_work_ledger_project(NewProject {
            id: "scope-project".into(),
            name: "Scope project".into(),
            color: "#2563eb".into(),
            description: String::new(),
            created_at_ms: 1,
        })
        .unwrap();
    database
        .create_work_ledger_task(NewTask {
            id: "scope-task".into(),
            project_id: "scope-project".into(),
            title: "Scope task".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1,
        })
        .unwrap();
    for (id, category, start_ms) in [
        ("linked-file", ActivityCategory::FileManagement, 0),
        ("unlinked-file", ActivityCategory::FileManagement, 60_000),
        ("linked-social", ActivityCategory::Social, 120_000),
        ("unlinked-social", ActivityCategory::Social, 180_000),
    ] {
        database
            .insert_segment(&segment(id, category, start_ms, start_ms + 60_000))
            .unwrap();
    }
    for id in ["linked-file", "linked-social"] {
        database
            .assign_work_ledger_activity(
                "scope-task",
                id,
                EvidenceProvenance::Manual,
                1.0,
                "confirmed",
                1,
            )
            .unwrap();
    }
    let service = AppService::new(database);
    let execution = execution_snapshot("openai", "gpt-frozen", 5_000);

    let id = service
        .queue_daily_analysis_scoped(
            "2026-08-07",
            0,
            300_000,
            ActivityScope::Meaningful,
            5_000,
            Some(&execution),
        )
        .unwrap()
        .unwrap();
    let job = service.database().get_ai_job(&id).unwrap().unwrap();
    let payload: serde_json::Value = serde_json::from_str(&job.payload_json).unwrap();

    assert_eq!(payload["monitoredSeconds"], 0);
    assert_eq!(payload["activeSeconds"], 0);
    assert!(payload["categorySeconds"].as_object().unwrap().is_empty());
}

#[test]
fn api_daily_analysis_requires_a_selected_provider_snapshot_and_freezes_it() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment(
            "dev-provider-snapshot",
            ActivityCategory::CreationDevelopment,
            1_000,
            61_000,
        ))
        .unwrap();
    let service = AppService::new(database);

    let missing = service
        .queue_daily_analysis("2026-07-12", 0, 120_000, 5_000, None)
        .unwrap();
    assert_eq!(missing, None);
    assert_eq!(service.database().ai_job_count().unwrap(), 0);

    let frozen = execution_snapshot("openai", "queued-model", 6_000);
    let job_id = service
        .queue_daily_analysis("2026-07-12", 0, 120_000, 6_000, Some(&frozen))
        .unwrap()
        .expect("selected provider queues work");

    let queued = service.database().get_ai_job(&job_id).unwrap().unwrap();
    assert_eq!(queued.execution.executor_id, frozen.executor_id);
    assert_eq!(queued.execution.model, frozen.model);
    assert_eq!(queued.execution.created_at_ms, frozen.created_at_ms);
    assert!(!queued.execution.evidence_hash.is_empty());
}

#[test]
fn stale_daily_analysis_cannot_replace_a_newer_evidence_result() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment(
            "initial",
            ActivityCategory::Research,
            1_000,
            61_000,
        ))
        .unwrap();
    let service = AppService::new(database);
    let old_hash = service
        .get_daily_analysis("2026-07-12", 0, 180_000)
        .unwrap()
        .evidence_hash;

    service
        .database()
        .insert_segment(&segment(
            "new-work",
            ActivityCategory::CreationDevelopment,
            61_000,
            121_000,
        ))
        .unwrap();
    let new_hash = service
        .get_daily_analysis("2026-07-12", 0, 180_000)
        .unwrap()
        .evidence_hash;
    assert_ne!(old_hash, new_hash);

    assert!(
        service
            .save_ai_daily_analysis_if_current(
                "2026-07-12",
                0,
                180_000,
                &new_hash,
                "new portrait",
                "new recommendation",
                &[],
                10_000,
            )
            .unwrap()
    );
    assert!(
        !service
            .save_ai_daily_analysis_if_current(
                "2026-07-12",
                0,
                180_000,
                &old_hash,
                "stale portrait",
                "stale recommendation",
                &[],
                11_000,
            )
            .unwrap()
    );

    let current = service
        .get_daily_analysis("2026-07-12", 0, 180_000)
        .unwrap();
    assert_eq!(current.source, "ai");
    assert_eq!(current.portrait, "new portrait");
}

#[test]
fn stale_daily_analysis_is_not_returned_after_live_evidence_changes() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment(
            "initial",
            ActivityCategory::Research,
            1_000,
            61_000,
        ))
        .unwrap();
    let service = AppService::new(database);
    let queued_hash = service
        .get_daily_analysis("2026-07-12", 0, 180_000)
        .unwrap()
        .evidence_hash;

    service
        .database()
        .insert_segment(&segment(
            "live-update",
            ActivityCategory::CreationDevelopment,
            61_000,
            121_000,
        ))
        .unwrap();

    assert!(
        service
            .save_ai_daily_analysis_if_current(
                "2026-07-12",
                0,
                180_000,
                &queued_hash,
                "AI portrait",
                "AI recommendation",
                &[],
                20_000,
            )
            .unwrap()
    );

    let analysis = service
        .get_daily_analysis("2026-07-12", 0, 180_000)
        .unwrap();
    assert_eq!(analysis.source, "local");
    assert_ne!(analysis.evidence_hash, queued_hash);
    assert_ne!(analysis.portrait, "AI portrait");
}

#[test]
fn trend_analysis_is_available_offline_from_aggregate_evidence() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&trend_segment(
            "trend-local",
            "Codex",
            ActivityCategory::CreationDevelopment,
            VideoPurpose::Unknown,
            1_000,
            3_601_000,
        ))
        .unwrap();
    let service = AppService::new(database);
    let evidence = one_day_trend_payload(&service);

    let analysis = service.get_trend_analysis(&evidence, 12_345).unwrap();

    assert_eq!(analysis.source, "local");
    assert_eq!(analysis.evidence_hash, evidence.evidence_hash);
    assert_eq!(analysis.range_start, "2026-07-12");
    assert_eq!(analysis.range_end, "2026-07-12");
    assert!(!analysis.summary.is_empty());
    assert!(!analysis.observations.is_empty());
    assert!(!analysis.suggestions.is_empty());
    assert_eq!(analysis.model, "deterministic-v1");
    assert_eq!(analysis.generated_at_ms, 12_345);

    let stored = service
        .database()
        .get_trend_analysis(
            &evidence.range.start_date,
            &evidence.range.end_date,
            &evidence.evidence_hash,
        )
        .unwrap()
        .expect("local fallback is persisted");
    assert_eq!(stored.source, "local");
    assert_eq!(stored.model, "deterministic-v1");
    assert_eq!(stored.generated_at_ms, 12_345);

    let reread = service.get_trend_analysis(&evidence, 99_999).unwrap();
    assert_eq!(reread, analysis);
}

#[test]
fn trend_analysis_candidates_are_finite_safe_and_reused_by_local_analysis() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&trend_segment(
            "candidate-source",
            "Codex",
            ActivityCategory::CreationDevelopment,
            VideoPurpose::Unknown,
            1_000,
            3_601_000,
        ))
        .unwrap();
    let service = AppService::new(database);
    let evidence = one_day_trend_payload(&service);
    let allowed = trend_analysis_allowed_candidates(&evidence);

    assert!(!allowed.summaries.is_empty());
    assert!(!allowed.observations.is_empty());
    assert!(!allowed.suggestions.is_empty());
    assert!(allowed.observations.len() <= 4);
    assert!(allowed.suggestions.len() <= 3);
    assert!(
        allowed
            .observations
            .contains(&"当前区间分类结构已有记录".to_string())
    );
    assert!(
        !allowed
            .observations
            .contains(&"分类差异有所变化".to_string())
    );
    assert!(
        !allowed
            .observations
            .contains(&"活动时段存在差异".to_string())
    );
    for text in allowed
        .summaries
        .iter()
        .chain(&allowed.observations)
        .chain(&allowed.suggestions)
    {
        assert!(!text.chars().any(|character| character.is_numeric()));
        for forbidden in [
            "你",
            "您",
            "懒惰",
            "自律",
            "表现",
            "质量差",
            "执行力",
            "能力",
            "性格",
            "情绪",
            "健康",
        ] {
            assert!(!text.contains(forbidden), "unsafe candidate: {text}");
        }
    }

    let local = service.get_trend_analysis(&evidence, 12_345).unwrap();
    assert!(allowed.summaries.contains(&local.summary));
    assert!(
        local
            .observations
            .iter()
            .all(|item| allowed.observations.contains(item))
    );
    assert!(
        local
            .suggestions
            .iter()
            .all(|item| allowed.suggestions.contains(item))
    );
    assert_eq!(
        local.observations.len(),
        local
            .observations
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
    );
}

#[test]
fn trend_analysis_identical_breakdowns_do_not_generate_change_candidates() {
    const DAY_MS: i64 = 86_400_000;
    let database = Database::open_in_memory().unwrap();
    for (id, start, end) in [
        ("previous-identical", -DAY_MS + 1_000, -DAY_MS + 3_601_000),
        ("current-identical", 1_000, 3_601_000),
    ] {
        database
            .insert_segment(&trend_segment(
                id,
                "Codex",
                ActivityCategory::CreationDevelopment,
                VideoPurpose::Unknown,
                start,
                end,
            ))
            .unwrap();
    }

    let evidence = one_day_trend_payload(&AppService::new(database));
    let allowed = trend_analysis_allowed_candidates(&evidence);

    assert!(
        !allowed
            .observations
            .contains(&"分类差异有所变化".to_string())
    );
    assert!(
        !allowed
            .observations
            .contains(&"应用分布有所变化".to_string())
    );
    assert!(
        !allowed
            .observations
            .contains(&"当前区间分类结构已有记录".to_string())
    );
}

#[test]
fn trend_analysis_different_breakdowns_generate_change_candidates() {
    const DAY_MS: i64 = 86_400_000;
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&trend_segment(
            "previous-different",
            "Terminal",
            ActivityCategory::Social,
            VideoPurpose::Unknown,
            -DAY_MS + 1_000,
            -DAY_MS + 3_601_000,
        ))
        .unwrap();
    database
        .insert_segment(&trend_segment(
            "current-different",
            "Codex",
            ActivityCategory::CreationDevelopment,
            VideoPurpose::Unknown,
            1_000,
            3_601_000,
        ))
        .unwrap();

    let evidence = one_day_trend_payload(&AppService::new(database));
    assert_eq!(evidence.comparison.active_seconds_delta_percent, Some(0.0));
    let allowed = trend_analysis_allowed_candidates(&evidence);

    assert!(
        allowed
            .observations
            .contains(&"分类差异有所变化".to_string())
    );
    assert!(
        allowed
            .observations
            .contains(&"应用分布有所变化".to_string())
    );
    assert!(
        !allowed
            .observations
            .contains(&"当前区间分类结构已有记录".to_string())
    );
}

#[test]
fn trend_analysis_scalar_changes_require_the_eight_percent_authoritative_delta() {
    const DAY_MS: i64 = 86_400_000;
    let database = Database::open_in_memory().unwrap();
    for (id, start, end) in [
        ("previous-threshold", -DAY_MS + 1_000, -DAY_MS + 3_601_000),
        ("current-threshold", 1_000, 3_601_000),
    ] {
        database
            .insert_segment(&trend_segment(
                id,
                "Codex",
                ActivityCategory::CreationDevelopment,
                VideoPurpose::Unknown,
                start,
                end,
            ))
            .unwrap();
    }
    let mut evidence = one_day_trend_payload(&AppService::new(database));

    evidence.comparison.active_seconds_delta_percent = Some(7.999);
    let below = trend_analysis_allowed_candidates(&evidence);
    assert!(
        !below
            .observations
            .contains(&"活动记录总量有所变化".to_string())
    );
    assert!(
        below
            .observations
            .contains(&"当前区间存在有效活动记录".to_string())
    );

    evidence.comparison.active_seconds_delta_percent = Some(-8.0);
    let at_threshold = trend_analysis_allowed_candidates(&evidence);
    assert!(
        at_threshold
            .observations
            .contains(&"活动记录总量有所变化".to_string())
    );
    assert!(
        !at_threshold
            .observations
            .contains(&"当前区间存在有效活动记录".to_string())
    );

    evidence.comparison.active_seconds_delta_percent = None;
    let without_baseline = trend_analysis_allowed_candidates(&evidence);
    assert!(
        !without_baseline
            .observations
            .contains(&"活动记录总量有所变化".to_string())
    );
}

#[test]
fn trend_analysis_queue_deduplicates_the_exact_evidence_hash() {
    let database = Database::open_in_memory().unwrap();
    let service = AppService::new(database);
    let evidence = one_day_trend_payload(&service);
    let (day_boundaries, comparison_boundaries) = one_day_trend_boundaries();
    let execution = execution_snapshot("openai", "queued-trend-model", 5_000);

    assert_eq!(
        service
            .queue_trend_analysis(
                &evidence,
                day_boundaries.clone(),
                comparison_boundaries.clone(),
                4_000,
                None,
                false,
            )
            .unwrap(),
        None,
        "offline or unconfigured providers must not create dead queue work"
    );
    let first = service
        .queue_trend_analysis(
            &evidence,
            day_boundaries.clone(),
            comparison_boundaries.clone(),
            5_000,
            Some(&execution),
            false,
        )
        .unwrap()
        .expect("configured provider queues work");
    let second = service
        .queue_trend_analysis(
            &evidence,
            day_boundaries.clone(),
            comparison_boundaries.clone(),
            6_000,
            Some(&execution),
            false,
        )
        .unwrap()
        .expect("same evidence reuses the pending job");

    assert_eq!(first, second);
    assert_eq!(service.database().ai_job_count().unwrap(), 1);
    let job = service.database().get_ai_job(&first).unwrap().unwrap();
    let payload: serde_json::Value = serde_json::from_str(&job.payload_json).unwrap();
    let keys: Vec<_> = payload
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        ["comparisonDayBoundariesMs", "dayBoundariesMs", "evidence"]
    );
    let evidence_keys: Vec<_> = payload["evidence"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        evidence_keys,
        [
            "comparison",
            "days",
            "evidenceHash",
            "quality",
            "range",
            "summary",
        ]
    );
    for forbidden in [
        "rawFiles",
        "historyPath",
        "windowTitle",
        "browserBody",
        "textSnippet",
    ] {
        assert!(!job.payload_json.contains(forbidden));
    }

    service
        .database()
        .mark_ai_job_running(&first, 6_000)
        .unwrap();
    service
        .database()
        .complete_ai_job_generation_audit(
            &first,
            0,
            6_500,
            Some("openai"),
            Some("gpt-test"),
            Some(0),
        )
        .unwrap();
    let automatic = service
        .queue_trend_analysis(
            &evidence,
            day_boundaries.clone(),
            comparison_boundaries.clone(),
            7_000,
            Some(&execution),
            false,
        )
        .unwrap()
        .unwrap();
    assert_eq!(automatic, first);
    assert_eq!(
        service
            .database()
            .get_ai_job(&first)
            .unwrap()
            .unwrap()
            .status,
        daily_task_monitor_core::ai::AiJobStatus::Complete
    );

    let forced = service
        .queue_trend_analysis(
            &evidence,
            day_boundaries,
            comparison_boundaries,
            8_000,
            Some(&execution),
            true,
        )
        .unwrap()
        .unwrap();
    assert_ne!(forced, first);
    assert_eq!(
        service
            .database()
            .get_ai_job(&first)
            .unwrap()
            .unwrap()
            .status,
        daily_task_monitor_core::ai::AiJobStatus::Complete
    );
    let requeued = service.database().get_ai_job(&forced).unwrap().unwrap();
    assert_eq!(
        requeued.status,
        daily_task_monitor_core::ai::AiJobStatus::Pending
    );
    assert_eq!(requeued.attempts, 0);
    assert_eq!(requeued.next_attempt_at_ms, 8_000);
    assert_eq!(
        service
            .database()
            .next_due_ai_job(8_000)
            .unwrap()
            .unwrap()
            .id,
        forced
    );
}

#[test]
fn scoped_trend_analysis_uses_strict_learning_activity_and_keeps_legacy_all_jobs_compatible() {
    const DAY_MS: i64 = 86_400_000;
    let database = Database::open_in_memory().unwrap();
    database
        .create_work_ledger_project(NewProject {
            id: "trend-scope-project".into(),
            name: "Trend scope project".into(),
            color: "#2563eb".into(),
            description: String::new(),
            created_at_ms: 1,
        })
        .unwrap();
    database
        .create_work_ledger_task(NewTask {
            id: "trend-scope-task".into(),
            project_id: "trend-scope-project".into(),
            title: "Trend scope task".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1,
        })
        .unwrap();
    for (id, category, start_ms) in [
        ("trend-game", ActivityCategory::Game, 0),
        (
            "trend-linked-file",
            ActivityCategory::FileManagement,
            60_000,
        ),
        ("trend-linked-social", ActivityCategory::Social, 120_000),
        (
            "trend-unlinked-file",
            ActivityCategory::FileManagement,
            180_000,
        ),
        ("trend-unlinked-social", ActivityCategory::Social, 240_000),
    ] {
        database
            .insert_segment(&segment(id, category, start_ms, start_ms + 60_000))
            .unwrap();
    }
    for id in ["trend-linked-file", "trend-linked-social"] {
        database
            .assign_work_ledger_activity(
                "trend-scope-task",
                id,
                EvidenceProvenance::Manual,
                1.0,
                "confirmed",
                1,
            )
            .unwrap();
    }
    let service = AppService::new(database);
    let all = service
        .get_trends_scoped(
            "2026-07-12",
            "2026-07-12",
            vec![0, DAY_MS],
            "2026-07-11",
            "2026-07-11",
            vec![-DAY_MS, 0],
            ActivityScope::All,
        )
        .unwrap();
    let meaningful = service
        .get_trends_scoped(
            "2026-07-12",
            "2026-07-12",
            vec![0, DAY_MS],
            "2026-07-11",
            "2026-07-11",
            vec![-DAY_MS, 0],
            ActivityScope::Meaningful,
        )
        .unwrap();

    assert_eq!(
        one_day_trend_payload(&service).evidence_hash,
        all.evidence_hash
    );
    assert_ne!(all.evidence_hash, meaningful.evidence_hash);
    assert_eq!(all.summary.monitored_seconds, 300);
    assert_eq!(meaningful.summary.monitored_seconds, 0);
    assert_eq!(meaningful.summary.active_seconds, 0);
    assert_eq!(
        meaningful
            .summary
            .category_breakdown
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        [] as [&str; 0]
    );

    let meaningful_analysis = service
        .get_trend_analysis_scoped(&meaningful, 5_000, ActivityScope::Meaningful)
        .unwrap();
    assert_eq!(
        meaningful_analysis.activity_scope,
        ActivityScope::Meaningful
    );
    assert_eq!(
        serde_json::to_value(&meaningful_analysis).unwrap()["activityScope"],
        "meaningful"
    );

    let execution = execution_snapshot("openai", "queued-trend-model", 6_000);
    let (day_boundaries, comparison_boundaries) = one_day_trend_boundaries();
    let legacy_all_id = service
        .queue_trend_analysis(
            &all,
            day_boundaries.clone(),
            comparison_boundaries.clone(),
            6_000,
            Some(&execution),
            false,
        )
        .unwrap()
        .unwrap();
    let scoped_all_id = service
        .queue_trend_analysis_scoped(
            &all,
            day_boundaries.clone(),
            comparison_boundaries.clone(),
            7_000,
            Some(&execution),
            false,
            ActivityScope::All,
        )
        .unwrap()
        .unwrap();
    let meaningful_id = service
        .queue_trend_analysis_scoped(
            &meaningful,
            day_boundaries,
            comparison_boundaries,
            8_000,
            Some(&execution),
            false,
            ActivityScope::Meaningful,
        )
        .unwrap()
        .unwrap();

    assert_eq!(legacy_all_id, scoped_all_id);
    assert_ne!(legacy_all_id, meaningful_id);
    assert_eq!(service.database().ai_job_count().unwrap(), 2);
    let legacy_all_job = service
        .database()
        .get_ai_job(&legacy_all_id)
        .unwrap()
        .unwrap();
    let legacy_all_payload: serde_json::Value =
        serde_json::from_str(&legacy_all_job.payload_json).unwrap();
    assert!(legacy_all_payload.get("activityScope").is_none());
    assert!(
        legacy_all_payload["evidence"]
            .get("activityScope")
            .is_none()
    );
    let decoded_legacy: TrendAnalysisJobPayload =
        serde_json::from_str(&legacy_all_job.payload_json).unwrap();
    assert_eq!(decoded_legacy.activity_scope, ActivityScope::All);

    let meaningful_job = service
        .database()
        .get_ai_job(&meaningful_id)
        .unwrap()
        .unwrap();
    let meaningful_payload: serde_json::Value =
        serde_json::from_str(&meaningful_job.payload_json).unwrap();
    assert_eq!(meaningful_payload["activityScope"], "meaningful");
    assert_eq!(
        meaningful_payload["evidence"]["activityScope"],
        "meaningful"
    );
    let decoded_meaningful: TrendAnalysisJobPayload =
        serde_json::from_str(&meaningful_job.payload_json).unwrap();
    assert_eq!(decoded_meaningful.activity_scope, ActivityScope::Meaningful);
}

#[test]
fn trend_markdown_contains_obsidian_frontmatter_callouts_and_evidence_tables() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&trend_segment(
            "trend-export",
            "Codex",
            ActivityCategory::CreationDevelopment,
            VideoPurpose::Unknown,
            1_000,
            3_601_000,
        ))
        .unwrap();
    let service = AppService::new(database);
    let evidence = one_day_trend_payload(&service);
    let analysis = service.get_trend_analysis(&evidence, 9_000).unwrap();

    let markdown = render_trend_markdown(&evidence, &analysis, 10_000);

    assert!(markdown.starts_with("---\ntype: trend-analysis\n"));
    assert!(markdown.contains("range_start: \"2026-07-12\""));
    assert!(markdown.contains("range_end: \"2026-07-12\""));
    assert!(markdown.contains("generated_at: \"1970-01-01T00:00:10Z\""));
    assert!(markdown.contains(&format!("evidence_hash: \"{}\"", evidence.evidence_hash)));
    assert!(markdown.contains("confidence:"));
    assert!(markdown.contains("> [!summary] 趋势摘要"));
    assert!(markdown.contains("> [!info] 数据质量"));
    assert!(markdown.contains("| 指标 | 当前周期 | 上一周期 | 变化 |"));
    assert!(markdown.contains("| 日期 | 活跃 | 学习 | 不活跃 | 切换 | 最长专注 |"));
    for heading in ["## 分类变化", "## 应用变化", "## 观察", "## 建议"] {
        assert!(markdown.contains(heading), "missing {heading}");
    }
}

#[test]
fn trend_markdown_export_includes_workbench_context_and_evidence_ids() {
    let database = Database::open_in_memory().unwrap();
    let service = AppService::new(database);
    let evidence = one_day_trend_payload(&service);
    let analysis = service.get_trend_analysis(&evidence, 9_000).unwrap();
    let workbench = service
        .get_trend_workbench(TrendWorkbenchRequest {
            start_date: "2026-07-12".into(),
            end_date: "2026-07-12".into(),
            selected_dates: None,
            timezone_offset_minutes: 0,
            granularity: Some(TrendGranularity::Week),
            metric: TrendMetric::ActiveSeconds,
            custom_baseline: Some(TrendDateRange {
                start_date: "2026-06-12".into(),
                end_date: "2026-06-12".into(),
            }),
        })
        .unwrap();

    let markdown = render_trend_markdown_with_workbench(&evidence, &analysis, &workbench, 10_000);

    for heading in [
        "## 粒度",
        "## 比较基准",
        "## 任务统计",
        "## 数据质量",
        "## Evidence ID",
    ] {
        assert!(markdown.contains(heading), "missing {heading}");
    }
    assert!(markdown.contains("| 当前粒度 | 周 |"));
    for baseline in ["当前区间", "上一等长区间", "上月同期", "自定义基准"] {
        assert!(markdown.contains(baseline), "missing {baseline}");
    }
    assert!(markdown.contains(&format!("`{}`", evidence.evidence_hash)));
    assert!(markdown.contains(&format!("`{}`", workbench.evidence_hash)));
    assert!(markdown.contains("## 区间总览"));
    assert!(markdown.contains("## 每日平均"));
    assert!(markdown.contains("个有效采样日"));
    assert!(markdown.contains("切换负荷"));
}

#[test]
fn stale_trend_analysis_completion_cannot_replace_the_newer_hash() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&trend_segment(
            "initial-trend",
            "Browser",
            ActivityCategory::Research,
            VideoPurpose::Unknown,
            1_000,
            61_000,
        ))
        .unwrap();
    let service = AppService::new(database);
    let old_evidence = one_day_trend_payload(&service);
    let (day_boundaries, comparison_boundaries) = one_day_trend_boundaries();
    let execution = execution_snapshot("openai", "queued-trend-model", 5_000);
    let job_id = service
        .queue_trend_analysis(
            &old_evidence,
            day_boundaries,
            comparison_boundaries,
            5_000,
            Some(&execution),
            false,
        )
        .unwrap()
        .unwrap();
    let queued: TrendAnalysisJobPayload = serde_json::from_str(
        &service
            .database()
            .get_ai_job(&job_id)
            .unwrap()
            .unwrap()
            .payload_json,
    )
    .unwrap();

    service
        .database()
        .insert_segment(&trend_segment(
            "new-trend-work",
            "Codex",
            ActivityCategory::CreationDevelopment,
            VideoPurpose::Unknown,
            61_000,
            121_000,
        ))
        .unwrap();
    let newer_evidence = one_day_trend_payload(&service);
    assert_ne!(old_evidence.evidence_hash, newer_evidence.evidence_hash);

    assert!(
        !service
            .save_ai_trend_analysis_if_current(
                &queued,
                "本区间记录显示活动有变化",
                &["活动分布有变化".into()],
                &["建议尝试固定连续任务".into()],
                "openai",
                "gpt-test",
                0.7,
                11_000,
            )
            .unwrap()
    );
    assert!(
        service
            .database()
            .get_trend_analysis(
                &old_evidence.range.start_date,
                &old_evidence.range.end_date,
                &old_evidence.evidence_hash,
            )
            .unwrap()
            .is_none()
    );

    let (day_boundaries, comparison_boundaries) = one_day_trend_boundaries();
    let current_job_id = service
        .queue_trend_analysis(
            &newer_evidence,
            day_boundaries,
            comparison_boundaries,
            12_000,
            Some(&execution),
            false,
        )
        .unwrap()
        .unwrap();
    let current_job: TrendAnalysisJobPayload = serde_json::from_str(
        &service
            .database()
            .get_ai_job(&current_job_id)
            .unwrap()
            .unwrap()
            .payload_json,
    )
    .unwrap();
    assert!(
        service
            .save_ai_trend_analysis_if_current(
                &current_job,
                "本区间记录显示活动有变化",
                &["活动分布有变化".into()],
                &["建议尝试固定连续任务".into()],
                "openai",
                "gpt-test",
                0.9,
                13_000,
            )
            .unwrap()
    );
    let current = service.get_trend_analysis(&newer_evidence, 14_000).unwrap();
    assert_eq!(current.source, "openai");
    assert_eq!(current.evidence_hash, newer_evidence.evidence_hash);
}

#[test]
fn forced_generation_prevents_old_trend_worker_from_saving_provider_result() {
    let service = AppService::new(Database::open_in_memory().unwrap());
    let evidence = one_day_trend_payload(&service);
    let (day_boundaries, comparison_boundaries) = one_day_trend_boundaries();
    let execution = execution_snapshot("openai", "queued-trend-model", 1_000);
    let id = service
        .queue_trend_analysis(
            &evidence,
            day_boundaries.clone(),
            comparison_boundaries.clone(),
            1_000,
            Some(&execution),
            false,
        )
        .unwrap()
        .unwrap();
    let old_run = service
        .database()
        .claim_next_due_ai_job(1_000)
        .unwrap()
        .unwrap();
    let queued: TrendAnalysisJobPayload = serde_json::from_str(&old_run.payload_json).unwrap();

    let forced = service
        .queue_trend_analysis(
            &evidence,
            day_boundaries,
            comparison_boundaries,
            2_000,
            Some(&execution),
            true,
        )
        .unwrap()
        .unwrap();
    assert_ne!(forced, id);
    assert!(
        !service
            .complete_ai_trend_analysis_job(
                &id,
                old_run.generation,
                &queued,
                "本区间记录显示活动有变化",
                &["活动分布有变化".into()],
                &["建议尝试固定连续任务".into()],
                "openai",
                "gpt-test",
                0.8,
                3_000,
            )
            .unwrap()
    );
    assert!(
        service
            .database()
            .get_trend_analysis(
                &evidence.range.start_date,
                &evidence.range.end_date,
                &evidence.evidence_hash,
            )
            .unwrap()
            .is_none()
    );
    let superseded = service.database().get_ai_job(&id).unwrap().unwrap();
    assert_eq!(
        superseded.status,
        daily_task_monitor_core::ai::AiJobStatus::Complete
    );
    assert!(
        superseded
            .last_error
            .to_ascii_lowercase()
            .contains("superseded")
    );
    let pending = service.database().get_ai_job(&forced).unwrap().unwrap();
    assert_eq!(
        pending.status,
        daily_task_monitor_core::ai::AiJobStatus::Pending
    );
    assert_eq!(pending.generation, old_run.generation + 1);

    let new_run = service
        .database()
        .claim_next_due_ai_job(2_000)
        .unwrap()
        .unwrap();
    assert!(
        service
            .complete_ai_trend_analysis_job(
                &forced,
                new_run.generation,
                &queued,
                "本区间记录显示活动有变化",
                &["活动分布有变化".into()],
                &["建议尝试固定连续任务".into()],
                "openai",
                "gpt-test",
                0.8,
                4_000,
            )
            .unwrap()
    );
    assert_eq!(
        service
            .database()
            .get_ai_job(&forced)
            .unwrap()
            .unwrap()
            .status,
        daily_task_monitor_core::ai::AiJobStatus::Complete
    );
    assert_eq!(
        service.get_trend_analysis(&evidence, 5_000).unwrap().source,
        "openai"
    );
}
