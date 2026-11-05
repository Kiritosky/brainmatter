//! Tauri commands. Session commands that hit Argon2 or Touch ID run on a
//! blocking thread; data commands run off the main thread via `async`.

use std::sync::Arc;

use bm_core::{
    backups::BackupInfo,
    ids::{AttachmentId, ChangeSetId, DocSourceId, FileLinkId, NodeId, ProjectId},
    model::*,
    templates::slug,
};
use bm_mcp::{Client, ClientRegistry, RegistryError, Scope};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::i18n::{german, tr};
use crate::{
    McpStatus,
    error::{ApiError, ApiResult},
    github::Github,
    integrations::{self as setup, Integrations, Target},
    session::{BackupStatus, ModelStatus, Session, Settings, Status},
    skill_import::{self, SkillImportPreview},
    skill_menu::SkillMenus,
    updates::AvailableUpdate,
};

type S<'a> = State<'a, Arc<Session>>;

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> ApiResult<T> + Send + 'static,
) -> ApiResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(ApiError::internal)?
}

// ------------------------------------------------------------- session ---

#[tauri::command]
pub fn session_status(session: S<'_>) -> Status {
    session.status()
}

#[tauri::command]
pub async fn create_vault(session: S<'_>, password: String, name: String) -> ApiResult<String> {
    let session = session.inner().clone();
    blocking(move || session.create(&password, &name)).await
}

/// Makes the vault from a copy of the backups, as on a new Mac. Asks for the folder;
/// returns `false` if the user cancels.
#[tauri::command]
pub async fn adopt_backup(
    app: tauri::AppHandle,
    session: S<'_>,
    password: String,
) -> ApiResult<bool> {
    let session = session.inner().clone();
    blocking(move || {
        let Some(folder) = app.dialog().file().blocking_pick_folder() else { return Ok(false) };
        let path = folder.into_path().map_err(ApiError::internal)?;
        session.adopt_backup(&path, &password)?;
        Ok(true)
    })
    .await
}

#[tauri::command]
pub async fn unlock_password(session: S<'_>, password: String) -> ApiResult<()> {
    let session = session.inner().clone();
    blocking(move || session.unlock_password(&password)).await
}

#[tauri::command]
pub async fn unlock_recovery(session: S<'_>, key: String) -> ApiResult<()> {
    let session = session.inner().clone();
    blocking(move || session.unlock_recovery(&key)).await
}

#[tauri::command]
pub async fn unlock_touch_id(session: S<'_>) -> ApiResult<()> {
    let session = session.inner().clone();
    blocking(move || session.unlock_touch_id(tr("Brainmatter entsperren", "unlock Brainmatter")))
        .await
}

#[tauri::command]
pub async fn enroll_touch_id(session: S<'_>) -> ApiResult<()> {
    let session = session.inner().clone();
    blocking(move || session.enroll_touch_id()).await
}

#[tauri::command(async)]
pub fn disable_touch_id(session: S<'_>) -> ApiResult<()> {
    session.disable_touch_id()
}

#[tauri::command]
pub async fn change_password(session: S<'_>, current: String, password: String) -> ApiResult<()> {
    let session = session.inner().clone();
    blocking(move || session.change_password(&current, &password)).await
}

#[tauri::command]
pub async fn rotate_recovery_key(session: S<'_>, current: String) -> ApiResult<String> {
    let session = session.inner().clone();
    blocking(move || session.rotate_recovery_key(&current)).await
}

#[tauri::command]
pub fn lock(session: S<'_>) {
    session.lock();
}

#[tauri::command]
pub fn get_settings(session: S<'_>) -> Settings {
    session.settings()
}

#[tauri::command]
pub fn update_settings(
    app: tauri::AppHandle,
    session: S<'_>,
    settings: Settings,
) -> ApiResult<Settings> {
    let settings = session.update_settings(settings)?;
    crate::apply_settings(&app, &settings);
    Ok(settings)
}

#[tauri::command]
pub fn app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
pub async fn check_update(app: tauri::AppHandle) -> ApiResult<Option<AvailableUpdate>> {
    crate::updates::check(&app).await
}

#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> ApiResult<()> {
    crate::updates::install(&app).await
}

/// Closes the quick capture window and gives the focus back.
#[tauri::command]
pub fn hide_capture(app: tauri::AppHandle) {
    crate::hide_capture(&app);
}

/// Called from quick capture when the vault needs more than Touch ID.
#[tauri::command]
pub fn show_main_window(app: tauri::AppHandle) {
    crate::show_main(&app);
}

/// Closes the menu bar panel; see [`crate::tray::hide_panel`].
#[tauri::command]
pub fn hide_tray(app: tauri::AppHandle, give_back: bool) {
    crate::tray::hide_panel(&app, give_back);
}

/// From the menu bar panel: the main window, with an entry or a panel open.
#[tauri::command]
pub fn open_in_main(app: tauri::AppHandle, entry: Option<String>, panel: Option<String>) {
    crate::tray::open_in_main(&app, crate::tray::OpenTarget { entry, panel });
}

