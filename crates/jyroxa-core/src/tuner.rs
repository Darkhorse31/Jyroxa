//! Throughput-driven auto-tuner state machine.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use crate::segment::PROBE_MIN_REMAINING;

/// Input provided to the tuner on each tick.
#[derive(Debug, Clone)]
pub struct TunerInput {
    /// Measured throughput in bytes/sec over the last interval.
    pub throughput: f64,
    /// Number of currently active workers.
    pub active_connections: usize,
    /// Number of throttle events (429/503) since last tick.
    pub throttle_events: u64,
    /// Number of generic errors since last tick.
    pub error_events: u64,
    /// Bytes left in the download (for probe gating).
    pub bytes_remaining: u64,
}

/// Output produced by the tuner after each tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunerOutput {
    /// The desired number of connections.
    pub target: usize,
}

/// Phase of the tuner state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Probing,
    Holding,
}

/// Per-host throughput record at a given connection count.
#[derive(Debug, Clone)]
struct Sample {
    connections: usize,
    throughput: f64,
}

/// Connection ramp used while probing upward.
const PROBE_LADDER: &[usize] = &[4, 8, 12, 16];

/// The tuner state machine.
#[derive(Debug)]
pub struct Tuner {
    target: usize,
    max_connections: usize,
    host_cap: usize,
    best: Sample,
    /// Best post-warmup smoothed throughput per configured target connection count.
    per_count_best: HashMap<usize, f64>,
    smoothed: f64,
    alpha: f64,
    phase: Phase,
    ticks_at_current: usize,
    warmup_ticks: usize,
    target_before_change: usize,
    throughput_before_change: f64,
    hold_ticks: usize,
    re_probe_interval: usize,
    was_throttled: bool,
    /// Best throughput measured at 1 connection (optional floor for fallbacks).
    one_conn_floor: Option<f64>,
    /// Recent raw throughput samples for median comparisons.
    recent_tp: VecDeque<f64>,
    /// Samples collected just before the last target increase.
    baseline_tp: VecDeque<f64>,
}

impl Tuner {
    pub fn new(initial_target: usize, max_connections: usize, host_cap: Option<usize>) -> Self {
        let cap = host_cap
            .unwrap_or(max_connections)
            .clamp(1, max_connections);
        let target = initial_target.clamp(1, cap.min(max_connections));
        Tuner {
            target,
            max_connections,
            host_cap: cap,
            best: Sample {
                connections: target,
                throughput: 0.0,
            },
            per_count_best: HashMap::new(),
            smoothed: 0.0,
            alpha: 0.3,
            phase: Phase::Probing,
            ticks_at_current: 0,
            // ~2 s warmup at 500 ms supervisor ticks (ignore first ~1 s, then measure).
            warmup_ticks: 4,
            target_before_change: target,
            throughput_before_change: 0.0,
            hold_ticks: 0,
            // Re-probe about every 10 s.
            re_probe_interval: 20,
            was_throttled: false,
            one_conn_floor: None,
            recent_tp: VecDeque::new(),
            baseline_tp: VecDeque::new(),
        }
    }

    fn push_sample(buf: &mut VecDeque<f64>, value: f64, cap: usize) {
        if value <= 0.0 {
            return;
        }
        buf.push_back(value);
        while buf.len() > cap {
            buf.pop_front();
        }
    }

    fn median(samples: &[f64]) -> f64 {
        if samples.is_empty() {
            return 0.0;
        }
        let mut sorted: Vec<f64> = samples.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        sorted[sorted.len() / 2]
    }

    fn probing_viable(&self, input: &TunerInput) -> bool {
        if input.bytes_remaining < PROBE_MIN_REMAINING {
            return false;
        }
        if input.throughput > 0.0 && (input.bytes_remaining as f64 / input.throughput) < 2.0 {
            return false;
        }
        true
    }

    pub fn target(&self) -> usize {
        self.target
    }

    pub fn best_connections(&self) -> usize {
        self.best.connections
    }

    pub fn best_throughput(&self) -> f64 {
        self.best.throughput
    }

    pub fn host_cap(&self) -> usize {
        self.host_cap
    }

    fn effective_best_throughput(&self) -> f64 {
        let mut floor = self.best.throughput;
        if let Some(one) = self.one_conn_floor {
            floor = floor.max(one);
        }
        floor
    }

