use std::collections::HashMap;

use rusqlite::params;

use super::{
    edges, projects, refs,
    rows::{self, clean_tags, clean_title, snake},
    writer::{Entity, Writer},
};
use crate::{
    error::{CoreError, Result},
    events::Event,
    ids::NodeId,
    model::{
        BodyEdit, Capture, CaptureResult, DEFAULT_KIND, EdgeOrigin, NewEdge, NewNode, Node,
        NodePatch, WIKILINK_RELATION,
    },
    wikilinks,
};

const MAX_CAPTURE_NODES: usize = 200;
const MAX_CAPTURE_LINKS: usize = 500;

pub(crate) fn create(w: &mut Writer<'_>, input: NewNode) -> Result<Node> {
    let title = clean_title(&input.title)?;
    let kind = snake(input.kind.as_deref(), DEFAULT_KIND);
    let props = input.props.as_ref().filter(|p| !p.is_null()).map(|p| p.to_string());
    let id = NodeId::new();
    // Entries an AI writes stay "unreviewed" until the user has seen them.
    let reviewed = !w.actor_is_ai;

    w.conn.execute(
        "INSERT INTO nodes (id, title, body, kind, props, reviewed, rev, created_at, updated_at,
                            created_by, updated_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?7, ?8, ?8)",
        params![id, title, input.body, kind, props, reviewed, w.now, w.actor],
    )?;
    for tag in clean_tags(&input.tags) {
        w.conn.execute("INSERT INTO node_tags (node_id, tag) VALUES (?1, ?2)", params![id, tag])?;
    }

    let node = rows::live_node(w.conn, &id)?;
    w.record(Entity::Node, id.as_str(), 1, None, Some(&node))?;
    w.emit(Event::NodeUpserted { id: id.clone() });

    let mut seen = Vec::new();
    for project in input.projects {
        if !seen.contains(&project) {
            projects::add_member(w, &project, &id)?;
            seen.push(project);
        }
    }
    sync_outgoing_wikilinks(w, &node)?;
    backfill_incoming_wikilinks(w, &node)?;
    Ok(node)
}

pub(crate) fn update(w: &mut Writer<'_>, id: &NodeId, patch: NodePatch) -> Result<Node> {
    let before = rows::live_node(w.conn, id)?;
    let mut after = before.clone();

    if let Some(title) = &patch.title {
        after.title = clean_title(title)?;
    }
    match patch.body {
        Some(BodyEdit::Replace(text)) => after.body = text,
        Some(BodyEdit::Append(text)) => after.body = join_paragraphs(&before.body, &text),
        Some(BodyEdit::Prepend(text)) => after.body = join_paragraphs(&text, &before.body),
        None => {}
    }
    if let Some(kind) = &patch.kind {
        after.kind = snake(Some(kind), DEFAULT_KIND);
    }
    if let Some(props) = patch.props {
        after.props = (!props.is_null()).then_some(props);
    }
    let removed = clean_tags(&patch.tags_remove);
    after.tags.retain(|t| !removed.contains(t));
    after.tags.extend(clean_tags(&patch.tags_add));
    after.tags = clean_tags(&after.tags);

    let content_changed = (&after.title, &after.body, &after.kind, &after.props, &after.tags)
        != (&before.title, &before.body, &before.kind, &before.props, &before.tags);
    after.reviewed = match patch.reviewed {
        Some(r) => r,
        None if content_changed => !w.actor_is_ai,
        None => before.reviewed,
    };
    if !content_changed && after.reviewed == before.reviewed {
        return Ok(before);
    }

    after.rev += 1;
    after.updated_at = w.now;
    after.updated_by = w.actor.clone();
    w.conn.execute(
        "UPDATE nodes SET title = ?2, body = ?3, kind = ?4, props = ?5, reviewed = ?6, rev = ?7,
                          updated_at = ?8, updated_by = ?9
         WHERE id = ?1",
        params![
            id,
            after.title,
            after.body,
            after.kind,
            after.props.as_ref().map(|p| p.to_string()),
            after.reviewed,
            after.rev,
            after.updated_at,
            after.updated_by
        ],
    )?;
    replace_tags(w, id, &before.tags, &after.tags)?;
    w.record(Entity::Node, id.as_str(), after.rev, Some(&before), Some(&after))?;
    w.emit(Event::NodeUpserted { id: id.clone() });

    if after.body != before.body {
        sync_outgoing_wikilinks(w, &after)?;
    }
    if !after.title.eq_ignore_ascii_case(&before.title) {
        backfill_incoming_wikilinks(w, &after)?;
    }
    Ok(after)
}

/// Moves a node to the trash together with all its edges.
pub(crate) fn delete(w: &mut Writer<'_>, id: &NodeId) -> Result<()> {
    let before = rows::live_node(w.conn, id)?;
    for edge in edges::active_edges_of(w.conn, id)? {
        edges::soft_delete(w, &edge)?;
    }
    let mut after = before.clone();
    after.deleted_at = Some(w.now);
    after.rev += 1;
    after.updated_at = w.now;
    after.updated_by = w.actor.clone();
    w.conn.execute(
        "UPDATE nodes SET deleted_at = ?2, rev = ?3, updated_at = ?4, updated_by = ?5 WHERE id = ?1",
        params![id, after.deleted_at, after.rev, after.updated_at, after.updated_by],
    )?;
    w.record(Entity::Node, id.as_str(), after.rev, Some(&before), Some(&after))?;
    w.emit(Event::NodeDeleted { id: id.clone() });
    Ok(())
}

