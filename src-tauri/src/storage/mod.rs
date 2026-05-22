pub mod rate;
pub mod retry;
pub mod s3;
pub mod webdav;

pub use rate::RateLimiter;
pub use retry::with_retry;

use crate::error::AppResult;
use crate::model::{BackendConfig, RemoteFileMeta};
use async_trait::async_trait;
use std::sync::Arc;

#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Returns a short human label, e.g. "s3:my-bucket".
    fn label(&self) -> String;

    /// Simple connectivity test — list 1 key or HEAD root.
    async fn ping(&self) -> AppResult<String>;

    /// List all objects under `prefix`. The returned key is the full key (including prefix).
    async fn list(&self, prefix: &str) -> AppResult<Vec<RemoteFileMeta>>;

    /// Upload bytes to `key`. Overwrites if already exists.
    async fn put(&self, key: &str, body: Vec<u8>) -> AppResult<()>;

    /// Download bytes for `key`.
    async fn get(&self, key: &str) -> AppResult<Vec<u8>>;

    /// Delete `key`. No-op if missing.
    async fn delete(&self, key: &str) -> AppResult<()>;

    /// Server-side copy from `src_key` to `dst_key`. Default impl: get + put.
    async fn copy(&self, src_key: &str, dst_key: &str) -> AppResult<()> {
        let bytes = self.get(src_key).await?;
        self.put(dst_key, bytes).await
    }

    /// Stream-upload a local file. Default impl reads the whole file into
    /// memory and calls `put`. Backends that support streaming (S3 via
    /// ByteStream::from_path) should override.
    async fn put_path(&self, key: &str, path: &std::path::Path) -> AppResult<()> {
        let p = path.to_path_buf();
        let bytes = tokio::task::spawn_blocking(move || std::fs::read(&p))
            .await
            .map_err(|e| crate::error::AppError::other(format!("read join: {e}")))??;
        self.put(key, bytes).await
    }

    /// Stream-download a remote object directly to a local file. Default impl
    /// reads into memory and writes atomically.
    async fn get_to_path(&self, key: &str, path: &std::path::Path) -> AppResult<()> {
        let bytes = self.get(key).await?;
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let tmp = path.with_extension("gsyncing.tmp");
            std::fs::write(&tmp, &bytes)?;
            std::fs::rename(&tmp, &path)?;
            Ok(())
        })
        .await
        .map_err(|e| crate::error::AppError::other(format!("write join: {e}")))??;
        Ok(())
    }
}

/// Adapter that wraps every operation in `with_retry` (capped exponential
/// backoff on transient errors) and optionally throttles bytes through a
/// shared `RateLimiter`. Borrowed from rclone: retry + bwlimit are intrinsic
/// to every IO call, not opt-in per call site.
pub struct RetryingBackend {
    inner: Arc<dyn StorageBackend>,
    rate: RateLimiter,
}

impl RetryingBackend {
    pub fn new(inner: Arc<dyn StorageBackend>, rate: RateLimiter) -> Self {
        Self { inner, rate }
    }
}

#[async_trait]
impl StorageBackend for RetryingBackend {
    fn label(&self) -> String {
        self.inner.label()
    }

    async fn ping(&self) -> AppResult<String> {
        with_retry("ping", || self.inner.ping()).await
    }

    async fn list(&self, prefix: &str) -> AppResult<Vec<RemoteFileMeta>> {
        with_retry(&format!("list {prefix}"), || self.inner.list(prefix)).await
    }

    async fn put(&self, key: &str, body: Vec<u8>) -> AppResult<()> {
        self.rate.acquire(body.len() as u64).await;
        with_retry(&format!("put {key}"), || {
            let b = body.clone();
            self.inner.put(key, b)
        })
        .await
    }

    async fn get(&self, key: &str) -> AppResult<Vec<u8>> {
        let bytes = with_retry(&format!("get {key}"), || self.inner.get(key)).await?;
        // Throttle on the way out — we already paid the network cost, but
        // delaying the next request keeps the rolling average honest.
        self.rate.acquire(bytes.len() as u64).await;
        Ok(bytes)
    }

    async fn delete(&self, key: &str) -> AppResult<()> {
        with_retry(&format!("delete {key}"), || self.inner.delete(key)).await
    }

    async fn copy(&self, src_key: &str, dst_key: &str) -> AppResult<()> {
        with_retry(&format!("copy {src_key} -> {dst_key}"), || {
            self.inner.copy(src_key, dst_key)
        })
        .await
    }

    async fn put_path(&self, key: &str, path: &std::path::Path) -> AppResult<()> {
        // Best-effort: charge the limiter based on file size up front.
        if let Ok(meta) = tokio::fs::metadata(path).await {
            self.rate.acquire(meta.len()).await;
        }
        with_retry(&format!("put_path {key}"), || {
            self.inner.put_path(key, path)
        })
        .await
    }

    async fn get_to_path(&self, key: &str, path: &std::path::Path) -> AppResult<()> {
        with_retry(&format!("get_to_path {key}"), || {
            self.inner.get_to_path(key, path)
        })
        .await?;
        if let Ok(meta) = tokio::fs::metadata(path).await {
            self.rate.acquire(meta.len()).await;
        }
        Ok(())
    }
}

pub async fn build(
    backend: &BackendConfig,
    rate: RateLimiter,
) -> AppResult<Arc<dyn StorageBackend>> {
    let raw: Arc<dyn StorageBackend> = match backend {
        BackendConfig::S3 { s3: cfg, .. } => Arc::new(s3::S3Storage::new(cfg.clone()).await?),
        BackendConfig::Webdav { webdav: cfg, .. } => {
            Arc::new(webdav::WebDAVStorage::new(cfg.clone())?)
        }
    };
    Ok(Arc::new(RetryingBackend::new(raw, rate)))
}
