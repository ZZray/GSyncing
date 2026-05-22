use crate::crypto;
use crate::error::{AppError, AppResult};
use crate::logbus::LogBus;
use crate::model::{
    AppSettings, BackendConfig, ConfigDoc, GameProfile, GameSyncStatus, SyncStateKind,
};
use crate::paths;
use crate::storage::{RateLimiter, StorageBackend};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, Notify, RwLock};
use uuid::Uuid;

pub struct AppState {
    pub handle: AppHandle,
    pub log: Arc<LogBus>,
    config: RwLock<ConfigDoc>,
    /// Per-game sync status — runtime only, not persisted (transient UI state).
    statuses: RwLock<HashMap<String, GameSyncStatus>>,
    /// Notify channel that wakes the scheduler when settings change.
    pub scheduler_notify: Notify,
    /// Notify channel that wakes the watcher when game list changes.
    pub watcher_notify: Notify,
    /// Per-game mutex to serialize sync runs for the same game.
    sync_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// Set of game ids currently being synced (deduplicates watcher-spawned tasks).
    in_flight: Mutex<HashSet<String>>,
    /// Cache of built storage backends keyed by backend name.
    backend_cache: Mutex<HashMap<String, Arc<dyn crate::storage::StorageBackend>>>,
    /// Cached settings flags accessible from sync contexts (window event
    /// handlers run on the main GUI thread and must NOT call `block_on` on
    /// tokio mutexes).
    pub close_to_tray: AtomicBool,
    /// Per-game cancel token. Engine checks before each file IO.
    cancel_tokens: Mutex<HashMap<String, tokio_util::sync::CancellationToken>>,
    /// Shared bandwidth limiter — passed to every RetryingBackend so changes
    /// to `max_bytes_per_sec` apply immediately without rebuilding clients.
    pub rate_limiter: RateLimiter,
    /// Append-only sync history for the stats dashboard.
    pub stats: crate::stats::StatStore,
}

impl AppState {
    pub async fn initialize(handle: AppHandle) -> AppResult<Self> {
        let log = Arc::new(LogBus::new(handle.clone()));
        let path = paths::config_file()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let config: ConfigDoc = if path.exists() {
            let buf = std::fs::read_to_string(&path)?;
            let mut doc: ConfigDoc = serde_json::from_str(&buf).unwrap_or_else(|e| {
                log.warn("config", format!("config parse failed, reset: {e}"));
                ConfigDoc::default()
            });
            // Decrypt sensitive fields lazily. Legacy plaintext entries pass
            // through `decrypt` unchanged, then get rewritten as ciphertext on
            // the next persist().
            decrypt_in_place(&mut doc.backends, &log);
            doc
        } else {
            ConfigDoc::default()
        };
        log.info(
            "config",
            format!(
                "loaded {} games, {} backends",
                config.games.len(),
                config.backends.len()
            ),
        );
        let close_to_tray = AtomicBool::new(config.settings.close_to_tray);
        let rate_limiter = RateLimiter::new(config.settings.max_bytes_per_sec);
        let stats = crate::stats::StatStore::new()?;
        Ok(Self {
            handle,
            log,
            config: RwLock::new(config),
            statuses: RwLock::new(HashMap::new()),
            scheduler_notify: Notify::new(),
            watcher_notify: Notify::new(),
            sync_locks: Mutex::new(HashMap::new()),
            in_flight: Mutex::new(HashSet::new()),
            backend_cache: Mutex::new(HashMap::new()),
            close_to_tray,
            cancel_tokens: Mutex::new(HashMap::new()),
            rate_limiter,
            stats,
        })
    }

    pub async fn snapshot(&self) -> ConfigDoc {
        // Returns the in-memory representation, which always holds plaintext
        // secrets — caller (e.g., export_config) must decide whether to
        // re-encrypt before writing to disk.
        self.config.read().await.clone()
    }

