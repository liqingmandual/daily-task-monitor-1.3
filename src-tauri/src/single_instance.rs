#[cfg(not(target_os = "windows"))]
mod platform {
    use std::ffi::c_int;
    use std::fs::{File, OpenOptions};
    use std::os::fd::AsRawFd;
    use std::path::PathBuf;

    const LOCK_EX: c_int = 2;
    const LOCK_NB: c_int = 4;

    unsafe extern "C" {
        fn flock(file_descriptor: c_int, operation: c_int) -> c_int;
    }

    pub struct SingleInstanceGuard {
        _file: File,
    }

    impl SingleInstanceGuard {
        pub fn acquire(name: &str) -> Option<Self> {
            let file = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .open(lock_path(name))
                .ok()?;
            if unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } != 0 {
                return None;
            }
            Some(Self { _file: file })
        }
    }

    fn lock_path(name: &str) -> PathBuf {
        let normalized = name
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                    character
                } else {
                    '_'
                }
            })
            .collect::<String>();
        std::env::temp_dir().join(format!("{normalized}.lock"))
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn CreateMutexW(
            attributes: *const c_void,
            initial_owner: i32,
            name: *const u16,
        ) -> *mut c_void;
        fn GetLastError() -> u32;
    }

    const ERROR_ALREADY_EXISTS: u32 = 183;

    pub struct SingleInstanceGuard {
        handle: *mut c_void,
    }

    impl SingleInstanceGuard {
        pub fn acquire(name: &str) -> Option<Self> {
            let name = format!("{name}\0").encode_utf16().collect::<Vec<_>>();
            let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
            if handle.is_null() {
                return None;
            }
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                unsafe { CloseHandle(handle) };
                return None;
            }
            Some(Self { handle })
        }
    }

    impl Drop for SingleInstanceGuard {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.handle) };
        }
    }
}

pub use platform::SingleInstanceGuard;

#[cfg(test)]
mod tests {
    use super::SingleInstanceGuard;

    #[test]
    fn rejects_a_second_instance_and_releases_after_drop() {
        let name = format!("Local\\DailyTaskMonitorDesktopTest{}", std::process::id());
        let first =
            SingleInstanceGuard::acquire(&name).expect("first instance should acquire lock");

        assert!(SingleInstanceGuard::acquire(&name).is_none());

        drop(first);
        assert!(SingleInstanceGuard::acquire(&name).is_some());
    }
}
