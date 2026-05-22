use crate::error::{AppError, AppResult};
use crate::logbus::LogEntry;
use crate::model::{AppSettings, BackendConfig, GameProfile, GameSyncStatus, SyncDirection};
use crate::state::AppState;
use crate::storage;
use crate::sync::engine::{SyncPreview, VersionInfo};
use crate::sync::snapshot::SnapshotSummary;
use crate::sync::{engine, scheduler, snapshot};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn bootstrap(_state: State<'_, Arc<AppState>>) -> AppResult<()> {
    Ok(())
}

#[tauri::command]
pub async fn list_games(state: State<'_, Arc<AppState>>) -> AppResult<Vec<GameProfile>> {
    Ok(state.list_games().await)
}

#[tauri::command]
pub async fn save_game(
    state: State<'_, Arc<AppState>>,
    game: GameProfile,
) -> AppResult<GameProfile> {
    state.save_game(game).await
}

#[tauri::command]
pub async fn delete_game(state: State<'_, Arc<AppState>>, id: String) -> AppResult<()> {
    state.delete_game(&id).await
}

#[tauri::command]
pub async fn list_backends(state: State<'_, Arc<AppState>>) -> AppResult<Vec<BackendConfig>> {
    Ok(state.list_backends().await)
}

#[tauri::command]
pub async fn save_backend(
    state: State<'_, Arc<AppState>>,
    backend: BackendConfig,
) -> AppResult<Vec<BackendConfig>> {
    state.save_backend(backend).await
}

#[tauri::command]
pub async fn delete_backend(
    state: State<'_, Arc<AppState>>,
    name: String,
) -> AppResult<Vec<BackendConfig>> {
    state.delete_backend(&name).await
}

#[tauri::command]
pub async fn test_backend(backend: BackendConfig) -> AppResult<String> {
    // Connectivity tests bypass the global limiter — a "test" button waiting
    // for tokens to refill would feel broken.
    let b = storage::build(&backend, storage::RateLimiter::new(0)).await?;
    b.ping().await
}

#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<AppState>>) -> AppResult<AppSettings> {
    Ok(state.get_settings().await)
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, Arc<AppState>>,
    settings: AppSettings,
) -> AppResult<AppSettings> {
    state.save_settings(settings).await
}

#[tauri::command]
pub async fn list_status(state: State<'_, Arc<AppState>>) -> AppResult<Vec<GameSyncStatus>> {
    Ok(state.list_status().await)
}

#[tauri::command]
pub async fn sync_one(
    state: State<'_, Arc<AppState>>,
    game_id: String,
    direction: SyncDirection,
) -> AppResult<()> {
    // Pre-flight: ensure game exists and a backend is configured so the UI
    // gets immediate feedback instead of silent failure inside the spawn.
    state.get_game(&game_id).await?;
    if state.list_backends().await.is_empty() {
        return Err(AppError::Config("尚未配置任何云存储后端".into()));
    }
    let st = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        let _ = engine::run_one(st, &game_id, direction).await;
    });
    Ok(())
}

#[tauri::command]
pub async fn sync_all(state: State<'_, Arc<AppState>>) -> AppResult<()> {
    if state.list_backends().await.is_empty() {
        return Err(AppError::Config("尚未配置任何云存储后端".into()));
    }
    let st = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        let _ = scheduler::sync_all_with_state(st).await;
    });
    Ok(())
}

#[tauri::command]
pub async fn read_log(state: State<'_, Arc<AppState>>, limit: usize) -> AppResult<Vec<LogEntry>> {
    Ok(state.log.snapshot(limit))
}

#[tauri::command]
pub async fn read_stats(
    state: State<'_, Arc<AppState>>,
) -> AppResult<Vec<crate::stats::StatEntry>> {
    state.stats.read_all().await
}

/// Persist the log ring buffer to disk on demand. Useful for users
/// reporting bugs — they can hit the button, open the data dir, and attach
/// the file.
#[tauri::command]
pub async fn flush_log(state: State<'_, Arc<AppState>>) -> AppResult<String> {
    state.log.persist_to_disk()?;
    Ok(crate::paths::log_file()?
        .to_string_lossy()
        .to_string())
}

/// Return the data-dir path so the UI can show it / open it in Explorer.
#[tauri::command]
pub async fn get_data_dir() -> AppResult<String> {
    Ok(crate::paths::data_dir()?
        .to_string_lossy()
        .to_string())
}

