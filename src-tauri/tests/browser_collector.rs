use daily_task_monitor_core::browser::{
    chromium_time_to_unix_ms, extract_html_summary, is_public_web_url, redact_url_for_storage,
    scan_chromium_history,
};
use rusqlite::{Connection, params};

fn unique_temp_dir(name: &str) -> std::path::PathBuf {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("daily-task-monitor-{name}-{suffix}"));
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn converts_chromium_microseconds_to_unix_milliseconds() {
    let unix_ms = 1_700_000_000_000_i64;
    let chromium_micros = (unix_ms + 11_644_473_600_000) * 1_000;

    assert_eq!(chromium_time_to_unix_ms(chromium_micros), unix_ms);
}

#[test]
fn scans_a_copied_chromium_history_database() {
    let temp = unique_temp_dir("history-test");
    let history_path = temp.join("History");
    let connection = Connection::open(&history_path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT);
             CREATE TABLE visits (id INTEGER PRIMARY KEY, url INTEGER, visit_time INTEGER);",
        )
        .unwrap();
    let unix_ms = 1_700_000_000_000_i64;
    let chrome_time = (unix_ms + 11_644_473_600_000) * 1_000;
    connection
        .execute(
            "INSERT INTO urls (id, url, title) VALUES (1, ?1, ?2)",
            params!["https://example.com/article", "Example article"],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO visits (id, url, visit_time) VALUES (1, 1, ?1)",
            [chrome_time],
        )
        .unwrap();
    drop(connection);

    let visits =
        scan_chromium_history(&history_path, unix_ms - 1_000, unix_ms + 1_000, 50).unwrap();

    assert_eq!(visits.len(), 1);
    assert_eq!(visits[0].url, "https://example.com/article");
    assert_eq!(visits[0].title, "Example article");
    assert_eq!(visits[0].visited_at_ms, unix_ms);
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn scans_committed_wal_data_while_the_browser_database_is_open() {
    let temp = unique_temp_dir("history-live-wal");
    let history_path = temp.join("History");
    let connection = Connection::open(&history_path).unwrap();
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .unwrap();
    connection
        .execute_batch(
            "CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT);
             CREATE TABLE visits (id INTEGER PRIMARY KEY, url INTEGER, visit_time INTEGER);",
        )
        .unwrap();
    let unix_ms = 1_700_000_000_000_i64;
    let chrome_time = (unix_ms + 11_644_473_600_000) * 1_000;
    connection
        .execute(
            "INSERT INTO urls (id, url, title) VALUES (1, ?1, ?2)",
            params!["https://example.com/live", "Live WAL page"],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO visits (id, url, visit_time) VALUES (1, 1, ?1)",
            [chrome_time],
        )
        .unwrap();

    let visits =
        scan_chromium_history(&history_path, unix_ms - 1_000, unix_ms + 1_000, 50).unwrap();
    assert_eq!(visits[0].title, "Live WAL page");

    drop(connection);
    std::fs::remove_dir_all(temp).unwrap();
}

#[test]
fn blocks_private_and_non_http_fetch_targets() {
    for blocked in [
        "http://127.0.0.1/private",
        "http://192.168.1.2/admin",
        "http://10.0.0.1/",
        "http://172.20.0.1/",
        "http://localhost:8765/",
        "http://[fe80::1]/private",
        "http://[ff02::1]/multicast",
        "http://[::ffff:127.0.0.1]/mapped-loopback",
        "http://[::ffff:10.0.0.1]/mapped-private",
        "file:///C:/secret.txt",
    ] {
        assert!(!is_public_web_url(blocked), "should block {blocked}");
    }
    assert!(is_public_web_url("https://example.com/article"));
}

#[test]
fn redacts_credentials_fragments_and_sensitive_query_values() {
    let value = redact_url_for_storage(
        "https://user:pass@example.com/callback?q=rust&token=secret&code=abc#private",
    );

    assert_eq!(
        value,
        "https://example.com/callback?q=rust&token=%5BREDACTED%5D&code=%5BREDACTED%5D"
    );
}

#[test]
fn extracts_title_description_and_bounded_visible_text() {
    let html = r#"
      <html><head><title>Learning Page</title>
      <meta name="description" content="A useful summary"></head>
      <body><nav>Navigation</nav><main><h1>Psychology</h1><p>Visible article text.</p></main>
      <script>secret()</script></body></html>
    "#;

    let summary = extract_html_summary(html, 80);

    assert_eq!(summary.title, "Learning Page");
    assert_eq!(summary.description, "A useful summary");
    assert!(summary.text.contains("Psychology"));
    assert!(!summary.text.contains("secret"));
    assert!(summary.text.chars().count() <= 80);
}