    async fn persist(&self) -> AppResult<()> {
        let mut doc = self.config.read().await.clone();
        encrypt_in_place(&mut doc.backends, &self.log);
        let path = paths::config_file()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let buf = serde_json::to_string_pretty(&doc)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, buf)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    // ---------- games ----------
    pub async fn list_games(&self) -> Vec<GameProfile> {
        self.config.read().await.games.clone()
    }

    pub async fn save_game(&self, mut game: GameProfile) -> AppResult<GameProfile> {
        if game.id.is_empty() {
            game.id = Uuid::new_v4().to_string();
        }
        {
            let mut doc = self.config.write().await;
            if let Some(existing) = doc.games.iter_mut().find(|g| g.id == game.id) {
                *existing = game.clone();
            } else {
                doc.games.push(game.clone());
            }
        }
        self.persist().await?;
        self.watcher_notify.notify_waiters();
        self.scheduler_notify.notify_waiters();
        self.log.info("game", format!("saved {}", game.name));
        Ok(game)
    }

    pub async fn delete_game(&self, id: &str) -> AppResult<()> {
        {
            let mut doc = self.config.write().await;
            let before = doc.games.len();
            doc.games.retain(|g| g.id != id);
            if doc.games.len() == before {
                return Err(AppError::GameNotFound(id.into()));
            }
        }
        self.statuses.write().await.remove(id);
        self.persist().await?;
        self.watcher_notify.notify_waiters();
        self.scheduler_notify.notify_waiters();
        Ok(())
    }

    pub async fn get_game(&self, id: &str) -> AppResult<GameProfile> {
        self.config
            .read()
            .await
            .games
            .iter()
            .find(|g| g.id == id)
            .cloned()
            .ok_or_else(|| AppError::GameNotFound(id.into()))
    }

    // ---------- backends ----------
    pub async fn list_backends(&self) -> Vec<BackendConfig> {
        self.config.read().await.backends.clone()
    }

    pub async fn save_backend(&self, backend: BackendConfig) -> AppResult<Vec<BackendConfig>> {
        {
            let mut doc = self.config.write().await;
            if let Some(existing) = doc.backends.iter_mut().find(|b| b.name() == backend.name()) {
                *existing = backend.clone();
            } else {
                doc.backends.push(backend.clone());
            }
        }
        self.backend_cache.lock().await.remove(backend.name());
        self.persist().await?;
        self.log
            .info("backend", format!("saved {}", backend.name()));
        Ok(self.list_backends().await)
    }

    pub async fn delete_backend(&self, name: &str) -> AppResult<Vec<BackendConfig>> {
        {
            let mut doc = self.config.write().await;
            let before = doc.backends.len();
            doc.backends.retain(|b| b.name() != name);
            if doc.backends.len() == before {
                return Err(AppError::BackendNotFound(name.into()));
            }
            if let Some(ref def) = doc.settings.default_backend {
                if def == name {
                    doc.settings.default_backend = None;
                }
            }
        }
        self.backend_cache.lock().await.remove(name);
        self.persist().await?;
        Ok(self.list_backends().await)
    }

    /// Get a cached backend client by name, building it lazily.
    pub async fn get_backend_client(&self, name: &str) -> AppResult<Arc<dyn StorageBackend>> {
        {
            let cache = self.backend_cache.lock().await;
            if let Some(b) = cache.get(name) {
                return Ok(b.clone());
            }
        }
        let cfg = self.get_backend(name).await?;
        let client = crate::storage::build(&cfg, self.rate_limiter.clone()).await?;
        self.backend_cache
            .lock()
            .await
            .insert(name.to_string(), client.clone());
        Ok(client)
    }

    pub async fn default_backend_client(&self) -> AppResult<Arc<dyn StorageBackend>> {
        let cfg = self.default_backend().await?;
        self.get_backend_client(cfg.name()).await
    }

    pub async fn get_backend(&self, name: &str) -> AppResult<BackendConfig> {
        self.config
            .read()
            .await
            .backends
            .iter()
            .find(|b| b.name() == name)
            .cloned()
            .ok_or_else(|| AppError::BackendNotFound(name.into()))
    }

