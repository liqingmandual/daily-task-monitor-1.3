use daily_task_monitor_core::db::Database;
use daily_task_monitor_core::sync::build_sync_event;
use daily_task_monitor_core::work_ledger::{NewProject, NewTask, TaskPriority};

#[test]
fn local_device_and_append_only_events_are_stable_and_idempotent() {
    let database = Database::open_in_memory().unwrap();
    let device_id = database.local_sync_device_id(1_000).unwrap();
    assert!(device_id.starts_with("device-"));
    assert_eq!(database.local_sync_device_id(2_000).unwrap(), device_id);

    let first = database
        .append_local_sync_event(
            2_000,
            "classification",
            "segment-1",
            "correct",
            r#"{"category":"research"}"#,
        )
        .unwrap();
    let second = database
        .append_local_sync_event(
            2_001,
            "classification_rule",
            "rule-1",
            "create",
            r#"{"kind":"app_title"}"#,
        )
        .unwrap();
    assert_eq!(first.device_id, device_id);
    assert_eq!(first.sequence, 1);
    assert_eq!(second.sequence, 2);

    let remote = build_sync_event(
        "device-remote",
        1,
        1_999,
        "annotation",
        "annotation-1",
        "upsert",
        r#"{"title":"Write tests"}"#,
    )
    .unwrap();
    assert_eq!(
        database
            .import_sync_events(vec![remote.clone()], 3_000)
            .unwrap(),
        1
    );
    assert_eq!(database.import_sync_events(vec![remote], 3_001).unwrap(), 0);

    let events = database.list_sync_events().unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].device_id, "device-remote");
    assert_eq!(events[1].event_id, first.event_id);
    assert_eq!(events[2].event_id, second.event_id);
}

#[test]
fn import_rejects_a_device_sequence_collision() {
    let database = Database::open_in_memory().unwrap();
    let first = build_sync_event(
        "device-remote",
        1,
        1_000,
        "annotation",
        "annotation-1",
        "upsert",
        r#"{"title":"First"}"#,
    )
    .unwrap();
    let conflicting = build_sync_event(
        "device-remote",
        1,
        1_001,
        "annotation",
        "annotation-1",
        "upsert",
        r#"{"title":"Conflicting"}"#,
    )
    .unwrap();
    assert_eq!(database.import_sync_events(vec![first], 2_000).unwrap(), 1);
    assert!(
        database
            .import_sync_events(vec![conflicting], 2_001)
            .is_err()
    );
    assert_eq!(database.list_sync_events().unwrap().len(), 1);
}

#[test]
fn organization_events_project_deterministically_into_local_tables() {
    let database = Database::open_in_memory().unwrap();
    let project = build_sync_event(
        "device-a",
        1,
        1_000,
        "project",
        "project-1",
        "upsert",
        r##"{"id":"project-1","name":"Orbit","color":"#2563eb","status":"active","description":"Local-first","createdAtMs":1000,"updatedAtMs":1000,"archivedAtMs":null}"##,
    )
    .unwrap();
    let task = build_sync_event(
        "device-a",
        2,
        1_001,
        "task",
        "task-1",
        "upsert",
        r#"{"id":"task-1","projectId":"project-1","title":"Implement sync","status":"todo","priority":"high","expectedOutput":"Green tests","dueDate":null,"createdAtMs":1001,"updatedAtMs":1001,"completedAtMs":null,"originKind":"manual","originKey":null,"originConfidence":null,"reviewState":"confirmed"}"#,
    )
    .unwrap();
    database
        .import_sync_events(vec![task.clone(), project.clone()], 2_000)
        .unwrap();
    assert_eq!(
        database
            .get_work_ledger_project("project-1")
            .unwrap()
            .unwrap()
            .name,
        "Orbit"
    );
    assert_eq!(
        database
            .get_work_ledger_task("task-1")
            .unwrap()
            .unwrap()
            .title,
        "Implement sync"
    );

    let completed = build_sync_event(
        "device-b",
        1,
        2_000,
        "task",
        "task-1",
        "upsert",
        r#"{"id":"task-1","projectId":"project-1","title":"Implement sync","status":"completed","priority":"high","expectedOutput":"Green tests","dueDate":null,"createdAtMs":1001,"updatedAtMs":2000,"completedAtMs":2000,"originKind":"manual","originKey":null,"originConfidence":null,"reviewState":"confirmed"}"#,
    )
    .unwrap();
    database.import_sync_events(vec![completed], 3_000).unwrap();
    assert_eq!(
        database
            .get_work_ledger_task("task-1")
            .unwrap()
            .unwrap()
            .status
            .as_str(),
        "completed"
    );
}

#[test]
fn first_export_bootstraps_legacy_organization_entities_once() {
    let database = Database::open_in_memory().unwrap();
    database
        .create_work_ledger_project(NewProject {
            id: "legacy-project".into(),
            name: "Legacy".into(),
            color: "#2563eb".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    database
        .create_work_ledger_task(NewTask {
            id: "legacy-task".into(),
            project_id: "legacy-project".into(),
            title: "Existing task".into(),
            priority: TaskPriority::Medium,
            expected_output: String::new(),
            due_date: None,
            created_at_ms: 1_001,
        })
        .unwrap();

    assert_eq!(database.ensure_organization_sync_snapshot().unwrap(), 2);
    assert_eq!(database.ensure_organization_sync_snapshot().unwrap(), 0);
    let events = database.list_sync_events().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].entity_kind, "project");
    assert_eq!(events[1].entity_kind, "task");
}
