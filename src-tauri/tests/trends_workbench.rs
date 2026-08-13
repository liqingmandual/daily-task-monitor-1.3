use chrono::NaiveDate;
use daily_task_monitor_core::ai::{AiExecutionMode, AiExecutionSnapshot};
use daily_task_monitor_core::app::{AppService, render_trend_markdown_with_workbench};
use daily_task_monitor_core::db::{ActivitySegmentRecord, Database};
use daily_task_monitor_core::domain::{
    ActivityCategory, ActivityDisplayKey, ActivityScope, ClassificationSource, InactivityReason,
    MeaningfulReason, VideoPurpose,
};
use daily_task_monitor_core::trend_analysis::{ResearchStatus, build_trend_research_input};
use daily_task_monitor_core::trends::{
    TrendBaselineKind, TrendDateRange, TrendEvidenceScope, TrendGranularity, TrendMetric,
    TrendMetricAvailabilityStatus, TrendRawEvidenceKind, TrendReviewState, TrendWorkbenchRequest,
    get_trend_workbench_scoped, trend_metric_availability,
};
use daily_task_monitor_core::work_ledger::{
    EvidenceProvenance, NewProject, NewTask, TaskPriority, TaskStatus,
};
use rusqlite::{Connection, params};
use std::collections::HashSet;

const DAY_MS: i64 = 86_400_000;

fn utc_ms(date: &str) -> i64 {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp_millis()
}

fn segment(
    id: &str,
    date: &str,
    offset_ms: i64,
    duration_ms: i64,
    category: ActivityCategory,
    confidence: f32,
) -> ActivitySegmentRecord {
    let started_at_ms = utc_ms(date) + offset_ms;
    ActivitySegmentRecord {
        id: id.into(),
        started_at_ms,
        ended_at_ms: started_at_ms + duration_ms,
        app: "Codex".into(),
        app_path: String::new(),
        title: "private title must not enter trend evidence".into(),
        category,
        video_purpose: VideoPurpose::Unknown,
        confidence,
        source: ClassificationSource::Rule,
        reason: "test".into(),
        model_version: "rules-v1".into(),
        needs_review: false,
        inactivity_reason: None,
    }
}

#[test]
fn trend_workbench_reports_inactivity_reason_distribution() {
    let database = Database::open_in_memory().unwrap();
    let mut input_idle = segment(
        "input-idle",
        "2026-07-12",
        0,
        60_000,
        ActivityCategory::Idle,
        1.0,
    );
    input_idle.inactivity_reason = Some(InactivityReason::InputIdle);
    let mut continuity_gap = segment(
        "continuity-gap",
        "2026-07-12",
        60_000,
        120_000,
        ActivityCategory::Idle,
        1.0,
    );
    continuity_gap.inactivity_reason = Some(InactivityReason::ContinuityGap);
    database.insert_segment(&input_idle).unwrap();
    database.insert_segment(&continuity_gap).unwrap();

    let payload = daily_task_monitor_core::trends::get_trend_workbench(
        &database,
        request(
            "2026-07-12",
            "2026-07-12",
            Some(TrendGranularity::Day),
            TrendMetric::IdleSeconds,
        ),
    )
    .unwrap();

    assert!(
        payload
            .inactivity_reason_distribution
            .iter()
            .any(|item| { item.reason == InactivityReason::InputIdle && item.seconds == 60 })
    );
    assert!(
        payload
            .inactivity_reason_distribution
            .iter()
            .any(|item| { item.reason == InactivityReason::ContinuityGap && item.seconds == 120 })
    );
}

#[test]
fn trend_workbench_canonicalizes_overlapping_collectors_for_every_view() {
    let database = Database::open_in_memory().unwrap();
    for item in [
        segment(
            "idle-a",
            "2026-07-12",
            0,
            4 * 3_600_000,
            ActivityCategory::Idle,
            1.0,
        ),
        segment(
            "idle-b",
            "2026-07-12",
            0,
            4 * 3_600_000,
            ActivityCategory::Idle,
            1.0,
        ),
        segment(
            "active",
            "2026-07-12",
            3_600_000,
            3_600_000,
            ActivityCategory::Research,
            1.0,
        ),
    ] {
        database.insert_segment(&item).unwrap();
    }

    let payload = daily_task_monitor_core::trends::get_trend_workbench(
        &database,
        request(
            "2026-07-12",
            "2026-07-12",
            Some(TrendGranularity::Day),
            TrendMetric::MonitoredSeconds,
        ),
    )
    .unwrap();

    assert_eq!(payload.summary.totals.monitored_seconds, 4 * 3_600);
    assert_eq!(payload.summary.totals.active_seconds, 3_600);
    assert_eq!(payload.summary.totals.idle_seconds, 3 * 3_600);
    assert_eq!(payload.buckets[0].values.monitored_seconds, 4 * 3_600);
    assert_eq!(payload.activity_composition.all.total_seconds, 4 * 3_600);
    assert_eq!(
        payload.buckets[0]
            .drilldown
            .raw_rows
            .iter()
            .map(|row| row.clipped_duration_seconds)
            .sum::<i64>(),
        4 * 3_600
    );
}

fn request(
    start_date: &str,
    end_date: &str,
    granularity: Option<TrendGranularity>,
    metric: TrendMetric,
) -> TrendWorkbenchRequest {
    TrendWorkbenchRequest {
        start_date: start_date.into(),
        end_date: end_date.into(),
        selected_dates: None,
        timezone_offset_minutes: 0,
        granularity,
        metric,
        custom_baseline: None,
    }
}

#[test]
fn trend_workbench_scope_filters_excluded_activity_and_changes_the_evidence_hash() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment(
            "dev",
            "2026-07-12",
            0,
            60_000,
            ActivityCategory::CreationDevelopment,
            1.0,
        ))
        .unwrap();
    database
        .insert_segment(&segment(
            "game",
            "2026-07-12",
            60_000,
            60_000,
            ActivityCategory::Game,
            1.0,
        ))
        .unwrap();
    let request = request(
        "2026-07-12",
        "2026-07-12",
        Some(TrendGranularity::Day),
        TrendMetric::ActiveSeconds,
    );

    let all = get_trend_workbench_scoped(&database, request.clone(), ActivityScope::All).unwrap();
    let meaningful =
        get_trend_workbench_scoped(&database, request, ActivityScope::Meaningful).unwrap();

    assert_eq!(all.summary.totals.active_seconds, 120);
    assert_eq!(meaningful.summary.totals.active_seconds, 60);
    assert_ne!(all.evidence_hash, meaningful.evidence_hash);
}

