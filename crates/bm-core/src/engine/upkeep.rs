//! Upkeep of the graph: how often entries are read, which paths in them are out
//! of date, and which `[[links]]` point to no entry. All of it is view state on
//! the writer connection, outside the undo history: reading changes nothing.

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};

use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    error::Result,
    ids::{ActorId, NodeId, ProjectId},
    model::{MissingLink, NodeBrief, OutdatedEntry, ReadStats},
    paths, wikilinks,
};

/// Reads by the same reader closer together than this count once.
pub(crate) const READ_WINDOW_MS: i64 = 30 * 60 * 1000;
/// Kinds that the AI apps read from files of their own, so their count says little.
const UNCOUNTED_KINDS: &str = "'template', 'skill'";

pub(crate) fn note_read(
    conn: &Connection,
    node: &NodeId,
    reader: &ActorId,
    now: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO node_stats (node_id, reads, last_read_at, last_read_by) VALUES (?1, 1, ?2, ?3)
         ON CONFLICT (node_id) DO UPDATE SET
             reads = reads + CASE
                 WHEN last_read_by = excluded.last_read_by
                  AND excluded.last_read_at - last_read_at < ?4 THEN 0 ELSE 1 END,
             last_read_at = excluded.last_read_at,
             last_read_by = excluded.last_read_by",
        params![node, now, reader, READ_WINDOW_MS],
    )?;
    Ok(())
}

pub(crate) fn read_stats(conn: &Connection, node: &NodeId) -> Result<Option<ReadStats>> {
    Ok(conn
        .query_row(
            "SELECT s.reads, s.last_read_at, a.name FROM node_stats s
             JOIN actors a ON a.id = s.last_read_by
             WHERE s.node_id = ?1 AND s.reads > 0",
            [node],
            |r| Ok(ReadStats { count: r.get(0)?, last_at: r.get(1)?, last_by: r.get(2)? }),
        )
        .optional()?)
}

pub(crate) fn checked_at(conn: &Connection, node: &NodeId) -> Result<Option<i64>> {
    Ok(conn
        .query_row("SELECT checked_at FROM node_stats WHERE node_id = ?1", [node], |r| r.get(0))
        .optional()?
        .flatten())
}

pub(crate) fn confirm_current(conn: &Connection, node: &NodeId, now: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO node_stats (node_id, checked_at) VALUES (?1, ?2)
         ON CONFLICT (node_id) DO UPDATE SET checked_at = excluded.checked_at",
        params![node, now],
    )?;
    Ok(())
}

/// Since when reads are counted: the update that introduced counting, or the
/// vault's creation.
pub(crate) fn reads_since(conn: &Connection) -> Result<i64> {
    let value: Option<String> = conn
        .query_row("SELECT value FROM vault_meta WHERE key = 'reads_since'", [], |r| r.get(0))
        .optional()?;
    Ok(value.and_then(|v| v.parse().ok()).unwrap_or(0))
}

