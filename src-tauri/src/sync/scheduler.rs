use crate::error::{AppError, AppResult};
use crate::model::SyncDirection;
use crate::state::AppState;
use crate::sync::engine;
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// Periodic auto-sync loop.
pub async fn run(state: Arc<AppState>) -> AppResult<()> {
    state.log.info("scheduler", "started");
    loop {
        let interval = state.get_settings().await.auto_sync_interval_sec;
        if interval == 0 {
            state.scheduler_notify.notified().await;
            continue;
        }
        tokio::select! {
            _ = sleep(Duration::from_secs(interval)) => {
                if let Err(e) = run_all_auto(state.clone()).await {
                    state.log.warn("scheduler", format!("run_all_auto: {e}"));
                }
            }
            _ = state.scheduler_notify.notified() => {
                continue;
            }
        }
    }
}

/// Run a sync pass for every game that has auto_sync enabled.
pub async fn run_all_auto(state: Arc<AppState>) -> AppResult<()> {
    let games = state.list_games().await;
    if !games.iter().any(|g| g.auto_sync) {
        return Ok(());
    }
    if state.list_backends().await.is_empty() {
        state
            .log
            .debug("scheduler", "skip auto-sync: no backend configured");
        return Ok(());
    }

    state.emit_global(true, Some("自动同步中".into()), None, None);
    let mut last_err: Option<String> = None;
    for g in games {
        if !g.auto_sync {
            continue;
        }
        if let Err(e) = engine::run_one(state.clone(), &g.id, SyncDirection::Auto).await {
            last_err = Some(format!("{}: {}", g.name, e));
        }
    }
    match last_err {
        Some(e) => state.emit_global(
            false,
            Some("自动同步完成（含错误）".into()),
            Some(Utc::now().timestamp_millis()),
            Some(e),
        ),
        None => state.emit_global(
            false,
            Some("自动同步完成".into()),
            Some(Utc::now().timestamp_millis()),
            None,
        ),
    }
    Ok(())
}

/// "Sync now" — runs every game regardless of auto_sync flag.
pub async fn sync_all_with_state(state: Arc<AppState>) -> AppResult<()> {
    let games = state.list_games().await;
    if state.list_backends().await.is_empty() {
        return Err(AppError::Config("no backend configured".into()));
    }
    state.emit_global(true, Some("一键同步中".into()), None, None);
    let mut last_err: Option<String> = None;
    for g in games {
        if let Err(e) = engine::run_one(state.clone(), &g.id, SyncDirection::Auto).await {
            last_err = Some(format!("{}: {}", g.name, e));
        }
    }
    match last_err.clone() {
        Some(e) => state.emit_global(
            false,
            Some("同步完成（含错误）".into()),
            Some(Utc::now().timestamp_millis()),
            Some(e),
        ),
        None => state.emit_global(
            false,
            Some("同步完成".into()),
            Some(Utc::now().timestamp_millis()),
            None,
        ),
    }
    if let Some(e) = last_err {
        return Err(AppError::Other(e));
    }
    Ok(())
}
