//! Named (manual) snapshots — distinct from the auto-version retention pool.
//!
//! Auto-versions live under `.gsyncing/versions/<rel>.<unix-ms>` and are
//! pruned to N entries by the sync engine. Named snapshots live under
//! `.gsyncing/snapshots/{manifests,files}/` and are **never** pruned.
//! Designed for the multi-ending playthrough scenario: user wants a labelled
//! "before the boss" save they can come back to weeks later.

use crate::error::{AppError, AppResult};
use crate::model::{NamedSnapshot, SnapshotFileEntry};
use crate::paths;
use crate::state::AppState;
use crate::sync::engine;
use crate::sync::scanner;
use chrono::Utc;
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use uuid::Uuid;

const SNAPSHOTS_PREFIX: &str = ".gsyncing/snapshots";

/// Frontend-friendly summary used by `list_snapshots`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotSummary {
    pub id: String,
    pub name: String,
    #[serde(rename = "createdAt")]
    pub created_at_ms: i64,
    #[serde(rename = "fileCount")]
    pub file_count: usize,
    #[serde(rename = "totalSize")]
    pub total_size: u64,
}

/// Capture the current local state under `name`. Reads via the scanner (so we
/// reuse mtime+size cache + glob filtering) and uploads in parallel.
pub async fn create(state: Arc<AppState>, game_id: &str, name: &str) -> AppResult<SnapshotSummary> {
    let name = sanitize_name(name)?;
    let game = state.get_game(game_id).await?;
    let lock = state.lock_for_game(game_id).await;
    let _guard = lock.lock().await;

    let backend = engine::backend_for_game_public(&state, &game).await?;
    let remote_prefix = engine::remote_prefix_for_public(&game);

    // Reuse prior_index as hash hints so unchanged files don't get rehashed.
    let prior = engine::load_prior_index_public(&game.id)
        .await
        .unwrap_or_default();
    let hints = prior
        .files
        .iter()
        .map(|(rel, e)| (rel.clone(), (e.size, e.modified_ms, e.sha256.clone())))
        .collect::<std::collections::HashMap<_, _>>();

    let game_for_scan = game.clone();
    let scan =
        tokio::task::spawn_blocking(move || scanner::scan_with_hints(&game_for_scan, &hints))
            .await
            .map_err(|e| AppError::other(format!("scan join: {e}")))??;

    if scan.files.is_empty() {
        return Err(AppError::Other(format!(
            "[{}] no local files to snapshot",
            game.name
        )));
    }

    let snapshot_id = Uuid::new_v4().to_string();
    let manifest_key = paths::to_remote_key(
        &remote_prefix,
        &format!("{SNAPSHOTS_PREFIX}/manifests/{snapshot_id}.json"),
    );
    let files_prefix = paths::to_remote_key(
        &remote_prefix,
        &format!("{SNAPSHOTS_PREFIX}/files/{snapshot_id}"),
    );

    state.log.info(
        "snapshot",
        format!(
            "[{}] creating snapshot '{}' with {} files",
            game.name,
            name,
            scan.files.len()
        ),
    );

    let settings = state.get_settings().await;
    let concurrency = settings.max_concurrency.max(1);
    let total_bytes: u64 = scan.files.iter().map(|f| f.size).sum();
    let n = scan.files.len();
    let done_count = Arc::new(AtomicUsize::new(0));
    let done_bytes = Arc::new(AtomicU64::new(0));
    let started_at = Utc::now().timestamp_millis();
    let cancel = state.cancel_token_for(game_id).await;
    state.emit_progress(game_id, "snapshot", 0, n, None, 0, total_bytes, started_at);

    let owned = scan.files.clone();
    let files_prefix_for_each = files_prefix.clone();
    let backend_clone = backend.clone();
    let state_emit = state.clone();
    let game_id_owned = game_id.to_string();

    let upload_results: Vec<AppResult<()>> = stream::iter(owned.into_iter())
        .map(|f| {
            let backend = backend_clone.clone();
            let prefix = files_prefix_for_each.clone();
            let done_count = done_count.clone();
            let done_bytes = done_bytes.clone();
            let state_emit = state_emit.clone();
            let cancel = cancel.clone();
            let game_id = game_id_owned.clone();
            async move {
                if cancel.is_cancelled() {
                    return Err(AppError::Other("snapshot cancelled".into()));
                }
                let key = format!("{prefix}/{}", f.relative);
                let size = f.size;
                state_emit.emit_progress(
                    &game_id,
                    "snapshot",
                    done_count.load(Ordering::Relaxed),
                    n,
                    Some(&f.relative),
                    done_bytes.load(Ordering::Relaxed),
                    total_bytes,
                    started_at,
                );
                // 64 MiB threshold mirrors engine — large files go streaming
                if size > 64 * 1024 * 1024 {
                    backend.put_path(&key, &f.absolute).await?;
                } else {
                    let p = f.absolute.clone();
                    let bytes = tokio::task::spawn_blocking(move || std::fs::read(&p))
                        .await
                        .map_err(|e| AppError::other(format!("read join: {e}")))??;
                    backend.put(&key, bytes).await?;
                }
                let new_count = done_count.fetch_add(1, Ordering::Relaxed) + 1;
                let new_bytes = done_bytes.fetch_add(size, Ordering::Relaxed) + size;
                state_emit.emit_progress(
                    &game_id,
                    "snapshot",
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

    // If any upload failed, surface the first error and don't write the
    // manifest — that prevents a "ghost" snapshot that points at incomplete
    // file set.
    for r in upload_results {
        r?;
    }

    let manifest_files: BTreeMap<String, SnapshotFileEntry> = scan
        .files
        .iter()
        .map(|f| {
            (
                f.relative.clone(),
                SnapshotFileEntry {
                    sha256: f.sha256.clone(),
                    size: f.size,
                    modified_ms: f.modified_ms,
                    root_index: f.root_index,
                },
            )
        })
        .collect();

    let manifest = NamedSnapshot {
        id: snapshot_id.clone(),
        game_id: game.id.clone(),
        name: name.clone(),
        created_at_ms: Utc::now().timestamp_millis(),
        files: manifest_files,
        total_size: total_bytes,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    backend.put(&manifest_key, manifest_bytes).await?;

    state.emit_progress(game_id, "done", 0, 0, None, 0, 0, 0);
    state.log.info(
        "snapshot",
        format!(
            "[{}] snapshot '{}' created ({} files, {} B)",
            game.name, name, n, total_bytes
        ),
    );

    Ok(SnapshotSummary {
        id: snapshot_id,
        name,
        created_at_ms: manifest.created_at_ms,
        file_count: n,
        total_size: total_bytes,
    })
}

pub async fn list(state: Arc<AppState>, game_id: &str) -> AppResult<Vec<SnapshotSummary>> {
    let game = state.get_game(game_id).await?;
    let backend = engine::backend_for_game_public(&state, &game).await?;
    let remote_prefix = engine::remote_prefix_for_public(&game);
    let manifests_prefix =
        paths::to_remote_key(&remote_prefix, &format!("{SNAPSHOTS_PREFIX}/manifests"));

    let entries = backend.list(&manifests_prefix).await?;
    let mut out = Vec::new();
    for m in entries {
        if !m.key.ends_with(".json") {
            continue;
        }
        match backend.get(&m.key).await {
            Ok(bytes) => match serde_json::from_slice::<NamedSnapshot>(&bytes) {
                Ok(snap) => out.push(SnapshotSummary {
                    file_count: snap.files.len(),
                    total_size: snap.total_size,
                    id: snap.id,
                    name: snap.name,
                    created_at_ms: snap.created_at_ms,
                }),
                Err(e) => {
                    state
                        .log
                        .warn("snapshot", format!("bad manifest {}: {e}", m.key));
                }
            },
            Err(e) => {
                state
                    .log
                    .warn("snapshot", format!("read manifest {}: {e}", m.key));
            }
        }
    }
    out.sort_by(|a, b| b.created_at_ms.cmp(&a.created_at_ms));
    Ok(out)
}

pub async fn restore(state: Arc<AppState>, game_id: &str, snapshot_id: &str) -> AppResult<()> {
    let game = state.get_game(game_id).await?;
    let lock = state.lock_for_game(game_id).await;
    let _guard = lock.lock().await;

    let backend = engine::backend_for_game_public(&state, &game).await?;
    let remote_prefix = engine::remote_prefix_for_public(&game);
    let manifest_key = paths::to_remote_key(
        &remote_prefix,
        &format!("{SNAPSHOTS_PREFIX}/manifests/{snapshot_id}.json"),
    );
    let files_prefix = paths::to_remote_key(
        &remote_prefix,
        &format!("{SNAPSHOTS_PREFIX}/files/{snapshot_id}"),
    );

    let manifest_bytes = backend.get(&manifest_key).await?;
    let manifest: NamedSnapshot = serde_json::from_slice(&manifest_bytes)
        .map_err(|e| AppError::Other(format!("manifest parse: {e}")))?;

    state.log.info(
        "snapshot",
        format!(
            "[{}] restoring snapshot '{}' ({} files)",
            game.name,
            manifest.name,
            manifest.files.len()
        ),
    );

    // Resolve local roots once.
    let roots: Vec<PathBuf> = game
        .save_paths
        .iter()
        .map(|s| paths::expand(s).unwrap_or_default())
        .collect();

    let settings = state.get_settings().await;
    let concurrency = settings.max_concurrency.max(1);
    let total_bytes = manifest.total_size;
    let n = manifest.files.len();
    let done_count = Arc::new(AtomicUsize::new(0));
    let done_bytes = Arc::new(AtomicU64::new(0));
    let started_at = Utc::now().timestamp_millis();
    let cancel = state.cancel_token_for(game_id).await;
    state.emit_progress(
        game_id,
        "restore-snapshot",
        0,
        n,
        None,
        0,
        total_bytes,
        started_at,
    );

    let entries: Vec<(String, SnapshotFileEntry)> = manifest
        .files
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let backend_clone = backend.clone();
    let state_emit = state.clone();
    let game_id_owned = game_id.to_string();
    let files_prefix_for_each = files_prefix.clone();
    let roots_for_each = roots.clone();

    let results: Vec<AppResult<()>> = stream::iter(entries.into_iter())
        .map(|(rel, entry)| {
            let backend = backend_clone.clone();
            let prefix = files_prefix_for_each.clone();
            let done_count = done_count.clone();
            let done_bytes = done_bytes.clone();
            let state_emit = state_emit.clone();
            let cancel = cancel.clone();
            let game_id = game_id_owned.clone();
            let roots = roots_for_each.clone();
            async move {
                if cancel.is_cancelled() {
                    return Err(AppError::Other("restore cancelled".into()));
                }
                let key = format!("{prefix}/{rel}");
                state_emit.emit_progress(
                    &game_id,
                    "restore-snapshot",
                    done_count.load(Ordering::Relaxed),
                    n,
                    Some(&rel),
                    done_bytes.load(Ordering::Relaxed),
                    total_bytes,
                    started_at,
                );
                let root_idx = if entry.root_index < roots.len() {
                    entry.root_index
                } else {
                    0
                };
                let root = roots
                    .get(root_idx)
                    .ok_or_else(|| AppError::Path(format!("no local root for {rel}")))?;
                let mut abs = root.clone();
                for seg in rel.split('/') {
                    abs.push(seg);
                }
                if entry.size > 64 * 1024 * 1024 {
                    backend.get_to_path(&key, &abs).await?;
                } else {
                    let bytes = backend.get(&key).await?;
                    let abs_owned = abs.clone();
                    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
                        if let Some(parent) = abs_owned.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        let tmp = abs_owned.with_extension("gsyncing.tmp");
                        std::fs::write(&tmp, &bytes)?;
                        std::fs::rename(&tmp, &abs_owned)?;
                        Ok(())
                    })
                    .await
                    .map_err(|e| AppError::other(format!("write join: {e}")))??;
                }
                let new_count = done_count.fetch_add(1, Ordering::Relaxed) + 1;
                let new_bytes = done_bytes.fetch_add(entry.size, Ordering::Relaxed) + entry.size;
                state_emit.emit_progress(
                    &game_id,
                    "restore-snapshot",
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

    // After a successful restore the local files mirror the snapshot. Reflect
    // that in prior_index AND remote/live index so the next two-way sync sees
    // local == remote == prior and doesn't trigger conflict rename.
    let new_index_entries: BTreeMap<String, engine::RemoteIndexEntry> = manifest
        .files
        .iter()
        .map(|(rel, e)| {
            (
                rel.clone(),
                engine::RemoteIndexEntry {
                    sha256: e.sha256.clone(),
                    size: e.size,
                    modified_ms: e.modified_ms,
                    root_index: e.root_index,
                },
            )
        })
        .collect();
    let new_index = engine::RemoteIndex {
        files: new_index_entries.clone(),
    };

    // Also re-upload the snapshot bytes to the live keys so a peer machine
    // doing a normal sync after the restore picks up the rolled-back state.
    let live_uploads: Vec<(String, SnapshotFileEntry)> = manifest
        .files
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    for (rel, entry) in live_uploads {
        let live_key = paths::to_remote_key(&remote_prefix, &rel);
        let snap_key = format!("{files_prefix}/{rel}");
        if let Err(e) = backend.copy(&snap_key, &live_key).await {
            // Fallback: get + put.
            state
                .log
                .debug("snapshot", format!("copy {snap_key} → {live_key}: {e}"));
            let bytes = backend.get(&snap_key).await?;
            backend.put(&live_key, bytes).await?;
        }
        let _ = entry; // silence unused
    }

    engine::save_remote_index_public(backend.as_ref(), &remote_prefix, &new_index).await?;
    if let Err(e) = engine::save_prior_index_public(game_id, &new_index).await {
        state.log.warn("snapshot", format!("save prior: {e}"));
    }

    state.emit_progress(game_id, "done", 0, 0, None, 0, 0, 0);
    state.log.info(
        "snapshot",
        format!("[{}] snapshot '{}' restored", game.name, manifest.name),
    );
    Ok(())
}

pub async fn delete(state: Arc<AppState>, game_id: &str, snapshot_id: &str) -> AppResult<()> {
    let game = state.get_game(game_id).await?;
    let backend = engine::backend_for_game_public(&state, &game).await?;
    let remote_prefix = engine::remote_prefix_for_public(&game);

    let manifest_key = paths::to_remote_key(
        &remote_prefix,
        &format!("{SNAPSHOTS_PREFIX}/manifests/{snapshot_id}.json"),
    );
    let files_prefix = paths::to_remote_key(
        &remote_prefix,
        &format!("{SNAPSHOTS_PREFIX}/files/{snapshot_id}"),
    );

    // Delete the manifest FIRST. After this returns, list_snapshots no
    // longer sees the entry, so even if blob sweep is interrupted (network
    // drop, app crash) the snapshot is effectively gone from the UI and the
    // orphan blobs are merely wasted storage — never a half-broken snapshot
    // that restore() would choke on.
    backend
        .delete(&manifest_key)
        .await
        .map_err(|e| AppError::Other(format!("delete manifest: {e}")))?;

    let blobs = backend.list(&files_prefix).await.unwrap_or_default();
    let settings = state.get_settings().await;
    let concurrency = settings.max_concurrency.max(1);
    let log = state.log.clone();
    stream::iter(blobs.into_iter())
        .for_each_concurrent(Some(concurrency), |m| {
            let backend = backend.clone();
            let log = log.clone();
            async move {
                if let Err(e) = backend.delete(&m.key).await {
                    log.warn("snapshot", format!("delete {} : {e}", m.key));
                }
            }
        })
        .await;

    state.log.info(
        "snapshot",
        format!("[{}] snapshot {} deleted", game.name, snapshot_id),
    );
    Ok(())
}

fn sanitize_name(name: &str) -> AppResult<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err(AppError::Other("快照名称不能为空".into()));
    }
    if n.len() > 100 {
        return Err(AppError::Other("快照名称过长（最多 100 字符）".into()));
    }
    // Replace path-hostile chars but keep all human-language characters.
    let cleaned: String = n
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    Ok(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_empty_rejected() {
        assert!(sanitize_name("").is_err());
        assert!(sanitize_name("   ").is_err());
        assert!(sanitize_name("\t").is_err());
    }

    #[test]
    fn sanitize_too_long_rejected() {
        let s = "a".repeat(101);
        assert!(sanitize_name(&s).is_err());
        // Exactly 100 chars OK.
        assert!(sanitize_name(&"a".repeat(100)).is_ok());
    }

    #[test]
    fn sanitize_replaces_path_hostile_chars() {
        assert_eq!(
            sanitize_name(r#"foo/bar\baz:qux*?"<>|"#).unwrap(),
            "foo_bar_baz_qux______"
        );
    }

    #[test]
    fn sanitize_keeps_chinese_and_emoji() {
        assert_eq!(
            sanitize_name("黑暗剧情线 - 决战前 🎮").unwrap(),
            "黑暗剧情线 - 决战前 🎮"
        );
    }

    #[test]
    fn sanitize_strips_control_chars() {
        let with_control = "name\x01\x02\twith\x1Fcontrol";
        let out = sanitize_name(with_control).unwrap();
        assert!(!out.chars().any(|c| c.is_control()));
        assert!(out.contains("name"));
        assert!(out.contains("with"));
        assert!(out.contains("control"));
    }

    #[test]
    fn manifest_round_trips_through_json() {
        use crate::model::SnapshotFileEntry;
        let mut files = BTreeMap::new();
        files.insert(
            "save.dat".to_string(),
            SnapshotFileEntry {
                sha256: "abc".into(),
                size: 12,
                modified_ms: 100,
                root_index: 0,
            },
        );
        let m = NamedSnapshot {
            id: "uuid-1".into(),
            game_id: "g1".into(),
            name: "黑暗剧情线-决战前".into(),
            created_at_ms: 1700000000_000,
            files,
            total_size: 12,
        };
        let json = serde_json::to_vec(&m).unwrap();
        let back: NamedSnapshot = serde_json::from_slice(&json).unwrap();
        assert_eq!(back.id, m.id);
        assert_eq!(back.name, m.name);
        assert_eq!(back.files.len(), 1);
        assert_eq!(back.total_size, 12);
    }

    #[test]
    fn summary_serialization_uses_camel_case() {
        let s = SnapshotSummary {
            id: "x".into(),
            name: "y".into(),
            created_at_ms: 0,
            file_count: 3,
            total_size: 42,
        };
        let json = serde_json::to_string(&s).unwrap();
        // Front-end relies on camelCase field names.
        assert!(json.contains("\"createdAt\""));
        assert!(json.contains("\"fileCount\""));
        assert!(json.contains("\"totalSize\""));
        assert!(!json.contains("\"created_at_ms\""));
    }
}
