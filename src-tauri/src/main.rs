#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    daily_task_monitor_core::desktop::run();
}