#[test]
fn trend_composition_uses_strict_learning_scope_and_merges_unknown_video_into_pending() {
    let database = Database::open_in_memory().unwrap();
    database
        .create_work_ledger_project(NewProject {
            id: "composition-project".into(),
            name: "Composition project".into(),
            color: "#2563eb".into(),
            description: String::new(),
            created_at_ms: utc_ms("2024-04-10"),
        })
        .unwrap();
    database
        .create_work_ledger_task(NewTask {
            id: "composition-task".into(),
            project_id: "composition-project".into(),
            title: "Composition task".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: utc_ms("2024-04-10"),
        })
        .unwrap();

    let start = utc_ms("2024-04-10");
    for (index, id, category, purpose) in [
        (
            0,
            "linked-file",
            ActivityCategory::FileManagement,
            VideoPurpose::Unknown,
        ),
        (
            1,
            "unlinked-file",
            ActivityCategory::FileManagement,
            VideoPurpose::Unknown,
        ),
        (
            2,
            "linked-social",
            ActivityCategory::Social,
            VideoPurpose::Unknown,
        ),
        (
            3,
            "unknown-video",
            ActivityCategory::VideoInput,
            VideoPurpose::Unknown,
        ),
    ] {
        let mut item = segment(id, "2024-04-10", index * 600_000, 600_000, category, 0.95);
        item.started_at_ms = start + index * 600_000;
        item.ended_at_ms = item.started_at_ms + 600_000;
        item.video_purpose = purpose;
        database.insert_segment(&item).unwrap();
    }
    for segment_id in ["linked-file", "linked-social"] {
        database
            .assign_work_ledger_activity(
                "composition-task",
                segment_id,
                EvidenceProvenance::Manual,
                1.0,
                "confirmed",
                start,
            )
            .unwrap();
    }

    let service = AppService::new(database);
    let scoped = service
        .get_trend_workbench_scoped(
            request(
                "2024-04-10",
                "2024-04-10",
                Some(TrendGranularity::Day),
                TrendMetric::MonitoredSeconds,
            ),
            ActivityScope::Meaningful,
        )
        .unwrap();
    let payload = service
        .get_trend_workbench(request(
            "2024-04-10",
            "2024-04-10",
            Some(TrendGranularity::Day),
            TrendMetric::MonitoredSeconds,
        ))
        .unwrap();

    assert_eq!(payload.activity_composition.all.total_seconds, 40 * 60);
    assert_eq!(payload.summary.pending_seconds, 10 * 60);
    assert_eq!(payload.summary.classification_coverage, 0.75);
    assert_eq!(
        scoped.summary.totals.active_seconds, 0,
        "learning scope excludes file management and social activity even when task-linked"
    );
    assert_eq!(
        scoped.summary.totals.monitored_seconds, 0,
        "file management, social activity and unknown video remain excluded"
    );
    assert_eq!(payload.activity_composition.meaningful.total_seconds, 0);
    assert_eq!(
        payload.buckets[0]
            .activity_composition
            .meaningful
            .total_seconds,
        0
    );
    assert!(payload.activity_composition.all.items.iter().any(|item| {
        item.key == ActivityDisplayKey::Pending
            && item.category == ActivityCategory::Pending
            && item.seconds == 10 * 60
    }));

    let rows = &payload.buckets[0].drilldown.raw_rows;
    let linked_file = rows
        .iter()
        .find(|row| row.evidence_id == "linked-file")
        .unwrap();
    assert!(!linked_file.meaningful);
    assert_eq!(linked_file.meaningful_reason, MeaningfulReason::Excluded);
    let unlinked_file = rows
        .iter()
        .find(|row| row.evidence_id == "unlinked-file")
        .unwrap();
    assert!(!unlinked_file.meaningful);
    assert_eq!(unlinked_file.meaningful_reason, MeaningfulReason::Excluded);
    let unknown_video = rows
        .iter()
        .find(|row| row.evidence_id == "unknown-video")
        .unwrap();
    assert_eq!(unknown_video.video_purpose, Some(VideoPurpose::Unknown));
    assert!(!unknown_video.meaningful);
}

fn sparse_request(
    start_date: &str,
    end_date: &str,
    selected_dates: serde_json::Value,
    granularity: TrendGranularity,
    metric: TrendMetric,
) -> TrendWorkbenchRequest {
    serde_json::from_value(serde_json::json!({
        "startDate": start_date,
        "endDate": end_date,
        "selectedDates": selected_dates,
        "timezoneOffsetMinutes": 0,
        "granularity": granularity,
        "metric": metric,
        "customBaseline": null
    }))
    .unwrap()
}

#[test]
fn sparse_dates_are_normalized_and_exclude_unselected_activity_tasks_and_ownership() {
    let database = Database::open_in_memory().unwrap();
    for item in [
        segment(
            "selected-one",
            "2024-04-01",
            0,
            60_000,
            ActivityCategory::CreationDevelopment,
            0.9,
        ),
        segment(
            "unselected",
            "2024-04-02",
            0,
            600_000,
            ActivityCategory::CreationDevelopment,
            0.9,
        ),
        segment(
            "selected-three",
            "2024-04-03",
            0,
            180_000,
            ActivityCategory::CreationDevelopment,
            0.9,
        ),
        segment(
            "previous-selected-one",
            "2024-03-29",
            0,
            30_000,
            ActivityCategory::CreationDevelopment,
            0.9,
        ),
        segment(
            "previous-unselected",
            "2024-03-30",
            0,
            900_000,
            ActivityCategory::CreationDevelopment,
            0.9,
        ),
        segment(
            "previous-selected-three",
            "2024-03-31",
            0,
            90_000,
            ActivityCategory::CreationDevelopment,
            0.9,
        ),
    ] {
        database.insert_segment(&item).unwrap();
    }
    database
        .create_work_ledger_project(NewProject {
            id: "sparse-project".into(),
            name: "Sparse project".into(),
            color: "#2563eb".into(),
            description: String::new(),
            created_at_ms: utc_ms("2024-03-01"),
        })
        .unwrap();
    for (task_id, title) in [
        ("selected-task", "Selected task"),
        ("unselected-task", "Unselected task"),
    ] {
        database
            .create_work_ledger_task(NewTask {
                id: task_id.into(),
                project_id: "sparse-project".into(),
                title: title.into(),
                priority: TaskPriority::Medium,
                expected_output: String::new(),
                due_date: None,
                created_at_ms: utc_ms("2024-03-01"),
            })
            .unwrap();
    }
    database
        .assign_work_ledger_activity(
            "selected-task",
            "selected-one",
            EvidenceProvenance::Manual,
            1.0,
            "selected",
            utc_ms("2024-04-01"),
        )
        .unwrap();
    database
        .assign_work_ledger_activity(
            "unselected-task",
            "unselected",
            EvidenceProvenance::Manual,
            1.0,
            "unselected",
            utc_ms("2024-04-02"),
        )
        .unwrap();
    database
        .transition_work_ledger_task_status(
            "selected-task",
            TaskStatus::Completed,
            utc_ms("2024-04-01"),
        )
        .unwrap();
    database
        .transition_work_ledger_task_status(
            "unselected-task",
            TaskStatus::Completed,
            utc_ms("2024-04-02"),
        )
        .unwrap();

    let payload = AppService::new(database)
        .get_trend_workbench(sparse_request(
            "2024-04-01",
            "2024-04-03",
            serde_json::json!(["2024-04-03", "2024-04-01", "2024-04-03"]),
            TrendGranularity::Day,
            TrendMetric::ActiveSeconds,
        ))
        .unwrap();
    let value = serde_json::to_value(&payload).unwrap();

    assert_eq!(value["range"]["selectionMode"], "selectedDates");
    assert_eq!(
        value["range"]["selectedDates"],
        serde_json::json!(["2024-04-01", "2024-04-03"])
    );
    assert_eq!(value["range"]["selectedDateCount"], 2);
    assert_eq!(value["range"]["envelopeDayCount"], 3);
    assert_eq!(
        payload
            .buckets
            .iter()
            .map(|bucket| bucket.start_date.as_str())
            .collect::<Vec<_>>(),
        ["2024-04-01", "2024-04-03"]
    );
    assert_eq!(payload.summary.totals.active_seconds, 240);
    assert_eq!(payload.summary.average_sample_day_count, 2);
    assert_eq!(payload.summary.daily_average.active_seconds, Some(120.0));
    assert_eq!(payload.summary.daily_median.active_seconds, 120.0);
    assert_eq!(payload.summary.recorded_day_count, 2);
    assert_eq!(payload.summary.missing_day_count, 0);
    assert_eq!(payload.summary.totals.completed_task_count, 1);
    assert_eq!(payload.summary.totals.linked_task_seconds, 60);
    let switching_load = payload
        .evidence
        .iter()
        .find(|item| item.id == "current.summary.switchesPerActiveHour")
        .expect("switching load evidence is present");
    assert_eq!(switching_load.scope, TrendEvidenceScope::Rate);
    assert_eq!(switching_load.metric, None);
    assert_eq!(payload.baselines[1].value, Some(120.0));
    assert_eq!(
        serde_json::to_value(&payload.baselines[1]).unwrap()["selectedDates"],
        serde_json::json!(["2024-03-29", "2024-03-31"])
    );
    let raw_ids = payload
        .buckets
        .iter()
        .flat_map(|bucket| bucket.drilldown.raw_rows.iter())
        .map(|row| row.evidence_id.as_str())
        .collect::<HashSet<_>>();
    assert!(raw_ids.contains("selected-one"));
    assert!(raw_ids.contains("selected-three"));
    assert!(!raw_ids.contains("unselected"));
    assert!(payload.buckets.iter().all(|bucket| {
        bucket
            .drilldown
            .completed_tasks
            .iter()
            .all(|task| task.task_id != "unselected-task")
            && bucket
                .drilldown
                .workflow_ownership
                .iter()
                .all(|ownership| ownership.evidence_id != "unselected")
    }));
}