    pub async fn default_backend(&self) -> AppResult<BackendConfig> {
        let doc = self.config.read().await;
        let name = doc
            .settings
            .default_backend
            .clone()
            .or_else(|| doc.backends.first().map(|b| b.name().to_string()))
            .ok_or_else(|| AppError::Config("no backend configured".into()))?;
        doc.backends
            .iter()
            .find(|b| b.name() == name)
            .cloned()
            .ok_or_else(|| AppError::BackendNotFound(name))
    }

    // ---------- settings ----------
    pub async fn get_settings(&self) -> AppSettings {
        self.config.read().await.settings.clone()
    }

    /// Blocking variant used only from the Tauri setup hook (which runs on
    /// the main thread, not inside a tokio task). `try_read` avoids ever
    /// deadlocking against an in-progress writer at startup.
    pub fn get_settings_blocking(&self) -> AppSettings {
        self.config
            .try_read()
            .map(|c| c.settings.clone())
            .unwrap_or_default()
    }

    pub async fn save_settings(&self, settings: AppSettings) -> AppResult<AppSettings> {
        {
            let mut doc = self.config.write().await;
            doc.settings = settings.clone();
        }
        self.close_to_tray
            .store(settings.close_to_tray, Ordering::Relaxed);
        self.rate_limiter.set_rate(settings.max_bytes_per_sec);
        self.persist().await?;
        self.scheduler_notify.notify_waiters();
        self.watcher_notify.notify_waiters();
        Ok(settings)
    }

    // ---------- status (runtime only, NOT persisted) ----------
    pub async fn list_status(&self) -> Vec<GameSyncStatus> {
        self.statuses.read().await.values().cloned().collect()
    }

    pub async fn update_status(&self, status: GameSyncStatus) {
        self.statuses
            .write()
            .await
            .insert(status.game_id.clone(), status.clone());
        let payload = serde_json::json!({ "gameId": status.game_id, "status": status });
        let _ = self.handle.emit("status-change", payload);
    }

    /// Try to mark a game as in-flight. Returns false if it was already.
    pub async fn try_acquire_in_flight(&self, id: &str) -> bool {
        self.in_flight.lock().await.insert(id.to_string())
    }

    pub async fn release_in_flight(&self, id: &str) {
        self.in_flight.lock().await.remove(id);
    }

    /// Get-or-create the cancel token for a game's current sync. A fresh token
    /// is minted on the first call after a previous one was cancelled — so
    /// "cancel then re-sync" works as expected.
    pub async fn cancel_token_for(&self, id: &str) -> tokio_util::sync::CancellationToken {
        let mut map = self.cancel_tokens.lock().await;
        if let Some(t) = map.get(id) {
            if !t.is_cancelled() {
                return t.clone();
            }
        }
        let token = tokio_util::sync::CancellationToken::new();
        map.insert(id.to_string(), token.clone());
        token
    }

    /// Trip the cancel token for a game. Returns true if a token existed.
    pub async fn cancel_sync(&self, id: &str) -> bool {
        let map = self.cancel_tokens.lock().await;
        if let Some(t) = map.get(id) {
            if !t.is_cancelled() {
                t.cancel();
                self.log.info("sync", format!("cancel requested for {id}"));
                return true;
            }
        }
        false
    }

    pub fn emit_global(
        &self,
        running: bool,
        message: Option<String>,
        success: Option<i64>,
        err: Option<String>,
    ) {
        let payload = serde_json::json!({
            "running": running,
            "message": message,
            "lastSuccessAt": success,
            "lastError": err,
        });
        let _ = self.handle.emit("global-sync", payload);
    }

    pub fn emit_progress(
        &self,
        game_id: &str,
        phase: &str,
        current: usize,
        total: usize,
        current_file: Option<&str>,
        bytes_done: u64,
        bytes_total: u64,
        started_at: i64,
    ) {
        let payload = serde_json::json!({
            "gameId": game_id,
            "phase": phase,
            "current": current,
            "total": total,
            "currentFile": current_file,
            "bytesDone": bytes_done,
            "bytesTotal": bytes_total,
            "startedAt": started_at,
            "now": chrono::Utc::now().timestamp_millis(),
        });
        let _ = self.handle.emit("sync-progress", payload);
    }

