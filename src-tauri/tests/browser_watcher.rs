use daily_task_monitor_core::browser_watcher::{
    BROWSER_WATCHER_PROTOCOL_VERSION, BrowserHeartbeat, BrowserWatcherEngine,
};
use daily_task_monitor_core::db::Database;

#[test]
fn measured_browser_slices_round_trip_with_provenance_and_duration() {
    let database = Database::open_in_memory().unwrap();
    let mut watcher = BrowserWatcherEngine::default();
    let heartbeat = |captured_at_ms| BrowserHeartbeat {
        protocol_version: BROWSER_WATCHER_PROTOCOL_VERSION,
        source_id: "chrome-default".into(),
        browser: "Chrome".into(),
        profile: "Default".into(),
        tab_id: "7".into(),
        captured_at_ms,
        url: "https://example.com/work".into(),
        title: "Work".into(),
        active: true,
        private: false,
    };

    watcher.ingest(heartbeat(10_000), 10_000, &[]).unwrap();
    let slice = watcher
        .ingest(heartbeat(40_000), 40_000, &[])
        .unwrap()
        .unwrap();
    assert!(database.insert_browser_activity_slice(&slice).unwrap());
    assert!(!database.insert_browser_activity_slice(&slice).unwrap());

    assert_eq!(
        database.browser_activity_summary(0, 60_000).unwrap(),
        (1, 30_000, Some(40_000))
    );
}