#[test]
fn sparse_week_month_and_baseline_dates_follow_selected_natural_groups() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment(
            "clamped-baseline",
            "2024-02-29",
            0,
            60_000,
            ActivityCategory::Research,
            0.9,
        ))
        .unwrap();
    let service = AppService::new(database);

    let weekly = service
        .get_trend_workbench(sparse_request(
            "2024-04-01",
            "2024-04-30",
            serde_json::json!(["2024-04-01", "2024-04-03", "2024-04-10", "2024-04-30"]),
            TrendGranularity::Week,
            TrendMetric::ActiveSeconds,
        ))
        .unwrap();
    assert_eq!(weekly.buckets.len(), 3);
    assert_eq!(
        weekly
            .buckets
            .iter()
            .map(|bucket| serde_json::to_value(bucket).unwrap()["selectedDates"].clone())
            .collect::<Vec<_>>(),
        [
            serde_json::json!(["2024-04-01", "2024-04-03"]),
            serde_json::json!(["2024-04-10"]),
            serde_json::json!(["2024-04-30"]),
        ]
    );
    assert_eq!(
        weekly
            .buckets
            .iter()
            .map(|bucket| bucket.missing_day_count)
            .collect::<Vec<_>>(),
        [2, 1, 1]
    );

    let monthly = service
        .get_trend_workbench(sparse_request(
            "2024-03-30",
            "2024-04-30",
            serde_json::json!(["2024-03-30", "2024-03-31", "2024-04-30"]),
            TrendGranularity::Month,
            TrendMetric::ActiveSeconds,
        ))
        .unwrap();
    assert_eq!(monthly.buckets.len(), 2);
    let previous_month = &monthly.baselines[2];
    let previous_month_value = serde_json::to_value(previous_month).unwrap();
    assert_eq!(
        previous_month_value["selectedDates"],
        serde_json::json!(["2024-02-29", "2024-03-30"])
    );
    assert_eq!(previous_month_value["selectedDateCount"], 2);
    assert_eq!(previous_month.value, Some(60.0));
}

#[test]
fn sparse_validation_empty_compatibility_hash_and_ai_identity_are_stable() {
    let service = AppService::new(Database::open_in_memory().unwrap());
    let first_request = sparse_request(
        "2024-04-01",
        "2024-04-03",
        serde_json::json!(["2024-04-01", "2024-04-03"]),
        TrendGranularity::Day,
        TrendMetric::ActiveSeconds,
    );
    let second_request = sparse_request(
        "2024-04-01",
        "2024-04-03",
        serde_json::json!(["2024-04-01", "2024-04-02"]),
        TrendGranularity::Day,
        TrendMetric::ActiveSeconds,
    );
    let first = service.get_trend_workbench(first_request).unwrap();
    let second = service.get_trend_workbench(second_request).unwrap();
    assert_ne!(first.evidence_hash, second.evidence_hash);
    let ai_input = serde_json::to_value(build_trend_research_input(&first)).unwrap();
    assert_eq!(ai_input["selectionMode"], "selectedDates");
    assert_eq!(
        ai_input["selectedDates"],
        serde_json::json!(["2024-04-01", "2024-04-03"])
    );

    let empty = service
        .get_trend_workbench(sparse_request(
            "2024-04-01",
            "2024-04-03",
            serde_json::json!([]),
            TrendGranularity::Day,
            TrendMetric::ActiveSeconds,
        ))
        .unwrap();
    assert_eq!(empty.buckets.len(), 3);
    assert_eq!(
        serde_json::to_value(&empty).unwrap()["range"]["selectionMode"],
        "continuous"
    );
    assert!(
        service
            .get_trend_workbench(sparse_request(
                "2024-04-01",
                "2024-04-03",
                serde_json::json!(["2024-03-31"]),
                TrendGranularity::Day,
                TrendMetric::ActiveSeconds,
            ))
            .is_err()
    );
}

#[test]
fn sparse_markdown_uses_workbench_identity_without_unselected_day_rows() {
    let database = Database::open_in_memory().unwrap();
    for item in [
        segment(
            "markdown-selected",
            "2024-04-01",
            0,
            60_000,
            ActivityCategory::Research,
            0.9,
        ),
        segment(
            "markdown-unselected",
            "2024-04-02",
            0,
            600_000,
            ActivityCategory::Research,
            0.9,
        ),
    ] {
        database.insert_segment(&item).unwrap();
    }
    let service = AppService::new(database);
    let legacy = service
        .get_trends(
            "2024-04-01",
            "2024-04-03",
            vec![
                utc_ms("2024-04-01"),
                utc_ms("2024-04-02"),
                utc_ms("2024-04-03"),
                utc_ms("2024-04-04"),
            ],
            "2024-03-29",
            "2024-03-31",
            vec![
                utc_ms("2024-03-29"),
                utc_ms("2024-03-30"),
                utc_ms("2024-03-31"),
                utc_ms("2024-04-01"),
            ],
        )
        .unwrap();
    let analysis = service.get_trend_analysis(&legacy, 1_000).unwrap();
    let workbench = service
        .get_trend_workbench(sparse_request(
            "2024-04-01",
            "2024-04-03",
            serde_json::json!(["2024-04-01", "2024-04-03"]),
            TrendGranularity::Day,
            TrendMetric::ActiveSeconds,
        ))
        .unwrap();

    let markdown = render_trend_markdown_with_workbench(&legacy, &analysis, &workbench, 1_000);
    assert!(markdown.contains("selection_mode: selectedDates"));
    assert!(markdown.contains("selected_dates: [\"2024-04-01\", \"2024-04-03\"]"));
    assert!(markdown.contains(&workbench.evidence_hash));
    assert!(!markdown.contains("2024-04-02"));
}

