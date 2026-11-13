//! Aggregate-throughput connection growth with error-only backoff.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::segment::PROBE_MIN_REMAINING;

#[derive(Debug, Clone)]
pub struct TunerInput {
    /// Aggregate bytes/sec over the last interval.
    pub throughput: f64,
    pub active_connections: usize,
    pub throttle_events: u64,
    pub error_events: u64,
    pub bytes_remaining: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunerOutput {
    pub target: usize,
}

/// Start all eight workers before taking any throughput samples.
pub const DEFAULT_START_CONNECTIONS: usize = 8;
const WARMUP_TICKS: usize = 1;
const MEASURE_TICKS: usize = 3;
const GAIN: f64 = 0.03;
const REPROBE_TICKS: usize = 6;
const CAP_RELAX_TICKS: usize = 60;

pub fn initial_connections(_total: u64, max_connections: usize) -> usize {
    DEFAULT_START_CONNECTIONS.min(max_connections.max(1))
}

/// Grow promptly while aggregate throughput improves. A plateau stops further
/// growth, but never retires accepted connections. Only actual transport errors
/// or HTTP 429/503 lower the target. Samples use medians to resist transient dips.
#[derive(Debug)]
pub struct Tuner {
    target: usize,
    max_connections: usize,
    host_cap: usize,
    measured: HashMap<usize, f64>,
    samples: Vec<f64>,
    warmup: usize,
    baseline: Option<f64>,
    holding: bool,
    hold_ticks: usize,
    ticks_since_error: usize,
    /// Connection count we doubled from while evaluating the trial target.
    trial_anchor: Option<usize>,
    rejected: HashSet<usize>,
}

impl Tuner {
    pub fn new(initial_target: usize, max_connections: usize, host_cap: Option<usize>) -> Self {
        let max_connections = max_connections.max(1);
        let cap = host_cap
            .unwrap_or(max_connections)
            .clamp(1, max_connections);
        Self {
            target: initial_target.clamp(1, cap),
            max_connections,
            host_cap: cap,
            measured: HashMap::new(),
            samples: Vec::with_capacity(MEASURE_TICKS),
            warmup: WARMUP_TICKS,
            baseline: None,
            holding: false,
            hold_ticks: 0,
            ticks_since_error: 0,
            trial_anchor: None,
            rejected: HashSet::new(),
        }
    }

    pub fn target(&self) -> usize {
        self.target
    }
    pub fn host_cap(&self) -> usize {
        self.host_cap
    }
    pub fn best_connections(&self) -> usize {
        self.best().map(|(count, _)| count).unwrap_or(self.target)
    }
    pub fn best_throughput(&self) -> f64 {
        self.best().map(|(_, speed)| speed).unwrap_or(0.0)
    }

    fn best(&self) -> Option<(usize, f64)> {
        self.measured
            .iter()
            .filter(|(count, speed)| **count <= self.host_cap && **speed > 0.0)
            .max_by(|a, b| a.1.total_cmp(b.1).then(b.0.cmp(a.0)))
            .map(|(count, speed)| (*count, *speed))
    }

    fn median(samples: &[f64]) -> f64 {
        let mut sorted = samples.to_vec();
        sorted.sort_by(f64::total_cmp);
        sorted[sorted.len() / 2]
    }

    fn probing_viable(&self, input: &TunerInput) -> bool {
        input.bytes_remaining >= PROBE_MIN_REMAINING
            && (input.throughput <= 0.0 || input.bytes_remaining as f64 / input.throughput >= 1.5)
    }

