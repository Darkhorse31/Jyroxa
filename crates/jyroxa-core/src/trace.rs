//! Opt-in engine trace (`JYROXA_TRACE=1` or `SWIFTGET_TRACE=1`).

use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use crate::segment::Segment;

static ENABLED: OnceLock<bool> = OnceLock::new();

pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| {
        matches!(std::env::var("JYROXA_TRACE").as_deref(), Ok("1" | "true" | "yes"))
            || matches!(std::env::var("SWIFTGET_TRACE").as_deref(), Ok("1" | "true" | "yes"))
    })
}

pub struct Trace {
    last_tick: Instant,
    last_total: u64,
    last_seg_current: Vec<u64>,
    last_line: Mutex<Option<String>>,
}

impl Trace {
    pub fn new() -> Self {
        Trace {
            last_tick: Instant::now(),
            last_total: 0,
            last_seg_current: Vec::new(),
            last_line: Mutex::new(None),
        }
    }

    pub fn maybe_tick(
        &mut self,
        active: usize,
        busy: usize,
        target: usize,
        queued: usize,
        retries: u64,
        segments: &[std::sync::Arc<Segment>],
        total_downloaded: u64,
    ) {
        if !enabled() {
            return;
        }
        let now = Instant::now();
        let dt = now.saturating_duration_since(self.last_tick).as_secs_f64();
        if dt < 1.0 {
            return;
        }
        self.last_tick = now;

        let idle = active.saturating_sub(busy);
        let agg_bps = (total_downloaded.saturating_sub(self.last_total)) as f64 / dt;
        self.last_total = total_downloaded;

        if self.last_seg_current.len() != segments.len() {
            self.last_seg_current = segments.iter().map(|s| s.current()).collect();
        }

        let mut parts: Vec<String> = Vec::new();
        for (i, seg) in segments.iter().enumerate() {
            if seg.is_done() {
                continue;
            }
            let cur = seg.current();
            let prev = self.last_seg_current.get(i).copied().unwrap_or(cur);
            let bps = (cur.saturating_sub(prev)) as f64 / dt;
            self.last_seg_current[i] = cur;
            let rem = seg.remaining();
            parts.push(format!("s{}:{rem}@{bps:.0}", seg.index));
        }

        let seg_s = if parts.is_empty() {
            "none".into()
        } else {
            parts.join(",")
        };

        let line = format!(
            "[trace] active={active} busy={busy} idle={idle} target={target} queued={queued} \
             retries_window={retries} dl={total_downloaded} {:.2}MB/s segs=[{seg_s}]",
            agg_bps / (1024.0 * 1024.0)
        );

        {
            let mut last = self.last_line.lock().unwrap();
            if last.as_deref() != Some(&line) {
                let mut stderr = std::io::stderr().lock();
                let _ = writeln!(stderr, "{line}");
                stderr.flush().ok();
                *last = Some(line);
            }
        }
    }
}
