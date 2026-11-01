//! Whole-view graph snapshots for rendering, and persisted layouts.

use std::collections::{HashMap, HashSet, VecDeque};

use rusqlite::{Connection, params};

use super::{analytics, edges, projects, rows, rows::EDGE_COLS, rows::edge_from_row};
use crate::{
    error::Result,
    ids::{NodeId, ProjectId},
    model::{
        GraphEdge, GraphNode, GraphSnapshot, LayoutPosition, NodeBrief, PathStep, ProjectBrief,
        ProjectOverview, Subgraph,
    },
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
             FROM nodes n INDEXED BY nodes_graph_idx
             LEFT JOIN layout_positions lp ON lp.view_id = ?1 AND lp.node_id = n.id
             WHERE n.deleted_at IS NULL AND n.kind != 'template'",
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
                rank: 0.0,
                cluster: 0,
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

    let links: Vec<(usize, usize)> = edges.iter().map(|e| (index[&e.src], index[&e.dst])).collect();
    let ranks = analytics::pagerank(nodes.len(), &links);
    let clusters = analytics::clusters(nodes.len(), &links);
    for ((node, rank), cluster) in nodes.iter_mut().zip(ranks).zip(clusters) {
        node.rank = rank;
        node.cluster = cluster;
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

/// Breadth-first traversal over live edges in both directions.
pub(crate) fn neighborhood(
    conn: &Connection,
    root: &NodeId,
    depth: usize,
    limit: usize,
) -> Result<Subgraph> {
    let root_brief = rows::live_node(conn, root).map(|n| NodeBrief {
        id: n.id,
        title: n.title,
        kind: n.kind,
    })?;
    let mut seen: HashMap<NodeId, usize> = HashMap::from([(root.clone(), 0)]);
    let mut order = vec![(root_brief, 0)];
    let mut queue = VecDeque::from([(root.clone(), 0usize)]);
    let mut truncated = false;

    'bfs: while let Some((id, d)) = queue.pop_front() {
        if d >= depth {
            continue;
        }
        for e in edges::active_edges_of(conn, &id)? {
            let other = if e.src == id { e.dst } else { e.src };
            if seen.contains_key(&other) {
                continue;
            }
            if order.len() >= limit {
                truncated = true;
                break 'bfs;
            }
            if let Some(b) = rows::brief(conn, &other)? {
                seen.insert(other.clone(), d + 1);
                order.push((b, d + 1));
                queue.push_back((other, d + 1));
            }
        }
    }

    let mut found = Vec::new();
    let mut edge_ids = HashSet::new();
    for (b, _) in &order {
        for e in edges::active_edges_from(conn, &b.id)? {
            if seen.contains_key(&e.dst) && edge_ids.insert(e.id.clone()) {
                found.push(e);
            }
        }
    }
    Ok(Subgraph { root: root.clone(), nodes: order, edges: found, truncated })
}

/// Shortest path (fewest hops, any edge direction), or `None` if the nodes
/// are not connected within `max_depth` hops.
pub(crate) fn shortest_path(
    conn: &Connection,
    from: &NodeId,
    to: &NodeId,
    max_depth: usize,
) -> Result<Option<Vec<PathStep>>> {
    rows::live_node(conn, from)?;
    rows::live_node(conn, to)?;
    let mut parent: HashMap<NodeId, Option<(NodeId, crate::model::Edge)>> =
        HashMap::from([(from.clone(), None)]);
    let mut queue = VecDeque::from([(from.clone(), 0usize)]);
    while let Some((id, d)) = queue.pop_front() {
        if &id == to {
            break;
        }
        if d >= max_depth {
            continue;
        }
        for e in edges::active_edges_of(conn, &id)? {
            let other = if e.src == id { e.dst.clone() } else { e.src.clone() };
            if !parent.contains_key(&other) {
                parent.insert(other.clone(), Some((id.clone(), e)));
                queue.push_back((other, d + 1));
            }
        }
    }
    if !parent.contains_key(to) {
        return Ok(None);
    }
    let mut steps = Vec::new();
    let mut cur = to.clone();
    loop {
        let via = parent.get(&cur).cloned().flatten();
        let node = rows::brief(conn, &cur)?.expect("visited nodes exist");
        let prev = via.as_ref().map(|(p, _)| p.clone());
        steps.push(PathStep { node, via: via.map(|(_, e)| e) });
        match prev {
            Some(p) => cur = p,
            None => break,
        }
    }
    steps.reverse();
    Ok(Some(steps))
}

pub(crate) fn project_overview(conn: &Connection, id: &ProjectId) -> Result<ProjectOverview> {
    let project = rows::live_project(conn, id)?;
    let all = projects::list(conn)?;

    let mut path = Vec::new();
    let mut cur = project.parent_id.clone();
    while let Some(pid) = cur {
        let Some(p) = all.iter().find(|s| s.project.id == pid) else { break };
        path.insert(0, ProjectBrief { id: p.project.id.clone(), name: p.project.name.clone() });
        cur = p.project.parent_id.clone();
    }
    let subprojects =
        all.iter().filter(|s| s.project.parent_id.as_ref() == Some(id)).cloned().collect();

    let snap = snapshot(conn, Some(id))?;
    let brief = |n: &GraphNode| NodeBrief {
        id: n.id.clone(),
        title: n.title.clone(),
        kind: n.kind.clone(),
    };
    let mut by_rank: Vec<&GraphNode> = snap.nodes.iter().filter(|n| n.degree > 0).collect();
    by_rank.sort_by(|a, b| b.rank.total_cmp(&a.rank).then_with(|| a.title.cmp(&b.title)));
    let hubs: Vec<(NodeBrief, i64)> =
        by_rank.iter().take(10).map(|n| (brief(n), n.degree)).collect();

    // Clusters are numbered by size; singletons are not a theme.
    let mut themes: Vec<Vec<NodeBrief>> = Vec::new();
    for cluster in 0..=snap.nodes.iter().map(|n| n.cluster).max().unwrap_or(0) {
        let members: Vec<&&GraphNode> = by_rank.iter().filter(|n| n.cluster == cluster).collect();
        if members.len() >= 3 && themes.len() < 6 {
            themes.push(members.iter().take(5).map(|n| brief(n)).collect());
        }
    }

    let mut by_recency: Vec<_> = snap.nodes.iter().collect();
    // Ids sort by creation, which settles entries changed in the same millisecond.
    by_recency.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| b.id.cmp(&a.id)));
    let brief = |n: &GraphNode| NodeBrief {
        id: n.id.clone(),
        title: n.title.clone(),
        kind: n.kind.clone(),
    };
    let recent = by_recency.iter().take(10).map(|n| brief(n)).collect();

    let mut kinds: HashMap<String, usize> = HashMap::new();
    for n in &snap.nodes {
        *kinds.entry(n.kind.clone()).or_default() += 1;
    }
    let mut kinds: Vec<(String, usize)> = kinds.into_iter().collect();
    kinds.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let tasks = snap.nodes.iter().filter(|n| n.kind == "task").take(20).map(brief).collect();
    let unreviewed = snap.nodes.iter().filter(|n| !n.reviewed).count();

    Ok(ProjectOverview {
        project,
        path,
        subprojects,
        node_count: snap.nodes.len(),
        edge_count: snap.edges.len(),
        hubs,
        themes,
        recent,
        kinds,
        tasks,
        unreviewed,
    })
}
