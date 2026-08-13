use daily_task_monitor_core::ai::{AiExecutionMode, AiExecutionSnapshot};
use daily_task_monitor_core::browser::BrowserVisit;
use daily_task_monitor_core::db::Database;
use daily_task_monitor_core::work_ledger::{
    EvidenceProvenance, NewProgressEntry, NewProject, NewTask, ProjectStatus, ProjectUpdate,
    TaskPriority, TaskStatus, TaskUpdate, WorkLedgerRepository,
};
use rusqlite::Connection;

const LEDGER_TABLES: [&str; 8] = [
    "projects",
    "tasks",
    "task_activity_links",
    "task_browser_links",
    "task_progress_entries",
    "work_ledger_ai_suggestions",
    "daily_goal_task_links",
    "focus_sessions",
];

#[test]
fn active_focus_session_is_recoverable_until_completion() {
    let database = Database::open_in_memory().unwrap();
    database
        .start_focus_session_for_task(
            "focus-active",
            "2026-08-13",
            "Ship the countdown",
            25,
            1_000,
            None,
        )
        .unwrap();

    let active = database.active_focus_session().unwrap().unwrap();
    assert_eq!(active.id, "focus-active");
    assert_eq!(active.planned_minutes, 25);
    assert_eq!(active.task_id, None);
    assert_eq!(active.paused_at_ms, None);
    assert!(
        !database
            .start_focus_session_for_task_if_none(
                "focus-duplicate",
                "2026-08-13",
                "Must reuse active",
                25,
                1_100,
                None,
            )
            .unwrap()
    );

    assert!(database.pause_focus_session("focus-active", 1_500).unwrap());
    assert!(!database.pause_focus_session("focus-active", 1_600).unwrap());
    let paused = database.active_focus_session().unwrap().unwrap();
    assert_eq!(paused.paused_at_ms, Some(1_500));
    assert!(
        database
            .resume_focus_session("focus-active", 2_500)
            .unwrap()
    );
    let resumed = database.active_focus_session().unwrap().unwrap();
    assert_eq!(resumed.paused_at_ms, None);
    assert_eq!(resumed.paused_total_ms, 1_000);

    assert!(database.pause_focus_session("focus-active", 2_700).unwrap());
    database
        .complete_focus_session("focus-active", 3_700, "done")
        .unwrap();
    assert!(database.active_focus_session().unwrap().is_none());
    let completed = database.list_focus_sessions(0, 4_000).unwrap();
    assert_eq!(completed[0].paused_at_ms, None);
    assert_eq!(completed[0].paused_total_ms, 2_000);

    database
        .start_focus_session("focus-expired", "2026-08-13", "Auto complete", 25, 10_000)
        .unwrap();
    assert!(
        database
            .complete_expired_focus_session("focus-expired", 1_510_000, 1_510_100)
            .unwrap()
    );
    assert!(
        !database
            .complete_expired_focus_session("focus-expired", 1_510_000, 1_510_200)
            .unwrap()
    );
    assert!(database.active_focus_session().unwrap().is_none());
    let automatically_completed = database.list_focus_sessions(1_500_000, 1_520_000).unwrap();
    assert_eq!(automatically_completed[0].ended_at_ms, Some(1_510_000));
    assert_eq!(automatically_completed[0].notified_at_ms, Some(1_510_100));
    assert_eq!(automatically_completed[0].outcome, "");
}

#[test]
fn v13_migration_keeps_only_the_latest_active_focus_session() {
    let path = unique_database_path("focus-single-active-migration");
    drop(Database::open(&path).unwrap());
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "DROP INDEX idx_focus_sessions_single_active;
             PRAGMA user_version = 12;
             INSERT INTO focus_sessions(
                id, goal_date, goal_text, planned_minutes, started_at_ms
             ) VALUES
                ('focus-old', '2026-08-13', 'Old', 25, 1000),
                ('focus-new', '2026-08-13', 'New', 25, 2000);",
        )
        .unwrap();
    drop(connection);

    let database = Database::open(&path).unwrap();
    assert_eq!(
        database.active_focus_session().unwrap().unwrap().id,
        "focus-new"
    );
    assert!(
        !database
            .start_focus_session_for_task_if_none(
                "focus-third",
                "2026-08-13",
                "Third",
                25,
                3_000,
                None,
            )
            .unwrap()
    );
    drop(database);

    let connection = Connection::open(&path).unwrap();
    let active_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM focus_sessions WHERE ended_at_ms IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(active_count, 1);
    let old_end: i64 = connection
        .query_row(
            "SELECT ended_at_ms FROM focus_sessions WHERE id='focus-old'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(old_end, 1_501_000);
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        15
    );
    drop(connection);
    let _ = std::fs::remove_file(path);
}

