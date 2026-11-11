//! Token-bucket rate limiter.
//!
//! Used both per-download and as a shared global cap. The limiter is cheap to
//! clone (it is backed by an `Arc`) so the same global limiter can be shared by
//! every worker of every concurrent download.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::Instant;

struct Bucket {
    /// Maximum number of tokens (bytes) the bucket can hold.
    capacity: f64,
    /// Current token balance.
    tokens: f64,
    /// Refill rate in tokens (bytes) per second.
    rate: f64,
    /// Last time the bucket was refilled.
    last: Instant,
}

/// A shareable token-bucket limiter. A `None` rate means "unlimited".
#[derive(Clone)]
pub struct RateLimiter {
    inner: Option<Arc<Mutex<Bucket>>>,
}

impl RateLimiter {
    /// Create a limiter. `bytes_per_sec == None` (or 0) disables limiting.
    pub fn new(bytes_per_sec: Option<u64>) -> Self {
        match bytes_per_sec {
            Some(rate) if rate > 0 => {
                let rate = rate as f64;
                // Allow a one-second burst so that bursty readers still average
                // out to the configured rate.
                let capacity = rate.max(64.0 * 1024.0);
                RateLimiter {
                    inner: Some(Arc::new(Mutex::new(Bucket {
                        capacity,
                        tokens: capacity,
                        rate,
                        last: Instant::now(),
                    }))),
                }
            }
            _ => RateLimiter { inner: None },
        }
    }

    /// An unlimited limiter.
    pub fn unlimited() -> Self {
        RateLimiter { inner: None }
    }

    /// Returns true if this limiter imposes no limit.
    pub fn is_unlimited(&self) -> bool {
        self.inner.is_none()
    }

    /// Acquire `n` tokens, waiting (asynchronously) until they are available.
    pub async fn acquire(&self, n: u64) {
        let Some(bucket) = &self.inner else {
            return;
        };
        let mut remaining = n as f64;
        while remaining > 0.0 {
            let mut b = bucket.lock().await;
            // Never try to take more than the whole bucket at once, otherwise a
            // request larger than `capacity` could never be satisfied.
            let want = remaining.min(b.capacity);

            // Refill based on elapsed time.
            let now = Instant::now();
            let elapsed = now.saturating_duration_since(b.last).as_secs_f64();
            b.tokens = (b.tokens + elapsed * b.rate).min(b.capacity);
            b.last = now;

            if b.tokens >= want {
                b.tokens -= want;
                remaining -= want;
                continue;
            }

            let deficit = want - b.tokens;
            let wait = deficit / b.rate;
            drop(b);
            tokio::time::sleep(Duration::from_secs_f64(wait.clamp(0.0, 5.0))).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unlimited_is_instant() {
        let l = RateLimiter::unlimited();
        let start = Instant::now();
        l.acquire(10_000_000).await;
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test(start_paused = true)]
    async fn enforces_rate() {
        // 1000 bytes/sec. The initial burst (capacity) is consumed instantly,
        // then subsequent acquisitions must wait proportionally.
        let l = RateLimiter::new(Some(1000));
        // Drain the initial burst.
        l.acquire(65 * 1024).await;
        let start = Instant::now();
        // Requesting another 1000 bytes at 1000 B/s should take ~1 second.
        l.acquire(1000).await;
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(900),
            "expected ~1s, got {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn large_request_terminates() {
        // A request larger than the bucket capacity must still complete.
        let l = RateLimiter::new(Some(1_000_000));
        tokio::time::timeout(Duration::from_secs(5), l.acquire(5_000_000))
            .await
            .expect("acquire should finish");
    }
}
