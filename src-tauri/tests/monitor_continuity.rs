use daily_task_monitor_core::db::{
    ActivitySampleWrite, ActivitySegmentRecord, Database, MonitoringContinuityCheckpoint,
};
use daily_task_monitor_core::domain::{
    ActivityCategory, ClassificationSource, InactivityReason, VideoPurpose,
};
use daily_task_monitor_core::monitor_continuity::{
    continuity_gap_segment, monitoring_gap_from_checkpoint,
};

fn active_segment(id: &str, started_at_ms: i64, ended_at_ms: i64) -> ActivitySegmentRecord {
    ActivitySegmentRecord {
        id: id.into(),
        started_at_ms,
        ended_at_ms,
        app: "Code".into(),
        app_path: String::new(),
        title: "IELTS study helper".into(),
        category: ActivityCategory::CreationDevelopment,
        video_purpose: VideoPurpose::Unknown,
        confidence: 0.95,
        source: ClassificationSource::Rule,
        reason: "fixture".into(),
        model_version: "rules-v1".into(),
        needs_review: false,
        inactivity_reason: None,
    }
}

#[test]
fn expected_monitoring_gap_becomes_continuity_inactivity_but_manual_pause_does_not() {
    let checkpoint = MonitoringContinuityCheckpoint {
        expected_tracking: true,
        last_observed_at_ms: 5_000,
        last_boot_started_at_ms: 0,
        last_uptime_ms: 5_000,
        updated_at_ms: 5_000,
    };

    let gap = monitoring_gap_from_checkpoint(&checkpoint, 65_000, 0, 65_000, 15_000)
        .expect("a sixty second monitoring gap should be backfilled");
    assert_eq!(gap.started_at_ms, 5_000);
    assert_eq!(gap.ended_at_ms, 65_000);
    assert_eq!(gap.reason, InactivityReason::ContinuityGap);
    let segment = continuity_gap_segment(&gap);
    assert_eq!(segment.started_at_ms, 5_000);
    assert_eq!(segment.ended_at_ms, 65_000);
    assert_eq!(segment.category, ActivityCategory::Idle);
    assert_eq!(
        segment.inactivity_reason,
        Some(InactivityReason::ContinuityGap)
    );

    let paused = MonitoringContinuityCheckpoint {
        expected_tracking: false,
        ..checkpoint
    };
    assert!(
        monitoring_gap_from_checkpoint(&paused, 65_000, 0, 65_000, 15_000).is_none(),
        "an explicit monitoring pause must never be backfilled"
    );
}

#[test]
fn continuity_checkpoint_round_trips_the_expected_monitoring_state() {
    let database = Database::open_in_memory().unwrap();
    let checkpoint = MonitoringContinuityCheckpoint {
        expected_tracking: true,
        last_observed_at_ms: 80_000,
        last_boot_started_at_ms: 1_000,
        last_uptime_ms: 79_000,
        updated_at_ms: 80_000,
    };
    database
        .save_monitoring_continuity_checkpoint(&checkpoint)
        .unwrap();
    assert_eq!(
        database.load_monitoring_continuity_checkpoint().unwrap(),
        Some(checkpoint)
    );
}

#[test]
fn monitoring_tick_persists_sample_segments_and_checkpoint_together() {
    let database = Database::open_in_memory().unwrap();
    let checkpoint = MonitoringContinuityCheckpoint {
        expected_tracking: true,
        last_observed_at_ms: 10_000,
        last_boot_started_at_ms: 0,
        last_uptime_ms: 10_000,
        updated_at_ms: 10_000,
    };
    database
        .record_monitoring_tick(
            &ActivitySampleWrite {
                id: "sample-10000".into(),
                sampled_at_ms: 10_000,
                app: "Code".into(),
                app_path: String::new(),
                title: "IELTS study helper".into(),
                idle_seconds: 0,
                key_presses: 1,
                mouse_events: 0,
                media_playing: false,
            },
            &[active_segment("active", 10_000, 10_000)],
            &checkpoint,
        )
        .unwrap();

    assert_eq!(
        database.load_monitoring_continuity_checkpoint().unwrap(),
        Some(checkpoint)
    );
    assert_eq!(database.list_segments(0, 10_001).unwrap().len(), 1);
    assert_eq!(
        database
            .repair_recent_monitoring_gaps(10_000, 10_000, 15_000)
            .unwrap()
            .repaired_gap_count,
        0,
        "the observation sample should be committed with its checkpoint"
    );
}

#[test]
fn recent_gap_repair_splits_bridged_activity_and_inserts_one_idle_interval() {
    let database = Database::open_in_memory().unwrap();
    for sampled_at_ms in [0_i64, 5_000, 65_000, 70_000] {
        database
            .insert_activity_sample(
                &format!("sample-{sampled_at_ms}"),
                sampled_at_ms,
                "Code",
                "",
                "IELTS study helper",
                0,
                0,
                0,
                false,
            )
            .unwrap();
    }
    database
        .insert_segment(&active_segment("bridged", 0, 70_000))
        .unwrap();

    let repaired = database
        .repair_recent_monitoring_gaps(70_000, 70_000, 15_000)
        .unwrap();
    assert_eq!(repaired.repaired_gap_count, 1);
    assert_eq!(repaired.repaired_seconds, 60);

    let mut segments = database.list_segments(0, 70_001).unwrap();
    segments.sort_by_key(|segment| (segment.started_at_ms, segment.id.clone()));
    assert_eq!(segments.len(), 3);
    assert_eq!(
        (
            segments[0].started_at_ms,
            segments[0].ended_at_ms,
            segments[0].category,
        ),
        (0, 5_000, ActivityCategory::CreationDevelopment)
    );
    assert_eq!(
        (
            segments[1].started_at_ms,
            segments[1].ended_at_ms,
            segments[1].category,
            segments[1].inactivity_reason,
        ),
        (
            5_000,
            65_000,
            ActivityCategory::Idle,
            Some(InactivityReason::LegacyGapRepair),
        )
    );
    assert_eq!(
        (
            segments[2].started_at_ms,
            segments[2].ended_at_ms,
            segments[2].category,
        ),
        (65_000, 70_000, ActivityCategory::CreationDevelopment)
    );

    let second_pass = database
        .repair_recent_monitoring_gaps(70_000, 70_000, 15_000)
        .unwrap();
    assert_eq!(second_pass.repaired_gap_count, 0);
    let mut second_segments = database.list_segments(0, 70_001).unwrap();
    second_segments.sort_by_key(|segment| (segment.started_at_ms, segment.id.clone()));
    assert_eq!(second_segments, segments);
}
