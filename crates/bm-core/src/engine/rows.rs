//! Row mapping and small lookups shared by the engine modules.

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::{
    error::{CoreError, Result},
    ids::{EdgeId, NodeId, ProjectId},
    model::{Edge, EdgeOrigin, Node, NodeBrief, Project, ProjectBrief},
};

pub(crate) const NODE_COLS: &str = "id, title, body, kind, props, reviewed, rev, created_at, \
     updated_at, created_by, updated_by, deleted_at";

pub(crate) fn node_from_row(r: &Row<'_>) -> rusqlite::Result<Node> {
    let props: Option<String> = r.get(4)?;
    Ok(Node {
        id: r.get(0)?,
        title: r.get(1)?,
        body: r.get(2)?,
        kind: r.get(3)?,
        props: props.and_then(|p| serde_json::from_str(&p).ok()),
        reviewed: r.get::<_, i64>(5)? != 0,
        rev: r.get(6)?,
        tags: Vec::new(),
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
        created_by: r.get(9)?,
        updated_by: r.get(10)?,
        deleted_at: r.get(11)?,
    })
}

pub(crate) fn tags(conn: &Connection, id: &NodeId) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare_cached("SELECT tag FROM node_tags WHERE node_id = ?1 ORDER BY tag")?;
    let tags = stmt.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(tags)
}

/// Node with tags; `None` if it never existed.
pub(crate) fn node(conn: &Connection, id: &NodeId) -> Result<Option<Node>> {
    let sql = format!("SELECT {NODE_COLS} FROM nodes WHERE id = ?1");
    let Some(mut node) = conn.query_row(&sql, [id], node_from_row).optional()? else {
        return Ok(None);
    };
    node.tags = tags(conn, id)?;
    Ok(Some(node))
}

/// Node that exists and is not in the trash.
pub(crate) fn live_node(conn: &Connection, id: &NodeId) -> Result<Node> {
    match node(conn, id)? {
        Some(n) if n.deleted_at.is_none() => Ok(n),
        _ => Err(CoreError::not_found("node", id.as_str())),
    }
}

pub(crate) fn brief(conn: &Connection, id: &NodeId) -> Result<Option<NodeBrief>> {
    Ok(conn
        .query_row("SELECT id, title, kind FROM nodes WHERE id = ?1", [id], brief_from_row)
        .optional()?)
}

pub(crate) fn brief_from_row(r: &Row<'_>) -> rusqlite::Result<NodeBrief> {
    Ok(NodeBrief { id: r.get(0)?, title: r.get(1)?, kind: r.get(2)? })
}

/// Live nodes with exactly this title (ASCII case-insensitive), oldest first.
pub(crate) fn nodes_titled(conn: &Connection, title: &str) -> Result<Vec<NodeBrief>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, title, kind FROM nodes
         WHERE title = ?1 COLLATE NOCASE AND deleted_at IS NULL ORDER BY created_at",
    )?;
    let rows = stmt.query_map([title.trim()], brief_from_row)?.collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

pub(crate) const EDGE_COLS: &str =
    "id, src, dst, relation, weight, note, origin, created_at, created_by, deleted_at";

pub(crate) fn edge_from_row(r: &Row<'_>) -> rusqlite::Result<Edge> {
    Ok(Edge {
        id: r.get(0)?,
        src: r.get(1)?,
        dst: r.get(2)?,
        relation: r.get(3)?,
        weight: r.get(4)?,
        note: r.get(5)?,
        origin: EdgeOrigin::parse(&r.get::<_, String>(6)?),
        created_at: r.get(7)?,
        created_by: r.get(8)?,
        deleted_at: r.get(9)?,
    })
}

pub(crate) fn edge(conn: &Connection, id: &EdgeId) -> Result<Option<Edge>> {
    let sql = format!("SELECT {EDGE_COLS} FROM edges WHERE id = ?1");
    Ok(conn.query_row(&sql, [id], edge_from_row).optional()?)
}

pub(crate) fn active_edge(
    conn: &Connection,
    src: &NodeId,
    dst: &NodeId,
    relation: &str,
) -> Result<Option<Edge>> {
    let sql = format!(
        "SELECT {EDGE_COLS} FROM edges
         WHERE src = ?1 AND dst = ?2 AND relation = ?3 AND deleted_at IS NULL"
    );
    Ok(conn.query_row(&sql, params![src, dst, relation], edge_from_row).optional()?)
}

pub(crate) const PROJECT_COLS: &str =
    "id, parent_id, name, description, color, icon, sort, created_at, created_by, deleted_at";

pub(crate) fn project_from_row(r: &Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get(0)?,
        parent_id: r.get(1)?,
        name: r.get(2)?,
        description: r.get(3)?,
        color: r.get(4)?,
        icon: r.get(5)?,
        sort: r.get(6)?,
        created_at: r.get(7)?,
        created_by: r.get(8)?,
        deleted_at: r.get(9)?,
    })
}

pub(crate) fn project(conn: &Connection, id: &ProjectId) -> Result<Option<Project>> {
    let sql = format!("SELECT {PROJECT_COLS} FROM projects WHERE id = ?1");
    Ok(conn.query_row(&sql, [id], project_from_row).optional()?)
}

pub(crate) fn live_project(conn: &Connection, id: &ProjectId) -> Result<Project> {
    match project(conn, id)? {
        Some(p) if p.deleted_at.is_none() => Ok(p),
        _ => Err(CoreError::not_found("project", id.as_str())),
    }
}

pub(crate) fn projects_of(conn: &Connection, node: &NodeId) -> Result<Vec<ProjectBrief>> {
    let mut stmt = conn.prepare_cached(
        "SELECT p.id, p.name FROM project_members m JOIN projects p ON p.id = m.project_id
         WHERE m.node_id = ?1 AND p.deleted_at IS NULL ORDER BY p.name",
    )?;
    let rows = stmt
        .query_map([node], |r| Ok(ProjectBrief { id: r.get(0)?, name: r.get(1)? }))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

// ------------------------------------------------------------ normalizing ---

pub(crate) fn clean_title(title: &str) -> Result<String> {
    let t = title.trim();
    if t.is_empty() {
        return Err(CoreError::Invalid("title must not be empty".into()));
    }
    if t.chars().count() > 300 {
        return Err(CoreError::Invalid("title is longer than 300 characters".into()));
    }
    Ok(t.to_string())
}

/// `"Code Snippet"` → `"code_snippet"`. Used for kinds and relations.
pub(crate) fn snake(value: Option<&str>, default: &str) -> String {
    let s: String = value
        .unwrap_or("")
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_whitespace() || c == '-' { '_' } else { c })
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    let s = s.trim_matches('_').to_string();
    if s.is_empty() { default.to_string() } else { s }
}

/// Lowercase, no leading `#`, spaces become dashes, deduplicated and sorted.
pub fn clean_tags<S: AsRef<str>>(tags: &[S]) -> Vec<String> {
    let mut out: Vec<String> = tags
        .iter()
        .map(|t| t.as_ref().trim().trim_start_matches('#').trim().to_lowercase().replace(' ', "-"))
        .filter(|t| !t.is_empty())
        .collect();
    out.sort();
    out.dedup();
    out
}
