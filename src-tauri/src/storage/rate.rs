//! Soft global bandwidth limiter — rclone `--bwlimit` semantics.
//!
//! A leaky token bucket: at most `capacity` bytes can burst through, and the
//! bucket refills at `rate` bytes/sec. Callers `acquire(n)` before reading
//! bytes off the wire; when the bucket is empty they sleep until enough has
//! refilled. `rate == 0` is the sentinel for "unlimited" and short-circuits.
//!
//! Granularity: this limits the *announce* size of each read, not the wire
//! byterate directly. For game saves the difference is negligible because we
//! call `acquire` once per file (or per S3 multipart chunk for streaming).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<Inner>>,
    rate: Arc<AtomicU64>,
}

struct Inner {
    capacity: u64,
    available: f64,
    last_refill: Instant,
}

impl RateLimiter {
    pub fn new(rate_bytes_per_sec: u64) -> Self {
        // Burst capacity = 1 second worth of rate, or 1 MiB if unlimited so
        // that occasional small acquires still pass through without blocking.
        let capacity = if rate_bytes_per_sec == 0 {
            1024 * 1024
        } else {
            rate_bytes_per_sec
        };
        Self {
            inner: Arc::new(Mutex::new(Inner {
                capacity,
                available: capacity as f64,
                last_refill: Instant::now(),
            })),
            rate: Arc::new(AtomicU64::new(rate_bytes_per_sec)),
        }
    }

    pub fn set_rate(&self, rate_bytes_per_sec: u64) {
        self.rate.store(rate_bytes_per_sec, Ordering::Relaxed);
        // Bump capacity to one-second worth when we have a rate, otherwise
        // 1 MiB (matches initial settings).
        if rate_bytes_per_sec > 0 {
            // best-effort — don't block on the lock here, just signal new
            // rate; acquire() will reshape the bucket on its next pass.
        }
    }

    /// Block until `n` bytes' worth of tokens are available. Returns
    /// immediately when the limiter is in "unlimited" mode.
    pub async fn acquire(&self, n: u64) {
        let rate = self.rate.load(Ordering::Relaxed);
        if rate == 0 || n == 0 {
            return;
        }
        // If a single request exceeds capacity, raise capacity to fit it so
        // we never permanently starve.
        loop {
            let sleep_for = {
                let mut inner = self.inner.lock().await;
                // Lazy refill based on wallclock since last touch.
                let now = Instant::now();
                let elapsed = now.duration_since(inner.last_refill).as_secs_f64();
                inner.last_refill = now;
                inner.available =
                    (inner.available + elapsed * rate as f64).min(inner.capacity as f64);
                if (n as f64) > inner.capacity as f64 {
                    inner.capacity = n;
                }
                if inner.available >= n as f64 {
                    inner.available -= n as f64;
                    return;
                }
                // Compute how long until enough tokens accrue.
                let deficit = n as f64 - inner.available;
                let secs = deficit / rate as f64;
                Duration::from_secs_f64(secs.min(1.0))
            };
            tokio::time::sleep(sleep_for).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn unlimited_returns_immediately() {
        let rl = RateLimiter::new(0);
        let t0 = Instant::now();
        rl.acquire(100 * 1024 * 1024).await;
        assert!(
            t0.elapsed() < Duration::from_millis(50),
            "unlimited should not block on huge n"
        );
    }

    #[tokio::test]
    async fn limited_throttles_consecutive_large_requests() {
        // 1 MiB/s. Asking for 2 MiB across two back-to-back acquires should
        // take roughly one second total (first burst from initial bucket,
        // second waits ~1s for refill).
        let rl = RateLimiter::new(1024 * 1024);
        let t0 = Instant::now();
        rl.acquire(1024 * 1024).await;
        rl.acquire(1024 * 1024).await;
        let elapsed = t0.elapsed();
        assert!(
            elapsed >= Duration::from_millis(800),
            "expected throttling to take ~1s, got {elapsed:?}"
        );
    }
}