fn unique_database_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "daily-task-monitor-{label}-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn seed_v1_database(path: &std::path::Path) {
    let database = Database::open(path).unwrap();
    database
        .insert_segment(&daily_task_monitor_core::db::ActivitySegmentRecord {
            id: "original-segment".into(),
            started_at_ms: 1_000,
            ended_at_ms: 4_000,
            app: "Codex".into(),
            app_path: "C:\\Codex.exe".into(),
            title: "Keep this".into(),
            category: daily_task_monitor_core::domain::ActivityCategory::CreationDevelopment,
            video_purpose: daily_task_monitor_core::domain::VideoPurpose::Unknown,
            confidence: 0.9,
            source: daily_task_monitor_core::domain::ClassificationSource::Rule,
            reason: "original".into(),
            model_version: "v1".into(),
            needs_review: false,
            inactivity_reason: None,
        })
        .unwrap();
    database
        .insert_browser_visit(
            "original-visit",
            "Chrome",
            "Default",
            &BrowserVisit {
                url: "https://example.com/work".into(),
                title: "Keep browser visit".into(),
                visited_at_ms: 2_000,
            },
            "example.com",
        )
        .unwrap();
    database
        .save_daily_goal(
            "2026-07-13",
            "Keep goal",
            "Keep expected output",
            "Keep actual output",
        )
        .unwrap();
    database
        .start_focus_session(
            "original-focus",
            "2026-07-13",
            "Keep focus session",
            25,
            3_000,
        )
        .unwrap();
    drop(database);

    let connection = Connection::open(path).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    connection
        .execute(
            "INSERT INTO classifications(
                segment_id, category, video_purpose, confidence, source, reason,
                model_version, needs_review
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                "original-segment",
                "creation_development",
                "unknown",
                0.9,
                "rule",
                "keep classification",
                "v1",
                false,
            ],
        )
        .unwrap();
    connection
        .execute_batch(
            "DROP TABLE task_progress_entries;
             DROP TABLE task_browser_links;
             DROP TABLE task_activity_links;
             DROP TABLE tasks;
             DROP TABLE projects;
             PRAGMA user_version = 1;",
        )
        .unwrap();
}

fn table_columns(connection: &Connection, table: &str) -> Vec<String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    statement
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
}

fn index_names(connection: &Connection, table: &str) -> Vec<String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA index_list({table})"))
        .unwrap();
    statement
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
}

