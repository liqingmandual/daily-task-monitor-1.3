use daily_task_monitor_core::context::{
    ExternalContextKind, parse_ics_calendar, parse_project_context_json,
};
use daily_task_monitor_core::db::Database;

#[test]
fn imported_context_stays_local_and_refreshes_per_source() {
    let database = Database::open_in_memory().unwrap();
    let calendar = parse_ics_calendar(
        "calendar-work",
        "Work Calendar",
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:event-1\r\nSUMMARY:Planning\r\nDTSTART:20260813T010000Z\r\nDTEND:20260813T013000Z\r\nEND:VEVENT\r\nEND:VCALENDAR",
        10,
    )
    .unwrap();
    assert_eq!(database.import_external_context(&calendar).unwrap(), 1);

    let projects = parse_project_context_json(
        r#"{"sourceId":"linear-export","sourceName":"Linear","projects":[{"id":"p1","name":"Orbit","tasks":[{"id":"t1","title":"Sync context","status":"started"}]}]}"#,
        11,
    )
    .unwrap();
    assert_eq!(database.import_external_context(&projects).unwrap(), 2);

    let items = database
        .list_external_context(1_786_580_000_000, 1_786_590_000_000)
        .unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].kind, ExternalContextKind::CalendarEvent);
    assert_eq!(items[1].kind, ExternalContextKind::Project);
    assert_eq!(items[2].kind, ExternalContextKind::Task);

    let refreshed = parse_project_context_json(
        r#"{"sourceId":"linear-export","sourceName":"Linear","projects":[{"id":"p2","name":"Orbit P1","tasks":[]}]}"#,
        12,
    )
    .unwrap();
    database.import_external_context(&refreshed).unwrap();
    let items = database.list_external_context(0, i64::MAX).unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|item| item.title == "Orbit P1"));
    assert!(!items.iter().any(|item| item.title == "Sync context"));
}
