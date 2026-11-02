//! Keeps mirrored documentation in step with its files while the vault is open: a
//! save in the editor shows up in the app, and to AI clients, about a second later.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use bm_core::{Engine, ids::DocSourceId, model::Ctx};
use bm_macos::watch::Watcher;

/// Long enough to take a save that writes several files as one change.
const LATENCY: Duration = Duration::from_millis(700);

/// Where a source is now. A bookmark follows a folder that was moved or renamed, and
/// the new place is remembered.
pub fn source_root(engine: &Engine, id: &DocSourceId) -> bm_core::Result<PathBuf> {
    let (path, bookmark) = engine.doc_source_location(id)?;
    let moved = bookmark.as_deref().and_then(bm_macos::bookmarks::resolve);
    if let Some(now) = moved.as_ref().and_then(|p| p.to_str())
        && now != path
    {
        engine.set_doc_source_path(id, now)?;
    }
    Ok(moved.unwrap_or_else(|| path.into()))
}

/// Watches every source that can be reached now; `None` if there is none. The
/// watcher holds the engine only while it syncs.
pub fn start(engine: &Arc<Engine>, ctx: Ctx) -> Option<Watcher> {
    let sources = engine.all_doc_sources().ok()?;
    let mut roots: Vec<(DocSourceId, PathBuf)> = Vec::new();
    for source in sources {
        // FSEvents reports real paths, without links such as /var → /private/var.
        if let Ok(root) = source_root(engine, &source.id).and_then(|r| Ok(r.canonicalize()?)) {
            roots.push((source.id, root));
        }
    }
    let mut folders: Vec<PathBuf> = roots
        .iter()
        .map(|(_, root)| match root.is_file() {
            true => root.parent().map_or_else(|| root.clone(), Path::to_path_buf),
            false => root.clone(),
        })
        .collect();
    folders.sort();
    folders.dedup();
    let engine = Arc::downgrade(engine);
    Watcher::new(&folders, LATENCY, move |changes| {
        let Some(engine) = engine.upgrade() else { return };
        for (id, root) in &roots {
            let touched = changes.iter().any(|c| {
                bm_core::docs::affects(root, &c.path, c.is_dir)
                    || (c.rescan && (c.path.starts_with(root) || root.starts_with(&c.path)))
            });
            if !touched {
                continue;
            }
            match engine.sync_doc_source(&ctx, id, root) {
                Ok(r) if r.added + r.updated + r.removed > 0 => tracing::info!(
                    "documentation followed its files: {} new, {} changed, {} gone",
                    r.added,
                    r.updated,
                    r.removed
                ),
                Ok(_) => {}
                Err(e) => tracing::info!("documentation source not reachable: {e}"),
            }
        }
    })
}