#[test]
fn day_buckets_include_leap_day_and_default_granularity_follows_range_length() {
    let service = AppService::new(Database::open_in_memory().unwrap());
    let leap = service
        .get_trend_workbench(request(
            "2024-02-28",
            "2024-03-01",
            None,
            TrendMetric::ActiveSeconds,
        ))
        .unwrap();
    assert_eq!(leap.granularity, TrendGranularity::Day);
    assert_eq!(
        leap.buckets
            .iter()
            .map(|bucket| bucket.start_date.as_str())
            .collect::<Vec<_>>(),
        ["2024-02-28", "2024-02-29", "2024-03-01"]
    );

    let weekly = service
        .get_trend_workbench(request(
            "2024-01-01",
            "2024-02-01",
            None,
            TrendMetric::ActiveSeconds,
        ))
        .unwrap();
    assert_eq!(weekly.granularity, TrendGranularity::Week);

    let monthly = service
        .get_trend_workbench(request(
            "2024-01-01",
            "2024-05-01",
            None,
            TrendMetric::ActiveSeconds,
        ))
        .unwrap();
    assert_eq!(monthly.granularity, TrendGranularity::Month);
}

#[test]
fn week_buckets_start_on_local_monday_and_clip_at_query_edges() {
    let payload = AppService::new(Database::open_in_memory().unwrap())
        .get_trend_workbench(request(
            "2024-12-29",
            "2025-01-07",
            Some(TrendGranularity::Week),
            TrendMetric::ActiveSeconds,
        ))
        .unwrap();

    assert_eq!(
        payload
            .buckets
            .iter()
            .map(|bucket| (bucket.start_date.as_str(), bucket.end_date.as_str()))
            .collect::<Vec<_>>(),
        [
            ("2024-12-29", "2024-12-29"),
            ("2024-12-30", "2025-01-05"),
            ("2025-01-06", "2025-01-07"),
        ]
    );
}

#[test]
fn month_buckets_follow_natural_months_and_clip_at_query_edges() {
    let payload = AppService::new(Database::open_in_memory().unwrap())
        .get_trend_workbench(request(
            "2024-01-30",
            "2024-03-02",
            Some(TrendGranularity::Month),
            TrendMetric::MonitoredSeconds,
        ))
        .unwrap();

    assert_eq!(
        payload
            .buckets
            .iter()
            .map(|bucket| (bucket.start_date.as_str(), bucket.end_date.as_str()))
            .collect::<Vec<_>>(),
        [
            ("2024-01-30", "2024-01-31"),
            ("2024-02-01", "2024-02-29"),
            ("2024-03-01", "2024-03-02"),
        ]
    );
}

#[test]
fn baselines_include_previous_equal_previous_month_and_optional_custom() {
    let service = AppService::new(Database::open_in_memory().unwrap());
    let mut workbench_request = request(
        "2024-03-31",
        "2024-03-31",
        Some(TrendGranularity::Day),
        TrendMetric::ActiveSeconds,
    );
    workbench_request.custom_baseline = Some(TrendDateRange {
        start_date: "2023-03-31".into(),
        end_date: "2023-03-31".into(),
    });

    let payload = service.get_trend_workbench(workbench_request).unwrap();
    assert_eq!(payload.baselines.len(), 4);
    assert_eq!(payload.baselines[0].kind, TrendBaselineKind::Current);
    assert_eq!(
        payload.baselines[1].range,
        TrendDateRange {
            start_date: "2024-03-30".into(),
            end_date: "2024-03-30".into(),
        }
    );
    assert_eq!(
        payload.baselines[1].kind,
        TrendBaselineKind::PreviousEqualLength
    );
    assert_eq!(
        payload.baselines[2].range,
        TrendDateRange {
            start_date: "2024-02-29".into(),
            end_date: "2024-02-29".into(),
        }
    );
    assert_eq!(
        payload.baselines[2].kind,
        TrendBaselineKind::PreviousMonthSamePeriod
    );
    assert_eq!(payload.baselines[3].kind, TrendBaselineKind::Custom);
    assert_eq!(payload.baselines[3].range.start_date, "2023-03-31");

    let leap_period = service
        .get_trend_workbench(request(
            "2024-03-01",
            "2024-03-03",
            Some(TrendGranularity::Day),
            TrendMetric::ActiveSeconds,
        ))
        .unwrap();
    assert_eq!(leap_period.baselines[1].range.start_date, "2024-02-27");
    assert_eq!(leap_period.baselines[1].range.end_date, "2024-02-29");
}

#[test]
fn zero_baseline_has_an_absolute_delta_without_an_infinite_percent_delta() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment(
            "current",
            "2024-04-10",
            0,
            60_000,
            ActivityCategory::CreationDevelopment,
            0.9,
        ))
        .unwrap();
    database
        .insert_segment(&segment(
            "sampled-zero-baseline",
            "2024-04-09",
            0,
            60_000,
            ActivityCategory::Idle,
            0.9,
        ))
        .unwrap();
    let payload = AppService::new(database)
        .get_trend_workbench(request(
            "2024-04-10",
            "2024-04-10",
            Some(TrendGranularity::Day),
            TrendMetric::ActiveSeconds,
        ))
        .unwrap();

    let _: i64 = payload.buckets[0].values.active_seconds;
    assert_eq!(payload.baselines[0].value, Some(60.0));
    assert_eq!(payload.baselines[1].value, Some(0.0));
    assert_eq!(payload.baselines[1].absolute_delta, Some(60.0));
    assert_eq!(payload.baselines[1].percent_delta, None);
    assert_eq!(payload.baselines[1].recorded_day_count, 1);
    assert!(payload.baselines[1].is_valid);
    assert!(
        payload.baselines[1]
            .evidence_ids
            .contains(&"previousEqualLength.summary.activeSeconds".to_string())
    );
    assert!(
        payload
            .evidence
            .iter()
            .any(|item| { item.id == "previousEqualLength.quality.valid" && item.value == 1.0 })
    );
    assert!(
        serde_json::to_string(&payload)
            .unwrap()
            .to_ascii_lowercase()
            .find("inf")
            .is_none()
    );
}

#[test]
fn unsampled_current_series_is_none_and_references_only_quality_evidence() {
    let service = AppService::new(Database::open_in_memory().unwrap());
    let payload = service
        .get_trend_workbench(request(
            "2024-04-10",
            "2024-04-10",
            Some(TrendGranularity::Day),
            TrendMetric::ActiveSeconds,
        ))
        .unwrap();
    let current = &payload.baselines[0];

    assert_eq!(current.kind, TrendBaselineKind::Current);
    assert!(!current.is_valid);
    assert_eq!(current.value, None);
    assert_eq!(current.recorded_day_count, 0);
    assert_eq!(current.missing_day_count, 1);
    assert_eq!(
        current.evidence_ids,
        vec![
            "current.quality.valid",
            "current.quality.recordedDayCount",
            "current.quality.missingDayCount",
        ]
    );
    assert!(payload.evidence.iter().any(|item| {
        item.id == "current.summary.activeSeconds"
            && item.metric == Some(TrendMetric::ActiveSeconds)
            && item.value == 0.0
    }));
}

#[test]
fn sampled_current_series_preserves_a_real_zero_value() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment(
            "sampled-current-zero",
            "2024-04-10",
            0,
            60_000,
            ActivityCategory::Idle,
            0.9,
        ))
        .unwrap();
    let payload = AppService::new(database)
        .get_trend_workbench(request(
            "2024-04-10",
            "2024-04-10",
            Some(TrendGranularity::Day),
            TrendMetric::ActiveSeconds,
        ))
        .unwrap();
    let current = &payload.baselines[0];

    assert!(current.is_valid);
    assert_eq!(current.value, Some(0.0));
    assert_eq!(current.recorded_day_count, 1);
    assert!(
        current
            .evidence_ids
            .contains(&"current.summary.activeSeconds".to_string())
    );
}

