use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    #[serde(rename = "accessKeyId")]
    pub access_key_id: String,
    #[serde(rename = "secretAccessKey")]
    pub secret_access_key: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default = "default_path_style", rename = "pathStyle")]
    pub path_style: bool,
}

fn default_path_style() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebDAVConfig {
    pub url: String,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub prefix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum BackendConfig {
    S3 { name: String, s3: S3Config },
    Webdav { name: String, webdav: WebDAVConfig },
}

impl BackendConfig {
    pub fn name(&self) -> &str {
        match self {
            BackendConfig::S3 { name, .. } => name,
            BackendConfig::Webdav { name, .. } => name,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GameProfile {
    pub id: String,
    pub name: String,
    #[serde(rename = "savePaths", default)]
    pub save_paths: Vec<String>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(
        rename = "remotePrefix",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub remote_prefix: Option<String>,
    #[serde(rename = "autoSync", default = "default_true")]
    pub auto_sync: bool,
    #[serde(
        rename = "processName",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub process_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    /// Per-game backend override. When None, falls back to the global default
    /// (see `AppSettings::default_backend`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    /// Optional category tag for visual grouping ("RPG", "Action" etc.).
    /// Free-form so users adding custom games can pick anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// User-pinned games appear at the top of the library, above the
    /// configured sort order. Stays on the card via a 📌 toggle.
    #[serde(default)]
    pub pinned: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncDirection {
    Auto,
    Push,
    Pull,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncStateKind {
    Idle,
    Scanning,
    Uploading,
    Downloading,
    Synced,
    Dirty,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSyncStatus {
    #[serde(rename = "gameId")]
    pub game_id: String,
    pub state: SyncStateKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(rename = "localFiles", default)]
    pub local_files: usize,
    #[serde(rename = "remoteFiles", default)]
    pub remote_files: usize,
    #[serde(
        rename = "lastSyncAt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub last_sync_at: Option<i64>,
    #[serde(rename = "lastError", default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ConflictPolicy {
    /// FreeFileSync default: keep both copies by renaming the loser with a
    /// timestamp suffix. Safest for irreplaceable data like game saves.
    RenameBoth,
    NewerWins,
    Ask,
    LocalWins,
    RemoteWins,
}

impl Default for ConflictPolicy {
    fn default() -> Self {
        Self::RenameBoth
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(
        rename = "defaultBackend",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub default_backend: Option<String>,
    #[serde(rename = "autoSyncIntervalSec", default = "default_interval")]
    pub auto_sync_interval_sec: u64,
    #[serde(rename = "enableFileWatcher", default = "default_true")]
    pub enable_file_watcher: bool,
    #[serde(rename = "enableExitSync", default)]
    pub enable_exit_sync: bool,
    #[serde(rename = "conflictPolicy", default)]
    pub conflict_policy: ConflictPolicy,
    /// How many concurrent in-flight transfers. Default 4 — saturates most
    /// home connections without overwhelming the backend.
    #[serde(rename = "maxConcurrency", default = "default_concurrency")]
    pub max_concurrency: usize,
    /// Number of historical versions to retain on the remote per file.
    /// 0 disables versioning. Default 5 — Syncthing-style safety net.
    #[serde(rename = "versionsToKeep", default = "default_versions")]
    pub versions_to_keep: usize,
    /// When true, manual "Sync now" actions show a dry-run preview modal first.
    /// FreeFileSync default — users see exactly what will happen before commit.
    #[serde(rename = "alwaysPreview", default = "default_true")]
    pub always_preview: bool,
    /// When true, closing the window hides it to the tray instead of quitting.
    /// Lets file-watcher / scheduler / process-watch keep running.
    #[serde(rename = "closeToTray", default = "default_true")]
    pub close_to_tray: bool,
    /// Soft bandwidth cap in bytes/sec, applied across all in-flight uploads
    /// and downloads. 0 = unlimited (rclone --bwlimit semantics).
    #[serde(rename = "maxBytesPerSec", default)]
    pub max_bytes_per_sec: u64,
    /// When true, post a system notification when a sync completes (especially
    /// useful when the window is minimized to the tray).
    #[serde(rename = "notifyOnComplete", default = "default_true")]
    pub notify_on_complete: bool,
    /// Auto-open DevTools on startup. Defaults to `true` for the v1.3.x
    /// diagnostic phase — once the user verifies the app loads correctly
    /// they should turn this off via Settings.
    #[serde(rename = "autoOpenDevtools", default = "default_true")]
    pub auto_open_devtools: bool,
    /// UI theme: "light" | "dark" | "auto" (follow system).
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Whether to check for app updates on launch.
    #[serde(rename = "autoCheckUpdates", default = "default_true")]
    pub auto_check_updates: bool,
}

fn default_theme() -> String {
    "light".to_string()
}

fn default_interval() -> u64 {
    600
}

fn default_concurrency() -> usize {
    4
}

fn default_versions() -> usize {
    5
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            default_backend: None,
            auto_sync_interval_sec: 600,
            enable_file_watcher: true,
            enable_exit_sync: false,
            conflict_policy: ConflictPolicy::default(),
            max_concurrency: default_concurrency(),
            versions_to_keep: default_versions(),
            always_preview: true,
            close_to_tray: true,
            max_bytes_per_sec: 0,
            notify_on_complete: true,
            auto_open_devtools: true,
            theme: default_theme(),
            auto_check_updates: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigDoc {
    #[serde(default)]
    pub games: Vec<GameProfile>,
    #[serde(default)]
    pub backends: Vec<BackendConfig>,
    #[serde(default)]
    pub settings: AppSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFileMeta {
    pub key: String,
    pub size: u64,
    pub etag: Option<String>,
    pub modified: Option<i64>,
}

/// A user-named snapshot — captures the game's full file set under a label
/// like "黑暗剧情线-决战前". Distinct from auto-versions (which are pruned
/// by `versions_to_keep`); manual snapshots stay forever until explicitly
/// deleted. Storage layout:
///
///   .gsyncing/snapshots/manifests/<id>.json   (this struct, JSON-serialized)
///   .gsyncing/snapshots/files/<id>/<rel-path> (the snapshotted blobs)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedSnapshot {
    pub id: String,
    #[serde(rename = "gameId")]
    pub game_id: String,
    pub name: String,
    #[serde(rename = "createdAt")]
    pub created_at_ms: i64,
    pub files: std::collections::BTreeMap<String, SnapshotFileEntry>,
    #[serde(rename = "totalSize", default)]
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotFileEntry {
    pub sha256: String,
    pub size: u64,
    #[serde(rename = "modifiedMs", default)]
    pub modified_ms: i64,
    #[serde(rename = "rootIndex", default)]
    pub root_index: usize,
}
