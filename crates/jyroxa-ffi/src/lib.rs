//! C ABI + JNI bindings for Jyroxa.
//!
//! This crate exposes a minimal but working control surface for embedding the
//! engine in other runtimes — in particular Android via JNI. Downloads run on a
//! shared Tokio runtime; each is identified by an `i64` handle and can be
//! started, paused, resumed, cancelled and polled for status (as JSON).
//!
//! Two parallel surfaces are provided over the same core:
//!   * a plain C ABI (`jyroxa_*`) for generic embedding / `dlopen`, and
//!   * JNI entry points (`Java_com_jyroxa_app_JyroxaFfi_*`) for Android.

use std::collections::HashMap;
use std::ffi::{c_char, c_int, CStr, CString};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use once_cell::sync::Lazy;
use tokio::runtime::Runtime;

use jyroxa_core::download::{Control, DownloadConfig, DownloadRequest, Downloader};
use jyroxa_core::events::{self, ProgressEvent};

static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build Tokio runtime")
});

static ENGINE: Lazy<Arc<Downloader>> = Lazy::new(|| {
    Arc::new(Downloader::new(DownloadConfig::default()).expect("failed to build Downloader"))
});

static REGISTRY: Lazy<Mutex<HashMap<i64, Handle>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static NEXT_ID: AtomicI64 = AtomicI64::new(1);

#[derive(Clone)]
struct Handle {
    control: Control,
    downloaded: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
    speed: Arc<Mutex<f64>>,
    state: Arc<Mutex<String>>,
}

/// Start a download. Returns a handle id (>0) or 0 on bad input.
fn start_impl(urls: Vec<String>, output: String, connections: usize) -> i64 {
    if urls.is_empty() || output.is_empty() {
        return 0;
    }
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);

    let handle = Handle {
        control: Control::new(),
        downloaded: Arc::new(AtomicU64::new(0)),
        total: Arc::new(AtomicU64::new(0)),
        speed: Arc::new(Mutex::new(0.0)),
        state: Arc::new(Mutex::new("starting".to_string())),
    };
    REGISTRY.lock().unwrap().insert(id, handle.clone());

    let (tx, mut rx) = events::channel();

    // Event consumer: translate progress events into the shared status fields.
    {
        let h = handle.clone();
        RUNTIME.spawn(async move {
            while let Some(ev) = rx.recv().await {
                match ev {
                    ProgressEvent::Started { total_size, .. } => {
                        if let Some(t) = total_size {
                            h.total.store(t, Ordering::Relaxed);
                        }
                        *h.state.lock().unwrap() = "running".into();
                    }
                    ProgressEvent::Progress {
                        downloaded,
                        total,
                        speed_bps,
                        ..
                    } => {
                        h.downloaded.store(downloaded, Ordering::Relaxed);
                        if let Some(t) = total {
                            h.total.store(t, Ordering::Relaxed);
                        }
                        *h.speed.lock().unwrap() = speed_bps;
                    }
                    ProgressEvent::Completed { total_size, .. } => {
                        h.downloaded.store(total_size, Ordering::Relaxed);
                        h.total.store(total_size, Ordering::Relaxed);
                        *h.state.lock().unwrap() = "completed".into();
                    }
                    ProgressEvent::Failed { error } => {
                        *h.state.lock().unwrap() = format!("failed: {error}");
                    }
                    _ => {}
                }
            }
        });
    }

    // The download task.
    {
        let engine = ENGINE.clone();
        let control = handle.control.clone();
        let state = handle.state.clone();
        RUNTIME.spawn(async move {
            let req = DownloadRequest {
                urls,
                output: output.into(),
                connections: connections.clamp(1, jyroxa_core::MAX_CONNECTIONS),
                speed_limit: None,
                sha256: None,
                md5: None,
                autotune: true,
            };
            match engine.download(req, Some(tx), control, None).await {
                Ok(_) => {
                    let mut s = state.lock().unwrap();
                    if !s.starts_with("failed") {
                        *s = "completed".into();
                    }
                }
                Err(jyroxa_core::Error::Cancelled) => {
                    *state.lock().unwrap() = "cancelled".into();
                }
                Err(e) => {
                    *state.lock().unwrap() = format!("failed: {e}");
                }
            }
        });
    }

    id
}

fn pause_impl(id: i64) -> bool {
    if let Some(h) = REGISTRY.lock().unwrap().get(&id) {
        h.control.pause();
        *h.state.lock().unwrap() = "paused".into();
        true
    } else {
        false
    }
}

fn resume_impl(id: i64) -> bool {
    if let Some(h) = REGISTRY.lock().unwrap().get(&id) {
        h.control.resume();
        *h.state.lock().unwrap() = "running".into();
        true
    } else {
        false
    }
}

fn cancel_impl(id: i64) -> bool {
    if let Some(h) = REGISTRY.lock().unwrap().get(&id) {
        h.control.cancel();
        true
    } else {
        false
    }
}

fn status_impl(id: i64) -> String {
    let reg = REGISTRY.lock().unwrap();
    match reg.get(&id) {
        Some(h) => {
            let downloaded = h.downloaded.load(Ordering::Relaxed);
            let total = h.total.load(Ordering::Relaxed);
            let speed = *h.speed.lock().unwrap();
            let state = h.state.lock().unwrap().clone();
            let progress = if total > 0 {
                (downloaded as f64 / total as f64 * 100.0).min(100.0)
            } else {
                0.0
            };
            serde_json::json!({
                "id": id,
                "state": state,
                "downloaded": downloaded,
                "total": total,
                "speed_bps": speed,
                "progress_pct": progress,
            })
            .to_string()
        }
        None => serde_json::json!({ "id": id, "state": "unknown" }).to_string(),
    }
}

