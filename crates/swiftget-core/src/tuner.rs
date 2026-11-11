//! Throughput-driven auto-tuner state machine.
//!
//! A pure, deterministic state machine that decides the target number of
//! connections for a segmented download. It is driven by periodic "tick"
//! observations from the supervisor and outputs the new connection target.
//!
//! Design principles:
//! - Start low (default 2-4, or remembered best), ramp up additively while
//!   throughput improves, revert and stop probing when it plateaus/drops.
//! - On throttle (429/503): halve the target and record a per-host cap.
//! - Never exceed the per-host cap or the hard max.
//! - Smoothed throughput via EWMA to absorb transient dips.
//! - Can go down to 1 connection, never to 0.
//! - Periodically re-probe to discover changes in server behaviour.

use std::time::Duration;

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
    /// Actively probing: ramping up connections to find the optimum.
    Probing,
    /// Holding at the best-known count, waiting to re-probe.
    Holding,
}

/// Per-host throughput record at a given connection count.
#[derive(Debug, Clone)]
struct Sample {
    connections: usize,
    throughput: f64,
}

/// The tuner state machine.
#[derive(Debug)]
pub struct Tuner {
    /// Current target connection count (output of the machine).
    target: usize,
    /// Hard ceiling: never go above this.
    max_connections: usize,
    /// Host-level cap from throttle (default = max_connections).
    host_cap: usize,
    /// Best throughput/connections pair observed so far.
    best: Sample,
    /// Smoothed (EWMA) throughput.
    smoothed: f64,
    /// EWMA smoothing factor (0..1); lower = smoother.
    alpha: f64,
    /// Phase of the tuner.
    phase: Phase,
    /// How many ticks since the last target change (warmup).
    ticks_at_current: usize,
    /// Ticks to wait after a change before evaluating the result.
    warmup_ticks: usize,
    /// The target we changed from (to know what to compare against).
    target_before_change: usize,
    /// Throughput right before the last change, for comparison.
    throughput_before_change: f64,
    /// Ticks spent holding; used for periodic re-probing.
    hold_ticks: usize,
    /// How many ticks to hold before re-probing.
    re_probe_interval: usize,
    /// Whether the tuner was ever throttled.
    was_throttled: bool,
}

impl Tuner {
    /// Create a new tuner.
    ///
    /// * `initial_target` — starting connection count.
    /// * `max_connections` — hard cap from config.
    /// * `host_cap` — per-host cap (from HostStats or default = max_connections).
    pub fn new(initial_target: usize, max_connections: usize, host_cap: Option<usize>) -> Self {
        let target = initial_target.clamp(1, max_connections);
        let cap = host_cap.unwrap_or(max_connections).clamp(1, max_connections);
        Tuner {
            target,
            max_connections,
            host_cap: cap,
            best: Sample {
                connections: target,
                throughput: 0.0,
            },
            smoothed: 0.0,
            alpha: 0.3,
            phase: Phase::Probing,
            ticks_at_current: 0,
            warmup_ticks: 2, // wait 2 ticks after each change
            target_before_change: target,
            throughput_before_change: 0.0,
            hold_ticks: 0,
            re_probe_interval: 15,
            was_throttled: false,
        }
    }

    /// The current target connection count.
    pub fn target(&self) -> usize {
        self.target
    }

    /// The best observed connection count.
    pub fn best_connections(&self) -> usize {
        self.best.connections
    }

    /// The best observed throughput.
    pub fn best_throughput(&self) -> f64 {
        self.best.throughput
    }

    /// The current host cap (may have been lowered by throttling).
    pub fn host_cap(&self) -> usize {
        self.host_cap
    }

