//! Schema migrations, tracked via `PRAGMA user_version`.

use rusqlite::Connection;

use super::StoreError;

type DataStep = fn(&Connection) -> rusqlite::Result<()>;

/// Index `i` migrates the schema from version `i` to `i + 1`: DDL, then an
/// optional Rust step for data that SQL cannot compute. Never edit a released
/// migration; append a new one instead.
const MIGRATIONS: &[(&str, Option<DataStep>)] =
    &[(V1, None), (V2, Some(backfill_wikilink_refs)), (V3, None), (V4, None)];

pub(super) fn migrate(conn: &mut Connection) -> Result<(), StoreError> {
    let current: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let latest = MIGRATIONS.len() as u32;
    if current > latest {
        return Err(StoreError::SchemaTooNew { found: current, supported: latest });
    }
    for (i, (sql, data)) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        if let Some(step) = data {
            step(&tx)?;
        }
        tx.pragma_update(None, "user_version", i as u32 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

// Timestamps are Unix epoch milliseconds. IDs are UUIDv7 strings.
const V1: &str = r#"
CREATE TABLE actors (
    id            TEXT PRIMARY KEY,
    kind          TEXT NOT NULL CHECK (kind IN ('user', 'ai')),
    name          TEXT NOT NULL,
    color         TEXT,
    token_hash    BLOB UNIQUE,
    scopes        TEXT NOT NULL DEFAULT '[]',
    project_scope TEXT,
    created_at    INTEGER NOT NULL,
    last_seen_at  INTEGER,
    revoked_at    INTEGER
);

-- `seq` is an explicit INTEGER PRIMARY KEY so the rowid stays stable across
-- VACUUM; nodes_fts is keyed on it.
CREATE TABLE nodes (
    seq        INTEGER PRIMARY KEY,
    id         TEXT NOT NULL UNIQUE,
    title      TEXT NOT NULL,
    body       TEXT NOT NULL DEFAULT '',
    kind       TEXT NOT NULL DEFAULT 'note',
    props      TEXT,
    reviewed   INTEGER NOT NULL DEFAULT 1,
    rev        INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    created_by TEXT NOT NULL REFERENCES actors(id),
    updated_by TEXT NOT NULL REFERENCES actors(id),
    deleted_at INTEGER
);
CREATE INDEX nodes_title_idx ON nodes(title COLLATE NOCASE) WHERE deleted_at IS NULL;
CREATE INDEX nodes_updated_idx ON nodes(updated_at);

CREATE TABLE edges (
    id         TEXT PRIMARY KEY,
    src        TEXT NOT NULL REFERENCES nodes(id),
    dst        TEXT NOT NULL REFERENCES nodes(id),
    relation   TEXT NOT NULL DEFAULT 'related_to',
    weight     REAL NOT NULL DEFAULT 1.0,
    note       TEXT,
    origin     TEXT NOT NULL DEFAULT 'manual'
               CHECK (origin IN ('manual', 'wikilink', 'ai', 'suggestion')),
    created_at INTEGER NOT NULL,
    created_by TEXT NOT NULL REFERENCES actors(id),
    deleted_at INTEGER
);
CREATE UNIQUE INDEX edges_unique_idx ON edges(src, dst, relation) WHERE deleted_at IS NULL;
CREATE INDEX edges_dst_idx ON edges(dst);

CREATE TABLE projects (
    id          TEXT PRIMARY KEY,
    parent_id   TEXT REFERENCES projects(id),
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    color       TEXT,
    icon        TEXT,
    sort        INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    created_by  TEXT NOT NULL REFERENCES actors(id),
    deleted_at  INTEGER
);
CREATE INDEX projects_parent_idx ON projects(parent_id);

CREATE TABLE project_members (
    project_id TEXT NOT NULL REFERENCES projects(id),
    node_id    TEXT NOT NULL REFERENCES nodes(id),
    added_at   INTEGER NOT NULL,
    added_by   TEXT NOT NULL REFERENCES actors(id),
    PRIMARY KEY (project_id, node_id)
) WITHOUT ROWID;
CREATE INDEX project_members_node_idx ON project_members(node_id);

CREATE TABLE node_tags (
    node_id TEXT NOT NULL REFERENCES nodes(id),
    tag     TEXT NOT NULL,
    PRIMARY KEY (node_id, tag)
) WITHOUT ROWID;
CREATE INDEX node_tags_tag_idx ON node_tags(tag);

-- Persisted graph layout per view, so the mental map stays stable.
-- view_id is a project id or 'all'.
CREATE TABLE layout_positions (
    view_id TEXT NOT NULL,
    node_id TEXT NOT NULL REFERENCES nodes(id),
    x       REAL NOT NULL,
    y       REAL NOT NULL,
    pinned  INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (view_id, node_id)
) WITHOUT ROWID;

CREATE TABLE attachments (
    id         TEXT PRIMARY KEY,
    node_id    TEXT NOT NULL REFERENCES nodes(id),
    filename   TEXT NOT NULL,
    mime       TEXT NOT NULL,
    size       INTEGER NOT NULL,
    blob_id    TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    created_by TEXT NOT NULL REFERENCES actors(id),
    deleted_at INTEGER
);
CREATE INDEX attachments_node_idx ON attachments(node_id);

-- One change set per user action or MCP tool call; the unit of undo.
CREATE TABLE change_sets (
    id          TEXT PRIMARY KEY,
    actor_id    TEXT NOT NULL REFERENCES actors(id),
    client_name TEXT,
    tool        TEXT,
    summary     TEXT NOT NULL DEFAULT '',
    at          INTEGER NOT NULL,
    reverted_by TEXT REFERENCES change_sets(id)
);
CREATE INDEX change_sets_at_idx ON change_sets(at);

-- before/after are zstd-compressed JSON snapshots; NULL means "did not exist".
CREATE TABLE revisions (
    id            INTEGER PRIMARY KEY,
    change_set_id TEXT NOT NULL REFERENCES change_sets(id),
    entity_type   TEXT NOT NULL,
    entity_id     TEXT NOT NULL,
    rev           INTEGER NOT NULL,
    before        BLOB,
    after         BLOB
);
CREATE INDEX revisions_entity_idx ON revisions(entity_type, entity_id);
CREATE INDEX revisions_change_set_idx ON revisions(change_set_id);

CREATE TABLE embeddings (
    node_id      TEXT PRIMARY KEY REFERENCES nodes(id),
    model        TEXT NOT NULL,
    dim          INTEGER NOT NULL,
    vec          BLOB NOT NULL,
    content_hash BLOB NOT NULL
);

CREATE TABLE link_suggestions (
    src        TEXT NOT NULL REFERENCES nodes(id),
    dst        TEXT NOT NULL REFERENCES nodes(id),
    score      REAL NOT NULL,
    status     TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'accepted', 'dismissed')),
    created_at INTEGER NOT NULL,
    PRIMARY KEY (src, dst)
) WITHOUT ROWID;