/// From the menu bar panel, which makes room for quick capture.
#[tauri::command]
pub fn toggle_capture(app: tauri::AppHandle) {
    crate::tray::hide_panel(&app, false);
    crate::toggle_capture(&app);
}

/// Called by the UI on user input so auto-lock measures real idleness.
#[tauri::command]
pub fn touch(session: S<'_>) {
    let _ = session.engine();
}

// ---------------------------------------------------------------- data ---

#[tauri::command(async)]
pub fn projects(session: S<'_>) -> ApiResult<Vec<ProjectSummary>> {
    let (engine, _) = session.engine()?;
    Ok(engine.projects()?)
}

#[tauri::command(async)]
pub fn create_project(session: S<'_>, input: NewProject) -> ApiResult<Project> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.create_project(&ctx, input)?)
}

#[tauri::command(async)]
pub fn rename_project(session: S<'_>, id: ProjectId, name: String) -> ApiResult<Project> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.update_project(&ctx, &id, Some(&name), None)?)
}

#[tauri::command(async)]
pub fn move_project(
    session: S<'_>,
    id: ProjectId,
    parent: Option<ProjectId>,
    position: usize,
) -> ApiResult<()> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.place_project(&ctx, &id, parent.as_ref(), position)?)
}

#[tauri::command(async)]
pub fn project_delete_preview(session: S<'_>, id: ProjectId) -> ApiResult<ProjectDeletePreview> {
    let (engine, _) = session.engine()?;
    Ok(engine.project_delete_preview(&id)?)
}

#[tauri::command(async)]
pub fn delete_project(session: S<'_>, id: ProjectId, with_entries: bool) -> ApiResult<usize> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.delete_project(&ctx, &id, with_entries)?)
}

#[tauri::command(async)]
pub fn add_to_project(session: S<'_>, project: ProjectId, nodes: Vec<NodeId>) -> ApiResult<usize> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.add_to_project(&ctx, &project, &nodes)?)
}

#[tauri::command(async)]
pub fn remove_from_project(
    session: S<'_>,
    project: ProjectId,
    nodes: Vec<NodeId>,
) -> ApiResult<usize> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.remove_from_project(&ctx, &project, &nodes)?)
}

/// As binary (`bm_core::wire`), since JSON is ten times the size for a large graph.
#[tauri::command(async)]
pub fn graph(session: S<'_>, project: Option<ProjectId>) -> ApiResult<tauri::ipc::Response> {
    let (engine, _) = session.engine()?;
    let snapshot = engine.graph_snapshot(project.as_ref())?;
    Ok(tauri::ipc::Response::new(bm_core::wire::encode(&snapshot)))
}

/// What changed of a view since it was loaded, in the same form as [`graph`].
#[tauri::command(async)]
pub fn graph_part(
    session: S<'_>,
    project: Option<ProjectId>,
    ids: Vec<NodeId>,
) -> ApiResult<tauri::ipc::Response> {
    let (engine, _) = session.engine()?;
    let part = engine.graph_part(project.as_ref(), &ids)?;
    Ok(tauri::ipc::Response::new(bm_core::wire::encode(&part)))
}

#[tauri::command(async)]
pub fn save_layout(session: S<'_>, view: String, positions: Vec<LayoutPosition>) -> ApiResult<()> {
    let (engine, _) = session.engine()?;
    Ok(engine.save_layout(&view, &positions)?)
}

#[tauri::command(async)]
pub fn node_detail(session: S<'_>, id: NodeId) -> ApiResult<NodeDetail> {
    let (engine, _) = session.engine()?;
    Ok(engine.node_detail(&id)?)
}

#[tauri::command(async)]
pub fn search(session: S<'_>, query: SearchQuery) -> ApiResult<Vec<SearchHit>> {
    let (engine, _) = session.engine()?;
    Ok(engine.search_hybrid_with(&query, || session.query_vector(&query.text))?)
}

#[tauri::command(async)]
pub fn model_status(session: S<'_>) -> ModelStatus {
    session.model_status()
}

#[tauri::command(async)]
pub fn download_model(session: S<'_>) -> ApiResult<()> {
    require_unlocked(&session)?;
    session.download_model();
    Ok(())
}

#[tauri::command(async)]
pub fn remove_model(session: S<'_>) -> ApiResult<()> {
    require_unlocked(&session)?;
    session.remove_model()
}

#[tauri::command(async)]
pub fn link_suggestions(session: S<'_>, id: NodeId) -> ApiResult<Vec<Similar>> {
    let (engine, _) = session.engine()?;
    Ok(engine.link_suggestions(&id)?)
}

#[tauri::command(async)]
pub fn dismiss_suggestion(session: S<'_>, a: NodeId, b: NodeId) -> ApiResult<()> {
    let (engine, _) = session.engine()?;
    Ok(engine.dismiss_suggestion(&a, &b)?)
}

#[tauri::command(async)]
pub fn accept_suggestion(session: S<'_>, src: NodeId, dst: NodeId) -> ApiResult<Edge> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.accept_suggestion(&ctx, &src, &dst, None)?)
}

