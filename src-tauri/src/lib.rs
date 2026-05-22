pub mod commands;
pub mod crypto;
pub mod error;
pub mod logbus;
pub mod model;
pub mod paths;
pub mod process_watch;
pub mod state;
pub mod stats;
pub mod storage;
pub mod sync;
pub mod tray;
pub mod watcher;

use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,gsyncing_lib=debug".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let state = tauri::async_runtime::block_on(async {
                state::AppState::initialize(handle.clone()).await
            })?;
            let state = Arc::new(state);
            app.manage(state.clone());

            // start background services
            let bg_state = state.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = sync::scheduler::run(bg_state).await {
                    tracing::error!("scheduler stopped: {e:#}");
                }
            });

            let watcher_state = state.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = watcher::run(watcher_state).await {
                    tracing::error!("watcher stopped: {e:#}");
                }
            });

            let proc_state = state.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = process_watch::run(proc_state).await {
                    tracing::error!("process_watch stopped: {e:#}");
                }
            });

            // Auto-open DevTools when the user explicitly opted in (default
            // ON during v1.3.x white-screen diagnosis; users flip it OFF in
            // Settings once the app loads cleanly).
            if state.get_settings_blocking().auto_open_devtools {
                if let Some(win) = handle.get_webview_window("main") {
                    win.open_devtools();
                }
            }

            // System tray + close-to-tray behaviour.
            if let Err(e) = tray::install(&handle, state.clone()) {
                tracing::warn!("tray install failed: {e}");
            }
            if let Some(win) = handle.get_webview_window("main") {
                let win_state = state.clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        // Read the cached AtomicBool — DO NOT block_on the
                        // tokio RwLock here. This handler runs on the GUI
                        // thread and a block_on against the same runtime can
                        // deadlock.
                        if win_state
                            .close_to_tray
                            .load(std::sync::atomic::Ordering::Relaxed)
                        {
                            if let Some(w) = win_state.handle.get_webview_window("main") {
                                let _ = w.hide();
                            }
                            api.prevent_close();
                            win_state
                                .log
                                .info("tray", "main window closed → hidden to tray");
                        }
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::list_games,
            commands::save_game,
            commands::delete_game,
            commands::list_backends,
            commands::save_backend,
            commands::delete_backend,
            commands::test_backend,
            commands::get_settings,
            commands::save_settings,
            commands::list_status,
            commands::sync_one,
            commands::sync_all,
            commands::sync_preview,
            commands::list_versions,
            commands::restore_version,
            commands::delete_version,
            commands::export_version,
            commands::cancel_sync,
            commands::sync_with_overrides,
            commands::create_snapshot,
            commands::list_snapshots,
            commands::restore_snapshot,
            commands::delete_snapshot,
            commands::export_config,
            commands::import_config,
            commands::read_log,
            commands::read_stats,
            commands::flush_log,
            commands::get_data_dir,
            commands::compute_save_size,
            commands::validate_game_paths,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                // Flush the log ring to disk so a crash / unexpected close
                // still leaves a readable trace for the user.
                if let Some(state) = app.try_state::<Arc<state::AppState>>() {
                    if let Err(e) = state.log.persist_to_disk() {
                        tracing::warn!("flush log on exit: {e}");
                    }
                }
            }
        });
}
