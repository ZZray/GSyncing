use crate::model::SyncDirection;
use crate::state::AppState;
use crate::sync::{engine, scheduler};
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager,
};

const RECENT_LIMIT: usize = 5;
const RECENT_PREFIX: &str = "recent:";

/// Build the tray icon + menu and wire it up. Called once during setup.
/// The "recent games" section is computed once at startup — there's no
/// runtime menu rebuilding (Tauri 2 makes that awkward), but the items
/// stay useful because the per-game lock just queues if you click a game
/// that hasn't been synced yet.
pub fn install(app: &AppHandle, state: Arc<AppState>) -> tauri::Result<()> {
    let sync_item = MenuItem::with_id(app, "sync_all", "立即同步全部", true, None::<&str>)?;
    let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let separator1 = PredefinedMenuItem::separator(app)?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出 GSyncing", true, None::<&str>)?;

    // Compute the "recent games" submenu from current state. This runs in
    // the setup hook (main thread) where AppState is already populated.
    let recent = recent_games_blocking(&state);
    let mut recent_items: Vec<MenuItem<_>> = Vec::new();
    for (id, name) in &recent {
        let label = format!("同步「{}」", truncate(name, 22));
        let item = MenuItem::with_id(
            app,
            format!("{RECENT_PREFIX}{id}"),
            label,
            true,
            None::<&str>,
        )?;
        recent_items.push(item);
    }

    let mut menu_refs: Vec<&dyn tauri::menu::IsMenuItem<_>> = Vec::new();
    menu_refs.push(&sync_item);
    if !recent_items.is_empty() {
        menu_refs.push(&separator1);
        for item in &recent_items {
            menu_refs.push(item);
        }
    }
    menu_refs.push(&separator2);
    menu_refs.push(&show_item);
    menu_refs.push(&quit_item);

    let menu = Menu::with_items(app, &menu_refs)?;

    // Reuse the bundled window icon.
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("GSyncing — 游戏存档云同步")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| handle_menu(app, event, state.clone()));

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

/// Pull the top `RECENT_LIMIT` games by most-recent successful sync.
/// Falls back to insertion order if no game has ever been synced yet.
/// Synchronous because we run inside Tauri's setup hook.
fn recent_games_blocking(state: &Arc<AppState>) -> Vec<(String, String)> {
    let games = futures::executor::block_on(state.list_games());
    let statuses = futures::executor::block_on(state.list_status());
    let mut scored: Vec<(i64, String, String)> = games
        .iter()
        .map(|g| {
            let last = statuses
                .iter()
                .find(|s| s.game_id == g.id)
                .and_then(|s| s.last_sync_at)
                .unwrap_or(0);
            (last, g.id.clone(), g.name.clone())
        })
        .collect();
    // Newest sync first; never-synced games trail (last=0 sorts to the end).
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored
        .into_iter()
        .take(RECENT_LIMIT)
        .map(|(_, id, name)| (id, name))
        .collect()
}

fn truncate(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = chars.into_iter().take(max_chars - 1).collect();
        out.push('…');
        out
    }
}

fn handle_menu(app: &AppHandle, event: MenuEvent, state: Arc<AppState>) {
    let id = event.id.as_ref();
    if let Some(game_id) = id.strip_prefix(RECENT_PREFIX) {
        let st = state.clone();
        let game_id = game_id.to_string();
        tauri::async_runtime::spawn(async move {
            if st.list_backends().await.is_empty() {
                st.log
                    .warn("tray", "skip recent-sync: no backend configured");
                return;
            }
            if let Err(e) =
                engine::run_one(st.clone(), &game_id, SyncDirection::Auto).await
            {
                st.log.warn("tray", format!("recent-sync {game_id}: {e}"));
            }
        });
        return;
    }
    match id {
        "show" => {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.unminimize();
                let _ = win.set_focus();
            }
        }
        "sync_all" => {
            let st = state.clone();
            tauri::async_runtime::spawn(async move {
                if st.list_backends().await.is_empty() {
                    st.log.warn("tray", "skip sync_all: no backend configured");
                    return;
                }
                if let Err(e) = scheduler::sync_all_with_state(st.clone()).await {
                    st.log.warn("tray", format!("sync_all: {e}"));
                }
            });
        }
        "quit" => {
            app.exit(0);
        }
        _ => {}
    }
}
