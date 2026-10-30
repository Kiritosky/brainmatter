//! The engine: the single entry point for reading and changing the knowledge
//! graph. One writer connection serializes all changes (each call is one
//! change set); a small pool of read connections serves queries concurrently
//! thanks to WAL. Committed changes are broadcast as [`Event`]s.
//!
//! The API is synchronous; async callers should use `spawn_blocking`.

mod edges;
mod graph;
mod history;
mod nodes;
mod projects;
mod refs;
mod rows;
mod search;
mod writer;

use std::{
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use tokio::sync::broadcast;

use crate::{
    error::{CoreError, Result},
    events::Event,
    ids::{ActorId, ChangeSetId, NodeId, ProjectId},
    model::*,
    store::{Db, DbKey},
    time::now_ms,
};
use writer::Writer;

const EVENT_BUFFER: usize = 1024;

pub struct Engine {
    writer: Mutex<Connection>,
    readers: Vec<Mutex<Connection>>,
    next_reader: AtomicUsize,
    events: broadcast::Sender<Event>,
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
                Ok(Mutex::new(conn))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            writer: Mutex::new(writer),
            readers,
            next_reader: AtomicUsize::new(0),
            events: broadcast::channel(EVENT_BUFFER).0,
        })
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
        Ok(self.write(ctx, format!("„{title}“ bearbeitet"), |w| nodes::update(w, id, patch))?.0)
    }

    pub fn delete_node(&self, ctx: &Ctx, id: &NodeId) -> Result<()> {
        let title = self.read(|c| Ok(rows::live_node(c, id)?.title))?;
        self.write(ctx, format!("„{title}“ gelöscht"), |w| nodes::delete(w, id))?;
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
            Ok(NodeDetail { node, projects, outgoing, incoming })
        })
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

    // ----------------------------------------------------------- history ---

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

        let cs = ChangeSetId::new();
        let now = now_ms();
        tx.execute(
            "INSERT INTO change_sets (id, actor_id, client_name, tool, summary, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![cs, ctx.actor, ctx.client, ctx.tool, summary, now],
        )?;

        let mut w = Writer::new(&tx, cs.clone(), ctx.actor.clone(), actor_is_ai, now);
        let out = f(&mut w)?;
        let (revisions, events) = w.finish();

        let committed = if revisions == 0 {
            tx.execute("DELETE FROM change_sets WHERE id = ?1", [&cs])?;
            None
        } else {
            Some(cs)
        };
        tx.commit()?;
        drop(conn);

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