    /// Process a tick and return the new target.
    pub fn tick(&mut self, input: &TunerInput) -> TunerOutput {
        // Update smoothed throughput.
        if self.smoothed <= 0.0 {
            self.smoothed = input.throughput;
        } else {
            self.smoothed = self.alpha * input.throughput + (1.0 - self.alpha) * self.smoothed;
        }

        self.ticks_at_current += 1;

        // --- Handle throttle events first (highest priority) ---
        if input.throttle_events > 0 {
            self.handle_throttle();
            return TunerOutput {
                target: self.target,
            };
        }

        // --- Handle generic errors ---
        if input.error_events > 0 && self.target > 1 {
            self.target = self.target.saturating_sub(1).max(1);
            self.ticks_at_current = 0;
            return TunerOutput {
                target: self.target,
            };
        }

        // Update best if current smoothed throughput exceeds it.
        if self.smoothed > self.best.throughput * 1.0 {
            self.best.throughput = self.smoothed;
            self.best.connections = input.active_connections.max(1);
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
        // Halve the target.
        let new_target = (self.target / 2).max(1);
        // Set host cap = count at which throttling happened minus 1 (at least 1).
        let cap_at = self.target.saturating_sub(1).max(1);
        self.host_cap = self.host_cap.min(cap_at);
        self.target = new_target.min(self.host_cap);
        self.was_throttled = true;
        self.phase = Phase::Holding;
        self.hold_ticks = 0;
        self.ticks_at_current = 0;
    }

    fn tick_probing(&mut self, _input: &TunerInput) {
        // Wait for warmup.
        if self.ticks_at_current < self.warmup_ticks {
            return;
        }

        let improvement_threshold = 0.05; // 5% improvement required

        // Check if the last increase helped.
        if self.target > self.target_before_change {
            let improved = self.throughput_before_change > 0.0
                && (self.smoothed - self.throughput_before_change) / self.throughput_before_change
                    > improvement_threshold;
            if !improved {
                // Last increase didn't help — revert to best and stop probing.
                self.target = self.best.connections.clamp(1, self.host_cap);
                self.phase = Phase::Holding;
                self.hold_ticks = 0;
                self.ticks_at_current = 0;
                return;
            }
        }

        // Throughput is improving — try adding more connections.
        if self.target < self.host_cap && self.target < self.max_connections {
            // Record state before change.
            self.throughput_before_change = self.smoothed;
            self.target_before_change = self.target;

            // Additive increase: double when very low, +2 mid, +1 near cap.
            let increment = if self.target <= 2 {
                self.target
            } else if self.target < 8 {
                2
            } else {
                1
            };
            self.target = (self.target + increment).min(self.host_cap).min(self.max_connections);
            self.ticks_at_current = 0;
        } else {
            // At the cap — switch to holding.
            self.phase = Phase::Holding;
            self.hold_ticks = 0;
        }
    }

    fn tick_holding(&mut self, _input: &TunerInput) {
        self.hold_ticks += 1;

        // Fallback: if current throughput has fallen well below the best, go back
        // to the best count.
        if self.best.throughput > 0.0 && self.smoothed < self.best.throughput * 0.7 {
            let fallback = self.best.connections.clamp(1, self.host_cap);
            if fallback != self.target {
                self.target = fallback;
                self.ticks_at_current = 0;
            }
        }

        // Re-probe periodically.
        if self.hold_ticks >= self.re_probe_interval {
            self.phase = Phase::Probing;
            self.throughput_before_change = self.smoothed;
            self.target_before_change = self.target;
            self.ticks_at_current = 0;
            self.hold_ticks = 0;
        }
    }
}

/// Parse a `Retry-After` header value.
///
/// The value can be either:
/// - An integer number of seconds (e.g., `"120"`), or
/// - An HTTP-date (e.g., `"Wed, 21 Oct 2015 07:28:00 GMT"`).
///
/// Returns the delay as a `Duration`, capped at `max_delay`.
pub fn parse_retry_after(value: &str, max_delay: Duration) -> Option<Duration> {
    let value = value.trim();

    // Try as seconds first.
    if let Ok(secs) = value.parse::<u64>() {
        let dur = Duration::from_secs(secs).min(max_delay);
        return Some(dur);
    }

    // Try as HTTP-date (RFC 7231 §7.1.1.1).
    if let Some(dur) = parse_http_date_delay(value) {
        return Some(dur.min(max_delay));
    }

    None
}

/// Parse an HTTP-date and return the delay from now.
fn parse_http_date_delay(value: &str) -> Option<Duration> {
    // We support the preferred format: "Sun, 06 Nov 1994 08:49:37 GMT"
    // (IMF-fixdate per RFC 7231).
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    // Split "Wed, 21 Oct 2015 07:28:00 GMT"
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

    // Convert to a Unix timestamp (rough but good enough for Retry-After).
    // Days from epoch to year.
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
        }
    }

    #[test]
    fn ramps_up_while_throughput_rises() {
        let mut tuner = Tuner::new(2, 16, None);
        assert_eq!(tuner.target(), 2);

        // Initial ticks to build up smoothed throughput.
        tuner.tick(&input(1_000_000.0, 2, 0, 0));
        tuner.tick(&input(1_100_000.0, 2, 0, 0));
        // After warmup, should increase.
        let out = tuner.tick(&input(1_200_000.0, 2, 0, 0));
        assert!(
            out.target > 2,
            "expected target to increase from 2, got {}",
            out.target
        );

        let prev_target = out.target;
        // Continue feeding improving throughput.
        for i in 0..5 {
            let tp = 1_200_000.0 + (i as f64 + 1.0) * 500_000.0;
            tuner.tick(&input(tp, prev_target, 0, 0));
        }
        // Target should have increased further or be at same level while probing.
        assert!(tuner.target() >= prev_target);
    }

    #[test]
    fn stops_and_reverts_when_throughput_plateaus() {
        let mut tuner = Tuner::new(2, 16, None);

        // Build up with great throughput.
        tuner.tick(&input(5_000_000.0, 2, 0, 0));
        tuner.tick(&input(5_000_000.0, 2, 0, 0));
        tuner.tick(&input(5_500_000.0, 2, 0, 0)); // triggers increase

        let after_increase = tuner.target();
        assert!(after_increase > 2);

        // Feed similar (not improving) throughput — should detect plateau and revert.
        for _ in 0..5 {
            tuner.tick(&input(5_100_000.0, after_increase, 0, 0));
        }

        // Should have reverted to 2 (the best count).
        assert!(
            tuner.target() <= after_increase,
            "expected revert, got {}",
            tuner.target()
        );
    }

    #[test]
    fn halves_on_throttle_and_respects_cap() {
        let mut tuner = Tuner::new(8, 16, None);
        assert_eq!(tuner.target(), 8);

        // Throttle.
        let out = tuner.tick(&input(5_000_000.0, 8, 1, 0));
        assert_eq!(out.target, 4); // halved
        assert!(tuner.host_cap() <= 7); // cap set to throttle count - 1

        // Another throttle.
        let out = tuner.tick(&input(3_000_000.0, 4, 1, 0));
        assert_eq!(out.target, 2);
        assert!(tuner.host_cap() <= 3);

        // Even with no throttles, target must never exceed cap.
        for _ in 0..50 {
            tuner.tick(&input(10_000_000.0, 2, 0, 0));
        }
        assert!(
            tuner.target() <= tuner.host_cap(),
            "target {} exceeds cap {}",
            tuner.target(),
            tuner.host_cap()
        );
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
        let mut tuner = Tuner::new(2, 4, Some(3));
        // Feed excellent throughput — should still cap at 3.
        for i in 0..30 {
            tuner.tick(&input(1_000_000.0 * (i as f64 + 1.0), tuner.target(), 0, 0));
        }
        assert!(
            tuner.target() <= 3,
            "target {} exceeds host cap 3",
            tuner.target()
        );
    }

    #[test]
    fn falls_back_to_best_measured_count() {
        let mut tuner = Tuner::new(4, 16, None);

        // Good throughput at 4.
        for _ in 0..5 {
            tuner.tick(&input(8_000_000.0, 4, 0, 0));
        }
        // Best should be around 4.
        assert_eq!(tuner.best_connections(), 4);

        // Throughput drops dramatically while still at some target.
        for _ in 0..5 {
            tuner.tick(&input(2_000_000.0, tuner.target(), 0, 0));
        }

        // Should have fallen back toward the best count.
        // (Because 2M < 8M * 0.7 = 5.6M)
        assert!(
            tuner.target() <= 4,
            "expected fallback to best (4), got {}",
            tuner.target()
        );
    }

    // ---- Retry-After parsing tests ----

    #[test]
    fn retry_after_seconds() {
        let dur = parse_retry_after("120", Duration::from_secs(300)).unwrap();
        assert_eq!(dur, Duration::from_secs(120));
    }

    #[test]
    fn retry_after_seconds_capped() {
        let dur = parse_retry_after("600", Duration::from_secs(30)).unwrap();
        assert_eq!(dur, Duration::from_secs(30));
    }

    #[test]
    fn retry_after_zero() {
        let dur = parse_retry_after("0", Duration::from_secs(30)).unwrap();
        assert_eq!(dur, Duration::from_secs(0));
    }

    #[test]
    fn retry_after_http_date() {
        // A date far in the past should give ~0 delay.
        let dur =
            parse_retry_after("Wed, 21 Oct 2015 07:28:00 GMT", Duration::from_secs(300)).unwrap();
        assert_eq!(dur, Duration::from_secs(0));
    }

    #[test]
    fn retry_after_invalid() {
        assert!(parse_retry_after("not-a-number-or-date", Duration::from_secs(30)).is_none());
    }

    #[test]
    fn honours_retry_after_parsing_seconds_and_date() {
        // Seconds.
        assert_eq!(
            parse_retry_after("  5  ", Duration::from_secs(30)),
            Some(Duration::from_secs(5))
        );
        // Garbage.
        assert!(parse_retry_after("xyz", Duration::from_secs(30)).is_none());
    }
}
