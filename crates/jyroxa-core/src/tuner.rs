//! Throughput-driven auto-tuner state machine.

use std::collections::{HashMap, HashSet, VecDeque};
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
    /// Measuring a new connection count; `ascending` says which way we are walking.
    Measuring { ascending: bool },
    /// Holding the best count found; re-probes upward now and then.
    Holding,
}

/// Ticks ignored after a connection-count change (new TCP/TLS streams ramping up).
const WARMUP_TICKS: usize = 3;
/// Ticks measured after the warm-up; their median is the level's throughput.
const MEASURE_TICKS: usize = 4;
/// A higher connection count must beat the best by this much to be kept.
const GAIN: f64 = 0.08;
/// Allow at most 20% loss per connection when testing a wider level.
const PER_CONNECTION_RETENTION: f64 = 0.80;
/// Hold this many ticks between upward re-probes (~10 s at 500 ms ticks).
const REPROBE_TICKS: usize = 20;
/// After a throttle, wait this long before probing again (~20 s).
const THROTTLE_REPROBE_TICKS: usize = 40;
/// A throttle-derived cap is relaxed after this many throttle-free ticks (~30 s).
const CAP_RELAX_TICKS: usize = 60;
/// Default first connection count when nothing is known about the host.
pub const DEFAULT_START_CONNECTIONS: usize = 8;

/// Range-capable downloads start at eight, bounded by the user's maximum.
pub fn initial_connections(_total: u64, max_connections: usize) -> usize {
    DEFAULT_START_CONNECTIONS.min(max_connections.max(1))
}

/// Throughput-driven connection tuner.
///
/// It starts wide (see [`initial_connections`]) and is a measured hill-climb
/// from there: every count it tries is held for a short warm-up, then measured
/// (median of several ticks). It doubles while each step is clearly faster and
/// otherwise holds the fastest count measured. It never drops below the start
/// count except on real throttling (429/503) or transport errors, so noisy samples can't collapse
/// a download to a single slow connection.
#[derive(Debug)]
pub struct Tuner {
    target: usize,
    max_connections: usize,
    /// Current soft ceiling from throttle events or connection errors.
    host_cap: usize,
    phase: Phase,
    /// Median throughput measured at each connection count.
    measured: HashMap<usize, f64>,
    /// Levels whose per-connection throughput failed the upward probe.
    rejected: HashSet<usize>,
    /// Raw samples taken at the current count, after warm-up.
    samples: Vec<f64>,
    ticks_at_current: usize,
    hold_ticks: usize,
    reprobe_after: usize,
    ticks_since_throttle: usize,
    /// Recent samples while holding (rolling refresh of the held level).
    recent: VecDeque<f64>,
}

impl Tuner {
    /// `initial_target` is the first connection count (usually
    /// [`DEFAULT_START_CONNECTIONS`] or the host's remembered best);
    /// `host_cap` is a soft ceiling remembered from earlier throttling.
    pub fn new(initial_target: usize, max_connections: usize, host_cap: Option<usize>) -> Self {
        let max_connections = max_connections.max(1);
        let cap = host_cap
            .unwrap_or(max_connections)
            .clamp(1, max_connections);
        let target = initial_target.clamp(1, cap);
        Tuner {
            target,
            max_connections,
            host_cap: cap,
            phase: Phase::Measuring { ascending: true },
            measured: HashMap::new(),
            rejected: HashSet::new(),
            samples: Vec::new(),
            ticks_at_current: 0,
            hold_ticks: 0,
            reprobe_after: REPROBE_TICKS,
            ticks_since_throttle: 0,
            recent: VecDeque::new(),
        }
    }

    pub fn target(&self) -> usize {
        self.target
    }

    pub fn host_cap(&self) -> usize {
        self.host_cap
    }

    /// The fastest measured connection count (or the current one if none yet).
    pub fn best_connections(&self) -> usize {
        self.best().map(|(c, _)| c).unwrap_or(self.target)
    }

    /// Throughput measured at [`Self::best_connections`] (0 if none yet).
    pub fn best_throughput(&self) -> f64 {
        self.best().map(|(_, tp)| tp).unwrap_or(0.0)
    }