pub(crate) fn capture(w: &mut Writer<'_>, cap: Capture) -> Result<CaptureResult> {
    if cap.nodes.is_empty() && cap.links.is_empty() {
        return Err(CoreError::Invalid("capture needs at least one node or link".into()));
    }
    if cap.nodes.len() > MAX_CAPTURE_NODES || cap.links.len() > MAX_CAPTURE_LINKS {
        return Err(CoreError::Invalid(format!(
            "capture is limited to {MAX_CAPTURE_NODES} nodes and {MAX_CAPTURE_LINKS} links"
        )));
    }

    let mut keys: HashMap<String, NodeId> = HashMap::new();
    let mut result = CaptureResult {
        change_set: None,
        nodes: Vec::new(),
        edges: Vec::new(),
        possible_duplicates: Vec::new(),
    };

    for cn in cap.nodes {
        let existing = rows::nodes_titled(w.conn, &cn.node.title)?;
        let mut input = cn.node;
        if let Some(p) = &cap.project {
            input.projects.push(p.clone());
        }
        let node = create(w, input)?;
        for dup in existing {
            result.possible_duplicates.push((node.id.clone(), dup));
        }
        if let Some(key) = cn.key {
            keys.insert(key.trim().to_lowercase(), node.id.clone());
        }
        result.nodes.push(node);
    }

    let resolve = |w: &Writer<'_>, reference: &str| -> Result<NodeId> {
        match keys.get(&reference.trim().to_lowercase()) {
            Some(id) => Ok(id.clone()),
            None => refs::resolve_node(w.conn, reference),
        }
    };
    let origin = if w.actor_is_ai { EdgeOrigin::Ai } else { EdgeOrigin::Manual };
    for link in cap.links {
        let src = resolve(w, &link.from)?;
        let dst = resolve(w, &link.to)?;
        let edge = edges::link(
            w,
            &NewEdge { src, dst, relation: link.relation, weight: None, note: link.note },
            origin,
        )?;
        result.edges.push(edge);
    }
    Ok(result)
}

fn join_paragraphs(a: &str, b: &str) -> String {
    match (a.trim_end().is_empty(), b.trim().is_empty()) {
        (true, _) => b.to_string(),
        (_, true) => a.to_string(),
        _ => format!("{}\n\n{}", a.trim_end(), b),
    }
}

pub(crate) fn replace_tags(
    w: &Writer<'_>,
    id: &NodeId,
    before: &[String],
    after: &[String],
) -> Result<()> {
    for tag in before.iter().filter(|t| !after.contains(t)) {
        w.conn
            .execute("DELETE FROM node_tags WHERE node_id = ?1 AND tag = ?2", params![id, tag])?;
    }
    for tag in after.iter().filter(|t| !before.contains(t)) {
        w.conn.execute("INSERT INTO node_tags (node_id, tag) VALUES (?1, ?2)", params![id, tag])?;
    }
    Ok(())
}

/// Makes the node's `mentions` edges from wikilinks match its body, and
/// records all targets (resolved or not) in `wikilink_refs` so nodes created
/// later can find their backlinks.
fn sync_outgoing_wikilinks(w: &mut Writer<'_>, node: &Node) -> Result<()> {
    let targets = refresh_wikilink_refs(w.conn, &node.id, &node.body)?;
    let mut desired: Vec<NodeId> = Vec::new();
    for target in &targets {
        if let Some(hit) = rows::nodes_titled(w.conn, target)?.into_iter().find(|b| b.id != node.id)
            && !desired.contains(&hit.id)
        {
            desired.push(hit.id);
        }
    }

    let existing: Vec<_> = edges::active_edges_from(w.conn, &node.id)?
        .into_iter()
        .filter(|e| e.origin == EdgeOrigin::Wikilink)
        .collect();
    for edge in existing.iter().filter(|e| !desired.contains(&e.dst)) {
        edges::soft_delete(w, edge)?;
    }
    for dst in desired.iter().filter(|d| !existing.iter().any(|e| &e.dst == *d)) {
        if rows::active_edge(w.conn, &node.id, dst, WIKILINK_RELATION)?.is_none() {
            edges::insert(w, &node.id, dst, WIKILINK_RELATION, 1.0, None, EdgeOrigin::Wikilink)?;
        }
    }
    Ok(())
}

/// Rewrites the node's rows in `wikilink_refs`; returns the targets.
pub(crate) fn refresh_wikilink_refs(
    conn: &rusqlite::Connection,
    id: &NodeId,
    body: &str,
) -> Result<Vec<String>> {
    let targets = wikilinks::targets(body);
    conn.execute("DELETE FROM wikilink_refs WHERE src = ?1", [id])?;
    for target in &targets {
        conn.execute(
            "INSERT OR IGNORE INTO wikilink_refs (src, target) VALUES (?1, ?2)",
            params![id, target.to_lowercase()],
        )?;
    }
    Ok(targets)
}

/// Connects existing nodes whose body already links to `[[node.title]]`.
fn backfill_incoming_wikilinks(w: &mut Writer<'_>, node: &Node) -> Result<()> {
    let mut stmt = w.conn.prepare_cached(
        "SELECT r.src FROM wikilink_refs r JOIN nodes n ON n.id = r.src
         WHERE r.target = ?1 AND r.src != ?2 AND n.deleted_at IS NULL",
    )?;
    let sources: Vec<NodeId> = stmt
        .query_map(params![node.title.to_lowercase(), node.id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);

    for src in sources {
        if rows::active_edge(w.conn, &src, &node.id, WIKILINK_RELATION)?.is_none() {
            edges::insert(w, &src, &node.id, WIKILINK_RELATION, 1.0, None, EdgeOrigin::Wikilink)?;
        }
    }
    Ok(())
}