#[tauri::command(async)]
pub fn merge_nodes(session: S<'_>, keep: NodeId, others: Vec<NodeId>) -> ApiResult<Node> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.merge_nodes(&ctx, &keep, &others)?)
}

/// What precedes the file in the body of `add_attachment`.
#[derive(Deserialize)]
struct AttachmentMeta {
    node: NodeId,
    filename: String,
    mime: Option<String>,
}

/// Body: JSON length (u32, little endian), JSON, file. Files travel as raw bytes
/// because JSON arrays would take seconds at this size, and file names as JSON
/// because headers cannot carry umlauts.
fn parse_attachment(body: &[u8]) -> Option<(AttachmentMeta, &[u8])> {
    let (len, rest) = body.split_first_chunk::<4>()?;
    let len = u32::from_le_bytes(*len) as usize;
    if len > rest.len() {
        return None;
    }
    let (meta, file) = rest.split_at(len);
    Some((serde_json::from_slice(meta).ok()?, file))
}

#[tauri::command(async)]
pub fn add_attachment(session: S<'_>, request: tauri::ipc::Request<'_>) -> ApiResult<Attachment> {
    let (engine, ctx) = session.engine()?;
    let tauri::ipc::InvokeBody::Raw(body) = request.body() else {
        return Err(ApiError::new(
            "invalid",
            tr("Die Datei kam nicht als Binärdaten an.", "The file did not arrive as binary data."),
        ));
    };
    let (meta, file) = parse_attachment(body).ok_or_else(|| {
        ApiError::new(
            "invalid",
            tr("Die Datei kam unvollständig an.", "The file arrived incomplete."),
        )
    })?;
    Ok(engine.add_attachment(&ctx, &meta.node, &meta.filename, meta.mime.as_deref(), file)?)
}

#[tauri::command(async)]
pub fn attachment_data(session: S<'_>, id: AttachmentId) -> ApiResult<tauri::ipc::Response> {
    let (engine, _) = session.engine()?;
    Ok(tauri::ipc::Response::new(engine.attachment_data(&id)?.1))
}

#[tauri::command(async)]
pub fn delete_attachment(session: S<'_>, id: AttachmentId) -> ApiResult<()> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.delete_attachment(&ctx, &id)?)
}

/// Saves a decrypted copy where the user chooses. Returns false if they cancel.
#[tauri::command(async)]
pub fn export_attachment(
    app: tauri::AppHandle,
    session: S<'_>,
    id: AttachmentId,
) -> ApiResult<bool> {
    let (engine, _) = session.engine()?;
    let a = engine.attachment(&id)?;
    let Some(target) = app.dialog().file().set_file_name(&a.filename).blocking_save_file() else {
        return Ok(false);
    };
    let path = target.into_path().map_err(ApiError::internal)?;
    std::fs::write(path, engine.attachment_data(&id)?.1).map_err(ApiError::internal)?;
    Ok(true)
}

#[tauri::command(async)]
pub fn orphans(session: S<'_>, limit: usize) -> ApiResult<Vec<NodeBrief>> {
    let (engine, _) = session.engine()?;
    Ok(engine.orphans(limit.min(500))?)
}

#[tauri::command(async)]
pub fn unreviewed(session: S<'_>, limit: usize) -> ApiResult<Vec<NodeBrief>> {
    let (engine, _) = session.engine()?;
    Ok(engine.unreviewed(limit.min(500))?)
}

/// A link as the inspector shows it, checked against the disk.
#[derive(Serialize)]
pub struct FileLinkStatus {
    #[serde(flatten)]
    link: FileLink,
    exists: bool,
    is_dir: bool,
}

/// The entry's links, following files that moved. An AI's link was made without
/// touching the disk, and only gets a bookmark once it is approved.
#[tauri::command(async)]
pub fn file_links(session: S<'_>, node: NodeId) -> ApiResult<Vec<FileLinkStatus>> {
    let (engine, _) = session.engine()?;
    let mut out = Vec::new();
    for mut link in engine.node_detail(&node)?.file_links {
        let bookmark = engine.file_link_bookmark(&link.id)?;
        let moved = bookmark.as_deref().and_then(bm_macos::bookmarks::resolve);
        if let Some(path) = moved.as_ref().and_then(|p| p.to_str())
            && path != link.path
        {
            engine.set_file_link_location(&link.id, path, None)?;
            link.path = path.to_string();
        }
        let meta = std::fs::metadata(&link.path);
        let is_dir = meta.as_ref().is_ok_and(|m| m.is_dir());
        out.push(FileLinkStatus { exists: meta.is_ok(), is_dir, link });
    }
    Ok(out)
}

/// Asks which files or folders to link. Returns how many were linked.
#[tauri::command(async)]
pub fn link_files(
    app: tauri::AppHandle,
    session: S<'_>,
    node: NodeId,
    folders: bool,
) -> ApiResult<usize> {
    let (engine, ctx) = session.engine()?;
    let dialog = app.dialog().file();
    let picked =
        if folders { dialog.blocking_pick_folders() } else { dialog.blocking_pick_files() };
    let mut linked = 0;
    for target in picked.unwrap_or_default() {
        let path = target.into_path().map_err(ApiError::internal)?;
        let text = path.to_str().ok_or_else(|| {
            ApiError::new(
                "invalid",
                tr("Dieser Dateiname wird nicht unterstützt.", "This file name is not supported."),
            )
        })?;
        let bookmark = bm_macos::bookmarks::create(&path);
        engine.add_file_link(&ctx, &node, text, None, bookmark.as_deref())?;
        linked += 1;
    }
    Ok(linked)
}