#[test]
fn migration_is_idempotent_and_preserves_existing_activity_data() {
    let path = unique_database_path("work-ledger-migration");
    seed_v1_database(&path);

    let database = Database::open(&path).unwrap();
    for table in LEDGER_TABLES {
        assert!(database.has_table(table).unwrap(), "missing table {table}");
    }
    assert_eq!(database.segment_count().unwrap(), 1);
    assert_eq!(
        database.list_segments(0, 5_000).unwrap()[0].title,
        "Keep this"
    );
    drop(database);

    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        15
    );
    assert_eq!(
        table_columns(&connection, "projects"),
        [
            "id",
            "name",
            "color",
            "status",
            "description",
            "created_at_ms",
            "updated_at_ms",
            "archived_at_ms",
        ]
    );
    assert_eq!(
        table_columns(&connection, "tasks"),
        [
            "id",
            "project_id",
            "title",
            "status",
            "priority",
            "expected_output",
            "due_date",
            "created_at_ms",
            "updated_at_ms",
            "completed_at_ms",
            "origin_kind",
            "origin_key",
            "origin_confidence",
            "review_state",
        ]
    );
    assert_eq!(
        table_columns(&connection, "task_activity_links"),
        [
            "task_id",
            "activity_segment_id",
            "provenance",
            "confidence",
            "reason",
            "created_at_ms",
        ]
    );
    assert_eq!(
        table_columns(&connection, "task_browser_links"),
        [
            "task_id",
            "browser_visit_id",
            "provenance",
            "confidence",
            "reason",
            "created_at_ms",
        ]
    );
    assert_eq!(
        table_columns(&connection, "task_progress_entries"),
        [
            "id",
            "task_id",
            "note",
            "created_at_ms",
            "origin_kind",
            "source_id",
            "source_date",
        ]
    );
    assert_eq!(
        table_columns(&connection, "focus_sessions"),
        [
            "id",
            "goal_date",
            "goal_text",
            "planned_minutes",
            "started_at_ms",
            "ended_at_ms",
            "outcome",
            "task_id",
            "paused_at_ms",
            "paused_total_ms",
            "notified_at_ms",
        ]
    );
    assert_eq!(
        table_columns(&connection, "daily_goal_task_links"),
        [
            "goal_row_id",
            "goal_date",
            "goal_text",
            "task_id",
            "confirmed_at_ms",
        ]
    );
    assert_eq!(
        table_columns(&connection, "work_ledger_ai_suggestions"),
        [
            "review_id",
            "evidence_kind",
            "evidence_id",
            "evidence_hash",
            "task_id",
            "confidence",
            "reason",
            "provider_id",
            "model",
            "created_at_ms",
        ]
    );
    let suggestion_schema = connection
        .query_row(
            "SELECT sql FROM sqlite_master
             WHERE type='table' AND name='work_ledger_ai_suggestions'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap();
    assert!(suggestion_schema.contains("review_id TEXT NOT NULL UNIQUE"));
    assert!(
        suggestion_schema
            .contains("FOREIGN KEY(review_id) REFERENCES ai_review_records(id) ON DELETE CASCADE")
    );
    for (table, index) in [
        ("tasks", "idx_tasks_project"),
        ("task_activity_links", "idx_task_activity_links_segment"),
        (
            "task_activity_links",
            "idx_task_activity_links_unique_segment",
        ),
        ("task_browser_links", "idx_task_browser_links_visit"),
        ("task_browser_links", "idx_task_browser_links_unique_visit"),
        ("task_progress_entries", "idx_task_progress_entries_task"),
        (
            "task_progress_entries",
            "idx_task_progress_entries_provenance",
        ),
        ("daily_goal_task_links", "idx_daily_goal_task_links_task"),
        (
            "work_ledger_ai_suggestions",
            "idx_work_ledger_ai_suggestions_task",
        ),
    ] {
        assert!(
            index_names(&connection, table)
                .iter()
                .any(|name| name == index),
            "missing index {index} on {table}"
        );
    }
    let ledger_sql = LEDGER_TABLES
        .iter()
        .map(|table| {
            connection
                .query_row(
                    "SELECT sql FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get::<_, String>(0),
                )
                .unwrap()
        })
        .collect::<Vec<_>>()
        .join("\n");
    for constraint in [
        "CHECK(status IN ('active', 'archived'))",
        "CHECK(status IN ('todo', 'in_progress', 'blocked', 'completed', 'cancelled'))",
        "CHECK(priority IN ('low', 'medium', 'high', 'urgent'))",
        "CHECK(provenance IN ('manual', 'rule', 'ai'))",
        "CHECK(confidence >= 0 AND confidence <= 1)",
        "PRIMARY KEY(task_id, activity_segment_id)",
        "PRIMARY KEY(task_id, browser_visit_id)",
        "PRIMARY KEY(evidence_kind, evidence_id)",
        "REFERENCES projects(id) ON DELETE CASCADE",
        "REFERENCES tasks(id) ON DELETE CASCADE",
        "REFERENCES tasks(id) ON DELETE SET NULL",
    ] {
        assert!(
            ledger_sql.contains(constraint),
            "missing constraint {constraint}"
        );
    }
    for (table, expected) in [
        ("activity_segments", "Keep this"),
        ("browser_visits", "Keep browser visit"),
        ("daily_goals", "Keep goal"),
        ("focus_sessions", "Keep focus session"),
        ("classifications", "keep classification"),
    ] {
        let column = match table {
            "activity_segments" | "browser_visits" => "title",
            "daily_goals" => "goals",
            "focus_sessions" => "goal_text",
            _ => "reason",
        };
        assert_eq!(
            connection
                .query_row(&format!("SELECT {column} FROM {table}"), [], |row| {
                    row.get::<_, String>(0)
                })
                .unwrap(),
            expected,
            "changed data in {table}"
        );
    }
    drop(connection);

    let reopened = Database::open(&path).unwrap();
    assert_eq!(reopened.segment_count().unwrap(), 1);
    assert_eq!(
        reopened
            .list_segments(0, 5_000)
            .unwrap()
            .into_iter()
            .filter(|segment| segment.id == "original-segment")
            .count(),
        1
    );
    drop(reopened);
    let _ = std::fs::remove_file(path);
}