    fn warmup_ticks_for_target(&self) -> usize {
        // Short warm-up when stepping up from a single fast connection.
        if self.target <= 2 {
            2
        } else {
            self.warmup_ticks
        }
    }

    fn record_post_warmup_sample(&mut self) {
        if self.ticks_at_current < self.warmup_ticks_for_target() || self.smoothed <= 0.0 {
            return;
        }
        let count = self.target;
        let entry = self.per_count_best.entry(count).or_insert(0.0);
        if self.smoothed > *entry {
            *entry = self.smoothed;
        }
        if self.smoothed > self.best.throughput {
            self.best.throughput = self.smoothed;
            self.best.connections = count;
        }
    }

    fn best_measured_count(&self) -> usize {
        self.per_count_best
            .iter()
            .filter(|(c, tp)| **c <= self.host_cap && **tp > 0.0)
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(c, _)| *c)
            .unwrap_or(self.best.connections.max(1))
            .clamp(1, self.host_cap)
    }

    fn maybe_fallback(&mut self, best_tp: f64) -> bool {
        if self.ticks_at_current < self.warmup_ticks_for_target() || best_tp <= 0.0 {
            return false;
        }
        if self.smoothed < best_tp * 0.85 {
            let fallback = self.best_measured_count();
            if fallback != self.target {
                self.target = fallback;
                self.ticks_at_current = 0;
                self.phase = Phase::Holding;
                self.hold_ticks = 0;
                return true;
            }
        }
        false
    }

    pub fn tick(&mut self, input: &TunerInput) -> TunerOutput {
        if self.smoothed <= 0.0 {
            self.smoothed = input.throughput;
        } else {
            self.smoothed = self.alpha * input.throughput + (1.0 - self.alpha) * self.smoothed;
        }

        if self.target == 1 && input.active_connections <= 1 && input.throughput > 0.0 {
            self.one_conn_floor = Some(
                self.one_conn_floor
                    .map(|v| v.max(input.throughput))
                    .unwrap_or(input.throughput),
            );
        }

        self.ticks_at_current += 1;
        Self::push_sample(&mut self.recent_tp, input.throughput, 7);
        self.record_post_warmup_sample();

        if input.throttle_events > 0 {
            self.handle_throttle();
            return TunerOutput {
                target: self.target,
            };
        }

        // Generic transport errors must not collapse the target.
        let _ = input.error_events;

        let best_tp = self.effective_best_throughput();
        if self.maybe_fallback(best_tp) {
            return TunerOutput {
                target: self.target,
            };
        }

        match self.phase {
            Phase::Probing => self.tick_probing(input),
            Phase::Holding => self.tick_holding(input),
        }

        TunerOutput {
            target: self.target,
        }
    }

    fn handle_throttle(&mut self) {
        let new_target = (self.target / 2).max(1);
        let cap_at = self.target.saturating_sub(1).max(1);
        self.host_cap = self.host_cap.min(cap_at);
        self.target = new_target.min(self.host_cap);
        self.was_throttled = true;
        self.phase = Phase::Holding;
        self.hold_ticks = 0;
        self.ticks_at_current = 0;
        // Wait longer before probing again after a throttle (≈20s at 500ms ticks).
        self.re_probe_interval = 40;
    }

    fn next_ladder_target(&self) -> Option<usize> {
        if self.target < 4 {
            let next = (self.target + 1).min(4);
            if next > self.target && next <= self.host_cap.min(self.max_connections) {
                return Some(next);
            }
        }
        for &step in PROBE_LADDER {
            if step > self.target && step <= self.host_cap && step <= self.max_connections {
                return Some(step);
            }
        }
        None
    }

    fn tick_probing(&mut self, input: &TunerInput) {
        if self.ticks_at_current < self.warmup_ticks_for_target() {
            return;
        }

        if !self.probing_viable(input) {
            self.phase = Phase::Holding;
            self.hold_ticks = 0;
            return;
        }

        const IMPROVEMENT: f64 = 0.06;
        const MEDIAN_SAMPLES: usize = 5;

        if self.target > self.target_before_change {
            let recent: Vec<f64> = self.recent_tp.iter().copied().collect();
            let baseline: Vec<f64> = self.baseline_tp.iter().copied().collect();
            let recent_med = Self::median(&recent);
            let base_med = Self::median(&baseline).max(self.throughput_before_change);
            let improved = base_med > 0.0 && recent_med >= base_med * (1.0 + IMPROVEMENT);
            if !improved {
                let fallback = if self.one_conn_floor.is_some() {
                    1
                } else {
                    self.best_measured_count()
                };
                self.target = fallback;
                self.phase = Phase::Holding;
                self.hold_ticks = 0;
                self.ticks_at_current = 0;
                self.re_probe_interval = 40;
                return;
            }
        }

        if self.target == 1 {
            if self.smoothed >= 50_000_000.0 {
                self.phase = Phase::Holding;
                self.hold_ticks = 0;
                self.best.throughput = self.smoothed;
                self.best.connections = 1;
                return;
            }
            if self.smoothed >= 2_000_000.0
                && self.ticks_at_current < self.warmup_ticks_for_target() + MEDIAN_SAMPLES
            {
                return;
            }
            if let Some(floor) = self.one_conn_floor {
                if self.smoothed >= 2_000_000.0 && self.smoothed < floor * 0.92 {
                    return;
                }
                // Single stream already saturates a fast link — extra connections only hurt.
                if self.smoothed >= 50_000_000.0 && self.smoothed >= floor * 0.97 {
                    self.phase = Phase::Holding;
                    self.hold_ticks = 0;
                    self.best.throughput = self.smoothed;
                    self.best.connections = 1;
                    return;
                }
            }
            if self.smoothed > 0.0
                && self.smoothed < 2_000_000.0
                && self.ticks_at_current >= self.warmup_ticks_for_target() + 2
                && self.probing_viable(input)
            {
                let jump = self
                    .max_connections
                    .min(self.host_cap)
                    .min(8)
                    .max(2);
                self.baseline_tp = self.recent_tp.clone();
                self.throughput_before_change =
                    Self::median(&self.baseline_tp.iter().copied().collect::<Vec<_>>());
                self.target = jump;
                self.target_before_change = jump;
                self.ticks_at_current = 0;
                return;
            }
        }

        if self.target == 2 {
            if let Some(floor) = self.one_conn_floor {
                if self.ticks_at_current >= 3 {
                    let recent_med =
                        Self::median(&self.recent_tp.iter().copied().collect::<Vec<_>>());
                    if recent_med < floor * 0.97 {
                        self.target = 1;
                        self.phase = Phase::Holding;
                        self.hold_ticks = 0;
                        self.ticks_at_current = 0;
                        self.re_probe_interval = 40;
                        return;
                    }
                }
            }
        }

        if let Some(floor) = self.one_conn_floor {
            let recent_med = Self::median(&self.recent_tp.iter().copied().collect::<Vec<_>>());
            let scale_req = if floor < 10_000_000.0 { 0.85 } else { 0.95 };
            if self.target >= 2
                && self.probing_viable(input)
                && recent_med >= floor * (self.target as f64) * scale_req
            {
                let doubled = if floor < 10_000_000.0 && self.target == 2 {
                    self.max_connections.min(self.host_cap).min(8)
                } else {
                    self.target.saturating_mul(2)
                };
                let jump = doubled
                    .min(self.host_cap)
                    .min(self.max_connections)
                    .max(self.target + 1);
                if jump > self.target {
                    self.baseline_tp = self.recent_tp.clone();
                    self.throughput_before_change = recent_med;
                    self.target = jump;
                    // Saturation scale-up already validated aggregate throughput.
                    self.target_before_change = jump;
                    self.ticks_at_current = 0;
                    return;
                }
            }
        }

        if let Some(next) = self.next_ladder_target() {
            self.baseline_tp = self.recent_tp.clone();
            self.throughput_before_change =
                Self::median(&self.baseline_tp.iter().copied().collect::<Vec<_>>())
                    .max(self.smoothed);
            self.target_before_change = self.target;
            self.target = next;
            self.ticks_at_current = 0;
        } else if self.target < self.host_cap && self.target < self.max_connections {
            self.baseline_tp = self.recent_tp.clone();
            self.throughput_before_change =
                Self::median(&self.baseline_tp.iter().copied().collect::<Vec<_>>())
                    .max(self.smoothed);
            self.target_before_change = self.target;
            self.target = (self.target + 2)
                .min(self.host_cap)
                .min(self.max_connections);
            self.ticks_at_current = 0;
        } else {
            self.phase = Phase::Holding;
            self.hold_ticks = 0;
        }
    }

    fn tick_holding(&mut self, input: &TunerInput) {
        self.hold_ticks += 1;

        let best_tp = self.effective_best_throughput();
        if self.maybe_fallback(best_tp) {
            return;
        }

        if self.hold_ticks >= self.re_probe_interval {
            if !self.probing_viable(input) {
                self.hold_ticks = 0;
                return;
            }
            let tried_multi = self.per_count_best.keys().any(|c| *c > 1);
            if tried_multi
                && self.best.connections == 1
                && self
                    .one_conn_floor
                    .map(|f| self.best.throughput >= f * 0.97)
                    .unwrap_or(false)
            {
                self.hold_ticks = 0;
                return;
            }
            self.baseline_tp = self.recent_tp.clone();
            self.throughput_before_change =
                Self::median(&self.baseline_tp.iter().copied().collect::<Vec<_>>())
                    .max(self.smoothed);
            self.target_before_change = self.target;
            self.ticks_at_current = 0;
            self.hold_ticks = 0;
            self.re_probe_interval = 20;

            if self.was_throttled {
                self.was_throttled = false;
                if self.target < self.host_cap && self.target < self.max_connections {
                    self.target += 1;
                    self.phase = Phase::Holding;
                }
            } else if let Some(next) = self.next_ladder_target() {
                self.phase = Phase::Probing;
                self.target = next;
            } else if self.target < self.host_cap && self.target < self.max_connections {
                self.phase = Phase::Probing;
                self.target = (self.target + 2)
                    .min(self.host_cap)
                    .min(self.max_connections);
            }
        }
    }
}

