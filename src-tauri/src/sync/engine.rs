use crate::error::{AppError, AppResult};
use crate::model::{
    AppSettings, ConflictPolicy, GameProfile, GameSyncStatus, SyncDirection, SyncStateKind,
};
use crate::paths;
use crate::state::AppState;
use crate::storage::StorageBackend;
use crate::sync::scanner::{self, LocalFile, ScanHints};
use chrono::Utc;
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

pub const META_KEY: &str = ".gsyncing/index.json";
pub const VERSIONS_PREFIX: &str = ".gsyncing/versions";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RemoteIndex {
    pub files: BTreeMap<String, RemoteIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteIndexEntry {
    pub sha256: String,
    pub size: u64,
    pub modified_ms: i64,
    pub root_index: usize,
}

#[derive(Debug, Clone)]
struct ConflictDescriptor {
    rel: String,
    local_file: LocalFile,
    local_rename: String,
    remote_rename: String,
}

#[derive(Debug, Clone)]
struct SyncPlan {
    /// Files to upload (path resolved locally, will be read at execute time).
    upload: Vec<LocalFile>,
    /// (rel, remote-entry) to download.
    download: Vec<(String, RemoteIndexEntry)>,
    /// Rel paths to delete on remote.
    delete_remote: Vec<String>,
    /// (rel, absolute path) to delete locally.
    delete_local: Vec<(String, PathBuf)>,
    /// Conflict-rename rescue copies.
    conflicts: Vec<ConflictDescriptor>,
    /// Index to persist after execution.
    new_index: RemoteIndex,
    /// Keys that already exist on remote — used to gate version archiving.
    existing_remote: BTreeSet<String>,
    /// Local roots, needed to resolve remote-key → local-path at execute time.
    roots: Vec<PathBuf>,
}

/// Serializable preview given to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct SyncPreview {
    #[serde(rename = "gameId")]
    pub game_id: String,
    pub direction: SyncDirection,
    pub uploads: Vec<PreviewItem>,
    pub downloads: Vec<PreviewItem>,
    #[serde(rename = "deleteRemote")]
    pub delete_remote: Vec<String>,
    #[serde(rename = "deleteLocal")]
    pub delete_local: Vec<String>,
    pub conflicts: Vec<PreviewConflict>,
    #[serde(rename = "totalBytes")]
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreviewItem {
    pub rel: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreviewConflict {
    pub rel: String,
    #[serde(rename = "localSize")]
    pub local_size: u64,
    #[serde(rename = "localMtime")]
    pub local_mtime: i64,
    #[serde(rename = "remoteSize")]
    pub remote_size: u64,
    #[serde(rename = "remoteMtime")]
    pub remote_mtime: i64,
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

pub async fn run_one(
    state: Arc<AppState>,
    game_id: &str,
    direction: SyncDirection,
) -> AppResult<()> {
    run_one_with_overrides(state, game_id, direction, ConflictOverrides::new()).await
}

pub async fn run_one_with_overrides(
    state: Arc<AppState>,
    game_id: &str,
    direction: SyncDirection,
    overrides: ConflictOverrides,
) -> AppResult<()> {
    let game = state.get_game(game_id).await?;
    let lock = state.lock_for_game(game_id).await;
    let _guard = lock.lock().await;

    state
        .set_state(&game.id, SyncStateKind::Scanning, Some("扫描本地文件"))
        .await;
    let started_at_wall = Utc::now().timestamp_millis();
    state.log.info(
        "sync",
        format!("[{}] start direction={:?}", game.name, direction),
    );

    let backend = backend_for_game(&state, &game).await?;
    let remote_prefix = remote_prefix_for(&game);
    let settings = state.get_settings().await;

    // Cross-machine advisory lock. Best-effort: not a true CAS, but covers
    // the common "two PCs syncing the same save folder" case loudly enough
    // that the user can react instead of double-writing silently.
    let heartbeat_cancel = tokio_util::sync::CancellationToken::new();
    let remote_lock_held =
        match crate::sync::remote_lock::try_acquire(backend.as_ref(), &remote_prefix).await {
            Ok(l) => {
                crate::sync::remote_lock::spawn_heartbeat(
                    backend.clone(),
                    remote_prefix.clone(),
                    l.clone(),
                    heartbeat_cancel.clone(),
                );
                Some(l)
            }
            Err(e) => {
                let status = GameSyncStatus {
                    game_id: game.id.clone(),
                    state: SyncStateKind::Error,
                    message: Some(e.to_string()),
                    local_files: 0,
                    remote_files: 0,
                    last_sync_at: None,
                    last_error: Some(e.to_string()),
                };
                state.update_status(status).await;
                state.emit_progress(&game.id, "done", 0, 0, None, 0, 0, 0);
                state.log.warn("sync", format!("[{}] {}", game.name, e));
                return Err(e);
            }
        };

    let plan = build_plan(
        &state,
        &game,
        backend.as_ref(),
        &remote_prefix,
        direction,
        &overrides,
    )
    .await?;

    let result = execute_plan(
        &state,
        &game,
        backend.clone(),
        &remote_prefix,
        plan,
        &settings,
    )
    .await;

    // Always release the remote lock + stop the heartbeat — even on error
    // path. Skipping cleanup would leave the lock blocking the next sync
    // until TTL expires.
    heartbeat_cancel.cancel();
    if let Some(held) = remote_lock_held {
        if let Err(e) =
            crate::sync::remote_lock::release(backend.as_ref(), &remote_prefix, &held).await
        {
            state.log.warn("sync", format!("release remote lock: {e}"));
        }
    }

    let local_files_count = scan_count(&game).await.unwrap_or(0);
    let remote_files_count = backend
        .list(&remote_prefix)
        .await
        .map(|v| v.iter().filter(|m| !m.key.contains("/.gsyncing/")).count())
        .unwrap_or(0);

    // Always clear the progress bar — frontend sees total=0 and hides itself.
    // Without this, an error mid-flight would leave the UI stuck at the last
    // emitted progress frame.
    state.emit_progress(&game.id, "done", 0, 0, None, 0, 0, 0);

    let now_ms = Utc::now().timestamp_millis();
    let duration_ms = now_ms - started_at_wall;
    let notify_enabled = state.get_settings().await.notify_on_complete;
    match result {
        Ok(()) => {
            let status = GameSyncStatus {
                game_id: game.id.clone(),
                state: SyncStateKind::Synced,
                message: Some("已同步".to_string()),
                local_files: local_files_count,
                remote_files: remote_files_count,
                last_sync_at: Some(now_ms),
                last_error: None,
            };
            state.update_status(status).await;
            let _ = state
                .stats
                .append(crate::stats::StatEntry {
                    ts: now_ms,
                    game_id: game.id.clone(),
                    direction: format!("{direction:?}").to_lowercase(),
                    success: true,
                    uploaded_files: 0, // populated below if we tracked it
                    downloaded_files: 0,
                    total_bytes: 0,
                    duration_ms,
                    error: None,
                })
                .await;
            if notify_enabled {
                send_notification(&state, &format!("{} 同步完成", game.name), None);
            }
            state.log.info("sync", format!("[{}] OK", game.name));
            Ok(())
        }
        Err(e) => {
            let status = GameSyncStatus {
                game_id: game.id.clone(),
                state: SyncStateKind::Error,
                message: Some(e.to_string()),
                local_files: local_files_count,
                remote_files: remote_files_count,
                last_sync_at: None,
                last_error: Some(e.to_string()),
            };
            state.update_status(status).await;
            let _ = state
                .stats
                .append(crate::stats::StatEntry {
                    ts: now_ms,
                    game_id: game.id.clone(),
                    direction: format!("{direction:?}").to_lowercase(),
                    success: false,
                    uploaded_files: 0,
                    downloaded_files: 0,
                    total_bytes: 0,
                    duration_ms,
                    error: Some(e.to_string()),
                })
                .await;
            if notify_enabled {
                send_notification(
                    &state,
                    &format!("{} 同步失败", game.name),
                    Some(e.to_string()),
                );
            }
            state
                .log
                .error("sync", format!("[{}] FAIL: {}", game.name, e));
            Err(e)
        }
    }
}

fn send_notification(state: &Arc<AppState>, title: &str, body: Option<String>) {
    use tauri_plugin_notification::NotificationExt;
    let mut builder = state.handle.notification().builder().title(title);
    if let Some(b) = body {
        builder = builder.body(b);
    }
    if let Err(e) = builder.show() {
        state.log.debug("notify", format!("show: {e}"));
    }
}

/// Run only the planning phase — no IO that changes either side. Used by the
/// dry-run preview path so the UI can show "will upload N / download M /
/// delete K" before the user confirms.
pub async fn preview(
    state: Arc<AppState>,
    game_id: &str,
    direction: SyncDirection,
) -> AppResult<SyncPreview> {
    let game = state.get_game(game_id).await?;
    let backend = backend_for_game(&state, &game).await?;
    let remote_prefix = remote_prefix_for(&game);
    let no_overrides = ConflictOverrides::new();
    let plan = build_plan(
        &state,
        &game,
        backend.as_ref(),
        &remote_prefix,
        direction,
        &no_overrides,
    )
    .await?;
    Ok(plan_to_preview(&plan, game.id.clone(), direction))
}

/// Public re-export of `backend_for_game` for sibling modules (snapshot.rs).
pub async fn backend_for_game_public(
    state: &Arc<AppState>,
    game: &GameProfile,
) -> AppResult<Arc<dyn StorageBackend>> {
    backend_for_game(state, game).await
}

/// Public re-export of `remote_prefix_for`.
pub fn remote_prefix_for_public(game: &GameProfile) -> String {
    remote_prefix_for(game)
}

/// Public re-export of `load_prior_index`.
pub async fn load_prior_index_public(game_id: &str) -> AppResult<RemoteIndex> {
    load_prior_index(game_id).await
}

/// Public re-export of `save_prior_index`.
pub async fn save_prior_index_public(game_id: &str, idx: &RemoteIndex) -> AppResult<()> {
    save_prior_index(game_id, idx).await
}

/// Public re-export of `save_remote_index`.
pub async fn save_remote_index_public(
    backend: &dyn StorageBackend,
    remote_prefix: &str,
    idx: &RemoteIndex,
) -> AppResult<()> {
    save_remote_index(backend, remote_prefix, idx).await
}

async fn backend_for_game(
    state: &Arc<AppState>,
    game: &GameProfile,
) -> AppResult<Arc<dyn StorageBackend>> {
    // Per-game override falls through to the default when the named backend
    // doesn't exist anymore — easier than failing the sync hard mid-run.
    if let Some(name) = &game.backend {
        if let Ok(client) = state.get_backend_client(name).await {
            return Ok(client);
        }
        state.log.warn(
            "sync",
            format!(
                "[{}] backend override '{}' missing, using default",
                game.name, name
            ),
        );
    }
    state.default_backend_client().await
}

fn plan_to_preview(plan: &SyncPlan, game_id: String, direction: SyncDirection) -> SyncPreview {
    let total_bytes: u64 = plan.upload.iter().map(|f| f.size).sum::<u64>()
        + plan.download.iter().map(|(_, e)| e.size).sum::<u64>();
    SyncPreview {
        game_id,
        direction,
        uploads: plan
            .upload
            .iter()
            .map(|f| PreviewItem {
                rel: f.relative.clone(),
                size: f.size,
            })
            .collect(),
        downloads: plan
            .download
            .iter()
            .map(|(rel, e)| PreviewItem {
                rel: rel.clone(),
                size: e.size,
            })
            .collect(),
        delete_remote: plan.delete_remote.clone(),
        delete_local: plan.delete_local.iter().map(|(r, _)| r.clone()).collect(),
        conflicts: plan
            .conflicts
            .iter()
            .map(|c| PreviewConflict {
                rel: c.rel.clone(),
                local_size: c.local_file.size,
                local_mtime: c.local_file.modified_ms,
                remote_size: 0,
                remote_mtime: 0,
            })
            .collect(),
        total_bytes,
    }
}

// ---------------------------------------------------------------------------
// Planning
// ---------------------------------------------------------------------------

/// User-supplied per-file conflict resolutions, keyed by the file's relative
/// path. When a key is present, `decide_conflict` short-circuits and returns
/// the user's choice instead of consulting `ConflictPolicy`.
pub type ConflictOverrides = std::collections::HashMap<String, String>;

async fn build_plan(
    state: &Arc<AppState>,
    game: &GameProfile,
    backend: &dyn StorageBackend,
    remote_prefix: &str,
    direction: SyncDirection,
    overrides: &ConflictOverrides,
) -> AppResult<SyncPlan> {
    let settings = state.get_settings().await;
    let prior_index = load_prior_index(&game.id).await.unwrap_or_default();
    let hints: ScanHints = prior_index
        .files
        .iter()
        .map(|(rel, e)| (rel.clone(), (e.size, e.modified_ms, e.sha256.clone())))
        .collect();

    let game_for_scan = game.clone();
    let hints_for_scan = hints.clone();
    let scan = tokio::task::spawn_blocking(move || {
        scanner::scan_with_hints(&game_for_scan, &hints_for_scan)
    })
    .await
    .map_err(|e| AppError::other(format!("scan join: {e}")))??;
    state.log.info(
        "sync",
        format!(
            "[{}] scanned {} local files ({} hashes reused)",
            game.name,
            scan.files.len(),
            scan.reused_hashes
        ),
    );

    let remote_index = load_remote_index(backend, remote_prefix)
        .await
        .unwrap_or_default();
    let existing_remote: BTreeSet<String> = remote_index.files.keys().cloned().collect();

    let plan = match direction {
        SyncDirection::Push => plan_push(&scan.files, &remote_index, &existing_remote, &scan.roots),
        SyncDirection::Pull => plan_pull(&scan.files, &remote_index, &prior_index, &scan.roots),
        SyncDirection::Auto => plan_two_way(
            &scan.files,
            &remote_index,
            &prior_index,
            &scan.roots,
            settings.conflict_policy,
            overrides,
        ),
    };
    Ok(plan)
}

fn plan_push(
    local_files: &[LocalFile],
    prev: &RemoteIndex,
    existing: &BTreeSet<String>,
    roots: &[PathBuf],
) -> SyncPlan {
    let mut upload: Vec<LocalFile> = Vec::new();
    let local_keys: BTreeSet<String> = local_files.iter().map(|f| f.relative.clone()).collect();
    for f in local_files {
        let changed = prev
            .files
            .get(&f.relative)
            .map(|e| e.sha256 != f.sha256)
            .unwrap_or(true);
        if changed {
            upload.push(f.clone());
        }
    }
    let delete_remote: Vec<String> = prev
        .files
        .keys()
        .filter(|k| !local_keys.contains(*k))
        .cloned()
        .collect();
    SyncPlan {
        upload,
        download: Vec::new(),
        delete_remote,
        delete_local: Vec::new(),
        conflicts: Vec::new(),
        new_index: build_index(local_files),
        existing_remote: existing.clone(),
        roots: roots.to_vec(),
    }
}

fn plan_pull(
    local_files: &[LocalFile],
    remote_index: &RemoteIndex,
    prior: &RemoteIndex,
    roots: &[PathBuf],
) -> SyncPlan {
    if remote_index.files.is_empty() {
        return SyncPlan {
            upload: Vec::new(),
            download: Vec::new(),
            delete_remote: Vec::new(),
            delete_local: Vec::new(),
            conflicts: Vec::new(),
            new_index: RemoteIndex::default(),
            existing_remote: BTreeSet::new(),
            roots: roots.to_vec(),
        };
    }
    let local_idx = scanner::index_by_relative(local_files);
    let download: Vec<(String, RemoteIndexEntry)> = remote_index
        .files
        .iter()
        .filter(|(rel, entry)| match local_idx.get(*rel) {
            Some(lf) => lf.sha256 != entry.sha256,
            None => true,
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let remote_keys: BTreeSet<String> = remote_index.files.keys().cloned().collect();
    let mut delete_local: Vec<(String, PathBuf)> = Vec::new();
    for lf in local_files {
        if prior.files.contains_key(&lf.relative) && !remote_keys.contains(&lf.relative) {
            delete_local.push((lf.relative.clone(), lf.absolute.clone()));
        }
    }
    SyncPlan {
        upload: Vec::new(),
        download,
        delete_remote: Vec::new(),
        delete_local,
        conflicts: Vec::new(),
        new_index: remote_index.clone(),
        existing_remote: remote_keys,
        roots: roots.to_vec(),
    }
}

fn plan_two_way(
    local_files: &[LocalFile],
    remote_index: &RemoteIndex,
    prior: &RemoteIndex,
    roots: &[PathBuf],
    policy: ConflictPolicy,
    overrides: &ConflictOverrides,
) -> SyncPlan {
    let local_idx = scanner::index_by_relative(local_files);

    let mut new_index = RemoteIndex::default();
    let mut upload: Vec<LocalFile> = Vec::new();
    let mut download: Vec<(String, RemoteIndexEntry)> = Vec::new();
    let mut delete_remote: Vec<String> = Vec::new();
    let mut delete_local: Vec<(String, PathBuf)> = Vec::new();
    let mut conflicts: Vec<ConflictDescriptor> = Vec::new();

    let all_keys: BTreeSet<String> = local_idx
        .keys()
        .cloned()
        .chain(remote_index.files.keys().cloned())
        .collect();

    for rel in all_keys {
        let local = local_idx.get(&rel).copied();
        let remote = remote_index.files.get(&rel).cloned();
        let prior_entry = prior.files.get(&rel).cloned();

        match (local, remote, prior_entry) {
            (Some(lf), Some(re), _) if lf.sha256 == re.sha256 => {
                new_index.files.insert(
                    rel.clone(),
                    RemoteIndexEntry {
                        sha256: lf.sha256.clone(),
                        size: lf.size,
                        modified_ms: lf.modified_ms,
                        root_index: lf.root_index,
                    },
                );
            }
            (Some(lf), Some(re), prior_entry) => {
                let local_changed = prior_entry
                    .as_ref()
                    .map(|p| p.sha256 != lf.sha256)
                    .unwrap_or(true);
                let remote_changed = prior_entry
                    .as_ref()
                    .map(|p| p.sha256 != re.sha256)
                    .unwrap_or(true);
                let action =
                    decide_conflict(lf, &re, local_changed, remote_changed, policy, overrides, &rel);
                match action {
                    ConflictResolution::PickLocal => {
                        upload.push(lf.clone());
                        new_index.files.insert(
                            rel.clone(),
                            RemoteIndexEntry {
                                sha256: lf.sha256.clone(),
                                size: lf.size,
                                modified_ms: lf.modified_ms,
                                root_index: lf.root_index,
                            },
                        );
                    }
                    ConflictResolution::PickRemote => {
                        download.push((rel.clone(), re.clone()));
                        new_index.files.insert(rel.clone(), re.clone());
                    }
                    ConflictResolution::RenameBoth => {
                        let stamp = conflict_suffix();
                        let local_rename = format!("{}.local-{}", rel, stamp);
                        let remote_rename = format!("{}.remote-{}", rel, stamp);
                        conflicts.push(ConflictDescriptor {
                            rel: rel.clone(),
                            local_file: lf.clone(),
                            local_rename,
                            remote_rename,
                        });
                        if lf.modified_ms >= re.modified_ms {
                            upload.push(lf.clone());
                            new_index.files.insert(
                                rel.clone(),
                                RemoteIndexEntry {
                                    sha256: lf.sha256.clone(),
                                    size: lf.size,
                                    modified_ms: lf.modified_ms,
                                    root_index: lf.root_index,
                                },
                            );
                        } else {
                            download.push((rel.clone(), re.clone()));
                            new_index.files.insert(rel.clone(), re.clone());
                        }
                    }
                }
            }
            (Some(lf), None, None) => {
                upload.push(lf.clone());
                new_index.files.insert(
                    rel.clone(),
                    RemoteIndexEntry {
                        sha256: lf.sha256.clone(),
                        size: lf.size,
                        modified_ms: lf.modified_ms,
                        root_index: lf.root_index,
                    },
                );
            }
            (Some(lf), None, Some(_)) => {
                delete_local.push((rel.clone(), lf.absolute.clone()));
            }
            (None, Some(re), None) => {
                download.push((rel.clone(), re.clone()));
                new_index.files.insert(rel.clone(), re.clone());
            }
            (None, Some(_re), Some(_)) => {
                delete_remote.push(rel.clone());
            }
            (None, None, _) => unreachable!(),
        }
    }

    SyncPlan {
        upload,
        download,
        delete_remote,
        delete_local,
        conflicts,
        new_index,
        existing_remote: remote_index.files.keys().cloned().collect(),
        roots: roots.to_vec(),
    }
}

enum ConflictResolution {
    PickLocal,
    PickRemote,
    RenameBoth,
}

fn decide_conflict(
    local: &LocalFile,
    remote: &RemoteIndexEntry,
    local_changed: bool,
    remote_changed: bool,
    policy: ConflictPolicy,
    overrides: &ConflictOverrides,
    rel: &str,
) -> ConflictResolution {
    // User-supplied override wins over policy. Keys: "local" / "remote" / "rename".
    if let Some(choice) = overrides.get(rel) {
        match choice.as_str() {
            "local" => return ConflictResolution::PickLocal,
            "remote" => return ConflictResolution::PickRemote,
            "rename" => return ConflictResolution::RenameBoth,
            _ => {} // unknown choice, fall through to policy
        }
    }
    if local_changed && !remote_changed {
        return ConflictResolution::PickLocal;
    }
    if !local_changed && remote_changed {
        return ConflictResolution::PickRemote;
    }
    match policy {
        ConflictPolicy::RenameBoth => ConflictResolution::RenameBoth,
        ConflictPolicy::LocalWins => ConflictResolution::PickLocal,
        ConflictPolicy::RemoteWins => ConflictResolution::PickRemote,
        ConflictPolicy::NewerWins | ConflictPolicy::Ask => {
            if local.modified_ms >= remote.modified_ms {
                ConflictResolution::PickLocal
            } else {
                ConflictResolution::PickRemote
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

async fn execute_plan(
    state: &Arc<AppState>,
    game: &GameProfile,
    backend: Arc<dyn StorageBackend>,
    remote_prefix: &str,
    plan: SyncPlan,
    settings: &AppSettings,
) -> AppResult<()> {
    // 1) Conflict rescue copies first — they read the originals before they
    //    get overwritten by step 2.
    if !plan.conflicts.is_empty() {
        state.log.warn(
            "sync",
            format!(
                "[{}] {} conflict(s) detected, preserving both copies",
                game.name,
                plan.conflicts.len()
            ),
        );
        for c in &plan.conflicts {
            // server-side copy of original → .remote-<ts>
            let src_key = paths::to_remote_key(remote_prefix, &c.rel);
            let dst_key = paths::to_remote_key(remote_prefix, &c.remote_rename);
            if let Err(e) = backend.copy(&src_key, &dst_key).await {
                state
                    .log
                    .warn("sync", format!("conflict copy remote {}: {}", c.rel, e));
            } else {
                state.log.info(
                    "sync",
                    format!("[{}] kept remote as {}", game.name, c.remote_rename),
                );
            }
            // upload local-side rename
            let p = c.local_file.absolute.clone();
            let bytes = match tokio::task::spawn_blocking(move || std::fs::read(&p)).await {
                Ok(Ok(b)) => b,
                Ok(Err(e)) => {
                    state
                        .log
                        .warn("sync", format!("conflict read local {}: {}", c.rel, e));
                    continue;
                }
                Err(e) => {
                    state.log.warn(
                        "sync",
                        format!("conflict read local {} (join): {}", c.rel, e),
                    );
                    continue;
                }
            };
            let key = paths::to_remote_key(remote_prefix, &c.local_rename);
            if let Err(e) = backend.put(&key, bytes).await {
                state.log.warn(
                    "sync",
                    format!("conflict push local-rename {}: {}", c.rel, e),
                );
            } else {
                state.log.info(
                    "sync",
                    format!("[{}] saved local as {}", game.name, c.local_rename),
                );
            }
        }
    }

    // 2) Uploads.
    if !plan.upload.is_empty() {
        state
            .set_state(
                &game.id,
                SyncStateKind::Uploading,
                Some(&format!("上传 {} 个文件", plan.upload.len())),
            )
            .await;
        let upload_refs: Vec<&LocalFile> = plan.upload.iter().collect();
        parallel_upload(
            state,
            game,
            backend.clone(),
            remote_prefix,
            &upload_refs,
            settings,
            &plan.existing_remote,
        )
        .await?;
    }

    // 3) Downloads.
    if !plan.download.is_empty() {
        state
            .set_state(
                &game.id,
                SyncStateKind::Downloading,
                Some(&format!("下载 {} 个文件", plan.download.len())),
            )
            .await;
        parallel_download(
            state,
            game,
            backend.clone(),
            remote_prefix,
            &plan.roots,
            &plan.download,
            settings,
        )
        .await?;
    }

    // 4) Remote deletions (with version archiving).
    if !plan.delete_remote.is_empty() {
        let backend2 = backend.clone();
        let prefix2 = remote_prefix.to_string();
        let settings_clone = settings.clone();
        let game_name = game.name.clone();
        let log = state.log.clone();
        stream::iter(plan.delete_remote.iter().cloned())
            .for_each_concurrent(Some(settings.max_concurrency.max(1)), |rel| {
                let backend = backend2.clone();
                let prefix = prefix2.clone();
                let settings = settings_clone.clone();
                let game_name = game_name.clone();
                let log = log.clone();
                async move {
                    let key = paths::to_remote_key(&prefix, &rel);
                    if settings.versions_to_keep > 0 {
                        if let Err(e) = archive_version(backend.as_ref(), &prefix, &rel, &key).await
                        {
                            log.debug("sync", format!("archive {rel}: {e}"));
                        }
                    }
                    if let Err(e) = backend.delete(&key).await {
                        log.warn("sync", format!("delete remote {rel}: {e}"));
                    } else {
                        log.info("sync", format!("[{}] del remote {}", game_name, rel));
                    }
                }
            })
            .await;
    }

    // 5) Local deletions.
    for (rel, abs) in &plan.delete_local {
        let p = abs.clone();
        if let Err(e) = tokio::task::spawn_blocking(move || std::fs::remove_file(&p)).await {
            state.log.warn("sync", format!("delete local {rel}: {e}"));
        } else {
            state
                .log
                .info("sync", format!("[{}] del local {}", game.name, rel));
        }
    }

    // 6) Persist new index + prune old versions + save prior snapshot.
    save_remote_index(backend.as_ref(), remote_prefix, &plan.new_index).await?;
    if settings.versions_to_keep > 0 {
        prune_versions(
            state,
            backend.as_ref(),
            remote_prefix,
            settings.versions_to_keep,
        )
        .await;
    }
    if let Err(e) = save_prior_index(&game.id, &plan.new_index).await {
        state.log.warn("sync", format!("save prior index: {e}"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Version listing & restore (powers the UI panel)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct VersionInfo {
    pub rel: String,
    pub key: String,
    #[serde(rename = "timestampMs")]
    pub timestamp_ms: i64,
    pub size: u64,
}

pub async fn list_versions(state: Arc<AppState>, game_id: &str) -> AppResult<Vec<VersionInfo>> {
    let game = state.get_game(game_id).await?;
    let backend = backend_for_game(&state, &game).await?;
    let remote_prefix = remote_prefix_for(&game);
    let versions_prefix = paths::to_remote_key(&remote_prefix, VERSIONS_PREFIX);
    let entries = backend.list(&versions_prefix).await?;
    let mut out = Vec::new();
    for m in entries {
        let after = match m.key.strip_prefix(&format!("{versions_prefix}/")) {
            Some(s) => s,
            None => continue,
        };
        let (rel, ts_part) = match after.rsplit_once('.') {
            Some(v) => v,
            None => continue,
        };
        let ts: i64 = match ts_part.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        out.push(VersionInfo {
            rel: rel.to_string(),
            key: m.key,
            timestamp_ms: ts,
            size: m.size,
        });
    }
    // newest first
    out.sort_by(|a, b| b.timestamp_ms.cmp(&a.timestamp_ms));
    Ok(out)
}

pub async fn delete_version(
    state: Arc<AppState>,
    game_id: &str,
    version_key: &str,
) -> AppResult<()> {
    let game = state.get_game(game_id).await?;
    let backend = backend_for_game(&state, &game).await?;
    let remote_prefix = remote_prefix_for(&game);
    let versions_prefix = paths::to_remote_key(&remote_prefix, VERSIONS_PREFIX);
    if !version_key.starts_with(&format!("{versions_prefix}/")) {
        return Err(AppError::Other(format!(
            "key '{version_key}' is not under this game's versions prefix"
        )));
    }
    backend.delete(version_key).await?;
    state.log.info(
        "version",
        format!("[{}] deleted single version {}", game.name, version_key),
    );
    Ok(())
}

pub async fn export_version(
    state: Arc<AppState>,
    game_id: &str,
    version_key: &str,
    local_path: &str,
) -> AppResult<()> {
    let game = state.get_game(game_id).await?;
    let backend = backend_for_game(&state, &game).await?;
    let remote_prefix = remote_prefix_for(&game);
    let versions_prefix = paths::to_remote_key(&remote_prefix, VERSIONS_PREFIX);
    if !version_key.starts_with(&format!("{versions_prefix}/")) {
        return Err(AppError::Other(format!(
            "key '{version_key}' is not under this game's versions prefix"
        )));
    }
    let abs = std::path::PathBuf::from(local_path);
    backend.get_to_path(version_key, &abs).await?;
    state.log.info(
        "version",
        format!("[{}] exported {} → {}", game.name, version_key, local_path),
    );
    Ok(())
}

pub async fn restore_version(
    state: Arc<AppState>,
    game_id: &str,
    version_key: &str,
) -> AppResult<()> {
    let game = state.get_game(game_id).await?;
    let lock = state.lock_for_game(&game.id).await;
    let _guard = lock.lock().await;

    let backend = backend_for_game(&state, &game).await?;
    let remote_prefix = remote_prefix_for(&game);
    let versions_prefix = paths::to_remote_key(&remote_prefix, VERSIONS_PREFIX);
    let after = version_key
        .strip_prefix(&format!("{versions_prefix}/"))
        .ok_or_else(|| AppError::Other(format!("version key not under prefix: {version_key}")))?;
    let (rel, _ts_part) = after
        .rsplit_once('.')
        .ok_or_else(|| AppError::Other(format!("malformed version key: {version_key}")))?;
    let rel = rel.to_string();

    // Resolve target local path from prior_index (since current scan may not
    // see the file if it was deleted).
    let mut prior = load_prior_index(&game.id).await.unwrap_or_default();
    let root_index = prior.files.get(&rel).map(|e| e.root_index).unwrap_or(0);
    let roots: Vec<PathBuf> = game
        .save_paths
        .iter()
        .map(|s| paths::expand(s).unwrap_or_default())
        .collect();
    let abs = resolve_local_path(&roots, root_index, &rel)?;

    let bytes = backend.get(version_key).await?;
    let size = bytes.len() as u64;
    let sha256 = hex::encode(Sha256::digest(&bytes));

    // Archive the current live file (if exists) so this restore is itself
    // versioned — symmetric with normal sync.
    let live_key = paths::to_remote_key(&remote_prefix, &rel);
    if state.get_settings().await.versions_to_keep > 0 {
        if let Err(e) = archive_version(backend.as_ref(), &remote_prefix, &rel, &live_key).await {
            state
                .log
                .debug("restore", format!("archive pre-restore {rel}: {e}"));
        }
    }

    // Push the restored content under the live key + write locally atomically.
    backend.put(&live_key, bytes.clone()).await?;
    write_file_atomic(&abs, bytes).await?;

    // CRITICAL: update both prior and remote index so the next two-way sync
    // sees local == remote == prior, instead of detecting "both sides changed"
    // and triggering rename-both conflict on the freshly-restored file.
    let modified_ms = std::fs::metadata(&abs)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or_else(|| Utc::now().timestamp_millis());

    let new_entry = RemoteIndexEntry {
        sha256: sha256.clone(),
        size,
        modified_ms,
        root_index,
    };

    prior.files.insert(rel.clone(), new_entry.clone());
    if let Err(e) = save_prior_index(&game.id, &prior).await {
        state
            .log
            .warn("restore", format!("save prior after restore: {e}"));
    }

    let mut remote_idx = load_remote_index(backend.as_ref(), &remote_prefix)
        .await
        .unwrap_or_default();
    remote_idx.files.insert(rel.clone(), new_entry);
    if let Err(e) = save_remote_index(backend.as_ref(), &remote_prefix, &remote_idx).await {
        state
            .log
            .warn("restore", format!("save remote index after restore: {e}"));
    }

    state.log.info(
        "restore",
        format!("[{}] restored {} from {}", game.name, rel, version_key),
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Concurrent IO helpers
// ---------------------------------------------------------------------------

async fn parallel_upload(
    state: &Arc<AppState>,
    game: &GameProfile,
    backend: Arc<dyn StorageBackend>,
    remote_prefix: &str,
    files: &[&LocalFile],
    settings: &AppSettings,
    existing_remote_keys: &BTreeSet<String>,
) -> AppResult<()> {
    if files.is_empty() {
        return Ok(());
    }
    let concurrency = settings.max_concurrency.max(1);
    let owned: Vec<LocalFile> = files.iter().map(|f| (*f).clone()).collect();
    let game_id = game.id.clone();
    let game_name = game.name.clone();
    let log = state.log.clone();
    let prefix = remote_prefix.to_string();
    let settings_clone = settings.clone();
    let existing = existing_remote_keys.clone();
    let t0 = std::time::Instant::now();
    let started_at = chrono::Utc::now().timestamp_millis();
    let n = owned.len();
    let total_bytes: u64 = owned.iter().map(|f| f.size).sum();
    let done_count = Arc::new(AtomicUsize::new(0));
    let done_bytes = Arc::new(AtomicU64::new(0));
    log.info(
        "sync",
        format!(
            "[{}] uploading {} files ({} B, concurrency={})",
            game_name, n, total_bytes, concurrency
        ),
    );
    state.emit_progress(&game_id, "upload", 0, n, None, 0, total_bytes, started_at);

    let cancel = state.cancel_token_for(&game_id).await;
    let state_for_emit = state.clone();
    let results: Vec<AppResult<()>> = stream::iter(owned)
        .map(|f| {
            let backend = backend.clone();
            let prefix = prefix.clone();
            let game_id = game_id.clone();
            let game_name = game_name.clone();
            let log = log.clone();
            let settings = settings_clone.clone();
            let existing = existing.clone();
            let done_count = done_count.clone();
            let done_bytes = done_bytes.clone();
            let state_emit = state_for_emit.clone();
            let cancel = cancel.clone();
            async move {
                if cancel.is_cancelled() {
                    return Err(AppError::Other("sync cancelled".into()));
                }
                state_emit.emit_progress(
                    &game_id,
                    "upload",
                    done_count.load(Ordering::Relaxed),
                    n,
                    Some(&f.relative),
                    done_bytes.load(Ordering::Relaxed),
                    total_bytes,
                    started_at,
                );
                let key = paths::to_remote_key(&prefix, &f.relative);
                let size = f.size;
                let large = size > LARGE_FILE_THRESHOLD;
                if settings.versions_to_keep > 0 && existing.contains(&f.relative) {
                    if let Err(e) =
                        archive_version(backend.as_ref(), &prefix, &f.relative, &key).await
                    {
                        log.debug(
                            "sync",
                            format!("[{}] archive {}: {e}", game_name, f.relative),
                        );
                    }
                }
                if large {
                    backend.put_path(&key, &f.absolute).await?;
                } else {
                    let p = f.absolute.clone();
                    let bytes = tokio::task::spawn_blocking(move || std::fs::read(&p))
                        .await
                        .map_err(|e| AppError::other(format!("read join: {e}")))??;
                    backend.put(&key, bytes).await?;
                }
                log.debug(
                    "sync",
                    format!("[{}] up {} ({} B)", game_name, f.relative, size),
                );
                let new_count = done_count.fetch_add(1, Ordering::Relaxed) + 1;
                let new_bytes = done_bytes.fetch_add(size, Ordering::Relaxed) + size;
                state_emit.emit_progress(
                    &game_id,
                    "upload",
                    new_count,
                    n,
                    Some(&f.relative),
                    new_bytes,
                    total_bytes,
                    started_at,
                );
                Ok(())
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;
    for r in results {
        r?;
    }
    log.info(
        "sync",
        format!(
            "[{}] uploaded {} files in {:.1}s",
            game.name,
            n,
            t0.elapsed().as_secs_f64()
        ),
    );
    Ok(())
}

async fn parallel_download(
    state: &Arc<AppState>,
    game: &GameProfile,
    backend: Arc<dyn StorageBackend>,
    remote_prefix: &str,
    roots: &[PathBuf],
    items: &[(String, RemoteIndexEntry)],
    settings: &AppSettings,
) -> AppResult<()> {
    if items.is_empty() {
        return Ok(());
    }
    let concurrency = settings.max_concurrency.max(1);
    let owned: Vec<(String, RemoteIndexEntry)> = items.to_vec();
    let game_id = game.id.clone();
    let game_name = game.name.clone();
    let log = state.log.clone();
    let prefix = remote_prefix.to_string();
    let roots = roots.to_vec();
    let n = owned.len();
    let total_bytes: u64 = owned.iter().map(|(_, e)| e.size).sum();
    let done_count = Arc::new(AtomicUsize::new(0));
    let done_bytes = Arc::new(AtomicU64::new(0));
    let t0 = std::time::Instant::now();
    let started_at = chrono::Utc::now().timestamp_millis();
    log.info(
        "sync",
        format!(
            "[{}] downloading {} files ({} B, concurrency={})",
            game_name, n, total_bytes, concurrency
        ),
    );
    state.emit_progress(&game_id, "download", 0, n, None, 0, total_bytes, started_at);
    let cancel = state.cancel_token_for(&game_id).await;
    let state_for_emit = state.clone();
    let results: Vec<AppResult<()>> = stream::iter(owned)
        .map(|(rel, entry)| {
            let backend = backend.clone();
            let prefix = prefix.clone();
            let game_id = game_id.clone();
            let game_name = game_name.clone();
            let log = log.clone();
            let roots = roots.clone();
            let done_count = done_count.clone();
            let done_bytes = done_bytes.clone();
            let state_emit = state_for_emit.clone();
            let cancel = cancel.clone();
            async move {
                if cancel.is_cancelled() {
                    return Err(AppError::Other("sync cancelled".into()));
                }
                state_emit.emit_progress(
                    &game_id,
                    "download",
                    done_count.load(Ordering::Relaxed),
                    n,
                    Some(&rel),
                    done_bytes.load(Ordering::Relaxed),
                    total_bytes,
                    started_at,
                );
                let key = paths::to_remote_key(&prefix, &rel);
                let abs = resolve_local_path(&roots, entry.root_index, &rel)?;
                let large = entry.size > LARGE_FILE_THRESHOLD;
                if large {
                    backend.get_to_path(&key, &abs).await?;
                } else {
                    let bytes = backend.get(&key).await?;
                    write_file_atomic(&abs, bytes).await?;
                }
                log.debug("sync", format!("[{}] dl {}", game_name, rel));
                let new_count = done_count.fetch_add(1, Ordering::Relaxed) + 1;
                let new_bytes = done_bytes.fetch_add(entry.size, Ordering::Relaxed) + entry.size;
                state_emit.emit_progress(
                    &game_id,
                    "download",
                    new_count,
                    n,
                    Some(&rel),
                    new_bytes,
                    total_bytes,
                    started_at,
                );
                Ok(())
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;
    for r in results {
        r?;
    }
    log.info(
        "sync",
        format!(
            "[{}] downloaded {} files in {:.1}s",
            game.name,
            n,
            t0.elapsed().as_secs_f64()
        ),
    );
    Ok(())
}

/// Files larger than this bypass read-into-Vec and use put_path/get_to_path
/// streaming, capping per-task memory at ~16 MiB.
const LARGE_FILE_THRESHOLD: u64 = 64 * 1024 * 1024;

async fn archive_version(
    backend: &dyn StorageBackend,
    remote_prefix: &str,
    rel: &str,
    src_key: &str,
) -> AppResult<()> {
    let stamp = Utc::now().timestamp_millis();
    let dst_rel = format!("{VERSIONS_PREFIX}/{rel}.{stamp}");
    let dst_key = paths::to_remote_key(remote_prefix, &dst_rel);
    backend.copy(src_key, &dst_key).await?;
    Ok(())
}

async fn prune_versions(
    state: &AppState,
    backend: &dyn StorageBackend,
    remote_prefix: &str,
    keep: usize,
) {
    let versions_prefix = paths::to_remote_key(remote_prefix, VERSIONS_PREFIX);
    let entries = match backend.list(&versions_prefix).await {
        Ok(v) => v,
        Err(e) => {
            state.log.debug("sync", format!("prune list: {e}"));
            return;
        }
    };
    let mut by_rel: BTreeMap<String, Vec<(i64, String)>> = BTreeMap::new();
    let mut skipped = 0usize;
    for m in entries {
        let key = &m.key;
        let after = match key.strip_prefix(&format!("{versions_prefix}/")) {
            Some(s) => s,
            None => continue,
        };
        let (rel, ts_part) = match after.rsplit_once('.') {
            Some((r, t)) => (r, t),
            None => {
                skipped += 1;
                continue;
            }
        };
        let ts: i64 = match ts_part.parse() {
            Ok(v) => v,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        by_rel
            .entry(rel.to_string())
            .or_default()
            .push((ts, key.clone()));
    }
    if skipped > 0 {
        state.log.debug(
            "sync",
            format!("prune: skipped {skipped} files not matching <rel>.<ms> pattern"),
        );
    }
    let mut total_pruned = 0usize;
    for (_rel, mut versions) in by_rel {
        if versions.len() <= keep {
            continue;
        }
        versions.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, key) in versions.into_iter().skip(keep) {
            if let Err(e) = backend.delete(&key).await {
                state.log.debug("sync", format!("prune delete {key}: {e}"));
            } else {
                total_pruned += 1;
            }
        }
    }
    if total_pruned > 0 {
        state
            .log
            .info("sync", format!("pruned {total_pruned} old versions"));
    }
}

// ---------------------------------------------------------------------------
// Misc helpers
// ---------------------------------------------------------------------------

async fn write_file_atomic(path: &std::path::Path, bytes: Vec<u8>) -> AppResult<()> {
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
    .map_err(|e| AppError::other(format!("write join: {e}")))??;
    Ok(())
}

fn resolve_local_path(roots: &[PathBuf], root_idx: usize, rel: &str) -> AppResult<PathBuf> {
    let root = roots.get(root_idx).ok_or_else(|| {
        AppError::Path(format!(
            "save path index {root_idx} out of range (only {} roots)",
            roots.len()
        ))
    })?;
    let mut p = root.clone();
    for seg in rel.split('/') {
        p.push(seg);
    }
    Ok(p)
}

async fn load_remote_index(
    backend: &dyn StorageBackend,
    remote_prefix: &str,
) -> AppResult<RemoteIndex> {
    let key = paths::to_remote_key(remote_prefix, META_KEY);
    match backend.get(&key).await {
        Ok(bytes) => {
            let idx: RemoteIndex = serde_json::from_slice(&bytes).unwrap_or_default();
            Ok(idx)
        }
        Err(_) => Ok(RemoteIndex::default()),
    }
}

async fn save_remote_index(
    backend: &dyn StorageBackend,
    remote_prefix: &str,
    idx: &RemoteIndex,
) -> AppResult<()> {
    let key = paths::to_remote_key(remote_prefix, META_KEY);
    let buf = serde_json::to_vec_pretty(idx)?;
    backend.put(&key, buf).await?;
    Ok(())
}

fn prior_index_path(game_id: &str) -> AppResult<PathBuf> {
    Ok(paths::data_dir()?
        .join("snapshots")
        .join(format!("{game_id}.json")))
}

async fn load_prior_index(game_id: &str) -> AppResult<RemoteIndex> {
    let path = prior_index_path(game_id)?;
    let bytes = match tokio::task::spawn_blocking(move || std::fs::read(&path))
        .await
        .map_err(|e| AppError::other(format!("read join: {e}")))?
    {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(RemoteIndex::default()),
        Err(e) => return Err(AppError::Io(e)),
    };
    Ok(serde_json::from_slice(&bytes).unwrap_or_default())
}

async fn save_prior_index(game_id: &str, idx: &RemoteIndex) -> AppResult<()> {
    let path = prior_index_path(game_id)?;
    let buf = serde_json::to_vec_pretty(idx)?;
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(&path, &buf)?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::other(format!("write join: {e}")))??;
    Ok(())
}

fn conflict_suffix() -> String {
    let now = Utc::now();
    format!(
        "{}-{:03}",
        now.format("%Y%m%d-%H%M%S"),
        now.timestamp_subsec_millis()
    )
}

fn build_index(files: &[LocalFile]) -> RemoteIndex {
    let mut idx = RemoteIndex::default();
    for f in files {
        idx.files.insert(
            f.relative.clone(),
            RemoteIndexEntry {
                sha256: f.sha256.clone(),
                size: f.size,
                modified_ms: f.modified_ms,
                root_index: f.root_index,
            },
        );
    }
    idx
}

fn sanitize_id(id: &str, name: &str) -> String {
    let mut h = Sha256::new();
    h.update(id.as_bytes());
    h.update(name.as_bytes());
    let short = hex::encode(&h.finalize()[..4]);
    let clean: String = name
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .filter(|c| !c.is_control())
        .collect();
    if clean.trim().is_empty() {
        short
    } else {
        format!("{}-{}", clean.trim(), short)
    }
}

fn remote_prefix_for(game: &GameProfile) -> String {
    game.remote_prefix
        .clone()
        .unwrap_or_else(|| format!("games/{}", sanitize_id(&game.id, &game.name)))
}

async fn scan_count(game: &GameProfile) -> AppResult<usize> {
    let game = game.clone();
    Ok(tokio::task::spawn_blocking(move || scanner::scan(&game))
        .await
        .map_err(|e| AppError::other(format!("scan join: {e}")))??
        .files
        .len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::scanner::LocalFile;

    fn lf(rel: &str, sha: &str, size: u64, mtime: i64) -> LocalFile {
        LocalFile {
            absolute: PathBuf::from(format!("/tmp/{rel}")),
            relative: rel.into(),
            root_index: 0,
            size,
            modified_ms: mtime,
            sha256: sha.into(),
        }
    }
    fn re(sha: &str, size: u64, mtime: i64) -> RemoteIndexEntry {
        RemoteIndexEntry {
            sha256: sha.into(),
            size,
            modified_ms: mtime,
            root_index: 0,
        }
    }

    fn no_ov() -> ConflictOverrides {
        ConflictOverrides::new()
    }

    #[test]
    fn decide_local_only_changed_picks_local() {
        let l = lf("a", "L", 1, 200);
        let r = re("R", 1, 100);
        match decide_conflict(&l, &r, true, false, ConflictPolicy::RenameBoth, &no_ov(), "a") {
            ConflictResolution::PickLocal => {}
            _ => panic!("expected PickLocal when only local changed"),
        }
    }

    #[test]
    fn decide_remote_only_changed_picks_remote() {
        let l = lf("a", "L", 1, 100);
        let r = re("R", 1, 200);
        match decide_conflict(&l, &r, false, true, ConflictPolicy::RenameBoth, &no_ov(), "a") {
            ConflictResolution::PickRemote => {}
            _ => panic!("expected PickRemote when only remote changed"),
        }
    }

    #[test]
    fn decide_both_changed_rename_both() {
        let l = lf("a", "L", 1, 200);
        let r = re("R", 1, 100);
        match decide_conflict(&l, &r, true, true, ConflictPolicy::RenameBoth, &no_ov(), "a") {
            ConflictResolution::RenameBoth => {}
            _ => panic!("expected RenameBoth under that policy"),
        }
    }

    #[test]
    fn decide_both_changed_newer_wins_local() {
        let l = lf("a", "L", 1, 300);
        let r = re("R", 1, 200);
        match decide_conflict(&l, &r, true, true, ConflictPolicy::NewerWins, &no_ov(), "a") {
            ConflictResolution::PickLocal => {}
            _ => panic!("local has newer mtime under newer-wins"),
        }
    }

    #[test]
    fn decide_both_changed_newer_wins_remote() {
        let l = lf("a", "L", 1, 100);
        let r = re("R", 1, 200);
        match decide_conflict(&l, &r, true, true, ConflictPolicy::NewerWins, &no_ov(), "a") {
            ConflictResolution::PickRemote => {}
            _ => panic!("remote has newer mtime under newer-wins"),
        }
    }

    #[test]
    fn decide_override_beats_policy() {
        let l = lf("a", "L", 1, 100);
        let r = re("R", 1, 999);
        let mut ov = no_ov();
        ov.insert("a".into(), "local".into());
        // Policy=RemoteWins would normally pick remote; the per-file
        // override forces "local" anyway.
        match decide_conflict(&l, &r, true, true, ConflictPolicy::RemoteWins, &ov, "a") {
            ConflictResolution::PickLocal => {}
            _ => panic!("override should beat policy"),
        }
        // Override for a different rel is ignored.
        match decide_conflict(&l, &r, true, true, ConflictPolicy::RemoteWins, &ov, "b") {
            ConflictResolution::PickRemote => {}
            _ => panic!("policy should win when override key doesn't match"),
        }
    }

    #[test]
    fn decide_local_wins_policy_ignores_changes() {
        let l = lf("a", "L", 1, 100);
        let r = re("R", 1, 999);
        match decide_conflict(&l, &r, true, true, ConflictPolicy::LocalWins, &no_ov(), "a") {
            ConflictResolution::PickLocal => {}
            _ => panic!("policy=LocalWins should always pick local on tie"),
        }
    }

    #[test]
    fn plan_push_new_files_get_uploaded() {
        let prev = RemoteIndex::default();
        let existing = BTreeSet::new();
        let local = vec![lf("a/b.dat", "h1", 10, 1)];
        let plan = plan_push(&local, &prev, &existing, &[PathBuf::from("/r")]);
        assert_eq!(plan.upload.len(), 1);
        assert!(plan.delete_remote.is_empty());
    }

    #[test]
    fn plan_push_unchanged_files_skipped() {
        let mut prev = RemoteIndex::default();
        prev.files.insert("a/b.dat".into(), re("h1", 10, 1));
        let existing: BTreeSet<String> = prev.files.keys().cloned().collect();
        let local = vec![lf("a/b.dat", "h1", 10, 1)];
        let plan = plan_push(&local, &prev, &existing, &[PathBuf::from("/r")]);
        assert!(plan.upload.is_empty());
    }

    #[test]
    fn plan_push_missing_local_marks_for_remote_deletion() {
        let mut prev = RemoteIndex::default();
        prev.files.insert("gone.dat".into(), re("hX", 1, 0));
        let existing = prev.files.keys().cloned().collect();
        let local: Vec<LocalFile> = vec![];
        let plan = plan_push(&local, &prev, &existing, &[PathBuf::from("/r")]);
        assert_eq!(plan.delete_remote, vec!["gone.dat".to_string()]);
    }

    #[test]
    fn plan_pull_empty_remote_is_noop() {
        let local = vec![lf("a", "h", 1, 1)];
        let plan = plan_pull(
            &local,
            &RemoteIndex::default(),
            &RemoteIndex::default(),
            &[PathBuf::from("/r")],
        );
        assert!(plan.download.is_empty());
        assert!(plan.delete_local.is_empty());
    }
}
