//! Encrypted SQLite (SQLCipher) storage.

mod schema;

use std::path::Path;

use rusqlite::{Connection, OpenFlags, ffi};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("wrong key or database is corrupt")]
    WrongKeyOrCorrupt,
    #[error("SQLCipher is not linked; refusing to open an unencrypted database")]
    CipherUnavailable,
    #[error("database schema v{found} is newer than this build supports (v{supported})")]
    SchemaTooNew { found: u32, supported: u32 },
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

/// Raw 256-bit SQLCipher key. Wiped from memory on drop.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct DbKey([u8; 32]);

impl DbKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// `x'…'` blob literal understood by `PRAGMA key` as a raw key (no KDF).
    fn pragma_literal(&self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut s = String::with_capacity(67);
        s.push_str("x'");
        for b in self.0 {
            s.push(HEX[(b >> 4) as usize] as char);
            s.push(HEX[(b & 0xf) as usize] as char);
        }
        s.push('\'');
        s
    }

    /// Sets this key on a connection that has not read anything yet.
    pub(crate) fn apply(&self, conn: &Connection) -> rusqlite::Result<()> {
        let mut literal = self.pragma_literal();
        let keyed = conn.pragma_update(None, "key", &literal);
        literal.zeroize();
        keyed
    }
}

pub struct Db {
    conn: Connection,
}

impl Db {
    /// Opens (or creates) the encrypted database at `path` and migrates it to
    /// the latest schema.
    pub fn open(path: &Path, key: &DbKey) -> Result<Self, StoreError> {
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(path, flags)?;

        // Without SQLCipher, `PRAGMA key` is silently ignored and data would be
        // written in plaintext, so verify the cipher is actually present.
        let cipher_version: Option<String> =
            conn.query_row("PRAGMA cipher_version", [], |r| r.get(0)).ok();
        if cipher_version.is_none_or(|v| v.is_empty()) {
            return Err(StoreError::CipherUnavailable);
        }

        key.apply(&conn)?;

        // The first real read fails with SQLITE_NOTADB if the key is wrong.
        match conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == ffi::ErrorCode::NotADatabase => {
                return Err(StoreError::WrongKeyOrCorrupt);
            }
            Err(e) => return Err(e.into()),
        }

        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;

        let mut db = Self { conn };
        schema::migrate(&mut db.conn)?;
        Ok(db)
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn into_conn(self) -> Connection {
        self.conn
    }

    pub fn schema_version(&self) -> Result<u32, StoreError> {
        Ok(self.conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }
}
