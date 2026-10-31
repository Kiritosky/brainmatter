//! Files attached to entries. The rows are versioned like everything else, so
//! adding, removing and moving an attachment can be undone; the encrypted
//! content lives in the [`crate::blobs::BlobStore`].

use rusqlite::{Connection, OptionalExtension, Row, params};

use super::writer::{Entity, Writer};
use crate::{
    error::Result,
    events::Event,
    ids::{AttachmentId, NodeId},
    model::Attachment,
};

const COLS: &str = "id, node_id, filename, mime, size, blob_id, created_at, created_by, deleted_at";

fn from_row(r: &Row<'_>) -> rusqlite::Result<Attachment> {
    Ok(Attachment {
        id: r.get(0)?,
        node_id: r.get(1)?,
        filename: r.get(2)?,
        mime: r.get(3)?,
        size: r.get(4)?,
        blob_id: r.get(5)?,
        created_at: r.get(6)?,
        created_by: r.get(7)?,
        deleted_at: r.get(8)?,
    })
}

pub(crate) fn get(conn: &Connection, id: &AttachmentId) -> Result<Option<Attachment>> {
    let sql = format!("SELECT {COLS} FROM attachments WHERE id = ?1");
    Ok(conn.query_row(&sql, [id], from_row).optional()?)
}

pub(crate) fn of_node(conn: &Connection, node: &NodeId) -> Result<Vec<Attachment>> {
    let sql = format!(
        "SELECT {COLS} FROM attachments WHERE node_id = ?1 AND deleted_at IS NULL
         ORDER BY created_at, id"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt.query_map([node], from_row)?.collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

pub(crate) fn insert(
    w: &mut Writer<'_>,
    node: &NodeId,
    filename: &str,
    mime: &str,
    size: i64,
    blob_id: &str,
) -> Result<Attachment> {
    let a = Attachment {
        id: AttachmentId::new(),
        node_id: node.clone(),
        filename: filename.to_string(),
        mime: mime.to_string(),
        size,
        blob_id: blob_id.to_string(),
        created_at: w.now,
        created_by: w.actor.clone(),
        deleted_at: None,
    };
    w.conn.execute(
        "INSERT INTO attachments (id, node_id, filename, mime, size, blob_id, created_at, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![a.id, a.node_id, a.filename, a.mime, a.size, a.blob_id, a.created_at, a.created_by],
    )?;
    w.record(Entity::Attachment, a.id.as_str(), 0, None::<&Attachment>, Some(&a))?;
    w.emit(Event::NodeUpserted { id: node.clone() });
    Ok(a)
}

/// Writes a changed row back; used for removing, moving and reverting.
pub(crate) fn store(w: &mut Writer<'_>, before: &Attachment, after: &Attachment) -> Result<()> {
    w.conn.execute(
        "UPDATE attachments SET node_id = ?2, deleted_at = ?3 WHERE id = ?1",
        params![after.id, after.node_id, after.deleted_at],
    )?;
    w.record(Entity::Attachment, after.id.as_str(), 0, Some(before), Some(after))?;
    w.emit(Event::NodeUpserted { id: before.node_id.clone() });
    if after.node_id != before.node_id {
        w.emit(Event::NodeUpserted { id: after.node_id.clone() });
    }
    Ok(())
}

pub(crate) fn remove(w: &mut Writer<'_>, a: &Attachment) -> Result<()> {
    let after = Attachment { deleted_at: Some(w.now), ..a.clone() };
    store(w, a, &after)
}

pub(crate) fn move_to(w: &mut Writer<'_>, a: &Attachment, node: &NodeId) -> Result<()> {
    let after = Attachment { node_id: node.clone(), ..a.clone() };
    store(w, a, &after)
}

/// What to call an uploaded file: no path, no control characters.
pub(crate) fn clean_filename(name: &str) -> Option<String> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let clean: String = base.chars().filter(|c| !c.is_control()).take(200).collect();
    let clean = clean.trim();
    (!clean.is_empty() && clean != "." && clean != "..").then(|| clean.to_string())
}

/// A valid `type/subtype`, else guessed from the file name. The UI decides what
/// to render by this value, so it never passes through unchecked.
pub(crate) fn clean_mime(mime: Option<&str>, filename: &str) -> String {
    let valid = |m: &&str| {
        let mut parts = m.split('/');
        let token = |p: Option<&str>| {
            p.is_some_and(|p| {
                !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b"+-.".contains(&b))
            })
        };
        token(parts.next()) && token(parts.next()) && parts.next().is_none()
    };
    if let Some(m) = mime.map(str::trim).filter(valid) {
        return m.to_ascii_lowercase();
    }
    let ext = filename.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        Some("md" | "markdown") => "text/markdown",
        Some("txt" | "log") => "text/plain",
        Some("json") => "application/json",
        Some("csv") => "text/csv",
        _ => "application/octet-stream",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filenames_lose_paths_and_control_characters() {
        assert_eq!(clean_filename("../../etc/passwd").as_deref(), Some("passwd"));
        assert_eq!(clean_filename("C:\\x\\Bild 1.png").as_deref(), Some("Bild 1.png"));
        assert_eq!(clean_filename("a\u{0}b\n.txt").as_deref(), Some("ab.txt"));
        assert_eq!(clean_filename(".."), None);
        assert_eq!(clean_filename("  "), None);
    }

    #[test]
    fn mime_types_are_validated_or_guessed() {
        assert_eq!(clean_mime(Some("IMAGE/PNG"), "x"), "image/png");
        assert_eq!(clean_mime(Some("text/html; x=<script>"), "a.pdf"), "application/pdf");
        assert_eq!(clean_mime(None, "Foto.JPG"), "image/jpeg");
        assert_eq!(clean_mime(Some(""), "kein-typ"), "application/octet-stream");
    }
}
