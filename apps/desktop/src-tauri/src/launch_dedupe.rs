//! Suppress duplicate `external-url` emissions (protocol handler + extension).

use std::sync::Mutex;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(3);

struct State {
    last: Option<(String, Instant)>,
}

static STATE: Mutex<State> = Mutex::new(State { last: None });

pub fn accept_external_url(url: &str) -> bool {
    let url = url.trim().to_string();
    if url.is_empty() {
        return false;
    }
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    if let Some((ref prev, at)) = guard.last {
        if prev == &url && now.duration_since(at) < WINDOW {
            return false;
        }
    }
    guard.last = Some((url, now));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupes_within_window() {
        assert!(accept_external_url("magnet:?xt=urn:btih:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert!(!accept_external_url("magnet:?xt=urn:btih:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert!(accept_external_url("magnet:?xt=urn:btih:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
    }
}