#[tauri::command(async)]
pub fn set_file_link_approved(session: S<'_>, id: FileLinkId, approved: bool) -> ApiResult<()> {
    let (engine, ctx) = session.engine()?;
    // Approving is the first time this app looks at a path an AI suggested.
    let link = engine.file_link(&id)?;
    if approved && engine.file_link_bookmark(&id)?.is_none() {
        let bookmark = bm_macos::bookmarks::create(std::path::Path::new(&link.path));
        engine.set_file_link_location(&id, &link.path, bookmark.as_deref())?;
    }
    Ok(engine.set_file_link_approved(&ctx, &id, approved)?)
}

#[tauri::command(async)]
pub fn delete_file_link(session: S<'_>, id: FileLinkId) -> ApiResult<()> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.delete_file_link(&ctx, &id)?)
}

fn run_open(args: &[&std::ffi::OsStr]) -> ApiResult<()> {
    let status = std::process::Command::new("/usr/bin/open").args(args).status();
    match status {
        Ok(s) if s.success() => Ok(()),
        _ => Err(ApiError::new(
            "not_found",
            tr("Die Datei wurde nicht gefunden.", "The file was not found."),
        )),
    }
}

#[tauri::command(async)]
pub fn reveal_file(session: S<'_>, id: FileLinkId) -> ApiResult<()> {
    let (engine, _) = session.engine()?;
    let path = engine.file_link(&id)?.path;
    run_open(&["-R".as_ref(), path.as_ref()])
}

/// Opens the file with its default app, which for a script or program means
/// running it. So a path an AI suggested can only be shown in the Finder until the
/// user has approved it.
#[tauri::command(async)]
pub fn open_file(session: S<'_>, id: FileLinkId) -> ApiResult<()> {
    let (engine, _) = session.engine()?;
    let link = engine.file_link(&id)?;
    if !link.approved_for_ai {
        return Err(ApiError::new(
            "invalid",
            "Diesen Pfad hat eine KI vorgeschlagen. Gib ihn erst frei oder zeige ihn im Finder.",
        ));
    }
    run_open(&[link.path.as_ref()])
}

#[tauri::command(async)]
pub fn favorites(session: S<'_>) -> ApiResult<Vec<NodeBrief>> {
    let (engine, _) = session.engine()?;
    Ok(engine.favorites()?)
}

#[tauri::command(async)]
pub fn set_favorite(session: S<'_>, id: NodeId, favorite: bool) -> ApiResult<()> {
    let (engine, _) = session.engine()?;
    Ok(engine.set_favorite(&id, favorite)?)
}

#[tauri::command(async)]
pub fn tag_counts(session: S<'_>) -> ApiResult<Vec<TagCount>> {
    let (engine, _) = session.engine()?;
    Ok(engine.tag_counts()?)
}

#[tauri::command(async)]
pub fn actors(session: S<'_>) -> ApiResult<Vec<Actor>> {
    let (engine, _) = session.engine()?;
    Ok(engine.actors()?)
}

#[tauri::command(async)]
pub fn doc_tree(session: S<'_>, project: ProjectId) -> ApiResult<Vec<DocPage>> {
    let (engine, _) = session.engine()?;
    Ok(engine.doc_tree(&project)?)
}

#[tauri::command(async)]
pub fn create_doc_page(
    session: S<'_>,
    project: ProjectId,
    title: String,
    parent: Option<NodeId>,
) -> ApiResult<Node> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.create_doc_page(&ctx, &project, &title, "", parent.as_ref(), Vec::new())?)
}

#[tauri::command(async)]
pub fn place_doc_page(
    session: S<'_>,
    id: NodeId,
    parent: Option<NodeId>,
    position: usize,
) -> ApiResult<()> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.place_doc_page(&ctx, &id, parent.as_ref(), position)?)
}

#[tauri::command(async)]
pub fn doc_sources(session: S<'_>, project: ProjectId) -> ApiResult<Vec<DocSource>> {
    let (engine, _) = session.engine()?;
    Ok(engine.doc_sources(&project)?)
}