#[test]
fn v5_migration_preserves_v4_focus_and_progress_with_manual_provenance() {
    let path = unique_database_path("work-ledger-v5-compatibility");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                color TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'active',
                description TEXT NOT NULL DEFAULT '',
                created_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL,
                archived_at_ms INTEGER
             );
             CREATE TABLE tasks (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                title TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'todo',
                priority TEXT NOT NULL DEFAULT 'medium',
                expected_output TEXT NOT NULL DEFAULT '',
                due_date TEXT,
                created_at_ms INTEGER NOT NULL,
                updated_at_ms INTEGER NOT NULL,
                completed_at_ms INTEGER,
                FOREIGN KEY(project_id) REFERENCES projects(id) ON DELETE CASCADE
             );
             CREATE TABLE task_progress_entries (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                note TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL,
                FOREIGN KEY(task_id) REFERENCES tasks(id) ON DELETE CASCADE
             );
             CREATE TABLE focus_sessions (
                id TEXT PRIMARY KEY,
                goal_date TEXT NOT NULL,
                goal_text TEXT NOT NULL,
                planned_minutes INTEGER NOT NULL,
                started_at_ms INTEGER NOT NULL,
                ended_at_ms INTEGER,
                outcome TEXT NOT NULL DEFAULT ''
             );
             INSERT INTO projects VALUES ('project-1', 'Archived work', '', 'active', '', 1, 1, NULL);
             INSERT INTO tasks VALUES ('task-1', 'project-1', 'Keep task', 'todo', 'medium', '', NULL, 1, 1, NULL);
             INSERT INTO task_progress_entries VALUES ('progress-1', 'task-1', 'Keep progress', 2000);
             INSERT INTO focus_sessions VALUES ('focus-1', '2026-07-12', 'Keep focus', 25, 1000, 2500, 'Keep outcome');
             PRAGMA user_version = 4;",
        )
        .unwrap();
    drop(connection);

    let database = Database::open(&path).unwrap();
    let progress = database
        .list_work_ledger_progress_entries("task-1")
        .unwrap();
    assert_eq!(progress.len(), 1);
    assert_eq!(progress[0].note, "Keep progress");
    assert_eq!(progress[0].origin_kind.as_str(), "manual");
    assert_eq!(progress[0].source_id, None);
    assert_eq!(progress[0].source_date, None);
    let focus = database.list_focus_sessions(0, 3_000).unwrap();
    assert_eq!(focus.len(), 1);
    assert_eq!(focus[0].outcome, "Keep outcome");
    assert_eq!(focus[0].task_id, None);
    drop(database);

    let reopened = Database::open(&path).unwrap();
    assert_eq!(
        reopened
            .list_work_ledger_progress_entries("task-1")
            .unwrap()
            .len(),
        1
    );
    drop(reopened);
    let _ = std::fs::remove_file(path);
}

