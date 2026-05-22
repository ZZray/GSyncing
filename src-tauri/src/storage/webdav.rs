use crate::error::{AppError, AppResult};
use crate::model::{RemoteFileMeta, WebDAVConfig};
use crate::storage::StorageBackend;
use async_trait::async_trait;
use reqwest::Method;
use reqwest_dav::{Auth, Client, ClientBuilder, Depth};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;

pub struct WebDAVStorage {
    client: Client,
    /// Lower-level HTTP client used for verbs that reqwest_dav 0.1 doesn't
    /// expose (notably COPY). Same Basic auth as `client`.
    raw_client: reqwest::Client,
    /// Origin URL trimmed of trailing slash, e.g. `https://dav.example.com`.
    host: String,
    base_prefix: String,
    auth: (String, String),
    /// Flips to `true` after a COPY request fails with a server-side
    /// "not implemented" / "method not allowed" — subsequent copies skip
    /// straight to get+put fallback, no point hammering the same dead route.
    copy_disabled: AtomicBool,
    created_dirs: Mutex<BTreeSet<String>>,
}

impl WebDAVStorage {
    pub fn new(cfg: WebDAVConfig) -> AppResult<Self> {
        if cfg.url.is_empty() {
            return Err(AppError::Config("url required".into()));
        }
        let client = ClientBuilder::new()
            .set_host(cfg.url.clone())
            .set_auth(Auth::Basic(cfg.username.clone(), cfg.password.clone()))
            .build()
            .map_err(|e| AppError::storage(format!("build webdav: {e}")))?;
        let raw_client = reqwest::Client::builder()
            // Hard 60s ceiling — WebDAV COPY on the server side is usually a
            // metadata-only operation, so anything past a minute means the
            // server has stalled and we should let retry/backoff take over.
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| AppError::storage(format!("build raw client: {e}")))?;
        let base_prefix = cfg
            .prefix
            .trim_start_matches('/')
            .trim_end_matches('/')
            .to_string();
        let host = cfg.url.trim_end_matches('/').to_string();
        Ok(Self {
            client,
            raw_client,
            host,
            base_prefix,
            auth: (cfg.username.clone(), cfg.password.clone()),
            copy_disabled: AtomicBool::new(false),
            created_dirs: Mutex::new(BTreeSet::new()),
        })
    }

    /// Issue a raw WebDAV COPY. Returns Err for any 4xx/5xx so the caller
    /// can decide to fall back. WebDAV COPY semantics (RFC 4918 §9.8):
    ///   COPY <src-path> HTTP/1.1
    ///   Destination: <absolute-dst-url>
    ///   Overwrite: T
    async fn try_native_copy(&self, src_path: &str, dst_path: &str) -> AppResult<()> {
        // Build absolute URLs the server will accept in the Destination header.
        let src_url = format!("{}{}", self.host, src_path);
        let dst_url = format!("{}{}", self.host, dst_path);
        let resp = self
            .raw_client
            .request(Method::from_bytes(b"COPY").unwrap(), &src_url)
            .basic_auth(&self.auth.0, Some(&self.auth.1))
            .header("Destination", &dst_url)
            .header("Overwrite", "T")
            .send()
            .await
            .map_err(|e| AppError::storage(format!("COPY {src_path} -> {dst_path}: {e}")))?;
        let status = resp.status();
        if status.is_success() {
            return Ok(());
        }
        // 400/501/405 typically mean "this WebDAV server can't COPY" — disable
        // future native attempts and propagate so the trait default kicks in.
        if status == reqwest::StatusCode::METHOD_NOT_ALLOWED
            || status == reqwest::StatusCode::NOT_IMPLEMENTED
            || status == reqwest::StatusCode::BAD_REQUEST
        {
            self.copy_disabled.store(true, Ordering::Relaxed);
        }
        let body = resp.text().await.unwrap_or_default();
        Err(AppError::storage(format!(
            "COPY {src_path} -> {dst_path} returned {status}: {body}"
        )))
    }

    fn full_path(&self, key: &str) -> String {
        let key = key.trim_start_matches('/');
        if self.base_prefix.is_empty() {
            format!("/{key}")
        } else {
            format!("/{}/{}", self.base_prefix, key)
        }
    }

    async fn ensure_dirs(&self, key: &str) -> AppResult<()> {
        let path = self.full_path(key);
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        if parts.len() <= 1 {
            return Ok(());
        }
        // Compute which dirs need mkcol under a short lock — DO NOT await
        // network calls while holding the lock or we serialize every parallel
        // upload behind the first one.
        let mut needed: Vec<String> = Vec::new();
        {
            let created = self.created_dirs.lock().await;
            let mut cur = String::new();
            for p in &parts[..parts.len() - 1] {
                cur.push('/');
                cur.push_str(p);
                if !created.contains(&cur) {
                    needed.push(cur.clone());
                }
            }
        }
        // mkcol outside the lock. Concurrent uploads may both try to create
        // the same intermediate dir — WebDAV servers return 405 for already-
        // existing collections, which we swallow.
        for cur in &needed {
            let _ = self.client.mkcol(cur).await;
        }
        if !needed.is_empty() {
            let mut created = self.created_dirs.lock().await;
            for cur in needed {
                created.insert(cur);
            }
        }
        Ok(())
    }

    /// Recursive list with Depth=1 fallback — many WebDAV servers (Jianguoyun /
    /// Nextcloud) refuse `Depth: infinity` or limit it severely.
    fn list_recursive<'a>(
        &'a self,
        prefix: &'a str,
        out: &'a mut Vec<RemoteFileMeta>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = AppResult<()>> + Send + 'a>> {
        Box::pin(async move {
            let path = self.full_path(prefix);
            let path = if path.ends_with('/') {
                path
            } else {
                format!("{path}/")
            };
            let entries = match self.client.list(&path, Depth::Number(1)).await {
                Ok(v) => v,
                Err(e) => {
                    let msg = format!("{e}");
                    if msg.contains("404") || msg.contains("Not Found") {
                        return Ok(());
                    }
                    if msg.contains("401") || msg.contains("403") {
                        return Err(AppError::storage(format!(
                            "WebDAV 凭据无效或权限不足 ({msg})"
                        )));
                    }
                    return Err(AppError::storage(format!("list {path}: {e}")));
                }
            };

            for entry in entries {
                match entry {
                    reqwest_dav::list_cmd::ListEntity::File(f) => {
                        let key = strip_webdav_prefix(&f.href, &self.base_prefix);
                        let modified = Some(f.last_modified.timestamp_millis());
                        out.push(RemoteFileMeta {
                            key,
                            size: f.content_length as u64,
                            etag: f.tag.clone(),
                            modified,
                        });
                    }
                    reqwest_dav::list_cmd::ListEntity::Folder(folder) => {
                        // Skip the directory we just listed (its own href).
                        let folder_key = strip_webdav_prefix(&folder.href, &self.base_prefix);
                        let parent_key = prefix.trim_end_matches('/');
                        let same = folder_key.trim_end_matches('/') == parent_key;
                        if same {
                            continue;
                        }
                        let sub_prefix = folder_key.trim_end_matches('/');
                        self.list_recursive(sub_prefix, out).await?;
                    }
                }
            }
            Ok(())
        })
    }
}

