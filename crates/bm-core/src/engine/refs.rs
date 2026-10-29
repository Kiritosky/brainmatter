//! Resolving human/AI-friendly references: full id, exact title, or short id
//! (last hex digits of the UUID).

use rusqlite::Connection;

use super::{rows, search};
use crate::{
    error::{CoreError, Result},
    ids::{NodeId, ProjectId},
    model::{NodeBrief, SearchQuery},
};

fn is_uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok()
}

fn is_short_id(s: &str) -> bool {
    (6..=12).contains(&s.len()) && s.chars().all(|c| c.is_ascii_hexdigit())
}

pub(crate) fn resolve_node(conn: &Connection, reference: &str) -> Result<NodeId> {
    let r = reference.trim();
    if r.is_empty() {
        return Err(CoreError::Invalid("empty node reference".into()));
    }
    if is_uuid(r) {
        let id = NodeId::from_raw(r.to_lowercase());
        rows::live_node(conn, &id)?;
        return Ok(id);
    }

    let mut titled = rows::nodes_titled(conn, r)?;
    match titled.len() {
        1 => return Ok(titled.remove(0).id),
        n if n > 1 => {
            return Err(CoreError::Ambiguous { reference: r.into(), candidates: titled });
        }
        _ => {}
    }

    let short = r.trim_start_matches('#').to_lowercase();
    if is_short_id(&short) {
        let mut stmt = conn.prepare_cached(
            "SELECT id, title, kind FROM nodes WHERE deleted_at IS NULL AND id LIKE '%' || ?1",
        )?;
        let mut hits: Vec<NodeBrief> =
            stmt.query_map([&short], rows::brief_from_row)?.collect::<rusqlite::Result<_>>()?;
        match hits.len() {
            1 => return Ok(hits.remove(0).id),
            n if n > 1 => {
                return Err(CoreError::Ambiguous { reference: r.into(), candidates: hits });
            }
            _ => {}
        }
    }

    let mut q = SearchQuery::new(r);
    q.limit = 5;
    let suggestions = search::search(conn, &q)?.into_iter().map(|h| h.node).collect();
    Err(CoreError::NotFound { kind: "node", reference: r.into(), suggestions })
}

pub(crate) fn resolve_project(conn: &Connection, reference: &str) -> Result<ProjectId> {
    let r = reference.trim();
    if is_uuid(r) {
        let id = ProjectId::from_raw(r.to_lowercase());
        rows::live_project(conn, &id)?;
        return Ok(id);
    }
    let mut stmt = conn.prepare_cached(
        "SELECT id FROM projects WHERE deleted_at IS NULL
         AND (name = ?1 COLLATE NOCASE OR (length(?2) >= 6 AND id LIKE '%' || ?2))",
    )?;
    let short = r.trim_start_matches('#').to_lowercase();
    let mut ids: Vec<ProjectId> =
        stmt.query_map([r, short.as_str()], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
    match ids.len() {
        1 => Ok(ids.remove(0)),
        0 => Err(CoreError::not_found("project", r)),
        _ => Err(CoreError::Invalid(format!("'{r}' matches several projects; use its id"))),
    }
}
