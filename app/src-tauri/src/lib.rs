//! Brainmatter desktop app: wires the core engine, the lock session and the
//! macOS integrations into Tauri.

mod commands;
pub mod context;
mod docs_watch;
pub mod error;
mod github;
pub mod i18n;
pub mod integrations;
mod perf;
pub mod session;
mod skill_import;
mod skill_menu;
mod tray;
mod unlock_prompt;
mod updates;

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicI64, Ordering},
    },
    time::Duration,
};

use bm_core::{Engine, semantic::Embedder, vault::KdfParams};
use bm_mcp::{Client, ClientRegistry, Hub};
use integrations::Target;
use parking_lot::Mutex;
use session::{Emit, Session, Settings, Theme};
use skill_menu::SkillMenus;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

const CAPTURE_WINDOW: &str = "capture";
/// Passed by `brainmatter-mcp` when it starts the app for an AI client: then
/// Brainmatter stays in the menu bar instead of opening its window.
const BACKGROUND_ARG: &str = "--background";

/// Debug builds use a separate vault so development never touches real data.
const VAULT_DIR: &str = if cfg!(debug_assertions) { "vault-dev" } else { "vault" };
/// ...and a separate port, so a dev build can run next to the installed app.
const MCP_ADDR: &str =
    if cfg!(debug_assertions) { "127.0.0.1:47475" } else { bm_mcp::DEFAULT_ADDR };

/// State of the MCP server, for the settings page.
pub struct McpStatus {
    pub addr: &'static str,
    pub error: Mutex<Option<String>>,
}

/// Gives MCP tools the engine of the lock session.
struct SessionHub(Arc<Session>);

impl Hub for SessionHub {
    fn engine(&self, client: &Client) -> Result<Arc<Engine>, String> {
        self.0.engine_for_client(&client.name)
    }

    fn open_engine(&self) -> Option<Arc<Engine>> {
        self.0.open_engine()
    }

    fn resolve_bookmark(&self, bookmark: &[u8]) -> Option<String> {
        bm_macos::bookmarks::resolve(bookmark)?.to_str().map(str::to_string)
    }

    fn embedder(&self) -> Option<Arc<dyn Embedder>> {
        self.0.embedder()
    }
}

fn show_main(app: &AppHandle) {
    tray::show_in_dock(app, true);
    let _ = app.show();
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Keeps the skill folders of connected tools in step with the vault. Waits a
/// moment after a change, so a burst of them makes one sync. `fresh` comes with
/// the start of the app and every unlock: then memory rules written by an older
/// version get the current wording too.
async fn keep_skill_menus(
    mut woken: tokio::sync::mpsc::UnboundedReceiver<bool>,
    session: Arc<Session>,
    registry: Arc<ClientRegistry>,
    status: Arc<McpStatus>,
    menus: Arc<SkillMenus>,
) {
    while let Some(mut fresh) = woken.recv().await {
        tokio::time::sleep(Duration::from_secs(1)).await;
        while let Ok(more) = woken.try_recv() {
            fresh |= more;
        }
        let (session, registry, status, menus) =
            (session.clone(), registry.clone(), status.clone(), menus.clone());
        let synced = tauri::async_runtime::spawn_blocking(move || {
            let Ok(local) = commands::local_integrations(&status) else { return };
            if fresh {
                for target in Target::ALL {
                    match local.refresh_rule(target) {
                        Ok(true) => tracing::info!("memory rule for {} updated", target.name()),
                        Ok(false) => {}
                        Err(e) => tracing::warn!("memory rule for {}: {e}", target.name()),
                    }
                }
            }
            // Locked, the folders stay as they are.
            if let Some(engine) = session.open_engine() {
                menus.sync(&engine, &registry.list(), &local, fresh);
            }
        });
        if let Err(e) = synced.await {
            tracing::error!("skill menu sync failed: {e}");
        }
    }
}

fn capture_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::Space)
}

/// Where the focus goes when quick capture closes.
#[derive(Default)]
struct CaptureState {
    /// Brainmatter had it. Otherwise it goes back to the previous app.
    return_to_main: AtomicBool,
}

fn tauri_theme(settings: &Settings) -> Option<tauri::Theme> {
    match settings.theme {
        Theme::System => None,
        Theme::Light => Some(tauri::Theme::Light),
        Theme::Dark => Some(tauri::Theme::Dark),
    }
}

