#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditionIdentity {
    pub storage_name: &'static str,
    pub credential_service: &'static str,
    pub mutex_name: &'static str,
}

pub fn resolve_edition_identity(
    storage_name: Option<&'static str>,
    credential_service: Option<&'static str>,
    mutex_name: Option<&'static str>,
) -> EditionIdentity {
    EditionIdentity {
        storage_name: storage_name.unwrap_or("DailyTaskMonitor"),
        credential_service: credential_service.unwrap_or("DailyTaskMonitor"),
        mutex_name: mutex_name.unwrap_or("Local\\DailyTaskMonitorDesktop"),
    }
}

pub fn current_edition_identity() -> EditionIdentity {
    resolve_edition_identity(
        option_env!("DAILY_TASK_MONITOR_STORAGE_NAME"),
        option_env!("DAILY_TASK_MONITOR_CREDENTIAL_SERVICE"),
        option_env!("DAILY_TASK_MONITOR_MUTEX_NAME"),
    )
}
