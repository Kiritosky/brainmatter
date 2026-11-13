//! Where the master password goes when an AI client finds the vault locked:
//! without Touch ID, or after the Touch ID prompt's password button. Either a
//! small window in the middle of the screen or the menu bar panel, as the
//! settings say. The main window stays as it is.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::session::{Session, UnlockPrompt};

pub const PROMPT_WINDOW: &str = "unlock";
/// In points. The window takes the height of what it shows.
const WIDTH: f64 = 380.0;
const HEIGHT: f64 = 230.0;

/// Shows where to type the password. Call on the main thread.
pub fn show(app: &AppHandle) {
    let session = app.state::<Arc<Session>>().inner().clone();
    let shown = match session.settings().unlock_prompt {
        UnlockPrompt::MenuBar => crate::tray::open_for_unlock(app) || show_window(app),
        UnlockPrompt::Center => show_window(app),
    };
    // Nowhere to answer it: end the request rather than keep the AI waiting.
    if !shown {
        session.dismiss_unlock_request();
    }
}

fn show_window(app: &AppHandle) -> bool {
    // Brainmatter may be hidden after the menu bar panel gave the focus back.
    let _ = app.show();
    if let Some(w) = app.get_webview_window(PROMPT_WINDOW) {
        let _ = w.center();
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.emit("bm:unlock-open", ());
        return true;
    }
    let theme = crate::tauri_theme(&app.state::<Arc<Session>>().settings());
    let built = WebviewWindowBuilder::new(app, PROMPT_WINDOW, WebviewUrl::App("index.html".into()))
        .title("Brainmatter")
        .inner_size(WIDTH, HEIGHT)
        .center()
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .resizable(false)
        .skip_taskbar(true)
        .shadow(false)
        .visible_on_all_workspaces(true)
        .theme(theme)
        .focused(true)
        .build();
    match built {
        Ok(_) => true,
        Err(e) => {
            tracing::error!("password window failed: {e}");
            false
        }
    }
}

/// Closes the window, gives the focus back to the app used before unless the
/// main window is open, and frees the web view: it is rarely needed again soon.
pub fn hide(app: &AppHandle) {
    let Some(window) = app.get_webview_window(PROMPT_WINDOW) else { return };
    let _ = window.hide();
    let _ = window.destroy();
    let main_open = app
        .get_webview_window("main")
        .is_some_and(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false));
    if !main_open {
        let _ = app.hide();
    }
}