/// Asks for a folder or Markdown file, and mirrors it into the project's docs.
/// `None` if the user cancels.
#[tauri::command(async)]
pub fn add_doc_source(
    app: tauri::AppHandle,
    session: S<'_>,
    project: ProjectId,
    folder: bool,
) -> ApiResult<Option<DocSyncReport>> {
    let (engine, ctx) = session.engine()?;
    let dialog = app.dialog().file();
    let picked = if folder {
        dialog.blocking_pick_folder()
    } else {
        dialog.add_filter("Markdown", &["md", "markdown", "mdx"]).blocking_pick_file()
    };
    let Some(picked) = picked else { return Ok(None) };
    let path = picked.into_path().map_err(ApiError::internal)?;
    let text = path.to_str().ok_or_else(|| {
        ApiError::new(
            "invalid",
            tr("Dieser Dateiname wird nicht unterstützt.", "This file name is not supported."),
        )
    })?;
    let bookmark = bm_macos::bookmarks::create(&path);
    let id = engine.add_doc_source(&ctx, &project, text, bookmark.as_deref())?;
    let report = engine.sync_doc_source(&ctx, &id, &path)?;
    drop(engine);
    session.watch_docs();
    Ok(Some(report))
}

/// Brings the project's mirrored pages up to date. Sources that cannot be reached
/// right now, such as on a disconnected drive, keep their pages and are named.
#[derive(Serialize)]
pub struct DocSyncStatus {
    changed: usize,
    skipped: usize,
    unreachable: Vec<String>,
}

#[tauri::command(async)]
pub fn sync_doc_sources(session: S<'_>, project: ProjectId) -> ApiResult<DocSyncStatus> {
    let (engine, ctx) = session.engine()?;
    let mut status = DocSyncStatus { changed: 0, skipped: 0, unreachable: Vec::new() };
    for source in engine.doc_sources(&project)? {
        let root = crate::docs_watch::source_root(&engine, &source.id)?;
        match engine.sync_doc_source(&ctx, &source.id, &root) {
            Ok(report) => {
                status.changed += report.added + report.updated + report.removed;
                status.skipped += report.skipped;
            }
            Err(e) => {
                tracing::info!("documentation source not reachable: {e}");
                status.unreachable.push(source.path);
            }
        }
    }
    Ok(status)
}

#[tauri::command(async)]
pub fn remove_doc_source(session: S<'_>, id: DocSourceId) -> ApiResult<()> {
    let (engine, ctx) = session.engine()?;
    engine.remove_doc_source(&ctx, &id)?;
    drop(engine);
    session.watch_docs();
    Ok(())
}

#[tauri::command(async)]
pub fn templates(session: S<'_>) -> ApiResult<Vec<TemplateInfo>> {
    let (engine, _) = session.engine()?;
    Ok(engine.templates(None)?)
}

#[tauri::command(async)]
pub fn skills(session: S<'_>) -> ApiResult<Vec<SkillInfo>> {
    let (engine, _) = session.engine()?;
    Ok(engine.skills(None)?)
}

/// Creates or changes a skill. The checks come first so they read in the app's language.
#[tauri::command(async)]
pub fn save_skill(session: S<'_>, input: SkillInput) -> ApiResult<SkillInfo> {
    let (engine, ctx) = session.engine()?;
    let name = slug(&input.title);
    if name.is_empty() {
        return Err(ApiError::new(
            "invalid",
            tr(
                "Der Skill braucht einen Namen mit Buchstaben oder Ziffern.",
                "The skill needs a name with letters or digits.",
            ),
        ));
    }
    if input.description.trim().is_empty() {
        return Err(ApiError::new(
            "invalid",
            tr(
                "Der Skill braucht eine Beschreibung. An ihr erkennt die KI, wann er passt.",
                "The skill needs a description. It tells the AI when the skill fits.",
            ),
        ));
    }
    let skills = engine.skills(None)?;
    let taken = skills.iter().find(|s| s.slug == name && Some(&s.node.id) != input.id.as_ref());
    if let Some(other) = taken {
        return Err(ApiError::new(
            "conflict",
            match german() {
                true => format!(
                    "Der Skill „{}“ hat schon den Namen {name}. Bitte einen anderen wählen.",
                    other.node.title
                ),
                false => format!(
                    "The skill “{}” has the name {name} already. Please choose another one.",
                    other.node.title
                ),
            },
        ));
    }
    Ok(engine.save_skill(&ctx, input)?)
}

/// Whether the skill goes into the skill folders of connected tools as it is, or
/// only as a pointer that loads it from the vault.
#[tauri::command(async)]
pub fn set_skill_on_disk(session: S<'_>, id: NodeId, on: bool) -> ApiResult<SkillInfo> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.set_skill_on_disk(&ctx, &id, on)?)
}

#[tauri::command]
pub async fn preview_skill_import(session: S<'_>, url: String) -> ApiResult<SkillImportPreview> {
    // A locked vault could not tell which skills exist; no need to ask GitHub then.
    session.engine()?;
    let mut preview = skill_import::preview(&Github::new()?, &url).await?;
    let (engine, _) = session.engine()?;
    skill_import::mark_existing(&mut preview, &engine)?;
    Ok(preview)
}

#[tauri::command]
pub async fn import_skills(
    session: S<'_>,
    url: String,
    revision: String,
    paths: Vec<String>,
    projects: Vec<ProjectId>,
) -> ApiResult<Vec<SkillInfo>> {
    session.engine()?;
    let fetched = skill_import::fetch(&Github::new()?, &url, &revision, &paths).await?;
    let session = session.inner().clone();
    // Encrypting up to a few megabytes per skill.
    blocking(move || {
        let (engine, ctx) = session.engine()?;
        Ok(engine.import_skills(&ctx, fetched, &projects)?)
    })
    .await
}

