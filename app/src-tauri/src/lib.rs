//! Brainmatter desktop app: wires the core engine, the lock session and the
//! macOS integrations into Tauri.

mod commands;
pub mod error;
pub mod integrations;
pub mod session;
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
use parking_lot::Mutex;
use session::{Emit, Session, Settings, Theme};
use tauri::{
    AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent,
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

const CAPTURE_WINDOW: &str = "capture";

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
    let _ = app.show();
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
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
    if let Err(e) = result {
        // Most likely another app holds the same keys.
        tracing::warn!("quick capture shortcut: {e}");
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Brainmatter öffnen", true, None::<&str>)?;
    let lock = MenuItem::with_id(app, "lock", "Sperren", true, Some("CmdOrCtrl+Shift+L"))?;
    let quit = MenuItem::with_id(app, "quit", "Beenden", true, Some("CmdOrCtrl+Q"))?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &lock, &sep, &quit])?;

    TrayIconBuilder::with_id("main")
        // Monochrome template image: macOS tints it for light/dark menu bars.
        .icon(Image::from_bytes(include_bytes!("../icons/tray-template.png"))?)
        .icon_as_template(true)
        .tooltip("Brainmatter")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "lock" => app.state::<Arc<Session>>().lock(),
            "quit" => {
                app.state::<Arc<Session>>().lock();
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,brainmatter=debug".into()),
        )
        .init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
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
            let dir = app.path().app_data_dir()?.join(VAULT_DIR);
            tracing::info!("vault directory: {}", dir.display());
            let handle = app.handle().clone();
            let emit: Emit = Arc::new(move |name, payload| {
                let _ = handle.emit(name, payload);
            });
            let registry = Arc::new(ClientRegistry::open(&dir.join("clients.json")));
            let session = Arc::new(Session::new(dir, emit, KdfParams::recommended()));
            let mcp_status = Arc::new(McpStatus { addr: MCP_ADDR, error: Mutex::new(None) });
            app.manage(session.clone());
            app.manage(registry.clone());
            app.manage(mcp_status.clone());
            app.manage(CaptureState::default());
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

            build_tray(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps Brainmatter running in the menu bar.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::session_status,
            commands::create_vault,
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
            commands::doc_tree,
            commands::create_doc_page,
            commands::place_doc_page,
            commands::doc_sources,
            commands::add_doc_source,
            commands::sync_doc_sources,
            commands::remove_doc_source,
            commands::templates,
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
            commands::backup_status,
            commands::backup_now,
            commands::restore_backup,
            commands::add_attachment,
            commands::attachment_data,
            commands::delete_attachment,
            commands::export_attachment,
            commands::orphans,
            commands::unreviewed,
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
