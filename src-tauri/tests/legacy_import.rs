use daily_task_monitor_core::db::Database;
use daily_task_monitor_core::legacy::import_activity_jsonl;

fn unique_temp_file() -> std::path::PathBuf {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("daily-task-monitor-legacy-{suffix}.jsonl"))
}

#[test]
fn imports_legacy_activity_without_modifying_the_source() {
    let source = unique_temp_file();
    let content = concat!(
        r#"{"start":"2026-07-10T08:00:00+08:00","end":"2026-07-10T08:10:00+08:00","durationSeconds":600,"app":"chrome","title":"psychology - Google Search","category":"搜索/调研","isIdle":false}"#,
        "\n",
        r#"{"start":"2026-07-10T08:10:00+08:00","end":"2026-07-10T08:20:00+08:00","durationSeconds":600,"app":"WeChat","title":"微信","category":"沟通","isIdle":false}"#,
        "\n"
    );
    std::fs::write(&source, content).unwrap();
    let before = std::fs::read(&source).unwrap();
    let db = Database::open_in_memory().unwrap();

    let first = import_activity_jsonl(&db, &source).unwrap();
    let second = import_activity_jsonl(&db, &source).unwrap();

    assert_eq!(first.imported, 2);
    assert_eq!(second.imported, 0);
    assert_eq!(db.segment_count().unwrap(), 2);
    assert_eq!(std::fs::read(&source).unwrap(), before);
    assert_eq!(
        db.segment_origin(&first.segment_ids[0]).unwrap().as_deref(),
        Some("legacy")
    );
    std::fs::remove_file(source).unwrap();
}
