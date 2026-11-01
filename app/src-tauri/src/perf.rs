//! Measuring the real app: `app/scripts/perf-app.sh` runs a build with
//! debug assertions against an invented vault and puts a script into its window.
//! None of this exists in a release build.

use std::{io::Write, path::PathBuf};

use tauri::{
    AppHandle, Listener, Runtime, Webview, webview::PageLoadEvent, webview::PageLoadPayload,
};

fn path(name: &str) -> Option<PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var_os(name).map(PathBuf::from)
}

pub fn script() -> Option<PathBuf> {
    path("BM_PERF_SCRIPT")
}

pub fn vault_dir() -> Option<PathBuf> {
    script().and(path("BM_PERF_VAULT"))
}

/// Writes what the script reports to a file, one line per event.
pub fn attach<R: Runtime>(app: &AppHandle<R>) {
    let Some(out) = script().and(path("BM_PERF_OUT")) else { return };
    app.listen("bm-perf", move |event| {
        let line = serde_json::from_str::<String>(event.payload()).unwrap_or_default();
        if let Ok(mut file) = std::fs::OpenOptions::new().append(true).create(true).open(&out) {
            let _ = writeln!(file, "{line}");
        }
    });
}

pub fn run_script<R: Runtime>(webview: &Webview<R>, payload: &PageLoadPayload<'_>) {
    if payload.event() != PageLoadEvent::Finished || webview.label() != "main" {
        return;
    }
    let Some(script) = script().and_then(|p| std::fs::read_to_string(p).ok()) else { return };
    // The most there is to draw.
    let _ = webview.window().maximize();
    let _ = webview.eval(script);
}
