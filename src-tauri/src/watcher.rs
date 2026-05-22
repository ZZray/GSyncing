use crate::error::AppResult;
use crate::model::SyncDirection;
use crate::paths;
use crate::state::AppState;
use crate::sync::engine;
use notify_debouncer_full::notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer, DebounceEventResult};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

const DEBOUNCE: Duration = Duration::from_secs(5);

/// Watches all configured save paths and triggers an auto-sync when changes settle.
pub async fn run(state: Arc<AppState>) -> AppResult<()> {
    state.log.info("watcher", "started");
    loop {
        let settings = state.get_settings().await;
        if !settings.enable_file_watcher {
            state.watcher_notify.notified().await;
            continue;
        }

        let games = state.list_games().await;
        let mut watch_paths: Vec<(String, PathBuf)> = Vec::new();
        for g in &games {
            if !g.auto_sync {
                continue;
            }
            for raw in &g.save_paths {
                if let Ok(p) = paths::expand(raw) {
                    if p.exists() {
                        watch_paths.push((g.id.clone(), p));
                    }
                }
            }
        }
        if watch_paths.is_empty() {
            state.log.debug("watcher", "no watch paths, sleeping");
            state.watcher_notify.notified().await;
            continue;
        }

        let (tx, mut rx) = mpsc::channel::<DebounceEventResult>(64);
        let mut debouncer = match new_debouncer(DEBOUNCE, None, move |res: DebounceEventResult| {
            let _ = tx.blocking_send(res);
        }) {
            Ok(d) => d,
            Err(e) => {
                state.log.error("watcher", format!("create debouncer: {e}"));
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            }
        };

        for (_gid, p) in &watch_paths {
            if let Err(e) = debouncer.watch(p, RecursiveMode::Recursive) {
                state
                    .log
                    .warn("watcher", format!("watch {}: {}", p.display(), e));
            }
        }
        state
            .log
            .info("watcher", format!("watching {} paths", watch_paths.len()));

        loop {
            tokio::select! {
                Some(res) = rx.recv() => {
                    let events = match res {
                        Ok(v) => v,
                        Err(e) => {
                            state.log.warn("watcher", format!("event errors: {e:?}"));
                            continue;
                        }
                    };
                    let mut affected_games: HashSet<String> = HashSet::new();
                    for evt in events {
                        for path in &evt.paths {
                            for (gid, root) in &watch_paths {
                                if path.starts_with(root) {
                                    affected_games.insert(gid.clone());
                                    break;
                                }
                            }
                        }
                    }
                    for gid in affected_games {
                        if !state.try_acquire_in_flight(&gid).await {
                            state.log.debug(
                                "watcher",
                                format!("skip {} — already in flight", gid),
                            );
                            continue;
                        }
                        let st = state.clone();
                        let gid_owned = gid.clone();
                        tokio::spawn(async move {
                            // Local file change → push direction so we never
                            // overwrite the user's in-progress save with an older
                            // remote copy.
                            if let Err(e) =
                                engine::run_one(st.clone(), &gid_owned, SyncDirection::Push).await
                            {
                                st.log.warn(
                                    "watcher",
                                    format!("auto sync {}: {}", gid_owned, e),
                                );
                            }
                            st.release_in_flight(&gid_owned).await;
                        });
                    }
                }
                _ = state.watcher_notify.notified() => {
                    // restart with fresh game list
                    break;
                }
            }
        }
    }
}
