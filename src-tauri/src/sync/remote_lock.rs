//! Best-effort cross-machine sync lock.
//!
//! Stored at `<remote_prefix>/.gsyncing/lock.json`. Contains the hostname
//! and a UNIX-ms timestamp. A lock older than TTL is considered abandoned
//! (the previous holder crashed / lost network) and may be force-acquired.
//!
//! **This is not a hard mutex.** S3 and WebDAV do not give atomic
//! compare-and-swap on plain object PUT, so two machines starting within
//! the same millisecond can both think they hold the lock. We accept that:
//!   - 99.9% of the time only one machine is syncing the same game.
//!   - In the rare race the per-file conflict-rename policy still saves data.
//! What we GUARANTEE: a user staring at the UI sees "machine X is syncing"
//! and can choose to wait or steal, instead of silently double-writing.

use crate::error::{AppError, AppResult};
use crate::paths;
use crate::storage::StorageBackend;
use chrono::Utc;
use serde::{Deserialize, Serialize};

const LOCK_KEY: &str = ".gsyncing/lock.json";
/// Locks older than this are treated as stale and can be force-acquired.
pub const STALE_TTL_MS: i64 = 5 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteLock {
    pub host: String,
    #[serde(rename = "acquiredAt")]
    pub acquired_at: i64,
    /// Random nonce so two simultaneous PUTs from the same hostname don't
    /// look like the same lock (rare but possible).
    pub nonce: String,
}

pub async fn try_acquire(
    backend: &dyn StorageBackend,
    remote_prefix: &str,
) -> AppResult<RemoteLock> {
    let key = paths::to_remote_key(remote_prefix, LOCK_KEY);
    // Probe existing lock first.
    if let Ok(bytes) = backend.get(&key).await {
        if let Ok(existing) = serde_json::from_slice::<RemoteLock>(&bytes) {
            let age = Utc::now().timestamp_millis() - existing.acquired_at;
            if age >= 0 && age < STALE_TTL_MS && existing.host != current_host() {
                return Err(AppError::Other(format!(
                    "另一台机器「{}」正在同步 (已持有锁 {} 秒，TTL {} 秒)",
                    existing.host,
                    age / 1000,
                    STALE_TTL_MS / 1000
                )));
            }
        }
    }
    let lock = RemoteLock {
        host: current_host(),
        acquired_at: Utc::now().timestamp_millis(),
        nonce: uuid::Uuid::new_v4().to_string(),
    };
    let body = serde_json::to_vec(&lock)?;
    backend.put(&key, body).await?;
    Ok(lock)
}

/// Start a heartbeat that refreshes the lock every TTL/2. Returns a shutdown
/// handle whose drop / cancel terminates the loop. Without this, a sync
/// that runs longer than `STALE_TTL_MS` would let another machine steal the
/// lock mid-flight.
pub fn spawn_heartbeat(
    backend: std::sync::Arc<dyn StorageBackend>,
    remote_prefix: String,
    held: RemoteLock,
    cancel: tokio_util::sync::CancellationToken,
) {
    tokio::spawn(async move {
        let interval = std::time::Duration::from_millis((STALE_TTL_MS / 2) as u64);
        loop {
            tokio::select! {
                _ = tokio::time::sleep(interval) => {
                    let key = paths::to_remote_key(&remote_prefix, LOCK_KEY);
                    let refreshed = RemoteLock {
                        host: held.host.clone(),
                        acquired_at: Utc::now().timestamp_millis(),
                        nonce: held.nonce.clone(),
                    };
                    if let Ok(body) = serde_json::to_vec(&refreshed) {
                        if let Err(e) = backend.put(&key, body).await {
                            tracing::warn!("lock heartbeat failed: {e}");
                        }
                    }
                }
                _ = cancel.cancelled() => break,
            }
        }
    });
}

pub async fn release(
    backend: &dyn StorageBackend,
    remote_prefix: &str,
    held: &RemoteLock,
) -> AppResult<()> {
    let key = paths::to_remote_key(remote_prefix, LOCK_KEY);
    // Verify we still own it before deleting — defensive against the race
    // where another machine stole the lock after our TTL expired.
    if let Ok(bytes) = backend.get(&key).await {
        if let Ok(current) = serde_json::from_slice::<RemoteLock>(&bytes) {
            if current.nonce != held.nonce {
                tracing::warn!(
                    "lock changed under us (was held by {}, now {})—skipping release",
                    held.host,
                    current.host
                );
                return Ok(());
            }
        }
    }
    let _ = backend.delete(&key).await;
    Ok(())
}

fn current_host() -> String {
    // hostname is best-effort identifying string. Falls back if unavailable.
    if let Ok(name) = hostname_string() {
        return name;
    }
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(windows)]
fn hostname_string() -> std::io::Result<String> {
    // Pulls COMPUTERNAME from env on Windows — this is the canonical name
    // shown in System Properties. No extra dep needed.
    std::env::var("COMPUTERNAME").map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
}

#[cfg(not(windows))]
fn hostname_string() -> std::io::Result<String> {
    std::env::var("HOSTNAME").map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_round_trips_via_json() {
        let lock = RemoteLock {
            host: "TEST-PC".into(),
            acquired_at: 1_700_000_000_000,
            nonce: "abc-123".into(),
        };
        let json = serde_json::to_vec(&lock).unwrap();
        let back: RemoteLock = serde_json::from_slice(&json).unwrap();
        assert_eq!(back.host, lock.host);
        assert_eq!(back.acquired_at, lock.acquired_at);
        assert_eq!(back.nonce, lock.nonce);
    }

    #[test]
    fn current_host_is_non_empty() {
        let h = current_host();
        assert!(!h.is_empty());
    }
}
