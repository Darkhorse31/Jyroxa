//! Windows-specific helpers (autostart, sleep prevention, shell open).

#[cfg(windows)]
mod win {
    use std::path::Path;
    use std::sync::atomic::{AtomicU32, Ordering};

    const ES_CONTINUOUS: u32 = 0x8000_0000;
    const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetThreadExecutionState(es_flags: u32) -> u32;
    }

    static SLEEP_GUARD: AtomicU32 = AtomicU32::new(0);

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
}

pub use win::*;
