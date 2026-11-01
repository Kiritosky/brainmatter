//! Documentation pages per project: entries of kind `doc`, plus their place in a
//! manually ordered tree. The page's text is versioned like any entry; its place
//! is view state, like a graph layout, and not part of the undo history.

use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, params};

use super::{nodes, rows, writer::Writer};
use crate::{
    docs::Scan,
    error::{CoreError, Result},
    events::Event,
    ids::{DocSourceId, NodeId, ProjectId},
    model::{BodyEdit, DOC_KIND, DocPage, DocSource, DocSyncReport, NewNode, Node, NodePatch},
};

/// The pages of a project in reading order. A page whose parent is gone moves up
/// to the top level rather than disappearing.
pub(crate) fn tree(conn: &Connection, project: &ProjectId) -> Result<Vec<DocPage>> {
    struct Row {
        page: DocPage,
        sort: i64,
    }
    let mut stmt = conn.prepare_cached(
        "SELECT d.node_id, n.title, n.kind, d.parent_id, d.sort, d.source_id, d.source_path,
                n.reviewed, n.updated_at
         FROM doc_pages d JOIN nodes n ON n.id = d.node_id
         WHERE d.project_id = ?1 AND n.deleted_at IS NULL",
    )?;
    let rows: Vec<Row> = stmt
        .query_map([project], |r| {
            let source: Option<DocSourceId> = r.get(5)?;
            Ok(Row {
                page: DocPage {
                    node: rows::brief_from_row(r)?,
                    parent: r.get(3)?,
                    depth: 0,
                    read_only: source.is_some(),
                    source_path: r.get(6)?,
                    reviewed: r.get::<_, i64>(7)? != 0,
                    updated_at: r.get(8)?,
                    tags: Vec::new(),
                },
                sort: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut tags: HashMap<NodeId, Vec<String>> = HashMap::new();
    let mut stmt = conn.prepare_cached(
        "SELECT t.node_id, t.tag FROM node_tags t JOIN doc_pages d ON d.node_id = t.node_id
         WHERE d.project_id = ?1 ORDER BY t.tag",
    )?;
    let found = stmt.query_map([project], |r| Ok((r.get::<_, NodeId>(0)?, r.get(1)?)))?;
    for row in found {
        let (id, tag) = row?;
        tags.entry(id).or_default().push(tag);
    }

    let live: HashSet<NodeId> = rows.iter().map(|r| r.page.node.id.clone()).collect();
    let mut children: HashMap<Option<NodeId>, Vec<Row>> = HashMap::new();
    for mut row in rows {
        row.page.parent = row.page.parent.filter(|p| live.contains(p));
        row.page.tags = tags.remove(&row.page.node.id).unwrap_or_default();
        children.entry(row.page.parent.clone()).or_default().push(row);
    }
    for list in children.values_mut() {
        list.sort_by(|a, b| {
            a.sort.cmp(&b.sort).then_with(|| a.page.node.title.cmp(&b.page.node.title))
        });
    }
    fn visit(
        parent: Option<NodeId>,
        depth: u32,
        children: &mut HashMap<Option<NodeId>, Vec<Row>>,
        out: &mut Vec<DocPage>,
    ) {
        for mut row in children.remove(&parent).unwrap_or_default() {
            row.page.depth = depth;
            let id = row.page.node.id.clone();
            out.push(row.page);
            visit(Some(id), depth + 1, children, out);
        }
    }
    let mut out = Vec::new();
    visit(None, 0, &mut children, &mut out);
    Ok(out)
}

/// `Some(project)` if this entry is a documentation page.
pub(crate) fn project_of(conn: &Connection, id: &NodeId) -> Result<Option<ProjectId>> {
    Ok(conn
        .query_row("SELECT project_id FROM doc_pages WHERE node_id = ?1", [id], |r| r.get(0))
        .optional()?)
}

pub(crate) fn is_mirrored(conn: &Connection, id: &NodeId) -> Result<bool> {
    let source: Option<Option<DocSourceId>> = conn
        .query_row("SELECT source_id FROM doc_pages WHERE node_id = ?1", [id], |r| r.get(0))
        .optional()?;
    Ok(source.flatten().is_some())
}

/// Pages from a folder are copies: changes belong into the file.
pub(crate) fn ensure_editable(conn: &Connection, id: &NodeId) -> Result<()> {
    if is_mirrored(conn, id)? {
        return Err(CoreError::Conflict(
            "this page mirrors a file and can only be changed there".into(),
        ));
    }
    Ok(())
}

fn next_sort(conn: &Connection, project: &ProjectId, parent: Option<&NodeId>) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT coalesce(max(sort), -1) + 1 FROM doc_pages
         WHERE project_id = ?1 AND parent_id IS ?2",
        params![project, parent],
        |r| r.get(0),
    )?)
}

fn check_parent(conn: &Connection, project: &ProjectId, parent: &NodeId) -> Result<()> {
    rows::live_node(conn, parent)?;
    if project_of(conn, parent)?.as_ref() != Some(project) {
        return Err(CoreError::Invalid("the parent page belongs to another project".into()));
    }
    Ok(())
}

pub(crate) fn create(
    w: &mut Writer<'_>,
    project: &ProjectId,
    title: &str,
    body: &str,
    parent: Option<&NodeId>,
    tags: Vec<String>,
) -> Result<Node> {
    rows::live_project(w.conn, project)?;
    if let Some(parent) = parent {
        check_parent(w.conn, project, parent)?;
    }
    let sort = next_sort(w.conn, project, parent)?;
    let input = NewNode {
        title: title.to_string(),
        body: body.to_string(),
        kind: Some(DOC_KIND.to_string()),
        projects: vec![project.clone()],
        tags,
        ..Default::default()
    };
    let node = nodes::create(w, input)?;
    w.conn.execute(
        "INSERT INTO doc_pages (node_id, project_id, parent_id, sort) VALUES (?1, ?2, ?3, ?4)",
        params![node.id, project, parent, sort],
    )?;
    Ok(node)
}

/// Puts a page under `parent` at `position` among its new siblings.
pub(crate) fn place(
    w: &mut Writer<'_>,
    id: &NodeId,
    parent: Option<&NodeId>,
    position: usize,
) -> Result<()> {
    let project = project_of(w.conn, id)?
        .ok_or_else(|| CoreError::not_found("documentation page", id.as_str()))?;
    ensure_editable(w.conn, id)?;
    if let Some(parent) = parent {
        check_parent(w.conn, &project, parent)?;
        // Walking up from the new parent must not reach the page itself.
        let mut cursor = Some(parent.clone());
        while let Some(current) = cursor {
            if &current == id {
                return Err(CoreError::Invalid("a page cannot go inside itself".into()));
            }
            cursor = w
                .conn
                .query_row("SELECT parent_id FROM doc_pages WHERE node_id = ?1", [&current], |r| {
                    r.get(0)
                })
                .optional()?
                .flatten();
        }
    }
    let mut stmt = w.conn.prepare_cached(
        "SELECT d.node_id FROM doc_pages d JOIN nodes n ON n.id = d.node_id
         WHERE d.project_id = ?1 AND d.parent_id IS ?2 AND d.node_id != ?3
           AND n.deleted_at IS NULL
         ORDER BY d.sort, n.title",
    )?;
    let mut siblings: Vec<NodeId> = stmt
        .query_map(params![project, parent, id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    siblings.insert(position.min(siblings.len()), id.clone());
    w.conn
        .execute("UPDATE doc_pages SET parent_id = ?2 WHERE node_id = ?1", params![id, parent])?;
    for (sort, sibling) in siblings.iter().enumerate() {
        w.conn.execute(
            "UPDATE doc_pages SET sort = ?2 WHERE node_id = ?1",
            params![sibling, sort as i64],
        )?;
    }
    w.emit(Event::NodeUpserted { id: id.clone() });
    Ok(())
}

// --------------------------------------------------------------- mirroring ---

fn source_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<DocSource> {
    Ok(DocSource {
        id: r.get(0)?,
        project_id: r.get(1)?,
        path: r.get(2)?,
        created_at: r.get(3)?,
        pages: r.get(4)?,
    })
}

const SOURCE_SELECT: &str = "SELECT s.id, s.project_id, s.path, s.created_at,
    (SELECT count(*) FROM doc_pages d JOIN nodes n ON n.id = d.node_id
     WHERE d.source_id = s.id AND n.deleted_at IS NULL)
    FROM doc_sources s WHERE s.deleted_at IS NULL";

pub(crate) fn sources(conn: &Connection, project: &ProjectId) -> Result<Vec<DocSource>> {
    let sql = format!("{SOURCE_SELECT} AND s.project_id = ?1 ORDER BY s.created_at, s.id");
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt.query_map([project], source_from_row)?.collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

pub(crate) fn source(conn: &Connection, id: &DocSourceId) -> Result<DocSource> {
    let sql = format!("{SOURCE_SELECT} AND s.id = ?1");
    conn.query_row(&sql, [id], source_from_row)
        .optional()?
        .ok_or_else(|| CoreError::not_found("documentation source", id.as_str()))
}

/// Brings the pages in line with what was read from the source: new files become
/// pages, changed ones are updated, and pages of vanished files go to the trash.
pub(crate) fn sync(w: &mut Writer<'_>, id: &DocSourceId, scan: &Scan) -> Result<DocSyncReport> {
    let source = source(w.conn, id)?;
    let mut report = DocSyncReport { skipped: scan.skipped, ..Default::default() };

    struct Known {
        node: NodeId,
        hash: Option<String>,
    }
    let mut known: HashMap<String, Known> = HashMap::new();
    {
        let mut stmt = w.conn.prepare_cached(
            "SELECT source_path, node_id, source_hash FROM doc_pages WHERE source_id = ?1",
        )?;
        let rows = stmt.query_map([id], |r| {
            Ok((r.get::<_, String>(0)?, Known { node: r.get(1)?, hash: r.get(2)? }))
        })?;
        for row in rows {
            let (path, page) = row?;
            known.insert(path, page);
        }
    }

    let mut placed: HashMap<&str, NodeId> = HashMap::new();
    for (sort, page) in scan.pages.iter().enumerate() {
        let parent = page.parent.as_deref().and_then(|p| placed.get(p)).cloned();
        let node = match known.remove(&page.path) {
            Some(existing) => {
                // A file that came back also brings its page back.
                let current = rows::node(w.conn, &existing.node)?;
                if current.as_ref().is_some_and(|n| n.deleted_at.is_some()) {
                    nodes::restore(w, &existing.node)?;
                    report.added += 1;
                } else if existing.hash.as_deref() == Some(page.hash.as_str()) {
                    report.unchanged += 1;
                } else {
                    report.updated += 1;
                }
                if existing.hash.as_deref() != Some(page.hash.as_str()) {
                    let patch = NodePatch {
                        title: Some(page.title.clone()),
                        body: Some(BodyEdit::Replace(page.body.clone())),
                        ..Default::default()
                    };
                    nodes::update(w, &existing.node, patch)?;
                }
                existing.node
            }
            None => {
                let input = NewNode {
                    title: page.title.clone(),
                    body: page.body.clone(),
                    kind: Some(DOC_KIND.to_string()),
                    projects: vec![source.project_id.clone()],
                    ..Default::default()
                };
                report.added += 1;
                let node = nodes::create(w, input)?;
                w.conn.execute(
                    "INSERT INTO doc_pages (node_id, project_id, source_id, source_path)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![node.id, source.project_id, id, page.path],
                )?;
                node.id
            }
        };
        w.conn.execute(
            "UPDATE doc_pages SET parent_id = ?2, sort = ?3, source_hash = ?4 WHERE node_id = ?1",
            params![node, parent, sort as i64, page.hash],
        )?;
        placed.insert(page.path.as_str(), node);
    }

    for gone in known.into_values() {
        if rows::node(w.conn, &gone.node)?.is_some_and(|n| n.deleted_at.is_none()) {
            nodes::delete(w, &gone.node)?;
            report.removed += 1;
        }
    }
    Ok(report)
}

/// Stops mirroring. The pages go to the trash, and the file stays untouched.
pub(crate) fn remove_source(w: &mut Writer<'_>, id: &DocSourceId) -> Result<()> {
    source(w.conn, id)?;
    let mut stmt = w.conn.prepare_cached(
        "SELECT d.node_id FROM doc_pages d JOIN nodes n ON n.id = d.node_id
         WHERE d.source_id = ?1 AND n.deleted_at IS NULL",
    )?;
    let pages: Vec<NodeId> =
        stmt.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    for page in pages {
        nodes::delete(w, &page)?;
    }
    w.conn.execute("UPDATE doc_sources SET deleted_at = ?2 WHERE id = ?1", params![id, w.now])?;
    Ok(())
}