// ---------------------------------------------------------------------------
// C ABI
// ---------------------------------------------------------------------------

unsafe fn cstr_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok().map(|s| s.to_string())
}

/// Start a single-URL download. Returns a handle id, or 0 on error.
///
/// # Safety
/// `url` and `output` must be valid, NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn jyroxa_start(
    url: *const c_char,
    output: *const c_char,
    connections: c_int,
) -> i64 {
    std::panic::catch_unwind(|| {
        let (Some(url), Some(output)) = (cstr_to_string(url), cstr_to_string(output)) else {
            return 0;
        };
        start_impl(vec![url], output, connections.max(1) as usize)
    })
    .unwrap_or(0)
}

/// Pause a download. Returns 1 on success, 0 if the id is unknown.
#[no_mangle]
pub extern "C" fn jyroxa_pause(id: i64) -> c_int {
    std::panic::catch_unwind(|| pause_impl(id) as c_int).unwrap_or(0)
}

/// Resume a paused download. Returns 1 on success, 0 if the id is unknown.
#[no_mangle]
pub extern "C" fn jyroxa_resume(id: i64) -> c_int {
    std::panic::catch_unwind(|| resume_impl(id) as c_int).unwrap_or(0)
}

/// Cancel a download. Returns 1 on success, 0 if the id is unknown.
#[no_mangle]
pub extern "C" fn jyroxa_cancel(id: i64) -> c_int {
    std::panic::catch_unwind(|| cancel_impl(id) as c_int).unwrap_or(0)
}

/// Return a JSON status string. The caller must free it with
/// [`jyroxa_free_string`].
#[no_mangle]
pub extern "C" fn jyroxa_status(id: i64) -> *mut c_char {
    let json = std::panic::catch_unwind(|| status_impl(id))
        .unwrap_or_else(|_| "{\"state\":\"error\"}".to_string());
    match CString::new(json) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free a string returned by [`jyroxa_status`].
///
/// # Safety
/// `ptr` must have been returned by [`jyroxa_status`] and not freed already.
#[no_mangle]
pub unsafe extern "C" fn jyroxa_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}

// ---------------------------------------------------------------------------
// JNI (Android) — class com.jyroxa.app.JyroxaFfi
// ---------------------------------------------------------------------------

use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jint, jlong, jstring};
use jni::JNIEnv;

/// `long nativeStart(String url, String output, int connections)`
#[no_mangle]
pub extern "system" fn Java_com_jyroxa_app_JyroxaFfi_nativeStart(
    mut env: JNIEnv,
    _class: JClass,
    url: JString,
    output: JString,
    connections: jint,
) -> jlong {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let url: String = match env.get_string(&url) {
            Ok(s) => s.into(),
            Err(_) => return 0,
        };
        let output: String = match env.get_string(&output) {
            Ok(s) => s.into(),
            Err(_) => return 0,
        };
        start_impl(vec![url], output, connections.max(1) as usize)
    }))
    .unwrap_or(0)
}

/// `boolean nativePause(long id)`
#[no_mangle]
pub extern "system" fn Java_com_jyroxa_app_JyroxaFfi_nativePause(
    _env: JNIEnv,
    _class: JClass,
    id: jlong,
) -> jboolean {
    std::panic::catch_unwind(|| pause_impl(id) as jboolean).unwrap_or(0)
}

/// `boolean nativeResume(long id)`
#[no_mangle]
pub extern "system" fn Java_com_jyroxa_app_JyroxaFfi_nativeResume(
    _env: JNIEnv,
    _class: JClass,
    id: jlong,
) -> jboolean {
    std::panic::catch_unwind(|| resume_impl(id) as jboolean).unwrap_or(0)
}

/// `boolean nativeCancel(long id)`
#[no_mangle]
pub extern "system" fn Java_com_jyroxa_app_JyroxaFfi_nativeCancel(
    _env: JNIEnv,
    _class: JClass,
    id: jlong,
) -> jboolean {
    std::panic::catch_unwind(|| cancel_impl(id) as jboolean).unwrap_or(0)
}

/// `String nativeStatus(long id)` — returns a JSON string.
#[no_mangle]
pub extern "system" fn Java_com_jyroxa_app_JyroxaFfi_nativeStatus<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: jlong,
) -> jstring {
    let json = std::panic::catch_unwind(|| status_impl(id))
        .unwrap_or_else(|_| "{\"state\":\"error\"}".to_string());
    match env.new_string(json) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_of_unknown_id() {
        let json = status_impl(999_999);
        assert!(json.contains("\"state\":\"unknown\""));
    }

    #[test]
    fn controls_unknown_id_return_false() {
        assert!(!pause_impl(123_456));
        assert!(!resume_impl(123_456));
        assert!(!cancel_impl(123_456));
    }

    #[test]
    fn rejects_empty_input() {
        assert_eq!(start_impl(vec![], "out".into(), 4), 0);
        assert_eq!(start_impl(vec!["http://x".into()], "".into(), 4), 0);
    }
}
