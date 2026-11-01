//! Change-set listing, per-node history and revert.

use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, params};

use super::{
    attachments, edges, file_links, nodes,
    projects::{self, Membership},
    rows,
    writer::{Entity, Writer, decode_snapshot},
};
use crate::{
    error::{CoreError, Result},
    events::Event,
    ids::{ActorId, AttachmentId, ChangeSetId, EdgeId, FileLinkId, NodeId, ProjectId},
    model::{Attachment, ChangeSetInfo, Edge, FileLink, Node, NodeRevision, Project},
};

const CHANGE_SET_SELECT: &str = "SELECT c.id, c.actor_id, a.name, c.client_name, c.tool, c.summary,
        c.at, c.reverted_by, (SELECT count(*) FROM revisions r WHERE r.change_set_id = c.id)
     FROM change_sets c JOIN actors a ON a.id = c.actor_id";

fn change_set_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ChangeSetInfo> {
    Ok(ChangeSetInfo {
        id: r.get(0)?,
        actor: r.get(1)?,
        actor_name: r.get(2)?,
        client: r.get(3)?,
        tool: r.get(4)?,
        summary: r.get(5)?,
        at: r.get(6)?,
        reverted_by: r.get(7)?,
        revisions: r.get(8)?,
    })
}

pub(crate) fn list(
    conn: &Connection,
    limit: usize,
    actor: Option<&ActorId>,
) -> Result<Vec<ChangeSetInfo>> {
    let sql = format!(
        "{CHANGE_SET_SELECT} WHERE ?1 IS NULL OR c.actor_id = ?1 ORDER BY c.at DESC, c.id DESC LIMIT ?2"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt
        .query_map(params![actor, limit as i64], change_set_from_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

pub(crate) fn get(conn: &Connection, id: &ChangeSetId) -> Result<Option<ChangeSetInfo>> {
    let sql = format!("{CHANGE_SET_SELECT} WHERE c.id = ?1");
    Ok(conn.query_row(&sql, [id], change_set_from_row).optional()?)
}

pub(crate) fn node_revisions(conn: &Connection, node: &NodeId) -> Result<Vec<NodeRevision>> {
    let mut stmt = conn.prepare_cached(
        "SELECT r.change_set_id, a.name, c.at, r.rev, r.before, r.after
         FROM revisions r JOIN change_sets c ON c.id = r.change_set_id
         JOIN actors a ON a.id = c.actor_id
         WHERE r.entity_type = 'node' AND r.entity_id = ?1 ORDER BY r.id DESC",
    )?;
    type Row = (ChangeSetId, String, i64, i64, Option<Vec<u8>>, Option<Vec<u8>>);
    let raw: Vec<Row> = stmt
        .query_map([node], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    raw.into_iter()
        .map(|(change_set, actor_name, at, rev, before, after)| {
            Ok(NodeRevision {
                change_set,
                actor_name,
                at,
                rev,
                before: before.as_deref().map(decode_snapshot).transpose()?,
                after: after.as_deref().map(decode_snapshot).transpose()?,
            })
        })
        .collect()
}

struct RawRevision {
    entity: Entity,
    id: String,
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
}

/// Restores every entity touched by `target` to its state before it, as a new
/// change set. Refuses if a later change touched the same entities (revert
/// the newer change first). Returns the number of restored revisions.
pub(crate) fn revert(w: &mut Writer<'_>, target: &ChangeSetId) -> Result<usize> {
    let reverted_by: Option<Option<ChangeSetId>> = w
        .conn
        .query_row("SELECT reverted_by FROM change_sets WHERE id = ?1", [target], |r| r.get(0))
        .optional()?;
    match reverted_by {
        None => return Err(CoreError::not_found("change set", target.as_str())),
        Some(Some(by)) => {
            return Err(CoreError::Conflict(format!("change set was already reverted by {by}")));
        }
        Some(None) => {}
    }

    let mut stmt = w.conn.prepare(
        "SELECT entity_type, entity_id, before, after FROM revisions
         WHERE change_set_id = ?1 ORDER BY id DESC",
    )?;
    let revisions: Vec<RawRevision> = stmt
        .query_map([target], |r| {
            let entity: String = r.get(0)?;
            Ok(RawRevision {
                entity: Entity::parse(&entity).unwrap_or(Entity::Node),
                id: r.get(1)?,
                before: r.get(2)?,
                after: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    if revisions.is_empty() {
        return Err(CoreError::Conflict(
            "nothing left to revert: everything this change touched was erased".into(),
        ));
    }

    // Only the newest revision of each entity must match the current state;
    // older revisions of the same entity chain onto what we just restored.
    let mut touched: HashSet<(Entity, String)> = HashSet::new();
    for rev in &revisions {
        let check = touched.insert((rev.entity, rev.id.clone()));
        match rev.entity {
            Entity::Node => revert_node(w, rev, check)?,
            Entity::Edge => revert_edge(w, rev, check)?,
            Entity::Project => revert_project(w, rev, check)?,
            Entity::Membership => revert_membership(w, rev, check)?,
            Entity::Attachment => revert_attachment(w, rev, check)?,
            Entity::FileLink => revert_file_link(w, rev, check)?,
        }
    }

    w.conn
        .execute("UPDATE change_sets SET reverted_by = ?2 WHERE id = ?1", params![target, w.cs])?;
    Ok(revisions.len())
}

fn conflict(entity: Entity, id: &str) -> CoreError {
    CoreError::Conflict(format!(
        "{} {id} was changed afterwards; revert the newer change first",
        entity.as_str()
    ))
}

/// Content equality for conflict detection. Revision numbers and the
/// `reviewed` flag are ignored: a revert-of-a-revert or merely viewing an
/// entry must not block undo.
fn same_content(a: &Node, b: &Node) -> bool {
    (&a.title, &a.body, &a.kind, &a.props, &a.tags, a.deleted_at.is_some())
        == (&b.title, &b.body, &b.kind, &b.props, &b.tags, b.deleted_at.is_some())
}

fn revert_node(w: &mut Writer<'_>, rev: &RawRevision, check: bool) -> Result<()> {
    let id = NodeId::from_raw(rev.id.clone());
    let current = rows::node(w.conn, &id)?.ok_or_else(|| conflict(Entity::Node, &rev.id))?;
    let after: Option<Node> = rev.after.as_deref().map(decode_snapshot).transpose()?;
    if check && !after.as_ref().is_some_and(|a| same_content(a, &current)) {
        return Err(conflict(Entity::Node, &rev.id));
    }
    let before: Option<Node> = rev.before.as_deref().map(decode_snapshot).transpose()?;

    let mut restored = current.clone();
    match &before {
        // Created by the target change set: move it to the trash.
        None => restored.deleted_at = Some(w.now),
        Some(b) => {
            restored.title = b.title.clone();
            restored.body = b.body.clone();
            restored.kind = b.kind.clone();
            restored.props = b.props.clone();
            restored.reviewed = b.reviewed;
            restored.tags = b.tags.clone();
            restored.deleted_at = b.deleted_at;
        }
    }
    // A node going back to the trash takes edges added by later changes along.
    if restored.deleted_at.is_some() && current.deleted_at.is_none() {
        for edge in edges::active_edges_of(w.conn, &id)? {
            edges::soft_delete(w, &edge)?;
        }
    }
    restored.rev = current.rev + 1;
    restored.updated_at = w.now;
    restored.updated_by = w.actor.clone();

    w.conn.execute(
        "UPDATE nodes SET title = ?2, body = ?3, kind = ?4, props = ?5, reviewed = ?6, rev = ?7,
                          updated_at = ?8, updated_by = ?9, deleted_at = ?10
         WHERE id = ?1",
        params![
            id,
            restored.title,
            restored.body,
            restored.kind,
            restored.props.as_ref().map(|p| p.to_string()),
            restored.reviewed,
            restored.rev,
            restored.updated_at,
            restored.updated_by,
            restored.deleted_at
        ],
    )?;
    nodes::replace_tags(w, &id, &current.tags, &restored.tags)?;
    nodes::refresh_wikilink_refs(w.conn, &id, &restored.body)?;
    w.record(Entity::Node, id.as_str(), restored.rev, Some(&current), Some(&restored))?;
    w.emit(match restored.deleted_at {
        Some(_) => Event::NodeDeleted { id },
        None => Event::NodeUpserted { id },
    });
    Ok(())
}

fn revert_edge(w: &mut Writer<'_>, rev: &RawRevision, check: bool) -> Result<()> {
    let id = EdgeId::from_raw(rev.id.clone());
    let current = rows::edge(w.conn, &id)?.ok_or_else(|| conflict(Entity::Edge, &rev.id))?;
    let after: Option<Edge> = rev.after.as_deref().map(decode_snapshot).transpose()?;
    if check && after.as_ref() != Some(&current) {
        return Err(conflict(Entity::Edge, &rev.id));
    }
    let before: Option<Edge> = rev.before.as_deref().map(decode_snapshot).transpose()?;
    let mut restored = before.clone().unwrap_or_else(|| current.clone());
    if before.is_none() {
        restored.deleted_at = Some(w.now);
    }
    if restored.deleted_at.is_none()
        && let Some(other) =
            rows::active_edge(w.conn, &restored.src, &restored.dst, &restored.relation)?
        && other.id != id
    {
        return Err(CoreError::Conflict(format!(
            "an equivalent edge {} exists; remove it before reverting",
            other.id
        )));
    }
    w.conn.execute(
        "UPDATE edges SET relation = ?2, weight = ?3, note = ?4, deleted_at = ?5 WHERE id = ?1",
        params![id, restored.relation, restored.weight, restored.note, restored.deleted_at],
    )?;
    w.record(Entity::Edge, id.as_str(), 0, Some(&current), Some(&restored))?;
    let (src, dst) = (restored.src.clone(), restored.dst.clone());
    w.emit(match restored.deleted_at {
        Some(_) => Event::EdgeDeleted { id, src, dst },
        None => Event::EdgeUpserted { id, src, dst },
    });
    Ok(())
}

fn revert_project(w: &mut Writer<'_>, rev: &RawRevision, check: bool) -> Result<()> {
    let id = ProjectId::from_raw(rev.id.clone());
    let current = rows::project(w.conn, &id)?.ok_or_else(|| conflict(Entity::Project, &rev.id))?;
    let after: Option<Project> = rev.after.as_deref().map(decode_snapshot).transpose()?;
    if check && after.as_ref() != Some(&current) {
        return Err(conflict(Entity::Project, &rev.id));
    }
    let before: Option<Project> = rev.before.as_deref().map(decode_snapshot).transpose()?;
    let mut restored = before.clone().unwrap_or_else(|| current.clone());
    if before.is_none() {
        restored.deleted_at = Some(w.now);
    }
    w.conn.execute(
        "UPDATE projects SET parent_id = ?2, name = ?3, description = ?4, color = ?5, icon = ?6,
                             sort = ?7, deleted_at = ?8
         WHERE id = ?1",
        params![
            id,
            restored.parent_id,
            restored.name,
            restored.description,
            restored.color,
            restored.icon,
            restored.sort,
            restored.deleted_at
        ],
    )?;
    w.record(Entity::Project, id.as_str(), 0, Some(&current), Some(&restored))?;
    w.emit(match restored.deleted_at {
        Some(_) => Event::ProjectDeleted { id },
        None => Event::ProjectUpserted { id },
    });
    Ok(())
}

fn revert_membership(w: &mut Writer<'_>, rev: &RawRevision, check: bool) -> Result<()> {
    let before: Option<Membership> = rev.before.as_deref().map(decode_snapshot).transpose()?;
    let after: Option<Membership> = rev.after.as_deref().map(decode_snapshot).transpose()?;
    let Some(m) = before.clone().or(after.clone()) else { return Ok(()) };
    let is_member = projects::is_member(w.conn, &m.project, &m.node)?;
    if check && is_member != after.is_some() {
        return Err(conflict(Entity::Membership, &rev.id));
    }
    match before {
        Some(_) if !is_member => {
            w.conn.execute(
                "INSERT INTO project_members (project_id, node_id, added_at, added_by)
                 VALUES (?1, ?2, ?3, ?4)",
                params![m.project, m.node, w.now, w.actor],
            )?;
            w.record(Entity::Membership, &m.key(), 0, None, Some(&m))?;
            w.emit(Event::MembershipChanged { project: m.project, node: m.node, member: true });
        }
        None if is_member => {
            w.conn.execute(
                "DELETE FROM project_members WHERE project_id = ?1 AND node_id = ?2",
                params![m.project, m.node],
            )?;
            w.record(Entity::Membership, &m.key(), 0, Some(&m), None)?;
            w.emit(Event::MembershipChanged { project: m.project, node: m.node, member: false });
        }
        _ => {}
    }
    Ok(())
}

fn revert_attachment(w: &mut Writer<'_>, rev: &RawRevision, check: bool) -> Result<()> {
    let id = AttachmentId::from_raw(rev.id.clone());
    let current =
        attachments::get(w.conn, &id)?.ok_or_else(|| conflict(Entity::Attachment, &rev.id))?;
    let after: Option<Attachment> = rev.after.as_deref().map(decode_snapshot).transpose()?;
    if check && after.as_ref() != Some(&current) {
        return Err(conflict(Entity::Attachment, &rev.id));
    }
    let before: Option<Attachment> = rev.before.as_deref().map(decode_snapshot).transpose()?;
    // Undoing the upload leaves the row in the trash, like undoing a new entry.
    let restored =
        before.unwrap_or_else(|| Attachment { deleted_at: Some(w.now), ..current.clone() });
    attachments::store(w, &current, &restored)
}

fn revert_file_link(w: &mut Writer<'_>, rev: &RawRevision, check: bool) -> Result<()> {
    let id = FileLinkId::from_raw(rev.id.clone());
    let current =
        file_links::get(w.conn, &id)?.ok_or_else(|| conflict(Entity::FileLink, &rev.id))?;
    // The path follows the file on disk, so a move is no conflict.
    let same_path = |l: FileLink| FileLink { path: current.path.clone(), ..l };
    let after: Option<FileLink> =
        rev.after.as_deref().map(decode_snapshot::<FileLink>).transpose()?.map(same_path);
    if check && after.as_ref() != Some(&current) {
        return Err(conflict(Entity::FileLink, &rev.id));
    }
    let before: Option<FileLink> =
        rev.before.as_deref().map(decode_snapshot::<FileLink>).transpose()?.map(same_path);
    let restored =
        before.unwrap_or_else(|| FileLink { deleted_at: Some(w.now), ..current.clone() });
    file_links::store(w, &current, &restored)
}

/// Entries touched by a change set, in first-touch order.
pub(crate) fn touched_nodes(
    conn: &Connection,
    cs: &ChangeSetId,
) -> Result<Vec<crate::model::NodeBrief>> {
    let mut stmt = conn.prepare_cached(
        "SELECT n.id, n.title, n.kind FROM revisions r JOIN nodes n ON n.id = r.entity_id
         WHERE r.change_set_id = ?1 AND r.entity_type = 'node'
         GROUP BY n.id ORDER BY min(r.id)",
    )?;
    let rows = stmt.query_map([cs], rows::brief_from_row)?.collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}