#[tauri::command(async)]
pub fn trash(session: S<'_>, limit: usize) -> ApiResult<Vec<TrashedNode>> {
    let (engine, _) = session.engine()?;
    Ok(engine.trash(limit.min(1000))?)
}

#[tauri::command(async)]
pub fn restore_node(session: S<'_>, id: NodeId) -> ApiResult<Node> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.restore_node(&ctx, &id)?)
}

#[tauri::command(async)]
pub fn purge_node(session: S<'_>, id: NodeId) -> ApiResult<()> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.purge_node(&ctx, &id)?)
}

#[tauri::command(async)]
pub fn create_node(session: S<'_>, input: NewNode) -> ApiResult<Node> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.create_node(&ctx, input)?)
}

#[tauri::command(async)]
pub fn update_node(session: S<'_>, id: NodeId, patch: NodePatch) -> ApiResult<Node> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.update_node(&ctx, &id, patch)?)
}

#[tauri::command(async)]
pub fn delete_node(session: S<'_>, id: NodeId) -> ApiResult<()> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.delete_node(&ctx, &id)?)
}

#[tauri::command(async)]
pub fn link(session: S<'_>, input: NewEdge) -> ApiResult<Edge> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.link(&ctx, input)?)
}

#[tauri::command(async)]
pub fn unlink(
    session: S<'_>,
    src: NodeId,
    dst: NodeId,
    relation: Option<String>,
) -> ApiResult<Vec<Edge>> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.unlink(&ctx, &src, &dst, relation.as_deref())?)
}

#[tauri::command(async)]
pub fn change_sets(session: S<'_>, limit: usize) -> ApiResult<Vec<ChangeSetInfo>> {
    let (engine, _) = session.engine()?;
    Ok(engine.change_sets(limit.min(500), None)?)
}

#[tauri::command(async)]
pub fn recent_changes(session: S<'_>, limit: usize) -> ApiResult<Vec<RecentChange>> {
    let (engine, _) = session.engine()?;
    Ok(engine.recent_changes(limit, None)?)
}

#[tauri::command(async)]
pub fn node_history(session: S<'_>, id: NodeId) -> ApiResult<Vec<NodeRevision>> {
    let (engine, _) = session.engine()?;
    Ok(engine.node_history(&id)?)
}

#[tauri::command(async)]
pub fn revert(session: S<'_>, change_set: ChangeSetId) -> ApiResult<RevertReport> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.revert(&ctx, &change_set)?)
}

// ------------------------------------------------------------- backups ---

#[tauri::command(async)]
pub fn backup_status(session: S<'_>) -> ApiResult<BackupStatus> {
    require_unlocked(&session)?;
    session.backup_status()
}

#[tauri::command]
pub async fn backup_now(session: S<'_>) -> ApiResult<BackupInfo> {
    let session = session.inner().clone();
    blocking(move || session.backup_now()).await
}

/// Asks for a second place for the backups, such as an external drive, and copies
/// there at once. Returns nothing if the user cancels.
#[tauri::command]
pub async fn choose_backup_copy(
    app: tauri::AppHandle,
    session: S<'_>,
) -> ApiResult<Option<BackupStatus>> {
    let session = session.inner().clone();
    blocking(move || {
        require_unlocked(&session)?;
        let Some(folder) = app.dialog().file().blocking_pick_folder() else { return Ok(None) };
        let path = folder.into_path().map_err(ApiError::internal)?;
        Ok(Some(session.set_backup_copy(Some(path))?))
    })
    .await
}

/// Stops copying to the second place; what is there stays.
#[tauri::command(async)]
pub fn remove_backup_copy(session: S<'_>) -> ApiResult<BackupStatus> {
    require_unlocked(&session)?;
    session.set_backup_copy(None)
}

#[tauri::command]
pub async fn restore_backup(session: S<'_>, name: String, password: String) -> ApiResult<()> {
    let session = session.inner().clone();
    blocking(move || session.restore_backup(&name, &password)).await
}

/// An unencrypted copy of everything, so this asks for the password even though the
/// vault is open. Returns nothing if the user cancels choosing a folder.
#[tauri::command]
pub async fn export_markdown(
    app: tauri::AppHandle,
    session: S<'_>,
    password: String,
) -> ApiResult<Option<bm_core::engine::ExportSummary>> {
    let session = session.inner().clone();
    blocking(move || {
        session.verify_password(&password)?;
        let Some(folder) = app.dialog().file().blocking_pick_folder() else { return Ok(None) };
        let path = folder.into_path().map_err(ApiError::internal)?;
        let (engine, _) = session.engine()?;
        Ok(Some(engine.export_markdown(&path)?))
    })
    .await
}

/// The folder the user chose to import from. Only that one can be imported, whatever
/// the web view asks for.
#[derive(Default)]
pub struct ImportFolder(std::sync::Mutex<Option<std::path::PathBuf>>);

