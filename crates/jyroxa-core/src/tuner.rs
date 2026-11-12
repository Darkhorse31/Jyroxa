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
/// Hold this many ticks between upward re-probes (~10 s at 500 ms ticks).
const REPROBE_TICKS: usize = 20;
/// After a throttle, wait this long before probing again (~20 s).
const THROTTLE_REPROBE_TICKS: usize = 40;
/// A throttle-derived cap is relaxed after this many throttle-free ticks (~30 s).
const CAP_RELAX_TICKS: usize = 60;
/// Default first connection count when nothing is known about the host.
pub const DEFAULT_START_CONNECTIONS: usize = 4;

/// Throughput-driven connection tuner.
///
/// It is a measured hill-climb: every connection count it tries is held for a
/// short warm-up, then measured (median of several ticks). It walks upward by
/// doubling while each step is clearly faster, and if the starting count turns
/// out to be the best it also walks *down* (halving) while fewer connections are
/// faster. It never settles on a count that measured slower than another count
/// it has tried, so auto mode can't end up slower than one connection when one
/// connection really is faster on a host.
#[derive(Debug)]
pub struct Tuner {
    target: usize,
    max_connections: usize,
    /// Current soft ceiling from throttle events (429/503).
    host_cap: usize,
    start: usize,
    phase: Phase,
    /// Median throughput measured at each connection count.
    measured: HashMap<usize, f64>,
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
            start: target,
            phase: Phase::Measuring { ascending: true },
            measured: HashMap::new(),
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
            .filter(|(c, tp)| **c <= self.host_cap && **tp > 0.0)
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
        // Generic transport errors are retried by the workers; they must not
        // collapse the connection count.
        let _ = input.error_events;

        if input.throttle_events > 0 {
            self.handle_throttle();
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

    fn handle_throttle(&mut self) {
        let cap_at = self.target.saturating_sub(1).max(1);
        self.host_cap = self.host_cap.min(cap_at);
        let next = (self.target / 2).max(1).min(self.host_cap);
        self.ticks_since_throttle = 0;
        // Measurements taken above the new cap no longer apply.
        let cap = self.host_cap;
        self.measured.retain(|c, _| *c <= cap);
        self.switch_to(next, Phase::Holding);
        self.reprobe_after = THROTTLE_REPROBE_TICKS;
    }

    fn tick_measuring(&mut self, input: &TunerInput, ascending: bool) {
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
        let prev_best = self.best_throughput();
        self.measured.insert(self.target, level_tp);
        let improved = prev_best <= 0.0 || level_tp >= prev_best * (1.0 + GAIN);
        let faster = level_tp > prev_best;

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
            // Going up did not help. If the start count is still the best, check
            // whether fewer connections are faster (e.g. a host that slows down
            // parallel streams), otherwise settle on the best count.
            let best = self.best_connections();
            if best == self.start && best > 1 && !self.measured.contains_key(&(best / 2).max(1)) {
                self.switch_to((best / 2).max(1), Phase::Measuring { ascending: false });
            } else {
                self.hold_best();
            }
        } else {
            let next = (self.target / 2).max(1);
            let clearly_faster = faster && level_tp >= prev_best * (1.0 + GAIN / 2.0);
            if clearly_faster && next < self.target && !self.measured.contains_key(&next) {
                self.switch_to(next, Phase::Measuring { ascending: false });
            } else {
                self.hold_best();
            }
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
        if next > self.target && !tried_and_worse {
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
            active_connections: 1,
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
        assert_eq!(t.target(), 4);
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
    fn walks_down_when_one_connection_is_fastest() {
        // A host that punishes parallel streams: 1 conn is fastest.
        let model = |c: usize| match c {
            1 => 17.5 * MB,
            2 => 12.0 * MB,
            4 => 7.0 * MB,
            _ => 6.0 * MB,
        };
        let mut t = Tuner::new(4, 16, None);
        run(&mut t, 80, model);
        assert_eq!(t.target(), 1);
        assert!(t.best_throughput() >= 17.0 * MB);
    }

    #[test]
    fn never_settles_slower_than_one_connection() {
        // Whatever the shape, the held count must be at least as fast as 1 conn
        // whenever 1 conn was measured.
        let shapes: Vec<Box<dyn Fn(usize) -> f64>> = vec![
            Box::new(|c| if c == 1 { 10.0 * MB } else { 9.0 * MB }),
            Box::new(|c| if c <= 2 { 10.0 * MB } else { 4.0 * MB }),
            Box::new(|c| 5.0 * MB / c as f64),
            Box::new(|c| (c as f64 * MB).min(6.0 * MB)),
        ];
        for model in shapes {
            let mut t = Tuner::new(4, 16, None);
            run(&mut t, 120, &model);
            let held = model(t.target());
            if let Some(one) = t.measured.get(&1) {
                assert!(held >= *one, "held {held} < one-conn {one} at {}", t.target());
            }
            for (_, tp) in t.measured.iter() {
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
    fn halves_on_throttle_and_respects_cap() {
        let mut t = Tuner::new(8, 16, None);
        t.tick(&input(5.0 * MB, 1));
        assert_eq!(t.target(), 4);
        assert!(t.host_cap() <= 7);
        t.tick(&input(3.0 * MB, 1));
        assert_eq!(t.target(), 2);
        assert!(t.host_cap() <= 3);
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
    fn errors_do_not_drop_target() {
        let mut t = Tuner::new(8, 16, None);
        let out = t.tick(&TunerInput {
            throughput: 5.0 * MB,
            active_connections: 8,
            throttle_events: 0,
            error_events: 5,
            bytes_remaining: 1 << 30,
        });
        assert_eq!(out.target, 8);
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