/// Parse a `Retry-After` header value.
pub fn parse_retry_after(value: &str, max_delay: Duration) -> Option<Duration> {
    let value = value.trim();

    if let Ok(secs) = value.parse::<u64>() {
        let dur = Duration::from_secs(secs).min(max_delay);
        return Some(dur);
    }

    if let Some(dur) = parse_http_date_delay(value) {
        return Some(dur.min(max_delay));
    }

    None
}

fn parse_http_date_delay(value: &str) -> Option<Duration> {
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() < 6 {
        return None;
    }

    let day: u32 = parts[1].parse().ok()?;
    let month_idx = months.iter().position(|&m| m == parts[2])?;
    let year: i64 = parts[3].parse().ok()?;
    let time_parts: Vec<&str> = parts[4].split(':').collect();
    if time_parts.len() != 3 {
        return None;
    }
    let hour: u32 = time_parts[0].parse().ok()?;
    let min: u32 = time_parts[1].parse().ok()?;
    let sec: u32 = time_parts[2].parse().ok()?;

    let y = if month_idx < 2 { year - 1 } else { year };
    let m = month_idx as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let month_adj = if m < 2 { m + 10 } else { m - 2 };
    let doy = (153 * month_adj + 2) / 5 + day as i64 - 1;
    let doe = yoe as i64 * 365 + yoe as i64 / 4 - yoe as i64 / 100 + doy;
    let days = era * 146097 + doe - 719468;
    let target_secs = days * 86400 + hour as i64 * 3600 + min as i64 * 60 + sec as i64;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    let now_secs = now.as_secs() as i64;

    if target_secs > now_secs {
        Some(Duration::from_secs((target_secs - now_secs) as u64))
    } else {
        Some(Duration::from_secs(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(throughput: f64, active: usize, throttle: u64, errors: u64) -> TunerInput {
        TunerInput {
            throughput,
            active_connections: active,
            throttle_events: throttle,
            error_events: errors,
            bytes_remaining: 100 << 20,
        }
    }

    fn warm_tick(tuner: &mut Tuner, tp: f64, active: usize) {
        for _ in 0..4 {
            tuner.tick(&input(tp, active, 0, 0));
        }
        tuner.tick(&input(tp, active, 0, 0));
    }

    #[test]
    fn ramps_up_while_throughput_rises() {
        let mut tuner = Tuner::new(4, 16, None);
        assert_eq!(tuner.target(), 4);

        for tp in [1_000_000.0, 1_100_000.0, 1_200_000.0, 1_300_000.0] {
            tuner.tick(&input(tp, 4, 0, 0));
        }
        let out = tuner.tick(&input(1_400_000.0, 4, 0, 0));
        assert!(out.target >= 4, "expected ramp from 4, got {}", out.target);
    }

    #[test]
    fn stops_and_reverts_when_throughput_plateaus() {
        let mut tuner = Tuner::new(4, 16, None);

        for _ in 0..6 {
            tuner.tick(&input(5_000_000.0, 4, 0, 0));
        }
        tuner.tick(&input(5_500_000.0, 4, 0, 0));

        let after_increase = tuner.target();
        assert!(after_increase >= 4);

        for _ in 0..8 {
            tuner.tick(&input(5_050_000.0, after_increase, 0, 0));
        }

        assert!(tuner.target() <= after_increase.max(4));
    }

    #[test]
    fn no_premature_fallback_during_warmup_after_ramp() {
        let mut tuner = Tuner::new(8, 16, None);
        tuner.best.throughput = 8_000_000.0;
        tuner.best.connections = 8;
        tuner.per_count_best.insert(8, 8_000_000.0);
        tuner.smoothed = 8_000_000.0;
        tuner.phase = Phase::Holding;
        tuner.ticks_at_current = 1;

        for tp in [400_000.0, 350_000.0, 300_000.0] {
            let out = tuner.tick(&input(tp, 8, 0, 0));
            assert_eq!(
                out.target, 8,
                "smoothed<best fallback must not run during post-change warm-up"
            );
        }
    }

    #[test]
    fn fallback_picks_best_measured_count() {
        let mut tuner = Tuner::new(8, 16, None);
        tuner.per_count_best.insert(4, 9_000_000.0);
        tuner.per_count_best.insert(8, 4_000_000.0);
        tuner.best.throughput = 9_000_000.0;
        tuner.best.connections = 4;
        tuner.smoothed = 500_000.0;
        tuner.ticks_at_current = tuner.warmup_ticks;

        let out = tuner.tick(&input(500_000.0, 8, 0, 0));
        assert_eq!(out.target, 4, "expected fallback to faster 4-conn sample");
    }

    #[test]
    fn one_conn_floor_only_when_target_is_one() {
        let mut tuner = Tuner::new(8, 16, None);
        for _ in 0..10 {
            tuner.tick(&input(500_000.0, 1, 0, 0));
        }
        assert!(tuner.one_conn_floor.is_none());

        let mut tuner = Tuner::new(1, 16, None);
        warm_tick(&mut tuner, 900_000.0, 1);
        assert!(tuner.one_conn_floor.is_some());
    }

    #[test]
    fn halves_on_throttle_and_respects_cap() {
        let mut tuner = Tuner::new(8, 16, None);
        assert_eq!(tuner.target(), 8);

        let out = tuner.tick(&input(5_000_000.0, 8, 1, 0));
        assert_eq!(out.target, 4);
        assert!(tuner.host_cap() <= 7);

        let out = tuner.tick(&input(3_000_000.0, 4, 1, 0));
        assert_eq!(out.target, 2);
        assert!(tuner.host_cap() <= 3);

        for _ in 0..50 {
            tuner.tick(&input(10_000_000.0, 2, 0, 0));
        }
        assert!(tuner.target() <= tuner.host_cap());
    }

    #[test]
    fn throttle_cap_can_be_one() {
        let mut tuner = Tuner::new(2, 16, None);
        let out = tuner.tick(&input(1_000_000.0, 2, 1, 0));
        assert_eq!(out.target, 1);
        assert_eq!(tuner.host_cap(), 1);
    }

    #[test]
    fn never_goes_below_one() {
        let mut tuner = Tuner::new(1, 16, None);
        for _ in 0..10 {
            let out = tuner.tick(&input(100.0, 1, 1, 0));
            assert!(out.target >= 1, "target went below 1: {}", out.target);
        }
    }

    #[test]
    fn never_goes_above_cap() {
        let mut tuner = Tuner::new(4, 16, Some(3));
        for i in 0..30 {
            tuner.tick(&input(1_000_000.0 * (i as f64 + 1.0), tuner.target(), 0, 0));
        }
        assert!(tuner.target() <= 3);
    }

    #[test]
    fn errors_do_not_drop_target() {
        let mut tuner = Tuner::new(8, 16, None);
        let out = tuner.tick(&input(5_000_000.0, 8, 0, 5));
        assert_eq!(out.target, 8);
    }

    #[test]
    fn retry_after_seconds() {
        let dur = parse_retry_after("120", Duration::from_secs(300)).unwrap();
        assert_eq!(dur, Duration::from_secs(120));
    }
}