/// Asks for a folder of Markdown notes and says what importing it would bring in.
/// Returns nothing if the user cancels.
#[tauri::command]
pub async fn import_preview(
    app: tauri::AppHandle,
    session: S<'_>,
) -> ApiResult<Option<bm_core::engine::ImportSummary>> {
    let session = session.inner().clone();
    blocking(move || {
        session.engine()?;
        let Some(folder) = app.dialog().file().blocking_pick_folder() else { return Ok(None) };
        let path = folder.into_path().map_err(ApiError::internal)?;
        let (engine, _) = session.engine()?;
        let summary = engine.import_preview(&path)?;
        *app.state::<ImportFolder>().0.lock().map_err(ApiError::internal)? = Some(path);
        Ok(Some(summary))
    })
    .await
}

/// Imports the folder of the last preview, in one change set.
#[tauri::command]
pub async fn import_markdown(
    app: tauri::AppHandle,
    session: S<'_>,
) -> ApiResult<bm_core::engine::ImportSummary> {
    let session = session.inner().clone();
    blocking(move || {
        let picked = app.state::<ImportFolder>().0.lock().map_err(ApiError::internal)?.take();
        let path = picked.ok_or_else(|| {
            ApiError::new(
                "invalid",
                tr("Bitte zuerst einen Ordner wählen.", "Please choose a folder first."),
            )
        })?;
        let (engine, ctx) = session.engine()?;
        Ok(engine.import_markdown(&ctx, &path)?)
    })
    .await
}

// ---------------------------------------------------------- AI clients ---

fn registry_error(e: RegistryError) -> ApiError {
    match e {
        RegistryError::DuplicateName(name) => ApiError::new(
            "invalid",
            match crate::i18n::german() {
                true => format!("Es gibt schon eine Verbindung namens „{name}“."),
                false => format!("There is a connection named “{name}” already."),
            },
        ),
        RegistryError::EmptyName => {
            ApiError::new("invalid", tr("Bitte einen Namen angeben.", "Please enter a name."))
        }
        RegistryError::NotFound => {
            ApiError::new("not_found", tr("Verbindung nicht gefunden.", "Connection not found."))
        }
        other => ApiError::internal(other),
    }
}

/// Managing access needs an unlocked vault, not just a running app.
fn require_unlocked(session: &Session) -> ApiResult<()> {
    if session.is_unlocked() { Ok(()) } else { Err(ApiError::locked()) }
}

fn bridge_path() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("brainmatter-mcp")))
        .unwrap_or_default()
}

pub(crate) fn local_integrations(status: &McpStatus) -> ApiResult<Integrations> {
    let home = std::env::var_os("HOME").ok_or_else(|| ApiError::internal("HOME is not set"))?;
    Ok(Integrations {
        home: home.into(),
        applications: "/Applications".into(),
        url: format!("http://{}/mcp", status.addr),
        bridge: bridge_path(),
        claude_config_dir: None,
    })
}

#[derive(Serialize)]
pub struct McpInfo {
    url: String,
    running: bool,
    error: Option<String>,
    /// Path of the stdio bridge for clients without HTTP support.
    bridge_path: String,
    bridge_exists: bool,
    clients: Vec<Client>,
    /// The memory rule to paste into a tool's own instructions, for tools that
    /// may write and for those that may only read.
    rule: String,
    rule_read_only: String,
}

#[tauri::command(async)]
pub fn mcp_info(
    session: S<'_>,
    registry: State<'_, Arc<ClientRegistry>>,
    status: State<'_, Arc<McpStatus>>,
) -> ApiResult<McpInfo> {
    require_unlocked(&session)?;
    let bridge = bridge_path();
    let error = status.error.lock().clone();
    Ok(McpInfo {
        url: format!("http://{}/mcp", status.addr),
        running: error.is_none(),
        error,
        bridge_exists: bridge.is_file(),
        bridge_path: bridge.display().to_string(),
        clients: registry.list(),
        rule: setup::rule_text(true),
        rule_read_only: setup::rule_text(false),
    })
}

#[tauri::command(async)]
pub fn integrations(
    session: S<'_>,
    registry: State<'_, Arc<ClientRegistry>>,
    status: State<'_, Arc<McpStatus>>,
    menus: State<'_, Arc<SkillMenus>>,
) -> ApiResult<Vec<setup::Status>> {
    require_unlocked(&session)?;
    let local = local_integrations(&status)?;
    let mut all: Vec<setup::Status> = Target::ALL.into_iter().map(|t| local.status(t)).collect();
    menus.annotate(&mut all, &registry.list(), &local);
    Ok(all)
}

/// Brings the skill folders in line right away, so the settings show the result.
fn sync_skill_menus(
    session: &Session,
    registry: &ClientRegistry,
    local: &Integrations,
    menus: &SkillMenus,
) -> ApiResult<()> {
    let (engine, _) = session.engine()?;
    menus.sync(&engine, &registry.list(), local, false);
    Ok(())
}

