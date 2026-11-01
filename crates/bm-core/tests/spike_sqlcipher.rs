//! Spike 1: SQLCipher encrypts everything (incl. FTS5 and WAL), rejects wrong
//! keys, and full-text search behaves well for German text.

use bm_core::{Db, DbKey, StoreError};
use rusqlite::params;

const SECRET: &str = "Nebenläufigkeit mit der Tokio Runtime";

fn key(b: u8) -> DbKey {
    DbKey::from_bytes([b; 32])
}

fn seed(db: &Db) {
    let c = db.conn();
    c.execute(
        "INSERT INTO actors (id, kind, name, created_at) VALUES ('u', 'user', 'Lasse', 0)",
        [],
    )
    .unwrap();
    c.execute(
        "INSERT INTO nodes (id, title, body, created_at, updated_at, created_by, updated_by)
         VALUES ('n1', 'Async Runtime', ?1, 0, 0, 'u', 'u')",
        params![SECRET],
    )
    .unwrap();
    c.execute("INSERT INTO node_tags (node_id, tag) VALUES ('n1', 'rust')", []).unwrap();
}

fn fts(db: &Db, query: &str) -> Vec<String> {
    let mut stmt = db
        .conn()
        .prepare(
            "SELECT n.id FROM nodes_fts f JOIN nodes n ON n.seq = f.rowid
             WHERE nodes_fts MATCH ?1 ORDER BY bm25(nodes_fts, 5.0, 1.0, 2.0)",
        )
        .unwrap();
    stmt.query_map([query], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
}

#[test]
fn encrypted_roundtrip_and_fts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.db");

    {
        let db = Db::open(&path, &key(1)).unwrap();
        assert_eq!(db.schema_version().unwrap(), 7);
        seed(&db);

        assert_eq!(fts(&db, "tokio"), ["n1"]);
        assert_eq!(fts(&db, "nebenlaufigkeit"), ["n1"], "diacritics folded");
        assert_eq!(fts(&db, "Nebenläufigkeit"), ["n1"]);
        assert_eq!(fts(&db, "neben*"), ["n1"], "prefix query");
        assert_eq!(fts(&db, "tags:rust"), ["n1"], "tag indexed via trigger");
        assert_eq!(fts(&db, "async"), ["n1"], "title indexed");

        db.conn().execute("UPDATE nodes SET deleted_at = 1 WHERE id = 'n1'", []).unwrap();
        assert!(fts(&db, "tokio").is_empty(), "soft-deleted nodes leave the index");
        db.conn().execute("UPDATE nodes SET deleted_at = NULL WHERE id = 'n1'", []).unwrap();
        assert_eq!(fts(&db, "tags:rust"), ["n1"], "restore re-indexes tags");

        // Inspect files while the WAL still holds uncheckpointed pages.
        for suffix in ["", "-wal"] {
            let file = dir.path().join(format!("vault.db{suffix}"));
            let Ok(bytes) = std::fs::read(&file) else { continue };
            assert!(!bytes.starts_with(b"SQLite format 3\0"), "{suffix}: plaintext header");
            for needle in [SECRET.as_bytes(), b"Async Runtime", b"Lasse"] {
                assert!(
                    !bytes.windows(needle.len()).any(|w| w == needle),
                    "{suffix}: found plaintext {:?}",
                    String::from_utf8_lossy(needle)
                );
            }
        }
    }

    assert!(matches!(Db::open(&path, &key(2)), Err(StoreError::WrongKeyOrCorrupt)));

    let db = Db::open(&path, &key(1)).unwrap();
    assert_eq!(fts(&db, "tokio"), ["n1"]);
}