#[test]
fn unsampled_baseline_is_none_and_exposes_stable_quality_evidence() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment(
            "current",
            "2024-04-10",
            0,
            60_000,
            ActivityCategory::CreationDevelopment,
            0.9,
        ))
        .unwrap();
    let payload = AppService::new(database)
        .get_trend_workbench(request(
            "2024-04-10",
            "2024-04-10",
            Some(TrendGranularity::Day),
            TrendMetric::ActiveSeconds,
        ))
        .unwrap();
    let baseline = &payload.baselines[1];

    assert!(!baseline.is_valid);
    assert_eq!(baseline.value, None);
    assert_eq!(baseline.absolute_delta, None);
    assert_eq!(baseline.percent_delta, None);
    assert_eq!(baseline.recorded_day_count, 0);
    assert_eq!(baseline.missing_day_count, 1);
    assert!(
        !baseline
            .evidence_ids
            .iter()
            .any(|id| id.ends_with("activeSeconds"))
    );
    for (id, value) in [
        ("previousEqualLength.quality.valid", 0.0),
        ("previousEqualLength.quality.recordedDayCount", 0.0),
        ("previousEqualLength.quality.missingDayCount", 1.0),
    ] {
        assert!(
            payload
                .evidence
                .iter()
                .any(|item| item.id == id && item.value == value),
            "missing {id}"
        );
        assert!(baseline.evidence_ids.contains(&id.to_string()));
    }
    assert!(!payload.evidence.iter().any(|item| {
        item.series_kind == TrendBaselineKind::PreviousEqualLength
            && item.metric == Some(TrendMetric::ActiveSeconds)
    }));
}

#[test]
fn linked_task_metric_is_available_and_zero_is_valid_only_with_other_range_evidence() {
    let availability = trend_metric_availability();
    let linked = availability
        .iter()
        .find(|item| item.metric == TrendMetric::LinkedTaskSeconds)
        .unwrap();
    assert_eq!(linked.status, TrendMetricAvailabilityStatus::Available);
    assert_eq!(linked.reason_code, None);
    assert_eq!(
        serde_json::to_value(linked).unwrap(),
        serde_json::json!({
            "metric": "linkedTaskSeconds",
            "status": "available",
            "reasonCode": null
        })
    );

    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment(
            "unlinked-sampling",
            "2024-04-10",
            0,
            60_000,
            ActivityCategory::Research,
            0.9,
        ))
        .unwrap();
    let payload = AppService::new(database)
        .get_trend_workbench(request(
            "2024-04-10",
            "2024-04-10",
            Some(TrendGranularity::Day),
            TrendMetric::LinkedTaskSeconds,
        ))
        .unwrap();
    assert_eq!(
        payload
            .metric_availability
            .iter()
            .find(|item| item.metric == TrendMetric::LinkedTaskSeconds)
            .unwrap()
            .status,
        TrendMetricAvailabilityStatus::Available
    );
    assert_eq!(payload.baselines[0].is_valid, true);
    assert_eq!(payload.baselines[0].value, Some(0.0));
    assert!(
        payload
            .evidence
            .iter()
            .any(|item| { item.id == "current.summary.linkedTaskSeconds" && item.value == 0.0 })
    );

    let empty = AppService::new(Database::open_in_memory().unwrap())
        .get_trend_workbench(request(
            "2024-04-10",
            "2024-04-10",
            Some(TrendGranularity::Day),
            TrendMetric::LinkedTaskSeconds,
        ))
        .unwrap();
    assert!(!empty.baselines[0].is_valid);
    assert_eq!(empty.baselines[0].value, None);
    assert!(
        !empty.baselines[0]
            .evidence_ids
            .contains(&"current.summary.linkedTaskSeconds".to_string())
    );
}

#[test]
fn fixed_timezone_offsets_follow_javascript_get_timezone_offset_direction() {
    let east_database = Database::open_in_memory().unwrap();
    east_database
        .insert_segment(&ActivitySegmentRecord {
            started_at_ms: utc_ms("2024-04-09") + 16 * 3_600_000 + 30 * 60_000,
            ended_at_ms: utc_ms("2024-04-09") + 16 * 3_600_000 + 31 * 60_000,
            ..segment(
                "utc-plus-eight",
                "2024-04-09",
                0,
                60_000,
                ActivityCategory::CreationDevelopment,
                0.9,
            )
        })
        .unwrap();
    let mut east_request = request(
        "2024-04-10",
        "2024-04-10",
        Some(TrendGranularity::Day),
        TrendMetric::ActiveSeconds,
    );
    east_request.timezone_offset_minutes = -480;
    let east = AppService::new(east_database)
        .get_trend_workbench(east_request)
        .unwrap();
    assert_eq!(east.buckets[0].values.active_seconds, 60);

    let west_database = Database::open_in_memory().unwrap();
    west_database
        .insert_segment(&ActivitySegmentRecord {
            started_at_ms: utc_ms("2024-04-10") + 4 * 3_600_000 + 30 * 60_000,
            ended_at_ms: utc_ms("2024-04-10") + 4 * 3_600_000 + 31 * 60_000,
            ..segment(
                "utc-minus-five",
                "2024-04-10",
                0,
                60_000,
                ActivityCategory::CreationDevelopment,
                0.9,
            )
        })
        .unwrap();
    let mut west_request = request(
        "2024-04-09",
        "2024-04-09",
        Some(TrendGranularity::Day),
        TrendMetric::ActiveSeconds,
    );
    west_request.timezone_offset_minutes = 300;
    let west = AppService::new(west_database)
        .get_trend_workbench(west_request)
        .unwrap();
    assert_eq!(west.buckets[0].values.active_seconds, 60);
}

#[test]
fn statistics_use_daily_values_and_report_missing_days_with_stable_evidence() {
    let database = Database::open_in_memory().unwrap();
    for item in [
        segment(
            "day-one",
            "2024-04-10",
            0,
            60_000,
            ActivityCategory::CreationDevelopment,
            0.9,
        ),
        segment(
            "day-three",
            "2024-04-12",
            0,
            180_000,
            ActivityCategory::CreationDevelopment,
            0.9,
        ),
    ] {
        database.insert_segment(&item).unwrap();
    }
    let payload = AppService::new(database)
        .get_trend_workbench(request(
            "2024-04-10",
            "2024-04-12",
            Some(TrendGranularity::Day),
            TrendMetric::ActiveSeconds,
        ))
        .unwrap();

    let _: i64 = payload.summary.totals.active_seconds;
    assert_eq!(payload.summary.totals.active_seconds, 240);
    assert_eq!(payload.summary.mean_per_bucket.active_seconds, 80.0);
    assert_eq!(payload.summary.daily_median.active_seconds, 60.0);
    assert_eq!(payload.summary.daily_max.active_seconds, 180.0);
    assert!((payload.summary.daily_sample_stddev.active_seconds - 91.651_513_899).abs() < 1e-6);
    assert!(
        (payload
            .summary
            .daily_coefficient_of_variation
            .active_seconds
            - 1.145_643_924)
            .abs()
            < 1e-6
    );
    assert_eq!(payload.summary.recorded_day_count, 2);
    assert_eq!(payload.summary.missing_day_count, 1);
    assert_eq!(payload.summary.classification_coverage, 1.0);
    assert_eq!(payload.summary.low_confidence_seconds, 0);
    assert_eq!(payload.summary.pending_seconds, 0);
    assert!(
        payload
            .summary
            .evidence_ids
            .contains(&"current.summary.activeSeconds".to_string())
    );
    assert!(
        payload
            .evidence
            .iter()
            .any(|item| item.id == "current.summary.activeSeconds" && item.value == 240.0)
    );
    assert!(
        payload
            .buckets
            .iter()
            .all(|bucket| !bucket.evidence_ids.is_empty())
    );
    assert_eq!(
        payload
            .evidence
            .iter()
            .map(|item| item.id.as_str())
            .collect::<HashSet<_>>()
            .len(),
        payload.evidence.len(),
        "evidence IDs must be unique"
    );
    assert_eq!(payload.evidence_hash.len(), 64);
    let serialized = serde_json::to_string(&payload).unwrap();
    assert!(!serialized.contains("private title"));
}