    /// Get (or create) the lock for a particular game.
    pub async fn lock_for_game(&self, id: &str) -> Arc<Mutex<()>> {
        let mut map = self.sync_locks.lock().await;
        map.entry(id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Public hook for export_config / import_config to round-trip the doc
    /// without going through persist().
    pub async fn replace_config(&self, mut doc: ConfigDoc) -> AppResult<()> {
        // Input may contain cipher OR plaintext (import case). Decrypt in
        // place so in-memory representation is always plaintext.
        decrypt_in_place(&mut doc.backends, &self.log);
        *self.config.write().await = doc;
        self.close_to_tray.store(
            self.config.read().await.settings.close_to_tray,
            std::sync::atomic::Ordering::Relaxed,
        );
        self.persist().await?;
        self.scheduler_notify.notify_waiters();
        self.watcher_notify.notify_waiters();
        Ok(())
    }

    pub async fn set_state(&self, game_id: &str, state: SyncStateKind, msg: Option<&str>) {
        let mut existing =
            self.statuses
                .read()
                .await
                .get(game_id)
                .cloned()
                .unwrap_or(GameSyncStatus {
                    game_id: game_id.to_string(),
                    state,
                    message: None,
                    local_files: 0,
                    remote_files: 0,
                    last_sync_at: None,
                    last_error: None,
                });
        existing.state = state;
        existing.message = msg.map(|s| s.to_string());
        if !matches!(state, SyncStateKind::Error) {
            existing.last_error = None;
        }
        self.update_status(existing).await;
    }
}

/// Encrypt secret fields on backends in place. Idempotent — already-encrypted
/// entries pass through.
fn encrypt_in_place(backends: &mut Vec<BackendConfig>, log: &LogBus) {
    for b in backends.iter_mut() {
        match b {
            BackendConfig::S3 { s3, .. } => {
                if !crypto::is_already_protected(&s3.access_key_id) {
                    match crypto::encrypt(&s3.access_key_id) {
                        Ok(v) => s3.access_key_id = v,
                        Err(e) => log.warn("crypto", format!("encrypt access_key_id: {e}")),
                    }
                }
                if !crypto::is_already_protected(&s3.secret_access_key) {
                    match crypto::encrypt(&s3.secret_access_key) {
                        Ok(v) => s3.secret_access_key = v,
                        Err(e) => log.warn("crypto", format!("encrypt secret: {e}")),
                    }
                }
            }
            BackendConfig::Webdav { webdav, .. } => {
                if !crypto::is_already_protected(&webdav.password) {
                    match crypto::encrypt(&webdav.password) {
                        Ok(v) => webdav.password = v,
                        Err(e) => log.warn("crypto", format!("encrypt webdav pw: {e}")),
                    }
                }
            }
        }
    }
}

/// Decrypt secret fields in place. Legacy plaintext entries pass through.
fn decrypt_in_place(backends: &mut Vec<BackendConfig>, log: &LogBus) {
    for b in backends.iter_mut() {
        match b {
            BackendConfig::S3 { s3, .. } => {
                if crypto::is_already_protected(&s3.access_key_id) {
                    match crypto::decrypt(&s3.access_key_id) {
                        Ok(v) => s3.access_key_id = v,
                        Err(e) => log.warn("crypto", format!("decrypt access_key_id: {e}")),
                    }
                }
                if crypto::is_already_protected(&s3.secret_access_key) {
                    match crypto::decrypt(&s3.secret_access_key) {
                        Ok(v) => s3.secret_access_key = v,
                        Err(e) => log.warn("crypto", format!("decrypt secret: {e}")),
                    }
                }
            }
            BackendConfig::Webdav { webdav, .. } => {
                if crypto::is_already_protected(&webdav.password) {
                    match crypto::decrypt(&webdav.password) {
                        Ok(v) => webdav.password = v,
                        Err(e) => log.warn("crypto", format!("decrypt webdav pw: {e}")),
                    }
                }
            }
        }
    }
}
