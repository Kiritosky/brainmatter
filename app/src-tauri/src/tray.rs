//! The menu bar. A left click on the icon opens a small panel: whether the vault
//! is open, unlocking, the latest changes and a search. A right click opens a
//! plain menu. While the main window is closed, Brainmatter lives only here and
//! leaves the Dock and ⌘Tab.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, Rect, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

use crate::{i18n::tr, session::Session};

pub const PANEL_WINDOW: &str = "tray";
/// In points. The panel sets its own height to what it shows.
const PANEL_WIDTH: f64 = 340.0;
const PANEL_HEIGHT: f64 = 400.0;
/// A closed panel keeps its web view this long, so that opening it again is
/// instant, and then gives the memory back.
const KEEP_HIDDEN: Duration = Duration::from_secs(60);
/// Clicking the icon of an open panel takes the focus from it first, which
/// closes it. A click that soon after counts as closing, not as opening again.
const REOPEN_GUARD: Duration = Duration::from_millis(300);

#[derive(Default)]
pub struct PanelState {
    hidden_at: Mutex<Option<Instant>>,
    /// Counts openings, so that a pending clean-up spares a panel shown again.
    opened: AtomicU64,
}

/// What the main window should show, sent from the panel.
#[derive(Clone, Serialize)]
pub struct OpenTarget {
    pub entry: Option<String>,
    pub panel: Option<String>,
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    app.manage(PanelState::default());
    let open = MenuItem::with_id(
        app,
        "open",
        tr("Brainmatter öffnen", "Open Brainmatter"),
        true,
        None::<&str>,
    )?;
    let lock =
        MenuItem::with_id(app, "lock", tr("Sperren", "Lock"), true, Some("CmdOrCtrl+Shift+L"))?;
    let quit = MenuItem::with_id(app, "quit", tr("Beenden", "Quit"), true, Some("CmdOrCtrl+Q"))?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &lock, &sep, &quit])?;

    TrayIconBuilder::with_id("main")
        // Monochrome template image: macOS tints it for light/dark menu bars.
        .icon(Image::from_bytes(include_bytes!("../icons/tray-template.png"))?)
        .icon_as_template(true)
        .tooltip("Brainmatter")
        .menu(&menu)
        // The left click opens the panel, the right click this menu.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => crate::show_main(app),
            "lock" => app.state::<Arc<Session>>().lock(),
            "quit" => {
                app.state::<Arc<Session>>().lock();
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                position,
                ..
            } = event
            {
                toggle_panel(tray.app_handle(), rect, position);
            }
        })
        .build(app)?;
    Ok(())
}

/// Brainmatter shows in the Dock and in ⌘Tab only while its window is open.
pub fn show_in_dock(app: &AppHandle, show: bool) {
    #[cfg(target_os = "macos")]
    {
        let policy = if show {
            tauri::ActivationPolicy::Regular
        } else {
            tauri::ActivationPolicy::Accessory
        };
        if let Err(e) = app.set_activation_policy(policy) {
            tracing::warn!("activation policy: {e}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, show);
}

fn toggle_panel(app: &AppHandle, icon: Rect, click: PhysicalPosition<f64>) {
    let existing = app.get_webview_window(PANEL_WINDOW);
    if existing.as_ref().is_some_and(|w| w.is_visible().unwrap_or(false)) {
        hide_panel(app, true);
        return;
    }
    let state = app.state::<PanelState>();
    if state.hidden_at.lock().is_some_and(|at| at.elapsed() < REOPEN_GUARD) {
        return;
    }
    state.opened.fetch_add(1, Ordering::Relaxed);
    let window = match existing {
        Some(w) => w,
        // Built on first use: most of the time nobody looks at it.
        None => {
            let theme = crate::tauri_theme(&app.state::<Arc<Session>>().settings());
            let built =
                WebviewWindowBuilder::new(app, PANEL_WINDOW, WebviewUrl::App("index.html".into()))
                    .title("Brainmatter")
                    .inner_size(PANEL_WIDTH, PANEL_HEIGHT)
                    .decorations(false)
                    .transparent(true)
                    .always_on_top(true)
                    .resizable(false)
                    .skip_taskbar(true)
                    .shadow(false)
                    .visible(false)
                    .visible_on_all_workspaces(true)
                    .theme(theme)
                    .build();
            match built {
                Ok(w) => w,
                Err(e) => {
                    tracing::error!("menu bar panel failed: {e}");
                    return;
                }
            }
        }
    };
    place(app, &window, icon, click);
    let _ = window.show();
    let _ = window.set_focus();
    let _ = window.emit("bm:tray-open", ());
}

/// Under the icon, kept on the screen it is on.
fn place(app: &AppHandle, window: &WebviewWindow, icon: Rect, click: PhysicalPosition<f64>) {
    let monitor = app.monitor_from_point(click.x, click.y).ok().flatten();
    let scale = monitor.as_ref().map_or(1.0, |m| m.scale_factor());
    let at = icon.position.to_physical::<f64>(scale);
    let size = icon.size.to_physical::<f64>(scale);
    let width = PANEL_WIDTH * scale;
    let mut x = at.x + size.width / 2.0 - width / 2.0;
    let y = at.y + size.height;
    if let Some(m) = monitor {
        let left = f64::from(m.position().x);
        let right = left + f64::from(m.size().width);
        x = x.clamp(left, (right - width).max(left));
    }
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

/// Closes the panel. `give_back` returns the focus to the app used before, unless
/// the main window is open; when the panel lost the focus, it is already there.
pub fn hide_panel(app: &AppHandle, give_back: bool) {
    let Some(window) = app.get_webview_window(PANEL_WINDOW) else { return };
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let _ = window.hide();
    let state = app.state::<PanelState>();
    *state.hidden_at.lock() = Some(Instant::now());
    let opened = state.opened.load(Ordering::Relaxed);
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(KEEP_HIDDEN).await;
        if handle.state::<PanelState>().opened.load(Ordering::Relaxed) != opened {
            return;
        }
        if let Some(w) = handle.get_webview_window(PANEL_WINDOW)
            && !w.is_visible().unwrap_or(true)
        {
            let _ = w.destroy();
        }
    });
    let main_open = app
        .get_webview_window("main")
        .is_some_and(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false));
    if give_back && !main_open {
        let _ = app.hide();
    }
}

/// Shows the main window with what the panel pointed at.
pub fn open_in_main(app: &AppHandle, target: OpenTarget) {
    hide_panel(app, false);
    crate::show_main(app);
    let _ = app.emit_to("main", "bm:open", target);
}
