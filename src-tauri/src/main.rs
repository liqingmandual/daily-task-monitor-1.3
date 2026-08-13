#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "windows")]
fn acquire_single_instance() -> bool {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateMutexW(
            attributes: *const c_void,
            initial_owner: i32,
            name: *const u16,
        ) -> *mut c_void;
        fn GetLastError() -> u32;
    }

    const ERROR_ALREADY_EXISTS: u32 = 183;
    let name: Vec<u16> = format!(
        "{}\0",
        daily_task_monitor_core::edition::current_edition_identity().mutex_name
    )
    .encode_utf16()
    .collect();
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    !handle.is_null() && unsafe { GetLastError() } != ERROR_ALREADY_EXISTS
}

#[cfg(not(target_os = "windows"))]
fn acquire_single_instance() -> bool {
    true
}

fn main() {
    if !acquire_single_instance() {
        return;
    }
    daily_task_monitor_core::desktop::run();
}