fn toggle_capture(app: &AppHandle) {
    let existing = app.get_webview_window(CAPTURE_WINDOW);
    if existing.as_ref().is_some_and(|w| w.is_visible().unwrap_or(false)) {
        hide_capture(app);
        return;
    }
    let main_focused =
        app.get_webview_window("main").is_some_and(|w| w.is_focused().unwrap_or(false));
    app.state::<CaptureState>().return_to_main.store(main_focused, Ordering::Relaxed);
    let _ = app.show();
    if let Some(w) = existing {
        let _ = w.center();
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.emit("bm:capture-open", ());
        return;
    }
    // Built on first use: most sessions never need its web view in memory.
    let theme = tauri_theme(&app.state::<Arc<Session>>().settings());
    let built =
        WebviewWindowBuilder::new(app, CAPTURE_WINDOW, WebviewUrl::App("index.html".into()))
            .title("Quick Capture")
            .inner_size(620.0, 168.0)
            .center()
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .resizable(false)
            .skip_taskbar(true)
            .shadow(false)
            .theme(theme)
            .focused(true)
            .build();
    if let Err(e) = built {
        tracing::error!("quick capture window failed: {e}");
    }
}

fn hide_capture(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(CAPTURE_WINDOW) {
        let _ = w.hide();
    }
    if !app.state::<CaptureState>().return_to_main.load(Ordering::Relaxed) {
        let _ = app.hide();
    }
}

