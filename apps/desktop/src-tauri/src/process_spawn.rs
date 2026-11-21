//! Shared child-process spawn policy for the desktop app.
//!
//! On Windows, console-subsystem sidecars (jyroxa CLI, yt-dlp, …) must not get a
//! visible console: `CREATE_NO_WINDOW` plus null stdin. Long-lived sidecars are
//! also assigned to a job object with `KILL_ON_JOB_CLOSE` so they die with the app.

use std::process::Stdio;

/// Win32 `CREATE_NO_WINDOW` — hide console windows for console-subsystem children.
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Creation flags applied to every desktop-spawned child on this platform.
pub fn child_creation_flags() -> u32 {
    #[cfg(windows)]
    {
        CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// Stdin null + Windows CREATE_NO_WINDOW for `std::process::Command`.
pub fn configure_std(cmd: &mut std::process::Command) {
    cmd.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(child_creation_flags());
    }
}

/// Stdin null + Windows CREATE_NO_WINDOW for `tokio::process::Command`.
pub fn configure_tokio(cmd: &mut tokio::process::Command) {
    cmd.stdin(Stdio::null());
    #[cfg(windows)]
    {
        // tokio::process::Command provides an inherent `creation_flags` on Windows.
        cmd.creation_flags(child_creation_flags());
    }
}

/// Assign a spawned tokio child to the app job so it dies when the app exits.
pub fn bind_lifetime_to_app(child: &tokio::process::Child) {
    #[cfg(windows)]
    {
        if let Some(handle) = child.raw_handle() {
            job::assign_current_job(handle);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = child;
    }
}

#[cfg(windows)]
mod job {
    use std::os::windows::raw::HANDLE;
    use std::ptr;
    use std::sync::OnceLock;

    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: u32 = 9;

    #[repr(C)]
    struct JobObjectBasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    struct JobObjectExtendedLimitInformation {
        basic_limit_information: JobObjectBasicLimitInformation,
        io_info: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(attrs: *mut core::ffi::c_void, name: *const u16) -> HANDLE;
        fn SetInformationJobObject(
            job: HANDLE,
            info_class: u32,
            info: *mut core::ffi::c_void,
            info_len: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: HANDLE, process: HANDLE) -> i32;
        fn CloseHandle(handle: HANDLE) -> i32;
    }

    struct JobHandle(HANDLE);

    // SAFETY: kernel32 job handles are process-scoped; we only create one and
    // never close it while the app runs (intentional leak until process exit).
    unsafe impl Send for JobHandle {}
    unsafe impl Sync for JobHandle {}

    impl Drop for JobHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }

    fn app_job() -> Option<&'static JobHandle> {
        static JOB: OnceLock<Option<JobHandle>> = OnceLock::new();
        JOB.get_or_init(|| {
            unsafe {
                let handle = CreateJobObjectW(ptr::null_mut(), ptr::null());
                if handle.is_null() {
                    return None;
                }
                let mut info = JobObjectExtendedLimitInformation {
                    basic_limit_information: JobObjectBasicLimitInformation {
                        per_process_user_time_limit: 0,
                        per_job_user_time_limit: 0,
                        limit_flags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                        minimum_working_set_size: 0,
                        maximum_working_set_size: 0,
                        active_process_limit: 0,
                        affinity: 0,
                        priority_class: 0,
                        scheduling_class: 0,
                    },
                    io_info: IoCounters {
                        read_operation_count: 0,
                        write_operation_count: 0,
                        other_operation_count: 0,
                        read_transfer_count: 0,
                        write_transfer_count: 0,
                        other_transfer_count: 0,
                    },
                    process_memory_limit: 0,
                    job_memory_limit: 0,
                    peak_process_memory_used: 0,
                    peak_job_memory_used: 0,
                };
                let ok = SetInformationJobObject(
                    handle,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                    &mut info as *mut _ as *mut core::ffi::c_void,
                    std::mem::size_of::<JobObjectExtendedLimitInformation>() as u32,
                );
                if ok == 0 {
                    CloseHandle(handle);
                    return None;
                }
                Some(JobHandle(handle))
            }
        })
        .as_ref()
    }

    pub(super) fn assign_current_job(process: HANDLE) {
        let Some(job) = app_job() else {
            return;
        };
        if process.is_null() {
            return;
        }
        unsafe {
            let _ = AssignProcessToJobObject(job.0, process);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_spawn_sets_create_no_window() {
        #[cfg(windows)]
        {
            assert_eq!(
                child_creation_flags(),
                CREATE_NO_WINDOW,
                "desktop child spawns must set CREATE_NO_WINDOW (0x08000000)"
            );
            assert_eq!(child_creation_flags(), 0x0800_0000);
        }
        #[cfg(not(windows))]
        {
            assert_eq!(child_creation_flags(), 0);
        }
    }

    #[test]
    fn configure_tokio_applies_flags_without_panic() {
        let mut cmd = tokio::process::Command::new("cmd");
        configure_tokio(&mut cmd);
        // Flags are applied in-place; smoke that the builder remains usable.
        let _ = cmd;
    }

    /// Spawns a long-lived child with the shared helper and asserts it has no
    /// visible main window (and no `conhost` child) while running.
    #[cfg(windows)]
    #[tokio::test]
    async fn hidden_child_has_zero_main_window_handle() {
        use std::time::Duration;

        let mut cmd = tokio::process::Command::new("cmd");
        cmd.args(["/C", "ping", "-n", "8", "127.0.0.1", ">NUL"]);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        configure_tokio(&mut cmd);
        let mut child = cmd.spawn().expect("spawn hidden ping");
        bind_lifetime_to_app(&child);
        let pid = child.id().expect("pid") as u32;

        tokio::time::sleep(Duration::from_millis(400)).await;

        let main_hwnd = main_window_handle(pid);
        assert_eq!(
            main_hwnd, 0,
            "CREATE_NO_WINDOW child must not expose a main window (hwnd={main_hwnd:#x})"
        );
        for conhost_pid in conhost_children(pid) {
            let hwnd = main_window_handle(conhost_pid);
            assert_eq!(
                hwnd, 0,
                "conhost child {conhost_pid} must not have a visible window (hwnd={hwnd:#x})"
            );
        }

        let _ = child.kill().await;
        let _ = child.wait().await;
    }

    #[cfg(windows)]
    fn main_window_handle(pid: u32) -> isize {
        use std::sync::atomic::{AtomicIsize, Ordering};

        static FOUND: AtomicIsize = AtomicIsize::new(0);

        FOUND.store(0, Ordering::SeqCst);

        #[link(name = "user32")]
        extern "system" {
            fn EnumWindows(
                cb: unsafe extern "system" fn(hwnd: *mut core::ffi::c_void, lparam: isize) -> i32,
                lparam: isize,
            ) -> i32;
            fn GetWindowThreadProcessId(hwnd: *mut core::ffi::c_void, pid: *mut u32) -> u32;
            fn IsWindowVisible(hwnd: *mut core::ffi::c_void) -> i32;
            fn GetWindow(hwnd: *mut core::ffi::c_void, cmd: u32) -> *mut core::ffi::c_void;
        }

        const GW_OWNER: u32 = 4;

        unsafe extern "system" fn callback(hwnd: *mut core::ffi::c_void, target_pid: isize) -> i32 {
            let mut window_pid = 0u32;
            unsafe {
                GetWindowThreadProcessId(hwnd, &mut window_pid);
                if window_pid as isize != target_pid {
                    return 1;
                }
                if IsWindowVisible(hwnd) == 0 {
                    return 1;
                }
                if !GetWindow(hwnd, GW_OWNER).is_null() {
                    return 1;
                }
                FOUND.store(hwnd as isize, Ordering::SeqCst);
            }
            0
        }

        unsafe {
            let _ = EnumWindows(callback, pid as isize);
        }
        FOUND.load(Ordering::SeqCst)
    }

    #[cfg(windows)]
    fn conhost_children(parent_pid: u32) -> Vec<u32> {
        use std::mem::{size_of, zeroed};

        #[repr(C)]
        struct ProcessEntry32W {
            dw_size: u32,
            cnt_usage: u32,
            th32_process_id: u32,
            th32_default_heap_id: usize,
            th32_module_id: u32,
            cnt_threads: u32,
            th32_parent_process_id: u32,
            pc_pri_class_base: i32,
            dw_flags: u32,
            sz_exe_file: [u16; 260],
        }

        #[link(name = "kernel32")]
        extern "system" {
            fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> *mut core::ffi::c_void;
            fn Process32FirstW(snap: *mut core::ffi::c_void, pe: *mut ProcessEntry32W) -> i32;
            fn Process32NextW(snap: *mut core::ffi::c_void, pe: *mut ProcessEntry32W) -> i32;
            fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
        }

        const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
        const INVALID: isize = -1;

        let mut out = Vec::new();
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap.is_null() || snap as isize == INVALID {
                return out;
            }
            let mut pe: ProcessEntry32W = zeroed();
            pe.dw_size = size_of::<ProcessEntry32W>() as u32;
            if Process32FirstW(snap, &mut pe) != 0 {
                loop {
                    if pe.th32_parent_process_id == parent_pid {
                        let name = String::from_utf16_lossy(
                            &pe.sz_exe_file[..pe
                                .sz_exe_file
                                .iter()
                                .position(|&c| c == 0)
                                .unwrap_or(pe.sz_exe_file.len())],
                        );
                        if name.eq_ignore_ascii_case("conhost.exe") {
                            out.push(pe.th32_process_id);
                        }
                    }
                    if Process32NextW(snap, &mut pe) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snap);
        }
        out
    }
}
