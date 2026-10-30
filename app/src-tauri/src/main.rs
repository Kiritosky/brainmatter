mod commands;
mod error;
mod session;

use std::{sync::Arc, time::Duration};

use session::Session;
use tauri::{
    AppHandle, Manager, RunEvent, WindowEvent,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

/// Debug builds use a separate vault so development never touches real data.
const VAULT_DIR: &str = if cfg!(debug_assertions) { "vault-dev" } else { "vault" };

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Brainmatter öffnen", true, None::<&str>)?;
    let lock = MenuItem::with_id(app, "lock", "Sperren", true, Some("CmdOrCtrl+Shift+L"))?;
    let quit = MenuItem::with_id(app, "quit", "Beenden", true, Some("CmdOrCtrl+Q"))?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &lock, &sep, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main")
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
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,brainmatter=debug".into()),
        )
        .init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .setup(|app| {
            let dir = app.path().app_data_dir()?.join(VAULT_DIR);
            tracing::info!("vault directory: {}", dir.display());
            let session = Arc::new(Session::new(dir, app.handle().clone()));
            app.manage(session.clone());

            tauri::async_runtime::spawn(async move {
                let mut tick = tokio::time::interval(Duration::from_secs(20));
                loop {
                    tick.tick().await;
                    session.maybe_auto_lock();
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
            commands::lock,
            commands::set_auto_lock,
            commands::touch,
            commands::projects,
            commands::create_project,
            commands::add_to_project,
            commands::remove_from_project,
            commands::graph,
            commands::save_layout,
            commands::node_detail,
            commands::search,
            commands::create_node,
            commands::update_node,
            commands::delete_node,
            commands::link,
            commands::unlink,
            commands::change_sets,
            commands::node_history,
            commands::revert,
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