#[async_trait]
impl StorageBackend for WebDAVStorage {
    fn label(&self) -> String {
        format!("webdav:/{}", self.base_prefix)
    }

    async fn ping(&self) -> AppResult<String> {
        let root = if self.base_prefix.is_empty() {
            "/".to_string()
        } else {
            format!("/{}/", self.base_prefix)
        };
        // Try to list root; if not exists, mkcol it.
        match self.client.list(&root, Depth::Number(0)).await {
            Ok(_) => Ok(format!("ok prefix=/{}", self.base_prefix)),
            Err(_) => {
                self.client
                    .mkcol(&root)
                    .await
                    .map_err(|e| AppError::storage(format!("mkcol root {root}: {e}")))?;
                Ok(format!("created prefix=/{}", self.base_prefix))
            }
        }
    }

    async fn list(&self, prefix: &str) -> AppResult<Vec<RemoteFileMeta>> {
        let mut out = Vec::new();
        self.list_recursive(prefix, &mut out).await?;
        Ok(out)
    }

    async fn put(&self, key: &str, body: Vec<u8>) -> AppResult<()> {
        self.ensure_dirs(key).await?;
        let path = self.full_path(key);
        self.client
            .put(&path, body)
            .await
            .map_err(|e| AppError::storage(format!("put {path}: {e}")))?;
        Ok(())
    }

    async fn get(&self, key: &str) -> AppResult<Vec<u8>> {
        let path = self.full_path(key);
        let resp = self
            .client
            .get(&path)
            .await
            .map_err(|e| AppError::storage(format!("get {path}: {e}")))?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| AppError::storage(format!("read body {path}: {e}")))?;
        Ok(bytes.to_vec())
    }

    async fn delete(&self, key: &str) -> AppResult<()> {
        let path = self.full_path(key);
        match self.client.delete(&path).await {
            Ok(_) => Ok(()),
            Err(e) => {
                let msg = format!("{e}");
                if msg.contains("404") {
                    Ok(())
                } else {
                    Err(AppError::storage(format!("delete {path}: {e}")))
                }
            }
        }
    }

    async fn copy(&self, src_key: &str, dst_key: &str) -> AppResult<()> {
        // Ensure destination dirs exist (some servers reject COPY into a
        // non-existent collection with 409 Conflict).
        self.ensure_dirs(dst_key).await?;
        // Skip native COPY if a previous request already proved this server
        // can't do it — saves a round trip per file.
        if !self.copy_disabled.load(Ordering::Relaxed) {
            let src = self.full_path(src_key);
            let dst = self.full_path(dst_key);
            match self.try_native_copy(&src, &dst).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    tracing::debug!("WebDAV native COPY failed, falling back to get+put: {e}");
                }
            }
        }
        // Fallback path — same as trait default but inlined so we don't lose
        // it when overriding the trait method.
        let bytes = self.get(src_key).await?;
        self.put(dst_key, bytes).await
    }
}

fn strip_webdav_prefix(href: &str, base_prefix: &str) -> String {
    // href may look like "/dav/<base_prefix>/<key>" or full URL. Find the base prefix segment.
    let decoded = percent_decode(href);
    let needle = if base_prefix.is_empty() {
        "".to_string()
    } else {
        format!("/{}/", base_prefix)
    };
    if !needle.is_empty() {
        if let Some(idx) = decoded.find(&needle) {
            return decoded[idx + needle.len()..].to_string();
        }
    }
    // fall back: take last path segments after the host
    if let Some(idx) = decoded.find("://") {
        if let Some(slash) = decoded[idx + 3..].find('/') {
            return decoded[idx + 3 + slash + 1..].to_string();
        }
    }
    decoded.trim_start_matches('/').to_string()
}

fn percent_decode(s: &str) -> String {
    // Accumulate raw bytes first so multi-byte UTF-8 sequences (e.g. Chinese
    // file names like "%E5%8E%9F%E7%A5%9E") decode correctly.
    let bytes = s.as_bytes();
    let mut buf: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                buf.push(v);
                i += 3;
                continue;
            }
        }
        buf.push(b);
        i += 1;
    }
    String::from_utf8_lossy(&buf).into_owned()
}
