//! Updates from GitHub Releases. Updates are signed with a key of this project,
//! independently of macOS code signing, and the updater rejects anything else.
//!
//! Checking is a request to GitHub and can be turned off in the settings.
//! Debug builds never check: they are not what the releases replace.

use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use bm_core::time::now_ms;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::{
    error::{ApiError, ApiResult},
    session::Session,
};

const CHECK_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;
const PROGRESS_INTERVAL_MS: i64 = 150;

#[derive(Debug, Clone, Serialize)]
pub struct AvailableUpdate {
    pub version: String,
    pub notes: Option<String>,
}

async fn find(app: &AppHandle) -> ApiResult<Option<tauri_plugin_updater::Update>> {
    if cfg!(debug_assertions) {
        return Ok(None);
    }
    let failed = |e: tauri_plugin_updater::Error| {
        ApiError::new("update_failed", format!("Update-Prüfung fehlgeschlagen: {e}"))
    };
    app.updater().map_err(failed)?.check().await.map_err(failed)
}

pub async fn check(app: &AppHandle) -> ApiResult<Option<AvailableUpdate>> {
    let found = find(app).await?;
    Ok(found.map(|u| AvailableUpdate { version: u.version.clone(), notes: u.body.clone() }))
}

/// Installs the newest update and restarts. The vault locks first, so keys do
/// not outlive the process any longer than needed.
pub async fn install(app: &AppHandle) -> ApiResult<()> {
    let update = find(app)
        .await?
        .ok_or_else(|| ApiError::new("update_failed", "Es gibt kein neues Update."))?;
    let emitter = app.clone();
    let done = Arc::new(AtomicI64::new(0));
    let last = AtomicI64::new(0);
    update
        .download_and_install(
            move |chunk, total| {
                let bytes = done.fetch_add(chunk as i64, Ordering::Relaxed) + chunk as i64;
                let now = now_ms();
                if now - last.load(Ordering::Relaxed) >= PROGRESS_INTERVAL_MS {
                    last.store(now, Ordering::Relaxed);
                    let payload = serde_json::json!({ "downloaded": bytes, "total": total });
                    let _ = emitter.emit("bm:update-progress", payload);
                }
            },
            || {},
        )
        .await
        .map_err(|e| ApiError::new("update_failed", format!("Update fehlgeschlagen: {e}")))?;
    app.state::<Arc<Session>>().lock();
    app.restart();
}

/// Called periodically: looks for an update once a day and tells the UI.
pub fn maybe_check(app: &AppHandle, last_check: &AtomicI64) {
    let session = app.state::<Arc<Session>>();
    if !session.settings().auto_update
        || now_ms() - last_check.load(Ordering::Relaxed) < CHECK_INTERVAL_MS
    {
        return;
    }
    last_check.store(now_ms(), Ordering::Relaxed);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match check(&app).await {
            Ok(Some(update)) => {
                let _ = app.emit("bm:update-available", update);
            }
            Ok(None) => {}
            // Being offline is normal, and the next check comes tomorrow.
            Err(e) => tracing::info!("{}", e.message),
        }
    });
}