/// Frontend-facing summary of a game's local on-disk save footprint.
/// Used by the game card to show "5.2 MB · 12 files" at a glance.
#[derive(serde::Serialize)]
pub struct GameSizeInfo {
    #[serde(rename = "fileCount")]
    pub file_count: usize,
    #[serde(rename = "totalBytes")]
    pub total_bytes: u64,
}

#[tauri::command]
pub async fn compute_save_size(
    state: State<'_, Arc<AppState>>,
    game_id: String,
) -> AppResult<GameSizeInfo> {
    let game = state.get_game(&game_id).await?;
    let scan = tokio::task::spawn_blocking(move || crate::sync::scanner::scan(&game))
        .await
        .map_err(|e| AppError::other(format!("scan join: {e}")))??;
    let total_bytes = scan.files.iter().map(|f| f.size).sum();
    Ok(GameSizeInfo {
        file_count: scan.files.len(),
        total_bytes,
    })
}

/// Validate an in-progress GameProfile (from the editor, BEFORE save). Runs
/// the scanner against the provided paths/globs and reports what it finds.
/// Used by the "验证路径" button to give immediate feedback on typos.
#[tauri::command]
pub async fn validate_game_paths(
    game: GameProfile,
) -> AppResult<GameSizeInfo> {
    let scan = tokio::task::spawn_blocking(move || crate::sync::scanner::scan(&game))
        .await
        .map_err(|e| AppError::other(format!("scan join: {e}")))??;
    let total_bytes = scan.files.iter().map(|f| f.size).sum();
    Ok(GameSizeInfo {
        file_count: scan.files.len(),
        total_bytes,
    })
}

#[tauri::command]
pub async fn sync_preview(
    state: State<'_, Arc<AppState>>,
    game_id: String,
    direction: SyncDirection,
) -> AppResult<SyncPreview> {
    state.get_game(&game_id).await?;
    if state.list_backends().await.is_empty() {
        return Err(AppError::Config("尚未配置任何云存储后端".into()));
    }
    engine::preview(state.inner().clone(), &game_id, direction).await
}

#[tauri::command]
pub async fn list_versions(
    state: State<'_, Arc<AppState>>,
    game_id: String,
) -> AppResult<Vec<VersionInfo>> {
    state.get_game(&game_id).await?;
    if state.list_backends().await.is_empty() {
        return Err(AppError::Config("尚未配置任何云存储后端".into()));
    }
    engine::list_versions(state.inner().clone(), &game_id).await
}

#[tauri::command]
pub async fn restore_version(
    state: State<'_, Arc<AppState>>,
    game_id: String,
    version_key: String,
) -> AppResult<()> {
    engine::restore_version(state.inner().clone(), &game_id, &version_key).await
}

#[tauri::command]
pub async fn delete_version(
    state: State<'_, Arc<AppState>>,
    game_id: String,
    version_key: String,
) -> AppResult<()> {
    engine::delete_version(state.inner().clone(), &game_id, &version_key).await
}

#[tauri::command]
pub async fn export_version(
    state: State<'_, Arc<AppState>>,
    game_id: String,
    version_key: String,
    local_path: String,
) -> AppResult<()> {
    engine::export_version(state.inner().clone(), &game_id, &version_key, &local_path).await
}

#[tauri::command]
pub async fn cancel_sync(state: State<'_, Arc<AppState>>, game_id: String) -> AppResult<bool> {
    Ok(state.cancel_sync(&game_id).await)
}

/// Fire-and-forget sync with explicit per-file conflict resolutions.
/// Used when ConflictPolicy::Ask shows the preview modal and the user
/// hand-picks "local" / "remote" / "rename" for each conflicted file.
#[tauri::command]
pub async fn sync_with_overrides(
    state: State<'_, Arc<AppState>>,
    game_id: String,
    direction: SyncDirection,
    overrides: std::collections::HashMap<String, String>,
) -> AppResult<()> {
    state.get_game(&game_id).await?;
    if state.list_backends().await.is_empty() {
        return Err(AppError::Config("尚未配置任何云存储后端".into()));
    }
    let st = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        let _ = engine::run_one_with_overrides(st, &game_id, direction, overrides).await;
    });
    Ok(())
}

#[tauri::command]
pub async fn create_snapshot(
    state: State<'_, Arc<AppState>>,
    game_id: String,
    name: String,
) -> AppResult<SnapshotSummary> {
    state.get_game(&game_id).await?;
    if state.list_backends().await.is_empty() {
        return Err(AppError::Config("尚未配置任何云存储后端".into()));
    }
    snapshot::create(state.inner().clone(), &game_id, &name).await
}