/// Entries nobody read or changed since `cutoff`, the longest unused first.
/// Entries count only from when reads are counted.
pub(crate) fn unread_since(conn: &Connection, cutoff: i64, limit: usize) -> Result<Vec<NodeBrief>> {
    if reads_since(conn)? >= cutoff {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT n.id, n.title, n.kind FROM nodes n
         LEFT JOIN node_stats s ON s.node_id = n.id
         WHERE n.deleted_at IS NULL AND n.kind NOT IN ({UNCOUNTED_KINDS})
           AND max(coalesce(s.last_read_at, 0), n.updated_at) < ?1
         ORDER BY max(coalesce(s.last_read_at, 0), n.updated_at), n.title COLLATE NOCASE
         LIMIT ?2"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![cutoff, limit as i64], super::rows::brief_from_row)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

// ----------------------------------------------------------------- folders ---

pub(crate) fn set_folder(
    conn: &Connection,
    project: &ProjectId,
    path: Option<&str>,
    now: i64,
) -> Result<()> {
    match path {
        Some(path) => conn.execute(
            "INSERT INTO project_folders (project_id, path, set_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (project_id) DO UPDATE SET path = excluded.path, set_at = excluded.set_at",
            params![project, path, now],
        )?,
        None => conn.execute("DELETE FROM project_folders WHERE project_id = ?1", [project])?,
    };
    Ok(())
}

/// Folders of the entry's projects and their ancestors.
pub(crate) fn folders_of(conn: &Connection, node: &NodeId) -> Result<Vec<PathBuf>> {
    let mut stmt = conn.prepare_cached(
        "WITH RECURSIVE up(id, parent_id) AS (
             SELECT p.id, p.parent_id FROM project_members m
             JOIN projects p ON p.id = m.project_id
             WHERE m.node_id = ?1 AND p.deleted_at IS NULL
             UNION
             SELECT p.id, p.parent_id FROM projects p JOIN up ON p.id = up.parent_id
             WHERE p.deleted_at IS NULL
         )
         SELECT DISTINCT f.path FROM up JOIN project_folders f ON f.project_id = up.id
         ORDER BY f.path",
    )?;
    let rows = stmt
        .query_map([node], |r| r.get::<_, String>(0))?
        .map(|p| p.map(PathBuf::from))
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// What [`issues`] needs of one entry, read inside a transaction so the file
/// checks can run after it.
pub(crate) struct PathCheck {
    pub(crate) body: String,
    pub(crate) roots: Vec<PathBuf>,
    pub(crate) updated_at: i64,
    pub(crate) checked_at: Option<i64>,
}

impl PathCheck {
    pub(crate) fn load(
        conn: &Connection,
        node: &NodeId,
        body: &str,
        updated_at: i64,
    ) -> Result<Self> {
        Ok(Self {
            body: body.to_string(),
            roots: folders_of(conn, node)?,
            updated_at,
            checked_at: checked_at(conn, node)?,
        })
    }

    /// Looks at the files. Once the user confirmed the entry after its last edit,
    /// only files changed after that count, and missing ones no longer do.
    pub(crate) fn issues(&self) -> Vec<crate::model::PathIssue> {
        if self.roots.is_empty() {
            return Vec::new();
        }
        let confirmed = self.checked_at.is_some_and(|c| c >= self.updated_at);
        let since = self.checked_at.map_or(self.updated_at, |c| c.max(self.updated_at));
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let mut issues =
            paths::check(&paths::mentioned(&self.body), &self.roots, home.as_deref(), since);
        if confirmed {
            issues.retain(|i| !i.missing);
        }
        issues
    }
}

/// Every entry in a project with a folder (or below one), with what to check.
pub(crate) fn path_checks(conn: &Connection) -> Result<Vec<(NodeBrief, PathCheck)>> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE tree(id, root) AS (
             SELECT f.project_id, f.path FROM project_folders f
             JOIN projects p ON p.id = f.project_id WHERE p.deleted_at IS NULL
             UNION
             SELECT p.id, tree.root FROM projects p JOIN tree ON p.parent_id = tree.id
             WHERE p.deleted_at IS NULL
         )
         SELECT DISTINCT m.node_id, tree.root FROM tree
         JOIN project_members m ON m.project_id = tree.id",
    )?;
    let mut roots: BTreeMap<NodeId, Vec<PathBuf>> = BTreeMap::new();
    for row in stmt.query_map([], |r| Ok((r.get::<_, NodeId>(0)?, r.get::<_, String>(1)?)))? {
        let (node, root) = row?;
        roots.entry(node).or_default().push(PathBuf::from(root));
    }

    let mut node_stmt = conn.prepare_cached(
        "SELECT n.title, n.kind, n.body, n.updated_at, s.checked_at FROM nodes n
         LEFT JOIN node_stats s ON s.node_id = n.id
         WHERE n.id = ?1 AND n.deleted_at IS NULL",
    )?;
    let mut out = Vec::new();
    for (id, roots) in roots {
        let row = node_stmt
            .query_row([&id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, Option<i64>>(4)?,
                ))
            })
            .optional()?;
        let Some((title, kind, body, updated_at, checked_at)) = row else { continue };
        out.push((
            NodeBrief { id, title, kind },
            PathCheck { body, roots, updated_at, checked_at },
        ));
    }
    Ok(out)
}

/// Entries with gone or changed paths: those with something gone first.
pub(crate) fn outdated(checks: Vec<(NodeBrief, PathCheck)>, limit: usize) -> Vec<OutdatedEntry> {
    let mut found: Vec<OutdatedEntry> = checks
        .into_iter()
        .filter_map(|(node, check)| {
            let issues = check.issues();
            (!issues.is_empty()).then_some(OutdatedEntry { node, issues })
        })
        .collect();
    let gone = |e: &OutdatedEntry| e.issues.iter().filter(|i| i.missing).count();
    found.sort_by(|a, b| {
        gone(b)
            .cmp(&gone(a))
            .then_with(|| b.issues.len().cmp(&a.issues.len()))
            .then_with(|| a.node.title.to_lowercase().cmp(&b.node.title.to_lowercase()))
    });
    found.truncate(limit);
    found
}

// ----------------------------------------------------------- missing links ---

/// `[[Titles]]` that no entry has, the most linked first.
pub(crate) fn missing_links(conn: &Connection, limit: usize) -> Result<Vec<MissingLink>> {
    let mut stmt = conn.prepare(
        "SELECT r.target, n.id, n.title, n.kind FROM wikilink_refs r
         JOIN nodes n ON n.id = r.src AND n.deleted_at IS NULL
         WHERE r.target NOT LIKE '%{{%' AND NOT EXISTS (
             SELECT 1 FROM nodes t WHERE bm_fold(t.title) = r.target AND t.deleted_at IS NULL
         )
         ORDER BY r.target, n.title COLLATE NOCASE",
    )?;
    let mut by_target: HashMap<String, Vec<NodeBrief>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for row in stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, NodeBrief { id: r.get(1)?, title: r.get(2)?, kind: r.get(3)? }))
    })? {
        let (target, source) = row?;
        if !by_target.contains_key(&target) {
            order.push(target.clone());
        }
        by_target.entry(target).or_default().push(source);
    }
    order.sort_by(|a, b| by_target[b].len().cmp(&by_target[a].len()).then_with(|| a.cmp(b)));
    order.truncate(limit);

    let mut body_stmt = conn.prepare_cached("SELECT body FROM nodes WHERE id = ?1")?;
    let mut out = Vec::with_capacity(order.len());
    for target in order {
        let sources = by_target.remove(&target).unwrap_or_default();
        // The title as written, not lowercased.
        let body: String = body_stmt.query_row([&sources[0].id], |r| r.get(0))?;
        let title = wikilinks::targets(&body)
            .into_iter()
            .find(|t| wikilinks::fold(t) == target)
            .unwrap_or(target);
        out.push(MissingLink { title, sources });
    }
    Ok(out)
}
