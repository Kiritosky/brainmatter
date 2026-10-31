//! Verified, encrypted snapshots of the database (PLAN.md §3).
//!
//! A backup is encrypted with the same data key as the database, so it stays
//! readable after a password change. Attachment files are not copied: they are
//! encrypted already and never removed, so older backups still find theirs.

use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

use crate::{
    error::{CoreError, Result},
    store::{Db, DbKey},
    time::now_ms,
};

const PREFIX: &str = "backup-";
const SUFFIX: &str = ".db";
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const KEEP_DAILY: usize = 7;
const KEEP_WEEKLY: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackupInfo {
    pub name: String,
    /// When it was taken (ms).
    pub at: i64,
    pub size: u64,
}

fn parse_name(name: &str) -> Option<i64> {
    name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?.parse().ok()
}

/// Newest first. Unfinished and foreign files are not listed.
pub fn list(dir: &Path) -> Result<Vec<BackupInfo>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(at) = parse_name(&name) {
            out.push(BackupInfo { name, at, size: entry.metadata()?.len() });
        }
    }
    out.sort_by_key(|b| std::cmp::Reverse(b.at));
    Ok(out)
}

/// Takes a snapshot of the live database, checks that it opens with `key` and
/// is intact, and only then gives it its final name. Writers keep working
/// meanwhile, thanks to WAL.
pub fn create(db: &Path, key: &DbKey, dir: &Path) -> Result<BackupInfo> {
    if !dir.is_dir() {
        fs::create_dir_all(dir)?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    let at = now_ms();
    let name = format!("{PREFIX}{at}{SUFFIX}");
    let tmp = dir.join(format!("{name}.tmp"));
    let result = snapshot(db, key, &tmp).and_then(|()| {
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
        verify(&tmp, key)?;
        fs::rename(&tmp, dir.join(&name))?;
        Ok(())
    });
    if result.is_err() {
        remove_with_sidecars(&tmp);
    }
    result?;
    let size = fs::metadata(dir.join(&name))?.len();
    prune(dir)?;
    Ok(BackupInfo { name, at, size })
}

fn snapshot(db: &Path, key: &DbKey, target: &Path) -> Result<()> {
    let conn = Db::open(db, key)?.into_conn();
    let target = target.to_str().ok_or_else(|| CoreError::Invalid("path is not UTF-8".into()))?;
    conn.execute("VACUUM INTO ?1", [target])?;
    Ok(())
}

/// Opens a database file read-only and runs SQLite's structure check.
pub fn verify(path: &Path, key: &DbKey) -> Result<()> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(path, flags)?;
    key.apply(&conn)?;
    let check: String = conn
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(|e| CoreError::Invalid(format!("backup cannot be read: {e}")))?;
    if check != "ok" {
        return Err(CoreError::Invalid(format!("backup is damaged: {check}")));
    }
    Ok(())
}

/// Replaces the database with a backup, after checking the backup and saving
/// the current state as a new backup, so this can be undone. Every connection
/// to the database must be closed.
pub fn restore(db: &Path, key: &DbKey, dir: &Path, name: &str) -> Result<BackupInfo> {
    // The name comes from the UI: no paths, only what `list` can show.
    if parse_name(name).is_none() || name.contains(['/', '\\']) {
        return Err(CoreError::Invalid(format!("not a backup: {name}")));
    }
    let source = dir.join(name);
    verify(&source, key)?;
    // Copied first: saving the current state prunes, which may remove `source`
    // if it is from the same day.
    let tmp = with_suffix(db, ".restore");
    fs::copy(&source, &tmp)?;
    let saved = match create(db, key, dir) {
        Ok(saved) => saved,
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
    };
    remove_sidecars(db);
    fs::rename(&tmp, db)?;
    Ok(saved)
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn remove_sidecars(path: &Path) {
    for suffix in ["-wal", "-shm"] {
        let _ = fs::remove_file(with_suffix(path, suffix));
    }
}

fn remove_with_sidecars(path: &Path) {
    let _ = fs::remove_file(path);
    remove_sidecars(path);
}

/// Of these snapshot times, the ones to keep: the newest per day for the last
/// [`KEEP_DAILY`] days with snapshots, then the newest per week for
/// [`KEEP_WEEKLY`] more weeks. Manual snapshots count like any other.
fn to_keep(times: &[i64]) -> Vec<i64> {
    let newest_per = |bucket_ms: i64, skip: &[i64]| {
        let mut buckets: BTreeMap<i64, i64> = BTreeMap::new();
        for &at in times.iter().filter(|at| !skip.contains(at)) {
            let slot = buckets.entry(at.div_euclid(bucket_ms)).or_insert(at);
            *slot = (*slot).max(at);
        }
        buckets.into_values().rev().collect::<Vec<_>>()
    };
    let mut keep: Vec<i64> = newest_per(DAY_MS, &[]).into_iter().take(KEEP_DAILY).collect();
    // Before the oldest kept day, or a weekly slot would go to a day already kept.
    let daily_from = keep.last().map_or(i64::MAX, |at| at.div_euclid(DAY_MS) * DAY_MS);
    let weekly = newest_per(7 * DAY_MS, &keep);
    keep.extend(weekly.into_iter().filter(|at| *at < daily_from).take(KEEP_WEEKLY));
    keep
}

fn prune(dir: &Path) -> Result<()> {
    let backups = list(dir)?;
    let keep = to_keep(&backups.iter().map(|b| b.at).collect::<Vec<_>>());
    for b in backups.iter().filter(|b| !keep.contains(&b.at)) {
        fs::remove_file(dir.join(&b.name))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_keeps_seven_days_and_four_weeks() {
        let now = 1000 * DAY_MS;
        // Two snapshots a day for 100 days.
        let times: Vec<i64> = (0..200).map(|i| now - i * DAY_MS / 2 - 1).collect();
        let keep = to_keep(&times);
        assert_eq!(keep.len(), KEEP_DAILY + KEEP_WEEKLY);
        assert!(keep.contains(&times[0]), "the newest always stays");
        let newest_days = keep.iter().filter(|at| now - **at <= 7 * DAY_MS).count();
        assert_eq!(newest_days, KEEP_DAILY);
        let oldest = keep.iter().min().unwrap();
        assert!(now - oldest > 21 * DAY_MS, "weeklies reach further back");
    }

    #[test]
    fn retention_never_drops_the_only_snapshots() {
        assert_eq!(to_keep(&[5]), [5]);
        assert!(to_keep(&[]).is_empty());
    }

    #[test]
    fn names_roundtrip_and_reject_anything_else() {
        assert_eq!(parse_name("backup-123.db"), Some(123));
        assert_eq!(parse_name("backup-123.db.tmp"), None);
        assert_eq!(parse_name("../backup-123.db"), None);
        assert_eq!(parse_name("brainmatter.db"), None);
    }
}