#[tauri::command]
pub async fn list_snapshots(
    state: State<'_, Arc<AppState>>,
    game_id: String,
) -> AppResult<Vec<SnapshotSummary>> {
    state.get_game(&game_id).await?;
    if state.list_backends().await.is_empty() {
        return Err(AppError::Config("尚未配置任何云存储后端".into()));
    }
    snapshot::list(state.inner().clone(), &game_id).await
}

#[tauri::command]
pub async fn restore_snapshot(
    state: State<'_, Arc<AppState>>,
    game_id: String,
    snapshot_id: String,
) -> AppResult<()> {
    snapshot::restore(state.inner().clone(), &game_id, &snapshot_id).await
}

#[tauri::command]
pub async fn delete_snapshot(
    state: State<'_, Arc<AppState>>,
    game_id: String,
    snapshot_id: String,
) -> AppResult<()> {
    snapshot::delete(state.inner().clone(), &game_id, &snapshot_id).await
}

#[tauri::command]
pub async fn export_config(
    state: State<'_, Arc<AppState>>,
    path: String,
    include_secrets: bool,
) -> AppResult<String> {
    let mut doc = state.snapshot().await;
    if !include_secrets {
        // Strip credentials before writing — exports get copied to less-trusted
        // places (USB sticks, chat threads) and we don't want secrets to leak
        // by accident. User explicitly opts in via the include_secrets flag.
        for b in doc.backends.iter_mut() {
            match b {
                crate::model::BackendConfig::S3 { s3, .. } => {
                    s3.access_key_id = String::new();
                    s3.secret_access_key = String::new();
                }
                crate::model::BackendConfig::Webdav { webdav, .. } => {
                    webdav.password = String::new();
                }
            }
        }
    }
    let buf = serde_json::to_vec_pretty(&doc)?;
    let p = std::path::PathBuf::from(&path);
    tokio::fs::write(&p, &buf).await.map_err(AppError::Io)?;
    Ok(p.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn import_config(
    state: State<'_, Arc<AppState>>,
    path: String,
    mode: String,
) -> AppResult<()> {
    let bytes = tokio::fs::read(&path).await.map_err(AppError::Io)?;
    let incoming: crate::model::ConfigDoc =
        serde_json::from_slice(&bytes).map_err(|e| AppError::Other(format!("解析失败: {e}")))?;
    match mode.as_str() {
        "replace" => state.replace_config(incoming).await?,
        "merge" => {
            let mut current = state.snapshot().await;
            // Games: incoming wins by id.
            let mut by_id: std::collections::HashMap<String, crate::model::GameProfile> = current
                .games
                .into_iter()
                .map(|g| (g.id.clone(), g))
                .collect();
            for g in incoming.games {
                by_id.insert(g.id.clone(), g);
            }
            current.games = by_id.into_values().collect();
            // Backends: incoming wins by name, BUT if the incoming entry was
            // exported without secrets (a "share-safe" export) and we already
            // have the same-named backend with real secrets locally, preserve
            // the local secrets — losing them silently to a merge would be
            // confusing.
            let mut by_name: std::collections::HashMap<String, crate::model::BackendConfig> =
                current
                    .backends
                    .into_iter()
                    .map(|b| (b.name().to_string(), b))
                    .collect();
            for incoming_b in incoming.backends {
                let key = incoming_b.name().to_string();
                let merged = match (by_name.get(&key), incoming_b) {
                    (
                        Some(crate::model::BackendConfig::S3 { s3: cur, .. }),
                        crate::model::BackendConfig::S3 { name, mut s3 },
                    ) => {
                        if s3.access_key_id.is_empty() {
                            s3.access_key_id = cur.access_key_id.clone();
                        }
                        if s3.secret_access_key.is_empty() {
                            s3.secret_access_key = cur.secret_access_key.clone();
                        }
                        crate::model::BackendConfig::S3 { name, s3 }
                    }
                    (
                        Some(crate::model::BackendConfig::Webdav { webdav: cur, .. }),
                        crate::model::BackendConfig::Webdav { name, mut webdav },
                    ) => {
                        if webdav.password.is_empty() {
                            webdav.password = cur.password.clone();
                        }
                        crate::model::BackendConfig::Webdav { name, webdav }
                    }
                    (_, other) => other,
                };
                by_name.insert(key, merged);
            }
            current.backends = by_name.into_values().collect();
            state.replace_config(current).await?;
        }
        _ => return Err(AppError::Other(format!("unknown mode: {mode}"))),
    }
    Ok(())
}
