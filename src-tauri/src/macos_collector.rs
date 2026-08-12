use std::ffi::{c_char, c_double, c_int, c_long, c_void};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::monitor::MonitorSample;

type CfArrayRef = *const c_void;
type CfDictionaryRef = *const c_void;
type CfNumberRef = *const c_void;
type CfStringRef = *const c_void;
type CfTypeRef = *const c_void;

const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const K_CF_NUMBER_SINT32_TYPE: c_int = 3;
const K_CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY: u32 = 1;
const K_CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS: u32 = 16;
const K_CG_NULL_WINDOW_ID: u32 = 0;
const K_CG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE: u32 = 0;
const K_CG_ANY_INPUT_EVENT_TYPE: u32 = u32::MAX;
const CLOCK_MONOTONIC_RAW: c_int = 4;
const PROC_PIDPATHINFO_MAXSIZE: usize = 4096;

#[repr(C)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFArrayGetCount(array: CfArrayRef) -> c_long;
    fn CFArrayGetValueAtIndex(array: CfArrayRef, index: c_long) -> CfTypeRef;
    fn CFDictionaryGetValue(dictionary: CfDictionaryRef, key: CfTypeRef) -> CfTypeRef;
    fn CFNumberGetValue(number: CfNumberRef, number_type: c_int, value: *mut c_void) -> bool;
    fn CFRelease(value: CfTypeRef);
    fn CFStringGetCString(
        string: CfStringRef,
        buffer: *mut c_char,
        buffer_size: c_long,
        encoding: u32,
    ) -> bool;
    fn CFStringGetMaximumSizeForEncoding(length: c_long, encoding: u32) -> c_long;
    fn CFStringGetLength(string: CfStringRef) -> c_long;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    static kCGWindowLayer: CfStringRef;
    static kCGWindowName: CfStringRef;
    static kCGWindowOwnerName: CfStringRef;
    static kCGWindowOwnerPID: CfStringRef;

    fn CGEventSourceSecondsSinceLastEventType(state_id: u32, event_type: u32) -> c_double;
    fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> CfArrayRef;
}

unsafe extern "C" {
    fn clock_gettime(clock_id: c_int, time: *mut Timespec) -> c_int;
    fn proc_pidpath(pid: c_int, buffer: *mut c_void, buffer_size: u32) -> c_int;
}

#[derive(Default)]
pub struct MacOsCollector;

impl MacOsCollector {
    pub fn sample(&mut self) -> MonitorSample {
        let observed_at_ms = now_ms();
        let (app, app_path, title) =
            foreground_window().unwrap_or_else(|| ("Unknown".into(), String::new(), String::new()));
        let idle_ms = idle_elapsed_ms();

        MonitorSample {
            observed_at_ms,
            last_input_at_ms: observed_at_ms.saturating_sub(idle_ms as i64),
            app,
            app_path,
            title,
            domain: String::new(),
            key_presses: 0,
            mouse_events: 0,
            media_playing: false,
        }
    }
}

pub fn system_uptime_ms() -> i64 {
    let mut value = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    if unsafe { clock_gettime(CLOCK_MONOTONIC_RAW, &mut value) } != 0 {
        return 0;
    }
    value
        .tv_sec
        .saturating_mul(1_000)
        .saturating_add(value.tv_nsec / 1_000_000)
}

fn foreground_window() -> Option<(String, String, String)> {
    let options =
        K_CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY | K_CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS;
    let windows = unsafe { CGWindowListCopyWindowInfo(options, K_CG_NULL_WINDOW_ID) };
    if windows.is_null() {
        return None;
    }

    let result = (0..unsafe { CFArrayGetCount(windows) }).find_map(|index| {
        let dictionary = unsafe { CFArrayGetValueAtIndex(windows, index) } as CfDictionaryRef;
        let layer = dictionary_i32(dictionary, unsafe { kCGWindowLayer })?;
        if layer != 0 {
            return None;
        }
        let pid = dictionary_i32(dictionary, unsafe { kCGWindowOwnerPID })?;
        let owner = dictionary_string(dictionary, unsafe { kCGWindowOwnerName })?;
        if owner.trim().is_empty() {
            return None;
        }
        let title = dictionary_string(dictionary, unsafe { kCGWindowName }).unwrap_or_default();
        Some((owner, process_path(pid), title))
    });

    unsafe { CFRelease(windows) };
    result
}

fn dictionary_i32(dictionary: CfDictionaryRef, key: CfTypeRef) -> Option<i32> {
    let value = unsafe { CFDictionaryGetValue(dictionary, key) } as CfNumberRef;
    if value.is_null() {
        return None;
    }
    let mut output = 0_i32;
    unsafe {
        CFNumberGetValue(
            value,
            K_CF_NUMBER_SINT32_TYPE,
            (&mut output as *mut i32).cast(),
        )
    }
    .then_some(output)
}

fn dictionary_string(dictionary: CfDictionaryRef, key: CfTypeRef) -> Option<String> {
    let value = unsafe { CFDictionaryGetValue(dictionary, key) } as CfStringRef;
    cf_string(value)
}

fn cf_string(value: CfStringRef) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let length = unsafe { CFStringGetLength(value) };
    let capacity = unsafe { CFStringGetMaximumSizeForEncoding(length, K_CF_STRING_ENCODING_UTF8) }
        .saturating_add(1);
    let mut buffer = vec![0_u8; usize::try_from(capacity).ok()?];
    if !unsafe {
        CFStringGetCString(
            value,
            buffer.as_mut_ptr().cast(),
            capacity,
            K_CF_STRING_ENCODING_UTF8,
        )
    } {
        return None;
    }
    let end = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    Some(String::from_utf8_lossy(&buffer[..end]).into_owned())
}

fn process_path(pid: i32) -> String {
    let mut buffer = vec![0_u8; PROC_PIDPATHINFO_MAXSIZE];
    let copied = unsafe {
        proc_pidpath(
            pid,
            buffer.as_mut_ptr().cast(),
            PROC_PIDPATHINFO_MAXSIZE as u32,
        )
    };
    if copied <= 0 {
        return String::new();
    }
    let value = String::from_utf8_lossy(&buffer[..copied as usize])
        .trim_end_matches('\0')
        .to_string();
    if Path::new(&value).exists() {
        value
    } else {
        String::new()
    }
}

fn idle_elapsed_ms() -> u64 {
    let seconds = unsafe {
        CGEventSourceSecondsSinceLastEventType(
            K_CG_EVENT_SOURCE_STATE_COMBINED_SESSION_STATE,
            K_CG_ANY_INPUT_EVENT_TYPE,
        )
    };
    if seconds.is_finite() && seconds >= 0.0 {
        (seconds * 1_000.0).min(u64::MAX as f64) as u64
    } else {
        0
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{MacOsCollector, system_uptime_ms};

    #[test]
    fn collector_reads_a_foreground_application_and_idle_clock() {
        let sample = MacOsCollector.sample();

        assert!(sample.observed_at_ms > 0);
        assert!(sample.last_input_at_ms <= sample.observed_at_ms);
        assert!(!sample.app.trim().is_empty());
    }

    #[test]
    fn monotonic_uptime_is_available() {
        assert!(system_uptime_ms() > 0);
    }
}