#[test]
fn failed_v5_migration_rolls_back_focus_column_and_user_version() {
    let path = unique_database_path("work-ledger-v5-atomic-migration");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE projects (id TEXT PRIMARY KEY);
             CREATE TABLE tasks (id TEXT PRIMARY KEY);
             CREATE TABLE focus_sessions (
                id TEXT PRIMARY KEY,
                goal_date TEXT NOT NULL,
                goal_text TEXT NOT NULL,
                planned_minutes INTEGER NOT NULL,
                started_at_ms INTEGER NOT NULL,
                ended_at_ms INTEGER,
                outcome TEXT NOT NULL DEFAULT ''
             );
             CREATE TABLE task_progress_entries (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                note TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL,
                origin_kind TEXT NOT NULL DEFAULT 'manual'
             );
             CREATE TABLE daily_goal_task_links (goal_row_id TEXT PRIMARY KEY);
             PRAGMA user_version = 4;",
        )
        .unwrap();
    drop(connection);

    assert!(Database::open(&path).is_err());

    let connection = Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        4
    );
    assert!(!table_columns(&connection, "focus_sessions").contains(&"task_id".to_string()));
    assert_eq!(
        table_columns(&connection, "task_progress_entries"),
        ["id", "task_id", "note", "created_at_ms", "origin_kind"]
    );
    assert_eq!(
        table_columns(&connection, "daily_goal_task_links"),
        ["goal_row_id"]
    );
    drop(connection);
    let _ = std::fs::remove_file(path);
}

#[test]
fn failed_v2_migration_rolls_back_ddl_and_user_version_together() {
    let path = unique_database_path("work-ledger-atomic-migration");
    seed_v1_database(&path);
    {
        let connection = Connection::open(&path).unwrap();
        connection
            .execute("CREATE TABLE tasks (id TEXT PRIMARY KEY)", [])
            .unwrap();
    }

    assert!(Database::open(&path).is_err());

    let connection = Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(table_columns(&connection, "tasks"), ["id"]);
    for table in [
        "projects",
        "task_activity_links",
        "task_browser_links",
        "task_progress_entries",
    ] {
        assert!(
            table_columns(&connection, table).is_empty(),
            "partial table {table}"
        );
    }
    drop(connection);
    let _ = std::fs::remove_file(path);
}

#[test]
fn repository_persists_projects_tasks_and_unique_evidence_links() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_segment(&daily_task_monitor_core::db::ActivitySegmentRecord {
            id: "segment-1".into(),
            started_at_ms: 1_000,
            ended_at_ms: 4_000,
            app: "Codex".into(),
            app_path: String::new(),
            title: "Build ledger".into(),
            category: daily_task_monitor_core::domain::ActivityCategory::CreationDevelopment,
            video_purpose: daily_task_monitor_core::domain::VideoPurpose::Unknown,
            confidence: 0.9,
            source: daily_task_monitor_core::domain::ClassificationSource::Rule,
            reason: "eligible fixture".into(),
            model_version: "rules-v1".into(),
            needs_review: false,
            inactivity_reason: None,
        })
        .unwrap();
    let repository = WorkLedgerRepository::new(&database);
    let project = repository
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Desktop rewrite".into(),
            color: "#3182ce".into(),
            description: "Offline ledger".into(),
            created_at_ms: 1_000,
        })
        .unwrap();
    assert_eq!(project.status, ProjectStatus::Active);

    let task = repository
        .create_task(NewTask {
            id: "task-1".into(),
            project_id: project.id.clone(),
            title: "Build ledger".into(),
            priority: TaskPriority::High,
            expected_output: "Working migration".into(),
            due_date: Some("2026-07-13".into()),
            created_at_ms: 2_000,
        })
        .unwrap();
    assert_eq!(task.status, TaskStatus::Todo);

    assert!(
        repository
            .assign_activity(
                &task.id,
                "segment-1",
                EvidenceProvenance::Ai,
                0.6,
                "suggested",
                3_000,
            )
            .unwrap()
    );
    assert!(
        !repository
            .assign_activity(
                &task.id,
                "segment-1",
                EvidenceProvenance::Ai,
                0.9,
                "duplicate",
                4_000,
            )
            .unwrap()
    );
    let links = repository.list_activity_links(&task.id).unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].confidence, 0.6);
    assert!(repository.remove_activity(&task.id, "segment-1").unwrap());
    assert!(!repository.remove_activity(&task.id, "segment-1").unwrap());
}