/// Sets up an installed AI tool in one step. The token goes straight into its
/// configuration and is never shown. An existing connection of the same tool
/// keeps working until the new one is in place. With `memory`, the tool's own
/// instructions also tell it to use Brainmatter unasked; with `skill_menu`, its
/// skill folder holds Brainmatter's commands and skills.
#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
pub fn connect_integration(
    session: S<'_>,
    registry: State<'_, Arc<ClientRegistry>>,
    status: State<'_, Arc<McpStatus>>,
    menus: State<'_, Arc<SkillMenus>>,
    target: Target,
    scopes: Vec<Scope>,
    projects: Vec<ProjectId>,
    memory: bool,
    skill_menu: bool,
) -> ApiResult<Client> {
    require_unlocked(&session)?;
    let local = local_integrations(&status)?;
    let name = target.name();
    let pending = format!("{name} ({})", tr("wird eingerichtet", "being set up"));

    // A leftover from an interrupted attempt must not block this one.
    if let Some(stale) = registry.list().into_iter().find(|c| c.name == pending) {
        registry.revoke(&stale.id).map_err(registry_error)?;
    }
    let (client, token) = registry.create(&pending, &scopes, &projects).map_err(registry_error)?;
    if let Err(e) = local.configure(target, &token) {
        let _ = registry.revoke(&client.id);
        return Err(ApiError::new("setup_failed", e.to_string()));
    }
    if let Some(old) = registry.list().into_iter().find(|c| c.name == name) {
        registry.revoke(&old.id).map_err(registry_error)?;
    }
    registry.rename(&client.id, name).map_err(registry_error)?;
    // The connection stands either way; the settings show whether the rule is in.
    let rule = memory.then(|| setup::rule_text(scopes.contains(&Scope::Write)));
    if let Err(e) = local.set_rule(target, rule.as_deref()) {
        tracing::warn!("memory rule for {name}: {e}");
    }
    let skill_menu = skill_menu && local.skills_dir(target).is_some();
    match menus.set(target, skill_menu) {
        Ok(()) => sync_skill_menus(&session, &registry, &local, &menus)?,
        Err(e) => tracing::warn!("skill menu for {name}: {}", e.kind()),
    }
    Ok(Client { name: name.to_string(), ..client })
}

/// Puts Brainmatter's commands and skills into a connected tool's skill folder,
/// or takes them out again.
#[tauri::command(async)]
pub fn set_skill_menu(
    session: S<'_>,
    registry: State<'_, Arc<ClientRegistry>>,
    status: State<'_, Arc<McpStatus>>,
    menus: State<'_, Arc<SkillMenus>>,
    target: Target,
    on: bool,
) -> ApiResult<()> {
    require_unlocked(&session)?;
    let local = local_integrations(&status)?;
    menus.set(target, on && local.skills_dir(target).is_some()).map_err(ApiError::internal)?;
    sync_skill_menus(&session, &registry, &local, &menus)
}

/// Puts the memory rule into a connected tool's instructions or takes it out.
#[tauri::command(async)]
pub fn set_memory_rule(
    session: S<'_>,
    registry: State<'_, Arc<ClientRegistry>>,
    status: State<'_, Arc<McpStatus>>,
    target: Target,
    on: bool,
) -> ApiResult<()> {
    require_unlocked(&session)?;
    let write = registry
        .list()
        .into_iter()
        .find(|c| c.name == target.name())
        .is_none_or(|c| c.scopes.contains(&Scope::Write));
    let rule = on.then(|| setup::rule_text(write));
    local_integrations(&status)?
        .set_rule(target, rule.as_deref())
        .map_err(|e| ApiError::new("setup_failed", e.to_string()))
}

#[derive(Serialize)]
pub struct CreatedClient {
    client: Client,
    /// Shown exactly once; only its hash is stored.
    token: String,
}

#[tauri::command(async)]
pub fn create_client(
    session: S<'_>,
    registry: State<'_, Arc<ClientRegistry>>,
    name: String,
    scopes: Vec<Scope>,
    projects: Vec<ProjectId>,
) -> ApiResult<CreatedClient> {
    require_unlocked(&session)?;
    let (client, token) = registry.create(&name, &scopes, &projects).map_err(registry_error)?;
    Ok(CreatedClient { client, token })
}

#[tauri::command(async)]
pub fn revoke_client(
    session: S<'_>,
    registry: State<'_, Arc<ClientRegistry>>,
    status: State<'_, Arc<McpStatus>>,
    menus: State<'_, Arc<SkillMenus>>,
    id: String,
) -> ApiResult<()> {
    require_unlocked(&session)?;
    let name = registry.list().into_iter().find(|c| c.id == id).map(|c| c.name);
    registry.revoke(&id).map_err(registry_error)?;
    let Some(target) = name.as_deref().and_then(Target::from_name) else { return Ok(()) };
    // The token is dead either way; a leftover entry would only cause errors
    // in that tool. Failing to tidy up must not fail the disconnect.
    let local = local_integrations(&status)?;
    if let Err(e) = local.remove(target) {
        tracing::warn!("could not remove {} configuration: {e}", target.name());
    }
    if let Err(e) = menus.set(target, false) {
        tracing::warn!("skill menu for {}: {}", target.name(), e.kind());
    }
    sync_skill_menus(&session, &registry, &local, &menus)
}
