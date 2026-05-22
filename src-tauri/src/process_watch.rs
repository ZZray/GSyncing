use crate::error::AppResult;
use crate::model::SyncDirection;
use crate::state::AppState;
use crate::sync::engine;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, RefreshKind, System};

/// Polls the running-process table once every `POLL_INTERVAL`. When a game's
/// associated process transitions from "running" to "not running", we fire a
/// Push sync — the user has just quit the game and the save is now stable on
/// disk.
const POLL_INTERVAL: Duration = Duration::from_secs(5);

pub async fn run(state: Arc<AppState>) -> AppResult<()> {
    state.log.info("proc-watch", "started");
    let mut sys =
        System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new()));
    let mut prev_running: HashSet<String> = HashSet::new();

    loop {
        let settings = state.get_settings().await;
        if !settings.enable_exit_sync {
            // Sleep on the notification channel — when settings change we wake.
            state.scheduler_notify.notified().await;
            continue;
        }

        // Collect process names of interest.
        let games = state.list_games().await;
        let watched: Vec<(String, String)> = games
            .iter()
            .filter_map(|g| {
                g.process_name
                    .as_ref()
                    .map(|p| (g.id.clone(), p.to_ascii_lowercase()))
            })
            .collect();
        if watched.is_empty() {
            tokio::select! {
                _ = tokio::time::sleep(POLL_INTERVAL) => {}
                _ = state.scheduler_notify.notified() => {}
            }
            continue;
        }

        // Refresh process list (cheap on Windows — just enumerates pids/names).
        sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::new(),
        );
        let running_names: HashSet<String> = sys
            .processes()
            .values()
            .map(|p| p.name().to_string_lossy().to_ascii_lowercase().to_string())
            .collect();

        let mut now_running: HashSet<String> = HashSet::new();
        for (gid, want) in &watched {
            // Match either bare name (e.g. "YuanShen.exe") or trailing component.
            let want_lc = want.trim().to_ascii_lowercase();
            let hit = running_names.iter().any(|n| {
                n == &want_lc
                    || n.rsplit(|c| c == '/' || c == '\\')
                        .next()
                        .map(|s| s == want_lc)
                        .unwrap_or(false)
            });
            if hit {
                now_running.insert(gid.clone());
            }
        }

        // Detect transitions running → not-running.
        for gid in prev_running
            .difference(&now_running)
            .cloned()
            .collect::<Vec<_>>()
        {
            if !state.try_acquire_in_flight(&gid).await {
                state
                    .log
                    .debug("proc-watch", format!("skip {} — already in flight", gid));
                continue;
            }
            let st = state.clone();
            let gid_owned = gid.clone();
            tokio::spawn(async move {
                st.log.info(
                    "proc-watch",
                    format!("game {gid_owned} exited — pushing save"),
                );
                if let Err(e) = engine::run_one(st.clone(), &gid_owned, SyncDirection::Push).await {
                    st.log
                        .warn("proc-watch", format!("exit-sync {}: {}", gid_owned, e));
                }
                st.release_in_flight(&gid_owned).await;
            });
        }
        prev_running = now_running;

        tokio::select! {
            _ = tokio::time::sleep(POLL_INTERVAL) => {}
            _ = state.scheduler_notify.notified() => {
                // Settings/game-list changed. Reset the running snapshot so
                // any process currently alive is treated as "newly seen" next
                // tick — we do NOT fire a phantom exit-sync just because the
                // settings page was opened. The user will still get a proper
                // exit-sync when they actually quit the game later.
                prev_running.clear();
            }
        }
    }
}