/// Applies the settings that live outside the web view.
fn apply_settings(app: &AppHandle, settings: &Settings) {
    let theme = tauri_theme(settings);
    for window in app.webview_windows().values() {
        let _ = window.set_theme(theme);
    }
    let shortcuts = app.global_shortcut();
    let shortcut = capture_shortcut();
    let result = match (settings.quick_capture, shortcuts.is_registered(shortcut)) {
        (true, false) => shortcuts.register(shortcut),
        (false, true) => shortcuts.unregister(shortcut),
        _ => Ok(()),
    };
    if let Err(e) = &result {
        // Most likely another app holds the same keys.
        tracing::warn!("quick capture shortcut: {e}");
    }
    app.state::<Arc<Session>>().set_capture_shortcut_blocked(result.is_err());
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,brainmatter=debug".into()),
        )
        .init();

    // The web view learns the language before its first script.
    let language = tauri::plugin::Builder::<tauri::Wry>::new("language")
        .js_init_script(i18n::script())
        .build();
    let builder = tauri::Builder::default().plugin(language);
    // A dev build and a measuring run work beside an installed copy, with their
    // own vault and port; the installed copy would otherwise take over.
    let builder = if cfg!(debug_assertions) || perf::script().is_some() {
        builder
    } else {
        builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
    };
    let app = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_capture(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let dir = match perf::vault_dir() {
                Some(dir) => dir,
                None => app.path().app_data_dir()?.join(VAULT_DIR),
            };
            perf::attach(app.handle());
            tracing::info!("vault directory: {}", dir.display());
            let handle = app.handle().clone();
            let (wake, woken) = tokio::sync::mpsc::unbounded_channel();
            let _ = wake.send(true);
            let emit: Emit = Arc::new(move |name, payload| {
                let changed = match name {
                    "bm:event" => payload["type"] == "change_set_committed",
                    "bm:resync" => true,
                    _ => false,
                };
                if changed || name == "bm:session" && payload["state"] == "unlocked" {
                    let _ = wake.send(!changed);
                }
                if name == "bm:unlock-request" {
                    let app = handle.clone();
                    let _ = handle.run_on_main_thread(move || unlock_prompt::show(&app));
                }
                // Unlocked or turned down, wherever: the password window goes,
                // without relying on its web view to notice.
                if name == "bm:session" && payload["unlock_request"].is_null() {
                    let app = handle.clone();
                    let _ = handle.run_on_main_thread(move || unlock_prompt::hide(&app));
                }
                let _ = handle.emit(name, payload);
            });
            let registry = Arc::new(ClientRegistry::open(&dir.join("clients.json")));
            let menus = Arc::new(SkillMenus::open(dir.join("skill-menus.json")));
            let session = Arc::new(Session::new(dir, emit, KdfParams::recommended()));
            let mcp_status = Arc::new(McpStatus { addr: MCP_ADDR, error: Mutex::new(None) });
            app.manage(session.clone());
            app.manage(registry.clone());
            app.manage(mcp_status.clone());
            app.manage(menus.clone());
            tauri::async_runtime::spawn(keep_skill_menus(
                woken,
                session.clone(),
                registry.clone(),
                mcp_status.clone(),
                menus,
            ));
            app.manage(CaptureState::default());
            app.manage(commands::ImportFolder::default());
            apply_settings(app.handle(), &session.settings());

            let hub: Arc<dyn Hub> = Arc::new(SessionHub(session.clone()));
            tauri::async_runtime::spawn(async move {
                if let Err(e) = bm_mcp::serve(MCP_ADDR, registry, hub, Default::default()).await {
                    tracing::error!("MCP server failed: {e}");
                    *mcp_status.error.lock() = Some(e.to_string());
                }
            });

            // Lives as long as the app; observers fire on the main thread.
            let observed = session.clone();
            std::mem::forget(bm_macos::power::observe(move |trigger| {
                observed.on_system_event(trigger)
            }));

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // The workspace checks once on unlock; this is for apps left running.
                let last_update_check = AtomicI64::new(bm_core::time::now_ms());
                let mut tick = tokio::time::interval(Duration::from_secs(20));
                loop {
                    tick.tick().await;
                    session.maybe_auto_lock();
                    session.unload_idle_model();
                    session.maybe_backup();
                    updates::maybe_check(&handle, &last_update_check);
                }
            });

            tray::build(app.handle())?;
            if std::env::args().any(|arg| arg == BACKGROUND_ARG) {
                tray::show_in_dock(app.handle(), false);
            } else {
                show_main(app.handle());
            }
            Ok(())
        })
        .on_page_load(perf::run_script)
        .on_window_event(|window, event| {
            // Closing the window keeps Brainmatter running in the menu bar, and
            // only there: it leaves the Dock until the window opens again.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
                if window.label() == "main" {
                    tray::show_in_dock(window.app_handle(), false);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::session_status,
            commands::create_vault,
            commands::adopt_backup,
            commands::unlock_password,
            commands::unlock_recovery,
            commands::unlock_touch_id,
            commands::enroll_touch_id,
            commands::disable_touch_id,
            commands::change_password,
            commands::rotate_recovery_key,
            commands::lock,
            commands::get_settings,
            commands::update_settings,
            commands::touch,
            commands::projects,
            commands::create_project,
            commands::add_to_project,
            commands::remove_from_project,
            commands::graph,
            commands::graph_part,
            commands::save_layout,
            commands::node_detail,
            commands::search,
            commands::model_status,
            commands::download_model,
            commands::remove_model,
            commands::link_suggestions,
            commands::dismiss_suggestion,
            commands::accept_suggestion,
            commands::merge_nodes,
            commands::app_version,
            commands::check_update,
            commands::install_update,
            commands::rename_project,
            commands::move_project,
            commands::project_delete_preview,
            commands::delete_project,
            commands::favorites,
            commands::set_favorite,
            commands::tag_counts,
            commands::actors,
            commands::doc_tree,
            commands::create_doc_page,
            commands::place_doc_page,
            commands::doc_sources,
            commands::add_doc_source,
            commands::sync_doc_sources,
            commands::remove_doc_source,
            commands::templates,
            commands::skills,
            commands::save_skill,
            commands::set_skill_on_disk,
            commands::preview_skill_import,
            commands::import_skills,
            commands::file_links,
            commands::link_files,
            commands::set_file_link_approved,
            commands::delete_file_link,
            commands::reveal_file,
            commands::open_file,
            commands::trash,
            commands::restore_node,
            commands::purge_node,
            commands::hide_capture,
            commands::show_main_window,
            commands::hide_tray,
            commands::dismiss_unlock_request,
            commands::hide_unlock_prompt,
            commands::open_in_main,
            commands::toggle_capture,
            commands::backup_status,
            commands::backup_now,
            commands::restore_backup,
            commands::choose_backup_copy,
            commands::remove_backup_copy,
            commands::export_markdown,
            commands::import_preview,
            commands::import_markdown,
            commands::add_attachment,
            commands::attachment_data,
            commands::delete_attachment,
            commands::export_attachment,
            commands::orphans,
            commands::unreviewed,
            commands::unread,
            commands::outdated,
            commands::missing_links,
            commands::confirm_current,
            commands::choose_project_folder,
            commands::clear_project_folder,
            commands::project_context,
            commands::create_node,
            commands::update_node,
            commands::delete_node,
            commands::link,
            commands::unlink,
            commands::change_sets,
            commands::recent_changes,
            commands::node_history,
            commands::revert,
            commands::mcp_info,
            commands::create_client,
            commands::revoke_client,
            commands::integrations,
            commands::connect_integration,
            commands::set_memory_rule,
            commands::set_skill_menu,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Brainmatter");

    app.run(|app, event| {
        // Clicking the Dock icon brings the hidden window back.
        if let RunEvent::Reopen { .. } = event {
            show_main(app);
        }
    });
}

#[cfg(test)]
mod tests {
    /// A window missing from the capability may still call the app's own
    /// commands, but not listen for events: it never learns that the vault
    /// was unlocked. The browser mock cannot catch this.
    #[test]
    fn every_window_has_the_capability() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        let windows = capability["windows"].as_array().unwrap();
        for label in [
            "main",
            super::CAPTURE_WINDOW,
            super::tray::PANEL_WINDOW,
            super::unlock_prompt::PROMPT_WINDOW,
        ] {
            assert!(windows.iter().any(|w| w == label), "{label} is missing from default.json");
        }
    }
}