#[test]
fn effective_activity_days_exclude_idle_only_and_task_only_recorded_days() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment(
            "active-day",
            "2024-04-10",
            0,
            60_000,
            ActivityCategory::CreationDevelopment,
            0.9,
        ))
        .unwrap();
    database
        .insert_segment(&segment(
            "idle-day",
            "2024-04-11",
            0,
            60_000,
            ActivityCategory::Idle,
            1.0,
        ))
        .unwrap();
    database
        .create_work_ledger_project(NewProject {
            id: "effective-project".into(),
            name: "Effective project".into(),
            color: "#2563eb".into(),
            description: String::new(),
            created_at_ms: utc_ms("2024-04-09"),
        })
        .unwrap();
    database
        .create_work_ledger_task(NewTask {
            id: "task-only-day".into(),
            project_id: "effective-project".into(),
            title: "Task only".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: utc_ms("2024-04-09"),
        })
        .unwrap();
    database
        .transition_work_ledger_task_status(
            "task-only-day",
            TaskStatus::Completed,
            utc_ms("2024-04-12"),
        )
        .unwrap();

    let service = AppService::new(database);
    let workbench_request = request(
        "2024-04-10",
        "2024-04-12",
        Some(TrendGranularity::Day),
        TrendMetric::ActiveSeconds,
    );
    let payload = service
        .get_trend_workbench(workbench_request.clone())
        .unwrap();
    assert_eq!(payload.summary.recorded_day_count, 3);
    assert_eq!(
        serde_json::to_value(&payload).unwrap()["summary"]["effectiveActivityDayCount"],
        1
    );

    let analysis = service
        .get_trend_research_analysis(workbench_request.clone())
        .unwrap();
    assert_eq!(analysis.status, ResearchStatus::LimitationsOnly);
    assert_eq!(
        service
            .queue_trend_research_analysis(
                workbench_request,
                1_000,
                Some(AiExecutionSnapshot {
                    execution_mode: AiExecutionMode::ApiKey,
                    executor_id: "queued-provider".into(),
                    model: "queued-model".into(),
                    evidence_hash: payload.evidence_hash,
                    created_at_ms: 1_000,
                }),
                false,
            )
            .unwrap(),
        None
    );
}

#[test]
fn daily_averages_use_every_selected_day_including_zero_record_days() {
    let database = Database::open_in_memory().unwrap();
    for item in [
        segment(
            "active-sample",
            "2024-04-10",
            0,
            120_000,
            ActivityCategory::CreationDevelopment,
            0.9,
        ),
        segment(
            "idle-sample",
            "2024-04-11",
            0,
            60_000,
            ActivityCategory::Idle,
            1.0,
        ),
    ] {
        database.insert_segment(&item).unwrap();
    }
    database
        .create_work_ledger_project(NewProject {
            id: "average-project".into(),
            name: "Average project".into(),
            color: "#2563eb".into(),
            description: String::new(),
            created_at_ms: utc_ms("2024-04-09"),
        })
        .unwrap();
    database
        .create_work_ledger_task(NewTask {
            id: "task-only-average-day".into(),
            project_id: "average-project".into(),
            title: "Task only average day".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: utc_ms("2024-04-09"),
        })
        .unwrap();
    database
        .transition_work_ledger_task_status(
            "task-only-average-day",
            TaskStatus::Completed,
            utc_ms("2024-04-12"),
        )
        .unwrap();

    let payload = AppService::new(database)
        .get_trend_workbench(request(
            "2024-04-10",
            "2024-04-12",
            Some(TrendGranularity::Day),
            TrendMetric::ActiveSeconds,
        ))
        .unwrap();

    assert_eq!(payload.summary.recorded_day_count, 3);
    assert_eq!(payload.summary.average_sample_day_count, 3);
    assert_eq!(payload.summary.daily_average.monitored_seconds, Some(60.0));
    assert_eq!(payload.summary.daily_average.active_seconds, Some(40.0));
    assert_eq!(payload.summary.daily_average.idle_seconds, Some(20.0));
    assert_eq!(payload.summary.daily_average.learning_seconds, Some(40.0));
    assert_eq!(payload.summary.switches_per_active_hour, Some(0.0));
    assert!(
        payload
            .summary
            .evidence_ids
            .contains(&"current.summary.dailyAverage.activeSeconds".to_string())
    );
}

#[test]
fn switches_per_active_hour_is_null_when_the_range_has_no_active_seconds() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment(
            "idle-only",
            "2024-04-10",
            0,
            120_000,
            ActivityCategory::Idle,
            1.0,
        ))
        .unwrap();

    let payload = AppService::new(database)
        .get_trend_workbench(request(
            "2024-04-10",
            "2024-04-10",
            Some(TrendGranularity::Day),
            TrendMetric::ActiveSeconds,
        ))
        .unwrap();

    assert_eq!(payload.summary.average_sample_day_count, 1);
    assert_eq!(payload.summary.switches_per_active_hour, None);
}

#[test]
fn activity_clipping_is_start_inclusive_end_exclusive_and_floors_each_day_piece() {
    let database = Database::open_in_memory().unwrap();
    let day_boundary = utc_ms("2024-04-11");
    database
        .insert_segment(&ActivitySegmentRecord {
            started_at_ms: day_boundary - 1_500,
            ended_at_ms: day_boundary + 1_500,
            ..segment(
                "cross-boundary",
                "2024-04-10",
                0,
                1_000,
                ActivityCategory::CreationDevelopment,
                0.9,
            )
        })
        .unwrap();
    database
        .insert_segment(&segment(
            "starts-at-exclusive-end",
            "2024-04-12",
            0,
            60_000,
            ActivityCategory::CreationDevelopment,
            0.9,
        ))
        .unwrap();

    let payload = AppService::new(database)
        .get_trend_workbench(request(
            "2024-04-10",
            "2024-04-11",
            Some(TrendGranularity::Day),
            TrendMetric::ActiveSeconds,
        ))
        .unwrap();

    assert_eq!(payload.buckets[0].values.active_seconds, 1);
    assert_eq!(payload.buckets[1].values.active_seconds, 1);
    assert_eq!(payload.summary.totals.active_seconds, 2);
    assert_eq!(
        payload
            .buckets
            .iter()
            .map(|bucket| bucket.values.active_seconds)
            .sum::<i64>(),
        payload.summary.totals.active_seconds
    );
}