    pub fn tick(&mut self, input: &TunerInput) -> TunerOutput {
        let failures = input.throttle_events.saturating_add(input.error_events);
        if failures > 0 {
            self.target = self
                .target
                .saturating_sub(failures.min(usize::MAX as u64) as usize)
                .max(1);
            self.host_cap = self.host_cap.min(self.target);
            self.measured.retain(|count, _| *count <= self.host_cap);
            self.samples.clear();
            self.baseline = None;
            self.warmup = WARMUP_TICKS;
            self.holding = true;
            self.hold_ticks = 0;
            self.ticks_since_error = 0;
            self.trial_anchor = None;
            return TunerOutput {
                target: self.target,
            };
        }
        self.ticks_since_error += 1;
        if self.host_cap < self.max_connections && self.ticks_since_error >= CAP_RELAX_TICKS {
            self.host_cap = (self.host_cap + (self.host_cap / 2).max(1)).min(self.max_connections);
            self.ticks_since_error = 0;
            self.holding = false;
            self.baseline = None;
        }
        // Do not confuse worker startup or the tail of a download with a level.
        if input.active_connections < self.target {
            self.samples.clear();
            self.warmup = WARMUP_TICKS;
            return TunerOutput {
                target: self.target,
            };
        }
        if self.warmup > 0 {
            self.warmup -= 1;
            return TunerOutput {
                target: self.target,
            };
        }
        if !input.throughput.is_finite() || input.throughput <= 0.0 {
            return TunerOutput {
                target: self.target,
            };
        }
        self.samples.push(input.throughput);
        if self.samples.len() < MEASURE_TICKS {
            return TunerOutput {
                target: self.target,
            };
        }
        let speed = Self::median(&self.samples);
        self.samples.clear();
        self.measured.insert(self.target, speed);

        if let Some(anchor) = self.trial_anchor {
            let anchor_tp = self
                .measured
                .get(&anchor)
                .copied()
                .unwrap_or(self.baseline.unwrap_or(speed));
            if speed >= anchor_tp * (1.0 + GAIN) {
                self.trial_anchor = None;
                self.baseline = Some(speed);
                self.holding = true;
                self.hold_ticks = 0;
            } else {
                self.rejected.insert(self.target);
                self.target = anchor;
                self.trial_anchor = None;
                self.holding = true;
                self.hold_ticks = 0;
                self.warmup = WARMUP_TICKS;
            }
            return TunerOutput {
                target: self.target,
            };
        }

        if self.baseline.is_none() {
            self.baseline = Some(speed);
            self.holding = true;
            self.hold_ticks = 0;
            return TunerOutput {
                target: self.target,
            };
        }

        if self.holding {
            self.hold_ticks += MEASURE_TICKS;
            if self.hold_ticks < REPROBE_TICKS {
                return TunerOutput {
                    target: self.target,
                };
            }
            self.holding = false;
            self.hold_ticks = 0;
        }

        let next = self.target.saturating_mul(2).min(self.host_cap);
        if next > self.target
            && !self.rejected.contains(&next)
            && self.probing_viable(input)
        {
            self.trial_anchor = Some(self.target);
            self.target = next;
            self.warmup = WARMUP_TICKS;
        } else {
            self.holding = true;
            self.hold_ticks = 0;
        }
        TunerOutput {
            target: self.target,
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
    const MB: f64 = 1_000_000.0;

    fn sample(speed: f64) -> TunerInput {
        TunerInput {
            throughput: speed,
            active_connections: 32,
            throttle_events: 0,
            error_events: 0,
            bytes_remaining: 1 << 30,
        }
    }

    fn run(t: &mut Tuner, ticks: usize, model: impl Fn(usize) -> f64) {
        for _ in 0..ticks {
            t.tick(&sample(model(t.target())));
        }
    }

    #[test]
    fn ranged_downloads_start_at_eight() {
        for total in [1 << 20, 100 << 20, 1 << 30] {
            assert_eq!(initial_connections(total, 32), 8);
            assert_eq!(initial_connections(total, 2), 2);
        }
    }

    #[test]
    fn probes_within_two_seconds_at_default_interval() {
        let mut t = Tuner::new(8, 32, None);
        // Baseline at 8, short hold, then trial to 16 when aggregate clearly rises.
        run(&mut t, 14, |c| if c == 8 { 10.0 * MB } else { 11.0 * MB });
        assert_eq!(t.target(), 16);
    }

    #[test]
    fn accepts_aggregate_gain_even_when_per_connection_speed_falls() {
        let mut t = Tuner::new(8, 32, None);
        run(&mut t, 60, |c| match c {
            8 => 20.0 * MB,
            16 => 25.0 * MB,
            _ => 30.0 * MB,
        });
        assert_eq!(t.target(), 32);
        assert_eq!(t.best_connections(), 32);
    }

    #[test]
    fn plateau_and_slow_samples_never_retire_connections() {
        let mut t = Tuner::new(8, 32, None);
        run(&mut t, 14, |c| if c == 8 { 20.0 * MB } else { 22.0 * MB });
        assert_eq!(t.target(), 16);
        for speed in [0.0, 1.0, 0.0, 2.0, 1.0, 1.0] {
            t.tick(&sample(speed));
            assert_eq!(t.target(), 16);
        }
    }

    #[test]
    fn isolated_slow_sample_does_not_stop_growth() {
        let mut t = Tuner::new(8, 32, None);
        for speed in [10.0 * MB, 10.0 * MB, 1.0, 10.0 * MB] {
            t.tick(&sample(speed));
        }
        assert_eq!(t.target(), 8);
        run(&mut t, 14, |c| if c == 8 { 10.0 * MB } else { 11.0 * MB });
        assert_eq!(t.target(), 16);
    }

    #[test]
    fn failed_probe_reverts_without_touching_errors() {
        let mut t = Tuner::new(8, 32, None);
        run(&mut t, 10, |_| 20.0 * MB);
        run(&mut t, 4, |c| if c == 8 { 20.0 * MB } else { 12.0 * MB });
        assert_eq!(t.target(), 8);
        assert!(t.rejected.contains(&16));
    }

    #[test]
    fn incomplete_worker_start_is_not_measured() {
        let mut t = Tuner::new(8, 32, None);
        let mut input = sample(20.0 * MB);
        input.active_connections = 4;
        for _ in 0..30 {
            t.tick(&input);
        }
        assert_eq!(t.target(), 8);
        assert_eq!(t.best_throughput(), 0.0);
    }

    #[test]
    fn error_backoff_and_cap_recovery() {
        let mut t = Tuner::new(8, 32, None);
        let mut input = sample(10.0 * MB);
        input.throttle_events = 2;
        input.error_events = 1;
        assert_eq!(t.tick(&input).target, 5);
        assert_eq!(t.host_cap(), 5);
        run(&mut t, 400, |c| c as f64 * MB);
        assert!(t.host_cap() > 5);
        assert!(t.target() > 5);
        assert!(t.target() <= 32);
    }

    #[test]
    fn repeated_errors_never_reduce_target_below_one() {
        let mut t = Tuner::new(8, 32, None);
        let mut input = sample(1.0);
        input.error_events = u64::MAX;
        for _ in 0..5 {
            assert_eq!(t.tick(&input).target, 1);
        }
    }

    #[test]
    fn short_tail_does_not_open_connections() {
        let mut t = Tuner::new(8, 32, None);
        let mut input = sample(20.0 * MB);
        input.bytes_remaining = 1 << 20;
        for _ in 0..30 {
            t.tick(&input);
        }
        assert_eq!(t.target(), 8);
    }

    #[test]
    fn honors_maximum_and_remembered_error_cap() {
        let mut t = Tuner::new(8, 6, Some(4));
        assert_eq!(t.target(), 4);
        run(&mut t, 200, |c| c as f64 * MB);
        assert!(t.target() <= 6);
    }

    #[test]
    fn retry_after_seconds() {
        assert_eq!(
            parse_retry_after("120", Duration::from_secs(30)),
            Some(Duration::from_secs(30))
        );
    }
}
