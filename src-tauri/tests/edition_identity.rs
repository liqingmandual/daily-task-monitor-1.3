use daily_task_monitor_core::edition::resolve_edition_identity;

#[test]
fn defaults_preserve_the_existing_app_identity() {
    let identity = resolve_edition_identity(None, None, None);

    assert_eq!(identity.storage_name, "DailyTaskMonitor");
    assert_eq!(identity.credential_service, "DailyTaskMonitor");
    assert_eq!(identity.mutex_name, "Local\\DailyTaskMonitorDesktop");
}

#[test]
fn build_overrides_isolate_storage_credentials_and_single_instance_lock() {
    let identity = resolve_edition_identity(
        Some("DailyTaskMonitorIndependent13"),
        Some("DailyTaskMonitorIndependent13"),
        Some("Local\\DailyTaskMonitorDesktopIndependent13"),
    );

    assert_eq!(identity.storage_name, "DailyTaskMonitorIndependent13");
    assert_eq!(identity.credential_service, "DailyTaskMonitorIndependent13");
    assert_eq!(
        identity.mutex_name,
        "Local\\DailyTaskMonitorDesktopIndependent13"
    );
}
