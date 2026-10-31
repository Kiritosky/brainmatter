use rusqlite::{Connection, params};

use super::{
    rows::{self, EDGE_COLS, edge_from_row, snake},
    writer::{Entity, Writer},
};
use crate::{
    error::{CoreError, Result},
    events::Event,
    ids::{EdgeId, NodeId},
    model::{DEFAULT_RELATION, Edge, EdgeOrigin, NewEdge},
};

/// Creates the edge, or returns the existing live edge with the same
/// endpoints and relation (linking is idempotent).
pub(crate) fn link(w: &mut Writer<'_>, input: &NewEdge, origin: EdgeOrigin) -> Result<Edge> {
    if input.src == input.dst {
        return Err(CoreError::Invalid("an entry cannot link to itself".into()));
    }
    rows::live_node(w.conn, &input.src)?;
    rows::live_node(w.conn, &input.dst)?;
    let relation = snake(input.relation.as_deref(), DEFAULT_RELATION);
    if let Some(existing) = rows::active_edge(w.conn, &input.src, &input.dst, &relation)? {
        return Ok(existing);
    }
    let weight = input.weight.unwrap_or(1.0);
    if !(weight.is_finite() && weight > 0.0) {
        return Err(CoreError::Invalid("weight must be a positive number".into()));
    }
    insert(w, &input.src, &input.dst, &relation, weight, input.note.as_deref(), origin)
}

pub(crate) fn insert(
    w: &mut Writer<'_>,
    src: &NodeId,
    dst: &NodeId,
    relation: &str,
    weight: f64,
    note: Option<&str>,
    origin: EdgeOrigin,
) -> Result<Edge> {
    let id = EdgeId::new();
    w.conn.execute(
        "INSERT INTO edges (id, src, dst, relation, weight, note, origin, created_at, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![id, src, dst, relation, weight, note, origin.as_str(), w.now, w.actor],
    )?;
    let edge = rows::edge(w.conn, &id)?.expect("edge was just inserted");
    w.record(Entity::Edge, id.as_str(), 0, None, Some(&edge))?;
    w.emit(Event::EdgeUpserted { id, src: src.clone(), dst: dst.clone() });
    Ok(edge)
}

pub(crate) fn soft_delete(w: &mut Writer<'_>, edge: &Edge) -> Result<()> {
    let mut after = edge.clone();
    after.deleted_at = Some(w.now);
    w.conn.execute("UPDATE edges SET deleted_at = ?2 WHERE id = ?1", params![edge.id, w.now])?;
    w.record(Entity::Edge, edge.id.as_str(), 0, Some(edge), Some(&after))?;
    w.emit(Event::EdgeDeleted {
        id: edge.id.clone(),
        src: edge.src.clone(),
        dst: edge.dst.clone(),
    });
    Ok(())
}

/// Edges that went to the trash along with `node`: those removed in the change
/// set that last touched it, which for a trashed node is the deletion. Times
/// would not do, since a link removed just before can have the same millisecond.
pub(crate) fn trashed_with(conn: &Connection, node: &NodeId) -> Result<Vec<Edge>> {
    query(
        conn,
        "(src = ?1 OR dst = ?1) AND deleted_at IS NOT NULL AND id IN (
             SELECT entity_id FROM revisions WHERE entity_type = 'edge' AND change_set_id = (
                 SELECT change_set_id FROM revisions
                 WHERE entity_type = 'node' AND entity_id = ?1 ORDER BY id DESC LIMIT 1))",
        node,
    )
}

pub(crate) fn restore(w: &mut Writer<'_>, edge: &Edge) -> Result<()> {
    let after = Edge { deleted_at: None, ..edge.clone() };
    w.conn.execute("UPDATE edges SET deleted_at = NULL WHERE id = ?1", [&edge.id])?;
    w.record(Entity::Edge, edge.id.as_str(), 0, Some(edge), Some(&after))?;
    w.emit(Event::EdgeUpserted {
        id: edge.id.clone(),
        src: edge.src.clone(),
        dst: edge.dst.clone(),
    });
    Ok(())
}

/// Removes live edges `src → dst` (all relations unless one is given). If none
/// exist in that direction, the reverse direction is tried, since callers
/// rarely remember which way a link points.
pub(crate) fn unlink(
    w: &mut Writer<'_>,
    src: &NodeId,
    dst: &NodeId,
    relation: Option<&str>,
) -> Result<Vec<Edge>> {
    let relation = relation.map(|r| snake(Some(r), DEFAULT_RELATION));
    let matching = |a: &NodeId, b: &NodeId| -> Result<Vec<Edge>> {
        Ok(active_edges_from(w.conn, a)?
            .into_iter()
            .filter(|e| &e.dst == b && relation.as_ref().is_none_or(|r| &e.relation == r))
            .collect())
    };
    let mut found = matching(src, dst)?;
    if found.is_empty() {
        found = matching(dst, src)?;
    }
    for edge in &found {
        soft_delete(w, edge)?;
    }
    Ok(found)
}

pub(crate) fn active_edges_from(conn: &Connection, src: &NodeId) -> Result<Vec<Edge>> {
    query(conn, "src = ?1 AND deleted_at IS NULL", src)
}

pub(crate) fn active_edges_to(conn: &Connection, dst: &NodeId) -> Result<Vec<Edge>> {
    query(conn, "dst = ?1 AND deleted_at IS NULL", dst)
}

pub(crate) fn active_edges_of(conn: &Connection, node: &NodeId) -> Result<Vec<Edge>> {
    query(conn, "(src = ?1 OR dst = ?1) AND deleted_at IS NULL", node)
}

fn query(conn: &Connection, filter: &str, node: &NodeId) -> Result<Vec<Edge>> {
    let sql = format!("SELECT {EDGE_COLS} FROM edges WHERE {filter} ORDER BY created_at");
    let mut stmt = conn.prepare_cached(&sql)?;
    let edges = stmt.query_map([node], edge_from_row)?.collect::<rusqlite::Result<_>>()?;
    Ok(edges)
}