#[test]
fn quality_totals_preserve_pending_and_low_confidence_integer_seconds() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment(
            "pending-low-confidence",
            "2024-04-10",
            0,
            61_500,
            ActivityCategory::Pending,
            0.6,
        ))
        .unwrap();
    let payload = AppService::new(database)
        .get_trend_workbench(request(
            "2024-04-10",
            "2024-04-10",
            Some(TrendGranularity::Day),
            TrendMetric::ClassificationCoverage,
        ))
        .unwrap();

    assert_eq!(payload.summary.totals.monitored_seconds, 61);
    assert_eq!(payload.summary.pending_seconds, 61);
    assert_eq!(payload.summary.low_confidence_seconds, 61);
    assert_eq!(payload.summary.classification_coverage, 0.0);
    assert_eq!(payload.baselines[0].value, Some(0.0));
}

#[test]
fn completed_tasks_use_start_inclusive_end_exclusive_and_day_boundary_buckets() {
    let database = Database::open_in_memory().unwrap();
    database
        .create_work_ledger_project(NewProject {
            id: "project".into(),
            name: "Project".into(),
            color: "#2563eb".into(),
            description: String::new(),
            created_at_ms: utc_ms("2024-04-09"),
        })
        .unwrap();
    let start_ms = utc_ms("2024-04-10");
    let end_ms = utc_ms("2024-04-12");
    for (index, completed_at_ms) in [start_ms, start_ms + DAY_MS, end_ms - 1, end_ms]
        .into_iter()
        .enumerate()
    {
        let id = format!("task-{index}");
        database
            .create_work_ledger_task(NewTask {
                id: id.clone(),
                project_id: "project".into(),
                title: id.clone(),
                priority: TaskPriority::Medium,
                expected_output: String::new(),
                due_date: None,
                created_at_ms: start_ms - 1,
            })
            .unwrap();
        database
            .transition_work_ledger_task_status(&id, TaskStatus::Completed, completed_at_ms)
            .unwrap();
    }

    let payload = AppService::new(database)
        .get_trend_workbench(request(
            "2024-04-10",
            "2024-04-11",
            Some(TrendGranularity::Day),
            TrendMetric::CompletedTaskCount,
        ))
        .unwrap();

    assert_eq!(payload.buckets[0].values.completed_task_count, 1);
    assert_eq!(payload.buckets[1].values.completed_task_count, 2);
    assert_eq!(payload.summary.totals.completed_task_count, 3);
    assert_eq!(
        payload
            .buckets
            .iter()
            .map(|bucket| bucket.values.completed_task_count)
            .sum::<i64>(),
        payload.summary.totals.completed_task_count
    );
}

#[test]
fn task_linked_time_deduplicates_shared_evidence_and_builds_private_drilldown() {
    let path = std::env::temp_dir().join(format!(
        "daily-task-monitor-trend-drilldown-{}-{}.db",
        std::process::id(),
        utc_ms("2024-04-10")
    ));
    let _ = std::fs::remove_file(&path);
    let database = Database::open(&path).unwrap();
    database
        .create_work_ledger_project(NewProject {
            id: "project".into(),
            name: "Trend project".into(),
            color: "#2563eb".into(),
            description: String::new(),
            created_at_ms: utc_ms("2024-04-01"),
        })
        .unwrap();
    for task_id in ["task-one", "task-two"] {
        database
            .create_work_ledger_task(NewTask {
                id: task_id.into(),
                project_id: "project".into(),
                title: task_id.into(),
                priority: TaskPriority::Medium,
                expected_output: String::new(),
                due_date: None,
                created_at_ms: utc_ms("2024-04-01"),
            })
            .unwrap();
    }

    let boundary = utc_ms("2024-04-11");
    database
        .insert_segment(&ActivitySegmentRecord {
            id: "shared-cross-day".into(),
            started_at_ms: boundary - 2_000,
            ended_at_ms: boundary + 2_000,
            app: "Codex".into(),
            app_path: String::new(),
            title: "secret full window title".into(),
            category: ActivityCategory::CreationDevelopment,
            video_purpose: VideoPurpose::Unknown,
            confidence: 0.6,
            source: ClassificationSource::Ai,
            reason: "needs review".into(),
            model_version: "test".into(),
            needs_review: true,
            inactivity_reason: None,
        })
        .unwrap();
    database
        .insert_segment(&segment(
            "task-one-only",
            "2024-04-11",
            10_000,
            6_000,
            ActivityCategory::Research,
            0.95,
        ))
        .unwrap();
    database
        .insert_segment(&ActivitySegmentRecord {
            title: "another secret raw title".into(),
            ..segment(
                "unlinked-raw",
                "2024-04-10",
                20 * 3_600_000,
                4_000,
                ActivityCategory::Research,
                0.95,
            )
        })
        .unwrap();
    database
        .insert_segment(&segment(
            "previous-baseline",
            "2024-04-09",
            10_000,
            5_000,
            ActivityCategory::Research,
            0.95,
        ))
        .unwrap();
    for segment_id in ["shared-cross-day", "task-one-only", "previous-baseline"] {
        database
            .assign_work_ledger_activity(
                "task-one",
                segment_id,
                EvidenceProvenance::Manual,
                1.0,
                "confirmed",
                utc_ms("2024-04-10"),
            )
            .unwrap();
    }

    // Simulate pre-ownership history where one evidence scope remains linked to two tasks.
    let raw = Connection::open(&path).unwrap();
    raw.execute("DROP INDEX idx_task_activity_links_unique_segment", [])
        .unwrap();
    raw.execute(
        "INSERT INTO task_activity_links(
            task_id, activity_segment_id, provenance, confidence, reason, created_at_ms
         ) VALUES (?1, ?2, 'manual', 1.0, 'historical shared ownership', ?3)",
        params!["task-two", "shared-cross-day", utc_ms("2024-04-10")],
    )
    .unwrap();
    drop(raw);

    database
        .start_focus_session_for_task(
            "focus-cross-day",
            "2024-04-10",
            "Focus on task two",
            25,
            boundary - 1_000,
            Some("task-two"),
        )
        .unwrap();
    database
        .complete_focus_session("focus-cross-day", boundary + 3_000, "done")
        .unwrap();
    database
        .transition_work_ledger_task_status("task-one", TaskStatus::Completed, boundary)
        .unwrap();

    let authoritative = database
        .work_ledger_range_rollup(utc_ms("2024-04-10"), utc_ms("2024-04-12"))
        .unwrap();
    assert_eq!(authoritative.projects[0].invested_seconds, 10);
    assert_eq!(authoritative.projects[0].focus_seconds, 4);

    let payload = AppService::new(database)
        .get_trend_workbench(request(
            "2024-04-10",
            "2024-04-11",
            Some(TrendGranularity::Day),
            TrendMetric::LinkedTaskSeconds,
        ))
        .unwrap();

    assert_eq!(
        trend_metric_availability()
            .into_iter()
            .find(|item| item.metric == TrendMetric::LinkedTaskSeconds)
            .unwrap()
            .status,
        TrendMetricAvailabilityStatus::Available
    );
    assert_eq!(payload.buckets[0].values.linked_task_seconds, 2);
    assert_eq!(payload.buckets[1].values.linked_task_seconds, 9);
    assert_eq!(payload.summary.totals.linked_task_seconds, 11);
    assert_eq!(
        payload
            .buckets
            .iter()
            .map(|bucket| bucket.values.linked_task_seconds)
            .sum::<i64>(),
        payload.summary.totals.linked_task_seconds
    );
    assert_eq!(payload.baselines[1].value, Some(5.0));
    assert_eq!(payload.baselines[1].absolute_delta, Some(6.0));
    assert!(
        payload
            .evidence
            .iter()
            .any(|item| { item.id == "current.summary.linkedTaskSeconds" && item.value == 11.0 })
    );

    let first = &payload.buckets[0].drilldown;
    assert_eq!(first.bucket_id, payload.buckets[0].id);
    assert_eq!(first.raw_rows.len(), 4);
    assert_eq!(
        first
            .raw_rows
            .iter()
            .filter(|row| row.evidence_id == "shared-cross-day")
            .count(),
        2
    );
    assert!(
        first
            .raw_rows
            .iter()
            .filter(|row| { row.evidence_id == "shared-cross-day" })
            .all(|row| row.shared)
    );
    assert!(first.raw_rows.iter().any(|row| {
        row.evidence_id == "shared-cross-day"
            && row.clipped_duration_seconds == 2
            && row.review_state == TrendReviewState::Pending
    }));
    assert!(
        first.raw_rows.iter().any(|row| {
            row.evidence_id == "focus-cross-day" && row.clipped_duration_seconds == 1
        })
    );
    assert!(first.raw_rows.iter().any(|row| {
        row.evidence_id == "unlinked-raw"
            && row.task_id.is_none()
            && row.project_id.is_none()
            && row.clipped_duration_seconds == 4
    }));
    assert_eq!(
        first
            .application_distribution
            .iter()
            .map(|item| item.seconds)
            .sum::<i64>(),
        6
    );
    assert_eq!(
        first
            .category_distribution
            .iter()
            .map(|item| item.seconds)
            .sum::<i64>(),
        6
    );
    assert_eq!(first.data_quality.recorded_day_count, 1);
    assert_eq!(first.data_quality.pending_seconds, 0);
    assert_eq!(first.data_quality.low_confidence_seconds, 2);
    assert_eq!(first.workflow_ownership.len(), 3);

    let second = &payload.buckets[1].drilldown;
    assert_eq!(second.completed_tasks.len(), 1);
    assert_eq!(second.completed_tasks[0].task_id, "task-one");
    assert_eq!(second.linked_project_rollups.len(), 1);
    assert_eq!(second.linked_project_rollups[0].linked_seconds, 9);
    assert_eq!(second.linked_task_rollups.len(), 2);
    assert!(second.linked_task_rollups.iter().any(|item| {
        item.task_id == "task-one" && item.linked_seconds == 8 && item.shared_evidence_count == 1
    }));
    assert!(second.linked_task_rollups.iter().any(|item| {
        item.task_id == "task-two"
            && item.linked_seconds == 3
            && item.activity_seconds == 2
            && item.focus_seconds == 3
            && item.shared_evidence_count == 1
    }));

    let serialized = serde_json::to_string(&payload).unwrap();
    assert!(!serialized.contains("secret full window title"));
    assert!(!serialized.contains("another secret raw title"));
    assert!(!serialized.contains("private title must not enter trend evidence"));
    let row_ids = payload
        .buckets
        .iter()
        .flat_map(|bucket| {
            bucket
                .drilldown
                .raw_rows
                .iter()
                .map(|row| row.row_id.as_str())
        })
        .collect::<Vec<_>>();
    let mut sorted_row_ids = row_ids.clone();
    sorted_row_ids.sort_unstable();
    assert_eq!(
        row_ids, sorted_row_ids,
        "raw row order and IDs must be stable"
    );

    drop(payload);
    let _ = std::fs::remove_file(path);
}

