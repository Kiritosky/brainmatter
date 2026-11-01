//! The engine: the single entry point for reading and changing the knowledge
//! graph. One writer connection serializes all changes (each call is one
//! change set); a small pool of read connections serves queries concurrently
//! thanks to WAL. Committed changes are broadcast as [`Event`]s.
//!
//! The API is synchronous; async callers should use `spawn_blocking`.

mod analytics;
mod attachments;
mod edges;
mod file_links;
mod graph;
mod history;
mod nodes;
mod projects;
mod refs;
mod rows;
mod search;
mod semantic;
mod writer;

use std::{
    collections::HashSet,
    path::Path,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
};

use parking_lot::{Mutex, RwLock};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use tokio::sync::broadcast;

use crate::{
    blobs::BlobStore,
    error::{CoreError, Result},
    events::Event,
    ids::{ActorId, AttachmentId, ChangeSetId, FileLinkId, NodeId, ProjectId},
    model::*,
    store::{Db, DbKey},
    time::now_ms,
};
use writer::Writer;

pub use semantic::{DUPLICATE_SCORE, Indexer, SUGGEST_SCORE, spawn_indexer};

const EVENT_BUFFER: usize = 1024;
const READER_CACHE_KIB: i64 = 32 * 1024;
/// Edits of the same entry closer together than this share one change set.
const COALESCE_WINDOW_MS: i64 = 5 * 60 * 1000;
/// Attachments are encrypted as a whole in memory, which sets the upper bound.
pub const MAX_ATTACHMENT_BYTES: usize = 200 * 1024 * 1024;

pub struct Engine {
    writer: Mutex<Connection>,
    readers: Vec<Mutex<Connection>>,
    next_reader: AtomicUsize,
    events: broadcast::Sender<Event>,
    vectors: RwLock<crate::semantic::VectorIndex>,
    /// Model of the loaded vectors, once semantic features are on.
    vector_model: RwLock<Option<String>>,
    /// Bumped by every committed change; the indexer watches it.
    generation: AtomicU64,
    blobs: Option<BlobStore>,
}

impl Engine {
    /// Opens the encrypted database (migrating it if needed) with `readers`
    /// additional read-only connections.
    pub fn open(path: &Path, key: &DbKey, readers: usize) -> Result<Self> {
        let writer = Db::open(path, key)?.into_conn();
        let readers = (0..readers.max(1))
            .map(|_| {
                let conn = Db::open(path, key)?.into_conn();
                conn.pragma_update(None, "query_only", "ON")?;
                // Decrypted pages stay in RAM only while unlocked; re-decrypting
                // them on every scan is what makes reads slow.
                conn.pragma_update(None, "cache_size", -READER_CACHE_KIB)?;
                Ok(Mutex::new(conn))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            writer: Mutex::new(writer),
            readers,
            next_reader: AtomicUsize::new(0),
            events: broadcast::channel(EVENT_BUFFER).0,
            vectors: RwLock::default(),
            vector_model: RwLock::new(None),
            generation: AtomicU64::new(0),
            blobs: None,
        })
    }

    /// Enables attachments.
    pub fn with_blobs(mut self, blobs: BlobStore) -> Self {
        self.blobs = Some(blobs);
        self
    }

