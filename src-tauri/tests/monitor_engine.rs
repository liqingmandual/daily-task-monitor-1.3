use daily_task_monitor_core::domain::ActivityCategory;
use daily_task_monitor_core::monitor::{MonitorEngine, MonitorSample};

fn sample(at_ms: i64, last_input_ms: i64, app: &str, media_playing: bool) -> MonitorSample {
    MonitorSample {
        observed_at_ms: at_ms,
        last_input_at_ms: last_input_ms,
        app: app.into(),
        app_path: format!(r"C:\Apps\{app}.exe"),
        title: "Window".into(),
        domain: String::new(),
        key_presses: 1,
        mouse_events: 0,
        media_playing,
    }
}

#[test]
fn executable_path_is_part_of_the_foreground_segment_identity() {
    let mut engine = MonitorEngine::new(6 * 60 * 1_000);
    let first = engine.ingest(MonitorSample {
        app_path: r"C:\Stable\Editor.exe".into(),
        ..sample(1_000, 1_000, "Editor", false)
    });
    assert_eq!(first.current.app_path, r"C:\Stable\Editor.exe");

    let switched = engine.ingest(MonitorSample {
        app_path: r"D:\Portable\Editor.exe".into(),
        ..sample(6_000, 6_000, "Editor", false)
    });

    assert_eq!(switched.completed.len(), 1);
    assert_eq!(switched.completed[0].app_path, r"C:\Stable\Editor.exe");
    assert_eq!(switched.current.app_path, r"D:\Portable\Editor.exe");
}

#[test]
fn sustained_reading_reclassifies_the_same_live_segment_in_place() {
    let mut engine = MonitorEngine::new(6 * 60 * 1_000);
    let first = engine.ingest(MonitorSample {
        title: "Research paper.pdf".into(),
        key_presses: 0,
        ..sample(0, 0, "Acrobat", false)
    });
    assert_eq!(first.current.category, ActivityCategory::Pending);

    let later = engine.ingest(MonitorSample {
        title: "Research paper.pdf".into(),
        key_presses: 0,
        ..sample(125_000, 125_000, "Acrobat", false)
    });

    assert!(later.completed.is_empty());
    assert_eq!(later.current.category, ActivityCategory::TextInput);
    assert_eq!(later.current.started_at_ms, 0);
    assert_eq!(later.current.id, first.current.id);
}

#[test]
fn browser_domain_is_available_to_local_rules() {
    let mut engine = MonitorEngine::new(6 * 60 * 1_000);
    let output = engine.ingest(MonitorSample {
        domain: "bing.com".into(),
        ..sample(1_000, 1_000, "msedge", false)
    });

    assert_eq!(output.current.category, ActivityCategory::Research);
}

#[test]
fn idle_segment_starts_at_the_last_input_instead_of_the_detection_time() {
    let mut engine = MonitorEngine::new(6 * 60 * 1_000);
    engine.ingest(sample(0, 0, "Codex", false));
    let output = engine.ingest(sample(6 * 60 * 1_000, 0, "Codex", false));

    assert_eq!(output.completed.len(), 1);
    assert_eq!(
        output.completed[0].category,
        ActivityCategory::CreationDevelopment
    );
    assert_eq!(output.completed[0].ended_at_ms, 0);
    assert_eq!(output.current.category, ActivityCategory::Idle);
    assert_eq!(output.current.started_at_ms, 0);
}

#[test]
fn playback_keeps_a_foreground_video_active_without_input() {
    let mut engine = MonitorEngine::new(6 * 60 * 1_000);
    engine.ingest(MonitorSample {
        title: "Lecture - YouTube".into(),
        ..sample(0, 0, "Chrome", true)
    });
    let output = engine.ingest(MonitorSample {
        title: "Lecture - YouTube".into(),
        ..sample(10 * 60 * 1_000, 0, "Chrome", true)
    });

    assert_eq!(output.current.category, ActivityCategory::VideoInput);
    assert_eq!(output.current.ended_at_ms, 10 * 60 * 1_000);
}

#[test]
fn pausing_video_restarts_the_idle_clock_instead_of_erasing_watch_time() {
    let mut engine = MonitorEngine::new(6 * 60 * 1_000);
    engine.ingest(MonitorSample {
        title: "Lecture - YouTube".into(),
        ..sample(0, 0, "Chrome", true)
    });
    engine.ingest(MonitorSample {
        title: "Lecture - YouTube".into(),
        ..sample(10 * 60 * 1_000, 0, "Chrome", true)
    });
    let paused = engine.ingest(MonitorSample {
        title: "Lecture - YouTube".into(),
        ..sample(10 * 60 * 1_000 + 5_000, 0, "Chrome", false)
    });
    assert_ne!(paused.current.category, ActivityCategory::Idle);

    let idle = engine.ingest(MonitorSample {
        title: "Lecture - YouTube".into(),
        ..sample(16 * 60 * 1_000 + 5_000, 0, "Chrome", false)
    });
    assert_eq!(idle.completed[0].ended_at_ms, 10 * 60 * 1_000 + 5_000);
    assert_eq!(idle.current.category, ActivityCategory::Idle);
    assert_eq!(idle.current.started_at_ms, 10 * 60 * 1_000 + 5_000);
}

#[test]
fn a_foreground_switch_finishes_the_previous_segment_once() {
    let mut engine = MonitorEngine::new(6 * 60 * 1_000);
    engine.ingest(sample(1_000, 1_000, "Codex", false));
    let output = engine.ingest(sample(6_000, 6_000, "Explorer", false));

    assert_eq!(output.completed.len(), 1);
    assert_eq!(output.completed[0].app, "Codex");
    assert_eq!(output.completed[0].ended_at_ms, 6_000);
    assert_eq!(output.current.app, "Explorer");
}

#[test]
fn pausing_clears_the_live_segment_so_resume_does_not_bridge_the_gap() {
    let mut engine = MonitorEngine::new(6 * 60 * 1_000);
    engine.ingest(sample(1_000, 1_000, "Codex", false));
    let paused = engine.take_current().unwrap();
    assert_eq!(paused.ended_at_ms, 1_000);

    let resumed = engine.ingest(sample(10 * 60 * 1_000, 10 * 60 * 1_000, "Codex", false));
    assert!(resumed.completed.is_empty());
    assert_eq!(resumed.current.started_at_ms, 10 * 60 * 1_000);
}
