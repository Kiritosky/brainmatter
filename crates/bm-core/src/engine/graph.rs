//! Whole-view graph snapshots for rendering, and persisted layouts.

use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, params};

use super::{projects, rows::EDGE_COLS, rows::edge_from_row};
use crate::{
    error::Result,
    ids::{NodeId, ProjectId},
    model::{GraphEdge, GraphNode, GraphSnapshot, LayoutPosition},
};

pub(crate) const ALL_VIEW: &str = "all";

/// Live nodes of the view (everything, or a project including its
/// subprojects), the edges between them and their saved positions.
pub(crate) fn snapshot(conn: &Connection, project: Option<&ProjectId>) -> Result<GraphSnapshot> {
    let view = project.map_or(ALL_VIEW.to_string(), |p| p.as_str().to_string());
    let scope: Option<HashSet<ProjectId>> =
        project.map(|p| projects::subtree(conn, p)).transpose()?.map(|v| v.into_iter().collect());

    let mut memberships: HashMap<NodeId, Vec<ProjectId>> = HashMap::new();
    {
        let mut stmt = conn.prepare_cached(
            "SELECT m.node_id, m.project_id FROM project_members m
             JOIN projects p ON p.id = m.project_id WHERE p.deleted_at IS NULL",
        )?;
        let rows =
            stmt.query_map([], |r| Ok((r.get::<_, NodeId>(0)?, r.get::<_, ProjectId>(1)?)))?;
        for row in rows {
            let (node, project) = row?;
            memberships.entry(node).or_default().push(project);
        }
    }

    let mut nodes = Vec::new();
    {
        let mut stmt = conn.prepare_cached(
            "SELECT n.id, n.title, n.kind, n.reviewed, n.updated_at, n.created_by,
                    lp.x, lp.y, coalesce(lp.pinned, 0)
             FROM nodes n
             LEFT JOIN layout_positions lp ON lp.view_id = ?1 AND lp.node_id = n.id
             WHERE n.deleted_at IS NULL",
        )?;
        let rows = stmt.query_map([&view], |r| {
            Ok(GraphNode {
                id: r.get(0)?,
                title: r.get(1)?,
                kind: r.get(2)?,
                reviewed: r.get::<_, i64>(3)? != 0,
                updated_at: r.get(4)?,
                created_by: r.get(5)?,
                projects: Vec::new(),
                degree: 0,
                x: r.get(6)?,
                y: r.get(7)?,
                pinned: r.get::<_, i64>(8)? != 0,
            })
        })?;
        for row in rows {
            let mut node = row?;
            node.projects = memberships.remove(&node.id).unwrap_or_default();
            let in_scope =
                scope.as_ref().is_none_or(|s| node.projects.iter().any(|p| s.contains(p)));
            if in_scope {
                nodes.push(node);
            }
        }
    }

    let index: HashMap<NodeId, usize> =
        nodes.iter().enumerate().map(|(i, n)| (n.id.clone(), i)).collect();
    let mut edges = Vec::new();
    {
        let sql = format!("SELECT {EDGE_COLS} FROM edges WHERE deleted_at IS NULL");
        let mut stmt = conn.prepare_cached(&sql)?;
        for edge in stmt.query_map([], edge_from_row)? {
            let e = edge?;
            if let (Some(&a), Some(&b)) = (index.get(&e.src), index.get(&e.dst)) {
                nodes[a].degree += 1;
                nodes[b].degree += 1;
                edges.push(GraphEdge {
                    id: e.id,
                    src: e.src,
                    dst: e.dst,
                    relation: e.relation,
                    origin: e.origin,
                });
            }
        }
    }

    Ok(GraphSnapshot { view, nodes, edges })
}

/// Layout is view state, not knowledge: stored without change sets.
pub(crate) fn save_layout(
    conn: &Connection,
    view: &str,
    positions: &[LayoutPosition],
) -> Result<()> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO layout_positions (view_id, node_id, x, y, pinned)
         SELECT ?1, ?2, ?3, ?4, ?5 WHERE EXISTS (SELECT 1 FROM nodes WHERE id = ?2)
         ON CONFLICT (view_id, node_id) DO UPDATE SET x = excluded.x, y = excluded.y,
                                                      pinned = excluded.pinned",
    )?;
    for p in positions {
        if p.x.is_finite() && p.y.is_finite() {
            stmt.execute(params![view, p.node, p.x, p.y, p.pinned])?;
        }
    }
    Ok(())
}