#[test]
fn workbench_request_and_enums_serialize_with_stable_camel_case_contract() {
    let mut workbench_request = request(
        "2024-04-10",
        "2024-04-12",
        Some(TrendGranularity::Week),
        TrendMetric::LongestFocusSeconds,
    );
    workbench_request.custom_baseline = Some(TrendDateRange {
        start_date: "2024-03-10".into(),
        end_date: "2024-03-12".into(),
    });
    let value = serde_json::to_value(workbench_request).unwrap();

    assert_eq!(
        value,
        serde_json::json!({
            "startDate": "2024-04-10",
            "endDate": "2024-04-12",
            "timezoneOffsetMinutes": 0,
            "granularity": "week",
            "metric": "longestFocusSeconds",
            "customBaseline": {
                "startDate": "2024-03-10",
                "endDate": "2024-03-12"
            }
        })
    );
    assert_eq!(
        serde_json::to_value(TrendBaselineKind::PreviousEqualLength).unwrap(),
        serde_json::json!("previousEqualLength")
    );
    assert_eq!(
        serde_json::to_value(TrendBaselineKind::PreviousMonthSamePeriod).unwrap(),
        serde_json::json!("previousMonthSamePeriod")
    );
    assert_eq!(
        serde_json::to_value(TrendRawEvidenceKind::Activity).unwrap(),
        serde_json::json!("activity")
    );
    assert_eq!(
        serde_json::to_value(TrendReviewState::Pending).unwrap(),
        serde_json::json!("pending")
    );
}

#[test]
fn range_accepts_366_days_rejects_367_and_legacy_get_trends_stays_compatible() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&segment(
            "legacy-evidence",
            "2024-04-10",
            10_000,
            1_500,
            ActivityCategory::CreationDevelopment,
            0.9,
        ))
        .unwrap();
    database
        .create_work_ledger_project(NewProject {
            id: "legacy-project".into(),
            name: "Legacy project".into(),
            color: "#2563eb".into(),
            description: String::new(),
            created_at_ms: utc_ms("2024-04-09"),
        })
        .unwrap();
    database
        .create_work_ledger_task(NewTask {
            id: "legacy-task".into(),
            project_id: "legacy-project".into(),
            title: "Legacy task".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: utc_ms("2024-04-09"),
        })
        .unwrap();
    database
        .assign_work_ledger_activity(
            "legacy-task",
            "legacy-evidence",
            EvidenceProvenance::Manual,
            1.0,
            "confirmed",
            utc_ms("2024-04-10"),
        )
        .unwrap();
    database
        .transition_work_ledger_task_status(
            "legacy-task",
            TaskStatus::Completed,
            utc_ms("2024-04-10"),
        )
        .unwrap();
    let service = AppService::new(database);
    let legacy_before = service
        .get_trends(
            "2024-04-10",
            "2024-04-10",
            vec![utc_ms("2024-04-10"), utc_ms("2024-04-10") + DAY_MS],
            "2024-04-09",
            "2024-04-09",
            vec![utc_ms("2024-04-09"), utc_ms("2024-04-10")],
        )
        .unwrap();
    let payload = service
        .get_trend_workbench(request(
            "2024-01-01",
            "2024-12-31",
            None,
            TrendMetric::CompletedTaskCount,
        ))
        .unwrap();
    assert_eq!(payload.range.day_count, 366);
    assert_eq!(payload.granularity, TrendGranularity::Month);
    assert!(
        service
            .get_trend_workbench(request(
                "2024-01-01",
                "2025-01-01",
                None,
                TrendMetric::ActiveSeconds,
            ))
            .is_err()
    );

    let legacy_after = service
        .get_trends(
            "2024-04-10",
            "2024-04-10",
            vec![utc_ms("2024-04-10"), utc_ms("2024-04-10") + DAY_MS],
            "2024-04-09",
            "2024-04-09",
            vec![utc_ms("2024-04-09"), utc_ms("2024-04-10")],
        )
        .unwrap();
    assert_eq!(legacy_after, legacy_before);
    assert_eq!(legacy_after.days[0].monitored_seconds, 1);
    assert_eq!(legacy_after.days[0].completed_task_count, 1);
    assert_eq!(legacy_after.work_ledger.tasks.len(), 1);
    assert_eq!(legacy_after.evidence_hash.len(), 64);
}