    fn best(&self) -> Option<(usize, f64)> {
        self.measured
            .iter()
            .filter(|(c, tp)| **c <= self.host_cap && **tp > 0.0 && !self.rejected.contains(c))
            .max_by(|a, b| {
                a.1.partial_cmp(b.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    // On a tie prefer fewer connections.
                    .then(b.0.cmp(a.0))
            })
            .map(|(c, tp)| (*c, *tp))
    }

    fn median(samples: &[f64]) -> f64 {
        if samples.is_empty() {
            return 0.0;
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = sorted.len() / 2;
        if sorted.len() % 2 == 0 {
            (sorted[mid - 1] + sorted[mid]) / 2.0
        } else {
            sorted[mid]
        }
    }

    /// Changing the connection count only pays off with enough data left.
    fn probing_viable(&self, input: &TunerInput) -> bool {
        if input.bytes_remaining < PROBE_MIN_REMAINING {
            return false;
        }
        let tp = self.best_throughput().max(input.throughput);
        !(tp > 0.0 && (input.bytes_remaining as f64 / tp) < 3.0)
    }

    fn ceiling(&self) -> usize {
        self.host_cap.min(self.max_connections).max(1)
    }

    fn switch_to(&mut self, count: usize, phase: Phase) {
        let count = count.clamp(1, self.ceiling());
        self.target = count;
        self.phase = phase;
        self.samples.clear();
        self.recent.clear();
        self.ticks_at_current = 0;
        self.hold_ticks = 0;
    }

    fn hold_best(&mut self) {
        let best = self.best_connections();
        self.switch_to(best, Phase::Holding);
    }

    pub fn tick(&mut self, input: &TunerInput) -> TunerOutput {
        self.ticks_at_current += 1;
        if input.throttle_events > 0 || input.error_events > 0 {
            self.handle_throttle(input.throttle_events.saturating_add(input.error_events));
            return TunerOutput {
                target: self.target,
            };
        }
        self.ticks_since_throttle += 1;
        self.relax_cap();

        match self.phase {
            Phase::Measuring { ascending } => self.tick_measuring(input, ascending),
            Phase::Holding => self.tick_holding(input),
        }

        TunerOutput {
            target: self.target,
        }
    }

    /// A throttle cap is a hint, not a life sentence: after a quiet period it
    /// grows back toward the configured maximum.
    fn relax_cap(&mut self) {
        if self.host_cap < self.max_connections && self.ticks_since_throttle >= CAP_RELAX_TICKS {
            self.host_cap = (self.host_cap + (self.host_cap / 2).max(1)).min(self.max_connections);
            self.ticks_since_throttle = 0;
        }
    }

    fn handle_throttle(&mut self, throttled: u64) {
        // Drop the failed/refused connections rather than halving the target.
        // Throughput fluctuations alone never lower this ceiling.
        let next = self.target.saturating_sub(throttled.max(1) as usize).max(1);
        self.host_cap = self.host_cap.min(next);
        self.ticks_since_throttle = 0;
        // Measurements taken above the new cap no longer apply.
        let cap = self.host_cap;
        self.measured.retain(|c, _| *c <= cap);
        self.switch_to(next, Phase::Holding);
        self.reprobe_after = THROTTLE_REPROBE_TICKS;
    }

    fn tick_measuring(&mut self, input: &TunerInput, ascending: bool) {
        // Do not compare a level until its workers are actually available.
        if input.active_connections < self.target {
            self.ticks_at_current = 0;
            self.samples.clear();
            return;
        }
        if self.ticks_at_current <= WARMUP_TICKS {
            return;
        }
        if input.throughput > 0.0 {
            self.samples.push(input.throughput);
        }
        if self.samples.len() < MEASURE_TICKS {
            // Not enough data left to finish measuring: keep the best so far.
            if !self.probing_viable(input) && !self.measured.is_empty() {
                self.hold_best();
            }
            return;
        }

        let level_tp = Self::median(&self.samples);
        let previous = self.best();
        let prev_best = previous.map(|(_, tp)| tp).unwrap_or(0.0);
        self.measured.insert(self.target, level_tp);
        let per_connection_holds = previous
            .map(|(count, tp)| {
                self.target <= count
                    || level_tp / self.target as f64 >= tp / count as f64 * PER_CONNECTION_RETENTION
            })
            .unwrap_or(true);
        let improved =
            (prev_best <= 0.0 || level_tp >= prev_best * (1.0 + GAIN)) && per_connection_holds;
        if !improved {
            self.rejected.insert(self.target);
            self.hold_best();
            return;
        }

        if !self.probing_viable(input) {
            self.hold_best();
            return;
        }

        if ascending {
            if improved {
                let next = self.target.saturating_mul(2).min(self.ceiling());
                if next > self.target {
                    self.switch_to(next, Phase::Measuring { ascending: true });
                } else {
                    self.hold_best();
                }
                return;
            }
            // Going up did not help: settle on the fastest count measured, which
            // is never below the starting count. Connections are only reduced on
            // real throttling or connection errors, not on noisy throughput.
            self.hold_best();
        } else {
            self.hold_best();
        }
    }

    fn tick_holding(&mut self, input: &TunerInput) {
        self.hold_ticks += 1;
        if self.ticks_at_current > WARMUP_TICKS && input.throughput > 0.0 {
            self.recent.push_back(input.throughput);
            while self.recent.len() > 8 {
                self.recent.pop_front();
            }
            // Keep the held level's figure current so later comparisons are fair
            // when the network changes under us.
            if self.recent.len() >= MEASURE_TICKS {
                let tp = Self::median(&self.recent.iter().copied().collect::<Vec<_>>());
                self.measured.insert(self.target, tp);
            }
        }

        if self.hold_ticks < self.reprobe_after || !self.probing_viable(input) {
            return;
        }
        self.reprobe_after = REPROBE_TICKS;
        let next = self.target.saturating_mul(2).min(self.ceiling());
        let tried_and_worse = self
            .measured
            .get(&next)
            .map(|tp| *tp < self.best_throughput() * (1.0 + GAIN))
            .unwrap_or(false);
        if next > self.target && !tried_and_worse && !self.rejected.contains(&next) {
            self.switch_to(next, Phase::Measuring { ascending: true });
        } else {
            self.hold_ticks = 0;
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

    fn input(throughput: f64, throttle: u64) -> TunerInput {
        TunerInput {
            throughput,
            active_connections: crate::MAX_CONNECTIONS,
            throttle_events: throttle,
            error_events: 0,
            bytes_remaining: 1 << 30,
        }
    }

    /// Drive the tuner against a model of throughput per connection count.
    fn run(tuner: &mut Tuner, ticks: usize, model: impl Fn(usize) -> f64) {
        for _ in 0..ticks {
            let tp = model(tuner.target());
            tuner.tick(&input(tp, 0));
        }
    }

    const MB: f64 = 1_000_000.0;

    #[test]
    fn starts_at_requested_count() {
        let t = Tuner::new(DEFAULT_START_CONNECTIONS, 16, None);
        assert_eq!(t.target(), 8);
        let t = Tuner::new(4, 2, None);
        assert_eq!(t.target(), 2);
    }

    #[test]
    fn ramps_up_while_more_connections_help() {
        // Per-connection limited host: 2.5 MB/s each, link tops out at 40 MB/s.
        let mut t = Tuner::new(4, 16, None);
        run(&mut t, 60, |c| (c as f64 * 2.5 * MB).min(40.0 * MB));
        assert_eq!(t.target(), 16);
    }

    #[test]
    fn stops_where_the_link_saturates() {
        // 8 connections already fill the link; 16 adds nothing.
        let mut t = Tuner::new(4, 16, None);
        run(&mut t, 60, |c| (c as f64 * 3.0 * MB).min(24.0 * MB));
        assert_eq!(t.target(), 8);
    }

    #[test]
    fn never_drops_below_start_without_throttle() {
        // Throughput falls as connections rise; noise must not collapse the count.
        let mut t = Tuner::new(16, 32, None);
        run(&mut t, 200, |c| 40.0 * MB / c as f64);
        assert_eq!(t.target(), 16);
    }

    #[test]
    fn ranged_downloads_start_at_eight() {
        assert_eq!(initial_connections(100 << 20, 32), 8);
        assert_eq!(initial_connections(100 << 20, 8), 8);
        assert_eq!(initial_connections(20 << 20, 32), 8);
        assert_eq!(initial_connections(1 << 20, 32), 8);
        assert_eq!(initial_connections(1 << 30, 1), 1);
    }

    #[test]
    fn holds_the_fastest_measured_count() {
        let shapes: Vec<Box<dyn Fn(usize) -> f64>> = vec![
            Box::new(|c| (c as f64 * MB).min(6.0 * MB)),
            Box::new(|c| if c >= 8 { 10.0 * MB } else { 4.0 * MB }),
            Box::new(|c| (c as f64 * 2.0 * MB).min(50.0 * MB)),
        ];
        for model in shapes {
            let mut t = Tuner::new(4, 32, None);
            run(&mut t, 150, &model);
            let held = model(t.target());
            for (_, tp) in t.measured.iter().filter(|(c, _)| !t.rejected.contains(c)) {
                assert!(held >= *tp * 0.999, "held {held} but measured {tp}");
            }
        }
    }

    #[test]
    fn noisy_samples_do_not_flap() {
        // Alternating noise around a flat 10 MB/s: the tuner must settle.
        let mut t = Tuner::new(4, 16, None);
        let mut i = 0usize;
        let mut changes = 0;
        let mut last = t.target();
        for _ in 0..200 {
            i += 1;
            let noise = if i % 2 == 0 { 1.04 } else { 0.96 };
            t.tick(&input(10.0 * MB * noise, 0));
            if t.target() != last {
                changes += 1;
                last = t.target();
            }
        }
        assert!(changes <= 12, "too many target changes: {changes}");
    }

    #[test]
    fn throttle_drops_refused_connections_and_caps() {
        let mut t = Tuner::new(16, 32, None);
        t.tick(&input(5.0 * MB, 6));
        assert_eq!(t.target(), 10);
        assert_eq!(t.host_cap(), 10);
        t.tick(&input(3.0 * MB, 1));
        assert_eq!(t.target(), 9);
        for _ in 0..50 {
            t.tick(&input(10.0 * MB, 0));
            assert!(t.target() <= t.host_cap());
        }
    }

    #[test]
    fn throttle_cap_can_be_one_but_target_never_zero() {
        let mut t = Tuner::new(2, 16, None);
        t.tick(&input(1.0 * MB, 1));
        assert_eq!(t.target(), 1);
        assert_eq!(t.host_cap(), 1);
        for _ in 0..10 {
            t.tick(&input(100.0, 1));
            assert!(t.target() >= 1);
        }
    }

    #[test]
    fn remembered_cap_is_soft_and_relaxes() {
        // A stale cap of 1 from an old throttle must not pin the download at 1
        // connection forever when the host no longer throttles.
        let mut t = Tuner::new(4, 16, Some(1));
        assert_eq!(t.target(), 1);
        run(&mut t, 400, |c| c as f64 * 2.0 * MB);
        assert!(t.host_cap() > 1, "cap never relaxed");
        assert!(t.target() >= 4, "still at {} connections", t.target());
    }

    #[test]
    fn never_goes_above_cap_or_max() {
        let mut t = Tuner::new(4, 6, None);
        run(&mut t, 100, |c| c as f64 * MB);
        assert!(t.target() <= 6);
    }

    #[test]
    fn connection_errors_back_off() {
        let mut t = Tuner::new(8, 16, None);
        let out = t.tick(&TunerInput {
            throughput: 5.0 * MB,
            active_connections: 8,
            throttle_events: 0,
            error_events: 5,
            bytes_remaining: 1 << 30,
        });
        assert_eq!(out.target, 3);
    }

    #[test]
    fn wider_probe_must_preserve_per_connection_throughput() {
        let mut t = Tuner::new(8, 16, None);
        // A 25% aggregate gain for twice the connections is not worth keeping.
        run(&mut t, 100, |c| if c == 8 { 20.0 * MB } else { 25.0 * MB });
        assert_eq!(t.target(), 8);
        assert_eq!(t.best_connections(), 8);
    }

    #[test]
    fn one_slow_sample_does_not_reduce_baseline() {
        let mut t = Tuner::new(8, 16, None);
        run(&mut t, 3, |_| 20.0 * MB);
        t.tick(&input(1.0, 0));
        assert_eq!(t.target(), 8);
    }

    #[test]
    fn incomplete_worker_start_does_not_measure_or_probe() {
        let mut t = Tuner::new(8, 16, None);
        let mut sample = input(20.0 * MB, 0);
        sample.active_connections = 4;
        for _ in 0..30 {
            t.tick(&sample);
        }
        assert_eq!(t.target(), 8);
        assert_eq!(t.best_throughput(), 0.0);
    }

    #[test]
    fn small_remainder_does_not_probe() {
        let mut t = Tuner::new(4, 16, None);
        for _ in 0..20 {
            t.tick(&TunerInput {
                throughput: 5.0 * MB,
                active_connections: 4,
                throttle_events: 0,
                error_events: 0,
                bytes_remaining: 1 << 20,
            });
        }
        assert_eq!(t.target(), 4);
    }

    #[test]
    fn retry_after_seconds() {
        let dur = parse_retry_after("120", Duration::from_secs(300)).unwrap();
        assert_eq!(dur, Duration::from_secs(120));
    }
}