#[test]
fn repository_updates_archives_and_deletes_ledger_records() {
    let database = Database::open_in_memory().unwrap();
    database
        .insert_browser_visit(
            "visit-1",
            "Chrome",
            "Default",
            &BrowserVisit {
                url: "https://example.com/research".into(),
                title: "Work research".into(),
                visited_at_ms: 2_000,
            },
            "example.com",
        )
        .unwrap();
    let repository = WorkLedgerRepository::new(&database);
    let project = repository
        .create_project(NewProject {
            id: "project-1".into(),
            name: "Before".into(),
            color: "#111111".into(),
            description: String::new(),
            created_at_ms: 1_000,
        })
        .unwrap();
    assert!(
        repository
            .update_project(
                &project.id,
                ProjectUpdate {
                    name: Some("After".into()),
                    color: None,
                    description: Some("Updated".into()),
                },
                2_000,
            )
            .unwrap()
    );
    assert_eq!(
        repository.get_project(&project.id).unwrap().unwrap().name,
        "After"
    );

    let task = repository
        .create_task(NewTask {
            id: "task-1".into(),
            project_id: project.id.clone(),
            title: "Before".into(),
            priority: TaskPriority::Low,
            expected_output: String::new(),
            due_date: Some("2026-07-13".into()),
            created_at_ms: 1_000,
        })
        .unwrap();
    let target_project = repository
        .create_project(NewProject {
            id: "project-2".into(),
            name: "Moved".into(),
            color: "#222222".into(),
            description: String::new(),
            created_at_ms: 2_500,
        })
        .unwrap();
    assert!(
        repository
            .update_task(
                &task.id,
                TaskUpdate {
                    project_id: Some(target_project.id.clone()),
                    title: Some("After".into()),
                    priority: Some(TaskPriority::Urgent),
                    expected_output: Some("Checked".into()),
                    due_date: Some(None),
                },
                3_000,
            )
            .unwrap()
    );
    let updated = repository.get_task(&task.id).unwrap().unwrap();
    assert_eq!(updated.title, "After");
    assert_eq!(updated.project_id, target_project.id);
    assert_eq!(updated.due_date, None);

    assert!(
        repository
            .assign_browser_visit(
                &task.id,
                "visit-1",
                EvidenceProvenance::Rule,
                0.7,
                "domain rule",
                4_000,
            )
            .unwrap()
    );
    assert_eq!(repository.list_browser_links(&task.id).unwrap().len(), 1);
    assert!(
        repository
            .remove_browser_visit(&task.id, "visit-1")
            .unwrap()
    );
    let entry = repository
        .add_progress_entry(NewProgressEntry {
            id: "progress-1".into(),
            task_id: task.id.clone(),
            note: "Started".into(),
            created_at_ms: 4_000,
        })
        .unwrap();
    assert_eq!(
        repository.list_progress_entries(&task.id).unwrap(),
        vec![entry.clone()]
    );
    assert!(repository.delete_progress_entry(&entry.id).unwrap());

    assert!(repository.archive_project(&project.id, 5_000).unwrap());
    assert_eq!(
        repository.get_project(&project.id).unwrap().unwrap().status,
        ProjectStatus::Archived
    );
    assert!(
        repository
            .archive_project(&target_project.id, 5_000)
            .unwrap()
    );
    assert!(repository.list_projects(false).unwrap().is_empty());
    assert!(repository.delete_project(&project.id).unwrap());
    assert!(repository.get_task(&task.id).unwrap().is_some());
    assert!(repository.delete_project(&target_project.id).unwrap());
    assert!(repository.get_task(&task.id).unwrap().is_none());
}

#[test]
fn v10_upgrade_preserves_a_realistic_445_job_audit_history() {
    let path = unique_database_path("v10-445-job-upgrade");
    let database = Database::open(&path).unwrap();
    for index in 0..445 {
        let payload = format!(r#"{{"index":{index}}}"#);
        database
            .enqueue_ai_job(
                "test-audit",
                &payload,
                1_000 + index,
                &AiExecutionSnapshot {
                    execution_mode: AiExecutionMode::ApiKey,
                    executor_id: "openai".into(),
                    model: "migration-test".into(),
                    evidence_hash: format!("hash-{index}"),
                    created_at_ms: 1_000 + index,
                },
            )
            .unwrap();
    }
    assert_eq!(database.ai_job_count().unwrap(), 445);
    drop(database);

    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch("PRAGMA user_version = 8;")
        .unwrap();
    drop(connection);

    let upgraded = Database::open(&path).unwrap();
    assert_eq!(upgraded.ai_job_count().unwrap(), 445);
    drop(upgraded);
    let connection = Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        15
    );
    drop(connection);
    let _ = std::fs::remove_file(path);
}