-- Never stores note content, only what was called by whom.
CREATE TABLE activity_log (
    id           INTEGER PRIMARY KEY,
    actor_id     TEXT NOT NULL REFERENCES actors(id),
    tool         TEXT NOT NULL,
    args_summary TEXT,
    duration_ms  INTEGER,
    at           INTEGER NOT NULL
);
CREATE INDEX activity_log_at_idx ON activity_log(at);

-- Full-text index. Lives inside SQLCipher, so it is encrypted at rest too.
-- Kept in sync by triggers; soft-deleted nodes are removed from the index.
CREATE VIRTUAL TABLE nodes_fts USING fts5(
    title, body, tags,
    tokenize = "unicode61 remove_diacritics 2",
    prefix = '2 3'
);

CREATE TRIGGER nodes_fts_ai AFTER INSERT ON nodes WHEN new.deleted_at IS NULL BEGIN
    INSERT INTO nodes_fts(rowid, title, body, tags) VALUES (new.seq, new.title, new.body, '');
END;

CREATE TRIGGER nodes_fts_au AFTER UPDATE OF title, body, deleted_at ON nodes BEGIN
    DELETE FROM nodes_fts WHERE rowid = old.seq;
    INSERT INTO nodes_fts(rowid, title, body, tags)
        SELECT new.seq, new.title, new.body,
               coalesce((SELECT group_concat(tag, ' ') FROM node_tags WHERE node_id = new.id), '')
        WHERE new.deleted_at IS NULL;
END;

CREATE TRIGGER nodes_fts_ad AFTER DELETE ON nodes BEGIN
    DELETE FROM nodes_fts WHERE rowid = old.seq;
END;

CREATE TRIGGER node_tags_fts_ai AFTER INSERT ON node_tags BEGIN
    UPDATE nodes_fts
       SET tags = (SELECT group_concat(tag, ' ') FROM node_tags WHERE node_id = new.node_id)
     WHERE rowid = (SELECT seq FROM nodes WHERE id = new.node_id);
END;

CREATE TRIGGER node_tags_fts_ad AFTER DELETE ON node_tags BEGIN
    UPDATE nodes_fts
       SET tags = coalesce((SELECT group_concat(tag, ' ') FROM node_tags WHERE node_id = old.node_id), '')
     WHERE rowid = (SELECT seq FROM nodes WHERE id = old.node_id);
END;
"#;

// Every `[[target]]` per node (lowercased), resolved or not, so creating a node
// can find the entries that already link to its title via an index lookup.
const V2: &str = r#"
CREATE TABLE wikilink_refs (
    src    TEXT NOT NULL REFERENCES nodes(id),
    target TEXT NOT NULL,
    PRIMARY KEY (target, src)
) WITHOUT ROWID;
CREATE INDEX wikilink_refs_src_idx ON wikilink_refs(src);
"#;

fn backfill_wikilink_refs(conn: &Connection) -> rusqlite::Result<()> {
    let mut select = conn.prepare("SELECT id, body FROM nodes WHERE deleted_at IS NULL")?;
    let mut insert =
        conn.prepare("INSERT OR IGNORE INTO wikilink_refs (src, target) VALUES (?1, ?2)")?;
    let rows: Vec<(String, String)> =
        select.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    for (id, body) in rows {
        for target in crate::wikilinks::targets(&body) {
            insert.execute(rusqlite::params![id, target.to_lowercase()])?;
        }
    }
    Ok(())
}

// Built-in FTS5 `rank` with our column weights (title 5×, body 1×, tags 2×), so
// `ORDER BY rank LIMIT n` can rank without an extra bm25() call per row.
const V3: &str = r#"
INSERT INTO nodes_fts(nodes_fts, rank) VALUES ('rank', 'bm25(5.0, 1.0, 2.0)');
"#;

// Consecutive edits of one entry by one actor share a change set, so autosave
// and multi-step AI edits stay one undo step.
const V4: &str = r#"
ALTER TABLE change_sets ADD COLUMN coalesce_key TEXT;
"#;