    fn blob_store(&self) -> Result<&BlobStore> {
        self.blobs.as_ref().ok_or_else(|| CoreError::Invalid("attachments are not enabled".into()))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    // ------------------------------------------------------------ actors ---

    /// Returns the actor with this kind and name, creating it if needed.
    /// Actors are not versioned.
    pub fn ensure_actor(&self, kind: ActorKind, name: &str) -> Result<Actor> {
        let kind_str = match kind {
            ActorKind::User => "user",
            ActorKind::Ai => "ai",
        };
        let conn = self.writer.lock();
        let existing: Option<ActorId> = conn
            .query_row(
                "SELECT id FROM actors WHERE kind = ?1 AND name = ?2 AND revoked_at IS NULL",
                params![kind_str, name],
                |r| r.get(0),
            )
            .optional()?;
        let id = match existing {
            Some(id) => id,
            None => {
                let id = ActorId::new();
                conn.execute(
                    "INSERT INTO actors (id, kind, name, created_at) VALUES (?1, ?2, ?3, ?4)",
                    params![id, kind_str, name, now_ms()],
                )?;
                id
            }
        };
        drop(conn);
        self.actor(&id)?.ok_or_else(|| CoreError::not_found("actor", id.as_str()))
    }

    /// The vault owner: the first user actor ever created.
    pub fn primary_user(&self) -> Result<Option<Actor>> {
        let id: Option<ActorId> = self.read(|c| {
            Ok(c.query_row(
                "SELECT id FROM actors WHERE kind = 'user' ORDER BY created_at, id LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?)
        })?;
        id.map_or(Ok(None), |id| self.actor(&id))
    }

    pub fn actor(&self, id: &ActorId) -> Result<Option<Actor>> {
        self.read(|c| {
            Ok(c.query_row(
                "SELECT id, kind, name, color, created_at FROM actors WHERE id = ?1",
                [id],
                |r| {
                    let kind: String = r.get(1)?;
                    Ok(Actor {
                        id: r.get(0)?,
                        kind: if kind == "ai" { ActorKind::Ai } else { ActorKind::User },
                        name: r.get(2)?,
                        color: r.get(3)?,
                        created_at: r.get(4)?,
                    })
                },
            )
            .optional()?)
        })
    }

    // ------------------------------------------------------------- nodes ---

    pub fn create_node(&self, ctx: &Ctx, input: NewNode) -> Result<Node> {
        let summary = format!("„{}“ erstellt", input.title.trim());
        Ok(self.write(ctx, summary, |w| nodes::create(w, input))?.0)
    }

    pub fn update_node(&self, ctx: &Ctx, id: &NodeId, patch: NodePatch) -> Result<Node> {
        let title = self.read(|c| Ok(rows::live_node(c, id)?.title))?;
        let key = format!("update:{id}");
        Ok(self
            .write_coalesced(ctx, format!("„{title}“ bearbeitet"), Some(&key), |w| {
                nodes::update(w, id, patch)
            })?
            .0)
    }

    pub fn delete_node(&self, ctx: &Ctx, id: &NodeId) -> Result<()> {
        let title = self.read(|c| Ok(rows::live_node(c, id)?.title))?;
        self.write(ctx, format!("„{title}“ gelöscht"), |w| nodes::delete(w, id))?;
        Ok(())
    }

    /// The template library, by title. With a project: what applies there, which
    /// is global templates plus those of the project and its ancestors.
    pub fn templates(&self, project: Option<&ProjectId>) -> Result<Vec<TemplateInfo>> {
        self.read(|c| {
            let applicable: Option<HashSet<ProjectId>> =
                project.map(|p| projects::ancestors_and_self(c, p)).transpose()?;
            let mut stmt = c.prepare_cached(
                "SELECT id, title, kind, body, reviewed FROM nodes
                 WHERE kind = ?1 AND deleted_at IS NULL ORDER BY title COLLATE NOCASE",
            )?;
            let found = stmt.query_map([TEMPLATE_KIND], |r| {
                Ok((rows::brief_from_row(r)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)? != 0))
            })?;
            let mut out = Vec::new();
            for row in found {
                let (node, body, reviewed) = row?;
                let projects = rows::projects_of(c, &node.id)?;
                let applies = projects.is_empty()
                    || applicable
                        .as_ref()
                        .is_none_or(|a| projects.iter().any(|p| a.contains(&p.id)));
                if applies {
                    out.push(TemplateInfo {
                        slug: crate::templates::slug(&node.title),
                        arguments: crate::templates::placeholders(&body),
                        node,
                        body,
                        projects,
                        reviewed,
                    });
                }
            }
            Ok(out)
        })
    }

    /// What is in the trash, most recently deleted first.
    pub fn trash(&self, limit: usize) -> Result<Vec<TrashedNode>> {
        self.read(|c| {
            let mut stmt = c.prepare_cached(
                "SELECT id, title, kind, deleted_at FROM nodes WHERE deleted_at IS NOT NULL
                 ORDER BY deleted_at DESC, id DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map([limit as i64], |r| {
                Ok(TrashedNode { node: rows::brief_from_row(r)?, deleted_at: r.get(3)? })
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
    }

    pub fn restore_node(&self, ctx: &Ctx, id: &NodeId) -> Result<Node> {
        let title = self
            .read(|c| rows::brief(c, id))?
            .ok_or_else(|| CoreError::not_found("node", id.as_str()))?
            .title;
        let summary = format!("„{title}“ wiederhergestellt");
        Ok(self.write(ctx, summary, |w| nodes::restore(w, id))?.0)
    }

    /// Erases an entry from the trash for good. This cannot be undone, so it is
    /// not offered to AI clients.
    pub fn purge_node(&self, ctx: &Ctx, id: &NodeId) -> Result<()> {
        let (unused, _) = self.write(ctx, "", |w| nodes::purge(w, id))?;
        // After the commit: a file must not vanish if the transaction fails. A
        // file left over here is harmless, only wasted space.
        if let Some(blobs) = &self.blobs {
            for blob in unused {
                if let Err(e) = blobs.remove(&blob) {
                    tracing::warn!("could not remove an attachment file: {e}");
                }
            }
        }
        Ok(())
    }

    /// Creates several nodes and links atomically (one change set).
    pub fn capture(&self, ctx: &Ctx, capture: Capture) -> Result<CaptureResult> {
        let summary = format!(
            "{} Einträge, {} Verbindungen erfasst",
            capture.nodes.len(),
            capture.links.len()
        );
        let (mut result, cs) = self.write(ctx, summary, |w| nodes::capture(w, capture))?;
        result.change_set = cs;
        Ok(result)
    }

    pub fn node(&self, id: &NodeId) -> Result<Option<Node>> {
        self.read(|c| rows::node(c, id))
    }

    pub fn node_detail(&self, id: &NodeId) -> Result<NodeDetail> {
        self.read(|c| {
            let node = rows::live_node(c, id)?;
            let projects = rows::projects_of(c, id)?;
            let view = |edge: Edge, other: &NodeId| -> Result<Option<EdgeView>> {
                Ok(rows::brief(c, other)?.map(|other| EdgeView { edge, other }))
            };
            let mut outgoing = Vec::new();
            for e in edges::active_edges_from(c, id)? {
                let dst = e.dst.clone();
                outgoing.extend(view(e, &dst)?);
            }
            let mut incoming = Vec::new();
            for e in edges::active_edges_to(c, id)? {
                let src = e.src.clone();
                incoming.extend(view(e, &src)?);
            }
            let attachments = attachments::of_node(c, id)?;
            let file_links = file_links::of_node(c, id)?;
            Ok(NodeDetail { node, projects, outgoing, incoming, attachments, file_links })
        })
    }

    // -------------------------------------------------------- file links ---

    /// Links an entry to a file or folder. Whether AI clients may read it depends
    /// on who links it; see [`file_links`].
    pub fn add_file_link(
        &self,
        ctx: &Ctx,
        node: &NodeId,
        path: &str,
        label: Option<&str>,
        bookmark: Option<&[u8]>,
    ) -> Result<FileLink> {
        let path = file_links::clean_path(path)?;
        let label = file_links::label_for(&path, label);
        let title = self.read(|c| Ok(rows::live_node(c, node)?.title))?;
        let summary = format!("„{label}“ mit „{title}“ verknüpft");
        Ok(self.write(ctx, summary, |w| file_links::insert(w, node, &path, &label, bookmark))?.0)
    }

    /// A live link of a live entry.
    pub fn file_link(&self, id: &FileLinkId) -> Result<FileLink> {
        self.read(|c| {
            let found = file_links::get(c, id)?.filter(|l| l.deleted_at.is_none());
            let link = found.ok_or_else(|| CoreError::not_found("file link", id.as_str()))?;
            rows::live_node(c, &link.node_id)?;
            Ok(link)
        })
    }

    pub fn file_link_bookmark(&self, id: &FileLinkId) -> Result<Option<Vec<u8>>> {
        self.read(|c| {
            Ok(c.query_row("SELECT bookmark FROM file_links WHERE id = ?1", [id], |r| r.get(0))
                .optional()?
                .flatten())
        })
    }

    /// Where a link's file is now. Follows the file on disk, so this is not a
    /// change the user made and leaves no trace in the history.
    pub fn set_file_link_location(
        &self,
        id: &FileLinkId,
        path: &str,
        bookmark: Option<&[u8]>,
    ) -> Result<()> {
        let path = file_links::clean_path(path)?;
        self.writer.lock().execute(
            "UPDATE file_links SET path = ?2, bookmark = coalesce(?3, bookmark) WHERE id = ?1",
            params![id, path, bookmark],
        )?;
        Ok(())
    }

    pub fn set_file_link_approved(&self, ctx: &Ctx, id: &FileLinkId, approved: bool) -> Result<()> {
        let link = self.file_link(id)?;
        let verb = if approved { "für KIs freigegeben" } else { "für KIs gesperrt" };
        let summary = format!("„{}“ {verb}", link.label);
        let after = FileLink { approved_for_ai: approved, ..link.clone() };
        self.write(ctx, summary, |w| file_links::store(w, &link, &after))?;
        Ok(())
    }

    pub fn delete_file_link(&self, ctx: &Ctx, id: &FileLinkId) -> Result<()> {
        let link = self.file_link(id)?;
        let summary = format!("Verknüpfung „{}“ entfernt", link.label);
        let after = FileLink { deleted_at: Some(now_ms()), ..link.clone() };
        self.write(ctx, summary, |w| file_links::store(w, &link, &after))?;
        Ok(())
    }

    // ------------------------------------------------------- attachments ---

    /// Encrypts `bytes` and attaches them to an entry. `mime` is checked and
    /// otherwise guessed from the file name.
    pub fn add_attachment(
        &self,
        ctx: &Ctx,
        node: &NodeId,
        filename: &str,
        mime: Option<&str>,
        bytes: &[u8],
    ) -> Result<Attachment> {
        let filename = attachments::clean_filename(filename)
            .ok_or_else(|| CoreError::Invalid("the file needs a name".into()))?;
        if bytes.len() > MAX_ATTACHMENT_BYTES {
            let limit = MAX_ATTACHMENT_BYTES / (1024 * 1024);
            return Err(CoreError::Invalid(format!("attachments are limited to {limit} MB")));
        }
        let mime = attachments::clean_mime(mime, &filename);
        let title = self.read(|c| Ok(rows::live_node(c, node)?.title))?;
        // Outside the transaction: encrypting a large file must not hold up writes.
        let blob = self.blob_store()?.put(bytes)?;
        let summary = format!("„{filename}“ an „{title}“ angehängt");
        let size = bytes.len() as i64;
        Ok(self
            .write(ctx, summary, |w| attachments::insert(w, node, &filename, &mime, size, &blob))?
            .0)
    }

    /// A live attachment of a live entry, without its content.
    pub fn attachment(&self, id: &AttachmentId) -> Result<Attachment> {
        self.read(|c| {
            let found = attachments::get(c, id)?.filter(|a| a.deleted_at.is_none());
            let a = found.ok_or_else(|| CoreError::not_found("attachment", id.as_str()))?;
            rows::live_node(c, &a.node_id)?;
            Ok(a)
        })
    }

    pub fn attachment_data(&self, id: &AttachmentId) -> Result<(Attachment, Vec<u8>)> {
        let a = self.attachment(id)?;
        let bytes = self.blob_store()?.get(&a.blob_id)?;
        Ok((a, bytes))
    }

    pub fn delete_attachment(&self, ctx: &Ctx, id: &AttachmentId) -> Result<()> {
        let a = self.attachment(id)?;
        let summary = format!("Anhang „{}“ entfernt", a.filename);
        self.write(ctx, summary, |w| attachments::remove(w, &a))?;
        Ok(())
    }

    /// Accepts a full id, an exact title or a short id (last hex digits).
    pub fn resolve_node(&self, reference: &str) -> Result<NodeId> {
        self.read(|c| refs::resolve_node(c, reference))
    }

    // ------------------------------------------------------------- edges ---

    pub fn link(&self, ctx: &Ctx, input: NewEdge) -> Result<Edge> {
        let origin = self.origin_for(ctx)?;
        Ok(self.write(ctx, "Verbindung erstellt", |w| edges::link(w, &input, origin))?.0)
    }

    pub fn unlink(
        &self,
        ctx: &Ctx,
        src: &NodeId,
        dst: &NodeId,
        relation: Option<&str>,
    ) -> Result<Vec<Edge>> {
        Ok(self.write(ctx, "Verbindung entfernt", |w| edges::unlink(w, src, dst, relation))?.0)
    }

    // ---------------------------------------------------------- projects ---

    pub fn create_project(&self, ctx: &Ctx, input: NewProject) -> Result<Project> {
        let summary = format!("Projekt „{}“ erstellt", input.name.trim());
        Ok(self.write(ctx, summary, |w| projects::create(w, input))?.0)
    }

    /// Returns how many nodes were newly added.
    pub fn add_to_project(
        &self,
        ctx: &Ctx,
        project: &ProjectId,
        nodes: &[NodeId],
    ) -> Result<usize> {
        let summary = format!("{} Einträge zu Projekt hinzugefügt", nodes.len());
        Ok(self
            .write(ctx, summary, |w| {
                let mut added = 0;
                for n in nodes {
                    added += projects::add_member(w, project, n)? as usize;
                }
                Ok(added)
            })?
            .0)
    }

    pub fn remove_from_project(
        &self,
        ctx: &Ctx,
        project: &ProjectId,
        nodes: &[NodeId],
    ) -> Result<usize> {
        let summary = format!("{} Einträge aus Projekt entfernt", nodes.len());
        Ok(self
            .write(ctx, summary, |w| {
                let mut removed = 0;
                for n in nodes {
                    removed += projects::remove_member(w, project, n)? as usize;
                }
                Ok(removed)
            })?
            .0)
    }

    /// Live projects in depth-first tree order.
    pub fn projects(&self) -> Result<Vec<ProjectSummary>> {
        self.read(projects::list)
    }

    pub fn resolve_project(&self, reference: &str) -> Result<ProjectId> {
        self.read(|c| refs::resolve_project(c, reference))
    }

    // ------------------------------------------------------------ search ---

    pub fn search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>> {
        self.read(|c| search::search(c, query))
    }

    // ---------------------------------------------------------- semantic ---

    /// Merges several entries into `keep` as one undoable change set.
    pub fn merge_nodes(&self, ctx: &Ctx, keep: &NodeId, others: &[NodeId]) -> Result<Node> {
        let title = self.read(|c| Ok(rows::live_node(c, keep)?.title))?;
        let entries = match others.len() {
            1 => "Ein Eintrag".to_string(),
            n => format!("{n} Einträge"),
        };
        let summary = format!("{entries} in „{title}“ zusammengeführt");
        Ok(self.write(ctx, summary, |w| nodes::merge(w, keep, others))?.0)
    }

    // ------------------------------------------------------------- graph ---

    /// Everything needed to render a view: all live nodes, or a project with
    /// its subprojects, plus edges between them and saved positions.
    pub fn graph_snapshot(&self, project: Option<&ProjectId>) -> Result<GraphSnapshot> {
        self.read(|c| graph::snapshot(c, project))
    }

    /// Persists node positions for a view (`"all"` or a project id).
    pub fn save_layout(&self, view: &str, positions: &[LayoutPosition]) -> Result<()> {
        let mut conn = self.writer.lock();
        let tx = conn.transaction()?;
        graph::save_layout(&tx, view, positions)?;
        tx.commit()?;
        Ok(())
    }

    /// Neighbourhood up to `depth` hops, at most `limit` nodes.
    pub fn neighborhood(&self, root: &NodeId, depth: usize, limit: usize) -> Result<Subgraph> {
        self.read(|c| graph::neighborhood(c, root, depth.clamp(1, 4), limit.clamp(1, 500)))
    }

    pub fn shortest_path(
        &self,
        from: &NodeId,
        to: &NodeId,
        max_depth: usize,
    ) -> Result<Option<Vec<PathStep>>> {
        self.read(|c| graph::shortest_path(c, from, to, max_depth.clamp(1, 8)))
    }

    pub fn project_overview(&self, project: &ProjectId) -> Result<ProjectOverview> {
        self.read(|c| graph::project_overview(c, project))
    }

    // ----------------------------------------------------------- history ---

    /// Latest change sets with the entries each one touched.
    pub fn recent_changes(&self, limit: usize, since: Option<i64>) -> Result<Vec<RecentChange>> {
        self.read(|c| {
            history::list(c, limit.clamp(1, 200), None)?
                .into_iter()
                .filter(|cs| since.is_none_or(|t| cs.at >= t))
                .map(|cs| {
                    Ok(RecentChange { nodes: history::touched_nodes(c, &cs.id)?, change_set: cs })
                })
                .collect()
        })
    }

    /// Records an MCP tool call (never its content) for the activity view.
    pub fn log_activity(
        &self,
        actor: &ActorId,
        tool: &str,
        args_summary: &str,
        duration_ms: i64,
    ) -> Result<()> {
        self.writer.lock().execute(
            "INSERT INTO activity_log (actor_id, tool, args_summary, duration_ms, at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![actor, tool, args_summary, duration_ms, now_ms()],
        )?;
        Ok(())
    }

    pub fn change_set(&self, id: &ChangeSetId) -> Result<Option<ChangeSetInfo>> {
        self.read(|c| history::get(c, id))
    }

    pub fn change_sets(&self, limit: usize, actor: Option<&ActorId>) -> Result<Vec<ChangeSetInfo>> {
        self.read(|c| history::list(c, limit, actor))
    }

    pub fn node_history(&self, id: &NodeId) -> Result<Vec<NodeRevision>> {
        self.read(|c| history::node_revisions(c, id))
    }

    /// Undoes a change set as a new change set.
    pub fn revert(&self, ctx: &Ctx, target: &ChangeSetId) -> Result<RevertReport> {
        let (restored, cs) =
            self.write(ctx, "Änderung rückgängig gemacht", |w| history::revert(w, target))?;
        Ok(RevertReport {
            reverted: target.clone(),
            change_set: cs.expect("a revert always records revisions"),
            restored,
        })
    }

    // --------------------------------------------------------- internals ---

    fn origin_for(&self, ctx: &Ctx) -> Result<EdgeOrigin> {
        let actor = self
            .actor(&ctx.actor)?
            .ok_or_else(|| CoreError::not_found("actor", ctx.actor.as_str()))?;
        Ok(if actor.kind == ActorKind::Ai { EdgeOrigin::Ai } else { EdgeOrigin::Manual })
    }

    fn read<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let n = self.readers.len();
        let start = self.next_reader.fetch_add(1, Ordering::Relaxed) % n;
        for i in 0..n {
            if let Some(conn) = self.readers[(start + i) % n].try_lock() {
                return f(&conn);
            }
        }
        f(&self.readers[start].lock())
    }

    /// Runs `f` in one IMMEDIATE transaction recorded as one change set.
    /// Changes that record no revisions leave no change set behind (`None`).
    fn write<T>(
        &self,
        ctx: &Ctx,
        summary: impl Into<String>,
        f: impl FnOnce(&mut Writer<'_>) -> Result<T>,
    ) -> Result<(T, Option<ChangeSetId>)> {
        self.write_coalesced(ctx, summary, None, f)
    }

    /// Like [`Self::write`], but continues the most recent change set if it
    /// has the same `coalesce` key, actor and client, is not reverted and is
    /// younger than [`COALESCE_WINDOW_MS`].
    fn write_coalesced<T>(
        &self,
        ctx: &Ctx,
        summary: impl Into<String>,
        coalesce: Option<&str>,
        f: impl FnOnce(&mut Writer<'_>) -> Result<T>,
    ) -> Result<(T, Option<ChangeSetId>)> {
        let summary = summary.into();
        let mut conn = self.writer.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

        let kind: Option<String> = tx
            .query_row(
                "SELECT kind FROM actors WHERE id = ?1 AND revoked_at IS NULL",
                [&ctx.actor],
                |r| r.get(0),
            )
            .optional()?;
        let actor_is_ai = match kind.as_deref() {
            Some(k) => k == "ai",
            None => return Err(CoreError::not_found("actor", ctx.actor.as_str())),
        };

        let now = now_ms();
        let reused: Option<ChangeSetId> = match coalesce {
            Some(key) => tx
                .query_row(
                    "SELECT id FROM (SELECT id, actor_id, client_name, coalesce_key, at, reverted_by
                                     FROM change_sets ORDER BY at DESC, id DESC LIMIT 1)
                     WHERE coalesce_key = ?1 AND actor_id = ?2 AND client_name IS ?3
                       AND reverted_by IS NULL AND at >= ?4",
                    params![key, ctx.actor, ctx.client, now - COALESCE_WINDOW_MS],
                    |r| r.get(0),
                )
                .optional()?,
            None => None,
        };
        let is_new = reused.is_none();
        let cs = match reused {
            Some(id) => {
                tx.execute("UPDATE change_sets SET at = ?2 WHERE id = ?1", params![id, now])?;
                id
            }
            None => {
                let id = ChangeSetId::new();
                tx.execute(
                    "INSERT INTO change_sets (id, actor_id, client_name, tool, summary, at, coalesce_key)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![id, ctx.actor, ctx.client, ctx.tool, summary, now, coalesce],
                )?;
                id
            }
        };

        let mut w = Writer::new(&tx, cs.clone(), ctx.actor.clone(), actor_is_ai, now);
        let out = f(&mut w)?;
        let (revisions, events) = w.finish();

        let committed = if revisions == 0 && is_new {
            tx.execute("DELETE FROM change_sets WHERE id = ?1", [&cs])?;
            None
        } else if revisions == 0 {
            // Nothing changed this time; the earlier change set stands as is.
            None
        } else {
            Some(cs)
        };
        tx.commit()?;
        drop(conn);

        self.generation.fetch_add(1, Ordering::Relaxed);
        self.sync_vectors(&events);
        for e in events {
            let _ = self.events.send(e);
        }
        if let Some(id) = &committed {
            let _ = self.events.send(Event::ChangeSetCommitted {
                id: id.clone(),
                actor: ctx.actor.clone(),
                summary,
            });
        }
        Ok((out, committed))
    }
}
