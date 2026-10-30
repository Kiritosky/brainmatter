//! Tauri commands. Session commands that hit Argon2 or Touch ID run on a
//! blocking thread; data commands run off the main thread via `async`.

use std::sync::Arc;

use bm_core::{
    ids::{ChangeSetId, NodeId, ProjectId},
    model::*,
};
use tauri::State;

use crate::{
    error::{ApiError, ApiResult},
    session::{Session, Settings, Status},
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
    blocking(move || session.unlock_touch_id("Brainmatter entsperren")).await
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
pub fn update_settings(session: S<'_>, settings: Settings) -> ApiResult<Settings> {
    session.update_settings(settings)
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

#[tauri::command(async)]
pub fn graph(session: S<'_>, project: Option<ProjectId>) -> ApiResult<GraphSnapshot> {
    let (engine, _) = session.engine()?;
    Ok(engine.graph_snapshot(project.as_ref())?)
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
    Ok(engine.search(&query)?)
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
pub fn node_history(session: S<'_>, id: NodeId) -> ApiResult<Vec<NodeRevision>> {
    let (engine, _) = session.engine()?;
    Ok(engine.node_history(&id)?)
}

#[tauri::command(async)]
pub fn revert(session: S<'_>, change_set: ChangeSetId) -> ApiResult<RevertReport> {
    let (engine, ctx) = session.engine()?;
    Ok(engine.revert(&ctx, &change_set)?)
}
