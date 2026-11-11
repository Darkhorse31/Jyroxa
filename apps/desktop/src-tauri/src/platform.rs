//! Windows-specific helpers (autostart, sleep prevention, shell open, single instance, close-to-tray).

use std::sync::atomic::{AtomicBool, Ordering};

pub static CLOSE_TO_TRAY: AtomicBool = AtomicBool::new(true);

pub fn set_close_to_tray(enabled: bool) {
    CLOSE_TO_TRAY.store(enabled, Ordering::Release);
}

pub fn close_to_tray() -> bool {
    CLOSE_TO_TRAY.load(Ordering::Relaxed)
}

#[cfg(windows)]
mod win {
    use std::path::Path;
    use std::sync::atomic::{AtomicU32, Ordering};

    const ES_CONTINUOUS: u32 = 0x8000_0000;
    const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
    const ERROR_ALREADY_EXISTS: u32 = 183;
    const SW_RESTORE: i32 = 9;

    #[link(name = "kernel32")]
    extern "system" {
        pub fn SetThreadExecutionState(es_flags: u32) -> u32;
        pub fn CreateMutexW(
            lpMutexAttributes: *const std::ffi::c_void,
            bInitialOwner: i32,
            lpName: *const u16,
        ) -> *mut std::ffi::c_void;
        pub fn GetLastError() -> u32;
        pub fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
    }

    #[link(name = "user32")]
    extern "system" {
        pub fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> *mut std::ffi::c_void;
        pub fn ShowWindow(hWnd: *mut std::ffi::c_void, nCmdShow: i32) -> i32;
        pub fn SetForegroundWindow(hWnd: *mut std::ffi::c_void) -> i32;
    }

    static SLEEP_GUARD: AtomicU32 = AtomicU32::new(0);

    #[allow(dead_code)]
    pub fn set_prevent_sleep(enabled: bool) {
        if enabled {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
            }
            SLEEP_GUARD.fetch_add(1, Ordering::SeqCst);
        } else if SLEEP_GUARD.fetch_sub(1, Ordering::SeqCst) == 1 {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS);
            }
        }
    }

    pub fn sync_prevent_sleep(active_downloads: usize, setting_on: bool) {
        if !setting_on {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS);
            }
            SLEEP_GUARD.store(0, Ordering::SeqCst);
            return;
        }
        if active_downloads > 0 {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
            }
        } else {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS);
            }
        }
    }

    pub fn set_run_at_startup(app_name: &str, exe: &Path, enable: bool, minimized: bool) {
        let Ok(reg) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .open_subkey_with_flags(
                r"Software\Microsoft\Windows\CurrentVersion\Run",
                winreg::enums::KEY_READ | winreg::enums::KEY_WRITE,
            )
        else {
            return;
        };
        if enable {
            let mut cmd = format!("\"{}\"", exe.display());
            if minimized {
                cmd.push_str(" --minimized");
            }
            let _ = reg.set_value(app_name, &cmd);
        } else {
            let _ = reg.delete_value(app_name);
        }
    }

    pub fn open_folder(path: &Path) -> Result<(), String> {
        let path = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf());
        std::process::Command::new("explorer")
            .arg(path.as_os_str())
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn open_file_in_folder(path: &Path) -> Result<(), String> {
        let path = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf());
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(path.as_os_str())
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub struct SingleInstanceGuard(*mut std::ffi::c_void);
    unsafe impl Send for SingleInstanceGuard {}
    unsafe impl Sync for SingleInstanceGuard {}

    impl Drop for SingleInstanceGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }

    pub fn try_acquire_single_instance(name: &str) -> Option<SingleInstanceGuard> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let wide: Vec<u16> = OsStr::new(name).encode_wide().chain(std::iter::once(0)).collect();
        unsafe {
            let handle = CreateMutexW(std::ptr::null(), 1, wide.as_ptr());
            if handle.is_null() {
                return None;
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(handle);
                return None;
            }
            Some(SingleInstanceGuard(handle))
        }
    }

    pub fn focus_existing_window(title: &str) {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let wide: Vec<u16> = OsStr::new(title).encode_wide().chain(std::iter::once(0)).collect();
        unsafe {
            let hwnd = FindWindowW(std::ptr::null(), wide.as_ptr());
            if !hwnd.is_null() {
                ShowWindow(hwnd, SW_RESTORE);
                SetForegroundWindow(hwnd);
            }
        }
    }

    pub const SINGLE_INSTANCE_PIPE: &str = r"\\.\pipe\SwiftGetSingleInstancePipe";

    pub fn send_to_first_instance(url: Option<&str>) -> bool {
        use std::io::Write;
        use std::fs::OpenOptions;

        if let Ok(mut file) = OpenOptions::new().write(true).open(SINGLE_INSTANCE_PIPE) {
            let msg = url.unwrap_or("");
            let _ = file.write_all(msg.as_bytes());
            true
        } else {
            false
        }
    }

    pub async fn run_single_instance_server(app: tauri::AppHandle) {
        use tauri::{Emitter, Manager};
        use tokio::io::AsyncReadExt;
        use tokio::net::windows::named_pipe::ServerOptions;

        let pipe_name = SINGLE_INSTANCE_PIPE;
        let mut server = match ServerOptions::new().first_pipe_instance(true).create(pipe_name) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Failed to create single instance pipe server: {}", e);
                return;
            }
        };

        loop {
            if server.connect().await.is_ok() {
                let mut buf = vec![0u8; 4096];
                let n = server.read(&mut buf).await.unwrap_or(0);
                let msg = String::from_utf8_lossy(&buf[..n]).trim().to_string();

                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.unminimize();
                    let _ = w.show();
                    let _ = w.set_focus();
                }

                if msg.starts_with("http://") || msg.starts_with("https://") {
                    let _ = app.emit("external-url", msg);
                }

                server = match ServerOptions::new().create(pipe_name) {
                    Ok(s) => s,
                    Err(_) => break,
                };
            }
        }
    }
}

#[cfg(not(windows))]
mod win {
    use std::path::Path;

    pub fn set_prevent_sleep(_enabled: bool) {}
    pub fn sync_prevent_sleep(_active: usize, _on: bool) {}
    pub fn set_run_at_startup(_n: &str, _exe: &Path, _en: bool, _min: bool) {}
    pub fn open_folder(path: &Path) -> Result<(), String> {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    pub fn open_file_in_folder(path: &Path) -> Result<(), String> {
        open_folder(path)
    }

    pub struct SingleInstanceGuard;
    pub fn try_acquire_single_instance(_name: &str) -> Option<SingleInstanceGuard> {
        Some(SingleInstanceGuard)
    }
    pub fn focus_existing_window(_title: &str) {}
    pub fn send_to_first_instance(_url: Option<&str>) -> bool { false }
    pub async fn run_single_instance_server(_app: tauri::AppHandle) {}
}

pub use win::*;
