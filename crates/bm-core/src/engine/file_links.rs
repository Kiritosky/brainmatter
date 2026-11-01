//! Links from entries to files and folders on the Mac. Only the reference is
//! stored, never the content. Versioned like attachments, so they can be undone.
//!
//! `approved_for_ai` decides whether AI clients may read the file through
//! Brainmatter. A link an AI creates starts without it: otherwise any client could
//! read any file by linking it first.

use rusqlite::{Connection, OptionalExtension, Row, params};

use super::writer::{Entity, Writer};
use crate::{
    error::{CoreError, Result},
    events::Event,
    ids::{FileLinkId, NodeId},
    model::FileLink,
};

const COLS: &str = "id, node_id, path, label, approved_for_ai, created_at, created_by, deleted_at";

fn from_row(r: &Row<'_>) -> rusqlite::Result<FileLink> {
    Ok(FileLink {
        id: r.get(0)?,
        node_id: r.get(1)?,
        path: r.get(2)?,
        label: r.get(3)?,
        approved_for_ai: r.get::<_, i64>(4)? != 0,
        created_at: r.get(5)?,
        created_by: r.get(6)?,
        deleted_at: r.get(7)?,
    })
}

pub(crate) fn get(conn: &Connection, id: &FileLinkId) -> Result<Option<FileLink>> {
    let sql = format!("SELECT {COLS} FROM file_links WHERE id = ?1");
    Ok(conn.query_row(&sql, [id], from_row).optional()?)
}

pub(crate) fn of_node(conn: &Connection, node: &NodeId) -> Result<Vec<FileLink>> {
    let sql = format!(
        "SELECT {COLS} FROM file_links WHERE node_id = ?1 AND deleted_at IS NULL
         ORDER BY created_at, id"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt.query_map([node], from_row)?.collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// An absolute path without `..`, so a link says plainly where it points.
pub(crate) fn clean_path(path: &str) -> Result<String> {
    let path = path.trim();
    let valid = path.starts_with('/')
        && !path.chars().any(char::is_control)
        && !path.split('/').any(|part| part == "..");
    if !valid {
        return Err(CoreError::Invalid(format!("not an absolute file path: {path}")));
    }
    let trimmed = path.trim_end_matches('/');
    Ok(if trimmed.is_empty() { "/".to_string() } else { trimmed.to_string() })
}

pub(crate) fn label_for(path: &str, label: Option<&str>) -> String {
    label
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| path.rsplit('/').next().filter(|n| !n.is_empty()).unwrap_or(path))
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect()
}

/// Links `path`, or returns the link the entry already has to it. Only the user
/// gets a link that AI clients may read right away.
pub(crate) fn insert(
    w: &mut Writer<'_>,
    node: &NodeId,
    path: &str,
    label: &str,
    bookmark: Option<&[u8]>,
) -> Result<FileLink> {
    if let Some(existing) = of_node(w.conn, node)?.into_iter().find(|l| l.path == path) {
        return Ok(existing);
    }
    let link = FileLink {
        id: FileLinkId::new(),
        node_id: node.clone(),
        path: path.to_string(),
        label: label.to_string(),
        approved_for_ai: !w.actor_is_ai,
        created_at: w.now,
        created_by: w.actor.clone(),
        deleted_at: None,
    };
    w.conn.execute(
        "INSERT INTO file_links
             (id, node_id, path, label, bookmark, approved_for_ai, created_at, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            link.id,
            link.node_id,
            link.path,
            link.label,
            bookmark,
            link.approved_for_ai,
            link.created_at,
            link.created_by
        ],
    )?;
    w.record(Entity::FileLink, link.id.as_str(), 0, None::<&FileLink>, Some(&link))?;
    w.emit(Event::NodeUpserted { id: node.clone() });
    Ok(link)
}

/// Writes a changed row back. The path is left alone: it follows the file on
/// disk and is not part of the history.
pub(crate) fn store(w: &mut Writer<'_>, before: &FileLink, after: &FileLink) -> Result<()> {
    w.conn.execute(
        "UPDATE file_links SET node_id = ?2, approved_for_ai = ?3, deleted_at = ?4 WHERE id = ?1",
        params![after.id, after.node_id, after.approved_for_ai, after.deleted_at],
    )?;
    w.record(Entity::FileLink, after.id.as_str(), 0, Some(before), Some(after))?;
    w.emit(Event::NodeUpserted { id: before.node_id.clone() });
    if after.node_id != before.node_id {
        w.emit(Event::NodeUpserted { id: after.node_id.clone() });
    }
    Ok(())
}

pub(crate) fn move_to(w: &mut Writer<'_>, link: &FileLink, node: &NodeId) -> Result<()> {
    let after = FileLink { node_id: node.clone(), ..link.clone() };
    store(w, link, &after)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_must_be_absolute_and_plain() {
        assert_eq!(clean_path(" /a/b.md ").unwrap(), "/a/b.md");
        assert_eq!(clean_path("/a/ordner/").unwrap(), "/a/ordner");
        assert_eq!(clean_path("/").unwrap(), "/");
        for bad in ["a/b.md", "~/b.md", "/a/../etc/passwd", "/a\n/b", ""] {
            assert!(clean_path(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn labels_default_to_the_file_name() {
        assert_eq!(label_for("/a/Spec.md", None), "Spec.md");
        assert_eq!(label_for("/a/Spec.md", Some("  ")), "Spec.md");
        assert_eq!(label_for("/a/Spec.md", Some("Die Spec")), "Die Spec");
        assert_eq!(label_for("/", None), "/");
    }
}
