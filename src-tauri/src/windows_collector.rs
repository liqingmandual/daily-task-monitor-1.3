use std::ffi::c_void;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};

use crate::monitor::MonitorSample;

type WindowHandle = *mut c_void;
type ProcessHandle = *mut c_void;

const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

#[repr(C)]
struct LastInputInfo {
    cb_size: u32,
    dw_time: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Point {
    x: i32,
    y: i32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetForegroundWindow() -> WindowHandle;
    fn GetWindowTextLengthW(window: WindowHandle) -> i32;
    fn GetWindowTextW(window: WindowHandle, text: *mut u16, count: i32) -> i32;
    fn GetWindowThreadProcessId(window: WindowHandle, process_id: *mut u32) -> u32;
    fn GetLastInputInfo(info: *mut LastInputInfo) -> i32;
    fn GetCursorPos(point: *mut Point) -> i32;
    fn GetAsyncKeyState(key: i32) -> i16;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetTickCount64() -> u64;
    fn OpenProcess(access: u32, inherit_handle: i32, process_id: u32) -> ProcessHandle;
    fn QueryFullProcessImageNameW(
        process: ProcessHandle,
        flags: u32,
        path: *mut u16,
        size: *mut u32,
    ) -> i32;
    fn CloseHandle(handle: ProcessHandle) -> i32;
}

pub struct WindowsCollector {
    last_cursor: Option<Point>,
    key_states: [bool; 256],
}

impl Default for WindowsCollector {
    fn default() -> Self {
        Self {
            last_cursor: None,
            key_states: [false; 256],
        }
    }
}

impl WindowsCollector {
    pub fn sample(&mut self) -> MonitorSample {
        let observed_at_ms = now_ms();
        let window = unsafe { GetForegroundWindow() };
        let title = window_title(window);
        let (app, app_path) =
            process_identity(window).unwrap_or_else(|| ("Unknown".into(), String::new()));
        let last_input_at_ms = observed_at_ms.saturating_sub(last_input_elapsed_ms() as i64);
        let (key_presses, mouse_events) = self.aggregate_input_signals();
        let media_playing = is_video_scene(&app, &title) && media_session_is_playing();

        MonitorSample {
            observed_at_ms,
            last_input_at_ms,
            app,
            app_path,
            title,
            domain: String::new(),
            key_presses,
            mouse_events,
            media_playing,
        }
    }

    fn aggregate_input_signals(&mut self) -> (u32, u32) {
        let mut key_presses = 0;
        for key in 8..=254 {
            let state = unsafe { GetAsyncKeyState(key) } as u16;
            let down = state & 0x8000 != 0;
            let pressed_since_last_poll = state & 0x0001 != 0;
            if pressed_since_last_poll || (down && !self.key_states[key as usize]) {
                key_presses += 1;
            }
            self.key_states[key as usize] = down;
        }

        let mut cursor = Point::default();
        let has_cursor = unsafe { GetCursorPos(&mut cursor) } != 0;
        let mut mouse_events =
            u32::from(has_cursor && self.last_cursor.is_some_and(|old| old != cursor));
        for button in 1..=6 {
            let state = unsafe { GetAsyncKeyState(button) } as u16;
            let down = state & 0x8000 != 0;
            let pressed_since_last_poll = state & 0x0001 != 0;
            if pressed_since_last_poll || (down && !self.key_states[button as usize]) {
                mouse_events += 1;
            }
            self.key_states[button as usize] = down;
        }
        if has_cursor {
            self.last_cursor = Some(cursor);
        }
        (key_presses, mouse_events)
    }
}

pub fn system_uptime_ms() -> i64 {
    (unsafe { GetTickCount64() }).min(i64::MAX as u64) as i64
}

fn window_title(window: WindowHandle) -> String {
    if window.is_null() {
        return String::new();
    }
    let length = unsafe { GetWindowTextLengthW(window) }.max(0) as usize;
    let mut buffer = vec![0_u16; length.saturating_add(1)];
    let copied =
        unsafe { GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32) }.max(0) as usize;
    String::from_utf16_lossy(&buffer[..copied])
}

fn process_identity(window: WindowHandle) -> Option<(String, String)> {
    if window.is_null() {
        return None;
    }
    let mut process_id = 0_u32;
    unsafe { GetWindowThreadProcessId(window, &mut process_id) };
    if process_id == 0 {
        return None;
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return None;
    }
    let mut buffer = vec![0_u16; 32_768];
    let mut length = buffer.len() as u32;
    let ok =
        unsafe { QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length) } != 0;
    unsafe { CloseHandle(process) };
    if !ok {
        return None;
    }
    let path = String::from_utf16_lossy(&buffer[..length as usize]);
    let name = Path::new(&path)
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())?;
    Some((name, path))
}

fn last_input_elapsed_ms() -> u64 {
    let mut info = LastInputInfo {
        cb_size: std::mem::size_of::<LastInputInfo>() as u32,
        dw_time: 0,
    };
    if unsafe { GetLastInputInfo(&mut info) } == 0 {
        return 0;
    }
    let tick = unsafe { GetTickCount64() } as u32;
    tick.wrapping_sub(info.dw_time) as u64
}

fn media_session_is_playing() -> bool {
    let Ok(operation) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync() else {
        return false;
    };
    let Ok(manager) = operation.join() else {
        return false;
    };
    let Ok(session) = manager.GetCurrentSession() else {
        return false;
    };
    let Ok(info) = session.GetPlaybackInfo() else {
        return false;
    };
    matches!(
        info.PlaybackStatus(),
        Ok(GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing)
    )
}

fn is_video_scene(app: &str, title: &str) -> bool {
    let value = format!("{app} {title}").to_ascii_lowercase();
    [
        "youtube", "bilibili", "netflix", "video", "player", "vlc", "lecture", "course", "课程",
        "视频",
    ]
    .iter()
    .any(|keyword| value.contains(keyword))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}
