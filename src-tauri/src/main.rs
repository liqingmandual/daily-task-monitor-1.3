#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod single_instance;

fn main() {
    let Some(_single_instance) = single_instance::SingleInstanceGuard::acquire(
        daily_task_monitor_core::edition::current_edition_identity().mutex_name,
    ) else {
        return;
    };
    daily_task_monitor_core::desktop::run();
}
