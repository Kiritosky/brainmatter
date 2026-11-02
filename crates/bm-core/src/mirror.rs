//! A second copy of the backups somewhere else, such as an external drive or a folder
//! another service keeps in sync, so the notes outlive the Mac.
//!
//! The copy holds what a new vault needs, all as encrypted as at home: the header that
//! the password or the recovery key opens, the backups, and the attachment files. On
//! another Mac, [`adopt`] makes a vault of it.

use std::{
    collections::HashSet,
    fs,
    io::{self, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use crate::{
    backups::{self, BackupInfo},
    error::CoreError,
    store::DbKey,
    vault::{BACKUPS_DIR, BLOBS_DIR, DB_FILE, HEADER_FILE, Vault, VaultError},
};

/// The folder the copy goes in, inside the one the user picks.
pub const FOLDER: &str = "Brainmatter-Backup";
const README: &str = "LIESMICH.txt";
const README_TEXT: &str = "\
Brainmatter-Backup

Dieser Ordner ist eine verschlüsselte Kopie deines Brainmatter-Tresors: die Backups,
die Anhänge und der Schlüsselkopf, den dein Passwort öffnet. Ohne Passwort oder
Wiederherstellungsschlüssel ist nichts davon lesbar.

Auf einem neuen Mac: Brainmatter installieren, beim ersten Start „Aus einem Backup
übernehmen“ wählen und diesen Ordner angeben.

---

This folder is an encrypted copy of your Brainmatter vault: the backups, the
attachments, and the header your password opens. Without the password or the recovery
key, none of it can be read.

On a new Mac: install Brainmatter, choose “Restore from a backup” on first launch and
pick this folder.
";

#[derive(Debug, thiserror::Error)]
pub enum MirrorError {
    #[error("the folder holds the backup of another vault")]
    OtherVault,
    #[error("the folder is the vault itself")]
    SameFolder,
    #[error("the folder holds no backup")]
    Empty,
    #[error("no backup in the folder can be read")]
    Damaged,
    #[error("there is a vault already")]
    VaultExists,
    #[error(transparent)]
    Vault(#[from] VaultError),
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub type Result<T> = std::result::Result<T, MirrorError>;

/// Where the copy lives for a folder the user picked: in it, unless it is a copy itself.
pub fn target_in(chosen: &Path) -> PathBuf {
    if chosen.file_name().is_some_and(|n| n == FOLDER) || Vault::exists(chosen) {
        chosen.to_path_buf()
    } else {
        chosen.join(FOLDER)
    }
}

/// The backups in a copy, newest first.
pub fn list(target: &Path) -> Result<Vec<BackupInfo>> {
    Ok(backups::list(&target.join(BACKUPS_DIR))?)
}

/// Brings the copy in `target` up to date with the vault in `vault_dir`. Each backup is
/// checked with `key` once copied. The copy keeps its backups as long as the vault does
/// (seven days, four weeks), even if the vault has lost its own.
pub fn update(vault_dir: &Path, key: &DbKey, target: &Path) -> Result<Vec<BackupInfo>> {
    let own = Vault::load(vault_dir)?;
    if same_place(vault_dir, target) {
        return Err(MirrorError::SameFolder);
    }
    if Vault::exists(target) && Vault::load(target)?.vault_id() != own.vault_id() {
        return Err(MirrorError::OtherVault);
    }
    make_dir(target)?;
    // Attachments first, the header last: once a copy has a header, it has all its
    // backups need.
    sync_files(&vault_dir.join(BLOBS_DIR), &target.join(BLOBS_DIR), true)?;
    let there = target.join(BACKUPS_DIR);
    make_dir(&there)?;
    let have: HashSet<String> = backups::list(&there)?.into_iter().map(|b| b.name).collect();
    let home = vault_dir.join(BACKUPS_DIR);
    for backup in backups::list(&home)? {
        if !have.contains(&backup.name) {
            copy_checked(&home.join(&backup.name), &there.join(&backup.name), key)?;
        }
    }
    backups::prune(&there)?;
    let header = fs::read(vault_dir.join(HEADER_FILE))?;
    if fs::read(target.join(HEADER_FILE)).ok().as_deref() != Some(&header[..]) {
        write_atomic(&target.join(HEADER_FILE), &header)?;
    }
    if !target.join(README).exists() {
        write_atomic(&target.join(README), README_TEXT.as_bytes())?;
    }
    list(target)
}

/// Makes a vault in `vault_dir`, which has none, from the copy in `source`: its newest
/// backup that `key` opens and is intact becomes the database. Returns that backup.
pub fn adopt(source: &Path, key: &DbKey, vault_dir: &Path) -> Result<BackupInfo> {
    if Vault::exists(vault_dir) {
        return Err(MirrorError::VaultExists);
    }
    let there = source.join(BACKUPS_DIR);
    let found = backups::list(&there)?;
    if found.is_empty() {
        return Err(MirrorError::Empty);
    }
    let chosen = found
        .iter()
        .find(|b| backups::verify(&there.join(&b.name), key).is_ok())
        .ok_or(MirrorError::Damaged)?
        .clone();
    make_dir(vault_dir)?;
    sync_files(&source.join(BLOBS_DIR), &vault_dir.join(BLOBS_DIR), false)?;
    let home = vault_dir.join(BACKUPS_DIR);
    make_dir(&home)?;
    for backup in &found {
        copy_atomic(&there.join(&backup.name), &home.join(&backup.name))?;
    }
    let db = vault_dir.join(DB_FILE);
    backups::remove_sidecars(&db);
    copy_atomic(&there.join(&chosen.name), &db)?;
    // Last: with the header, the folder counts as a vault.
    copy_atomic(&source.join(HEADER_FILE), &vault_dir.join(HEADER_FILE))?;
    Ok(chosen)
}

fn same_place(vault_dir: &Path, target: &Path) -> bool {
    let (Ok(vault), Ok(target)) = (vault_dir.canonicalize(), target.canonicalize()) else {
        return false;
    };
    target.starts_with(&vault)
}

/// A drive formatted for Windows has no Unix permissions; the files are encrypted anyway.
fn make_dir(dir: &Path) -> io::Result<()> {
    if !dir.is_dir() {
        fs::create_dir_all(dir)?;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

fn tmp_of(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

/// The file appears under its name only once it is complete and on the disk, so a
/// drive pulled out halfway leaves nothing that looks whole.
fn copy_atomic(from: &Path, to: &Path) -> io::Result<()> {
    let tmp = tmp_of(to);
    let result = fs::copy(from, &tmp)
        .and_then(|_| fs::File::open(&tmp)?.sync_all())
        .and_then(|()| fs::rename(&tmp, to));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    let _ = fs::set_permissions(to, fs::Permissions::from_mode(0o600));
    result
}

fn copy_checked(from: &Path, to: &Path, key: &DbKey) -> Result<()> {
    let tmp = tmp_of(to);
    let result = (|| {
        fs::copy(from, &tmp)?;
        fs::File::open(&tmp)?.sync_all()?;
        backups::verify(&tmp, key)?;
        fs::rename(&tmp, to)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    let _ = fs::set_permissions(to, fs::Permissions::from_mode(0o600));
    result
}

fn write_atomic(to: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = tmp_of(to);
    let mut file = fs::File::create(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&tmp, to)?;
    let _ = fs::set_permissions(to, fs::Permissions::from_mode(0o600));
    Ok(())
}

/// The files one level down, as attachments lie (`ab/abcdef…`), by relative path.
fn files_in(dir: &Path) -> io::Result<Vec<(PathBuf, u64)>> {
    let mut out = Vec::new();
    let Ok(groups) = fs::read_dir(dir) else { return Ok(out) };
    for group in groups {
        let group = group?;
        if !group.file_type()?.is_dir() {
            continue;
        }
        for file in fs::read_dir(group.path())? {
            let file = file?;
            let name = file.file_name();
            if file.file_type()?.is_file() && !name.to_string_lossy().ends_with(".tmp") {
                out.push((Path::new(&group.file_name()).join(name), file.metadata()?.len()));
            }
        }
    }
    Ok(out)
}

/// Copies what `to` lacks; with `remove`, also removes what `from` no longer has, as
/// erasing an entry for good does.
fn sync_files(from: &Path, to: &Path, remove: bool) -> io::Result<()> {
    if !from.is_dir() {
        return Ok(());
    }
    let wanted = files_in(from)?;
    for (path, size) in &wanted {
        let target = to.join(path);
        if fs::metadata(&target).map(|m| m.len()).ok() != Some(*size) {
            make_dir(target.parent().unwrap_or(to))?;
            copy_atomic(&from.join(path), &target)?;
        }
    }
    if remove {
        let keep: HashSet<&PathBuf> = wanted.iter().map(|(p, _)| p).collect();
        for (path, _) in files_in(to)? {
            if !keep.contains(&path) {
                fs::remove_file(to.join(path))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picked_folder_gets_a_copy_inside_unless_it_is_one() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(target_in(dir.path()), dir.path().join(FOLDER));
        let named = dir.path().join(FOLDER);
        assert_eq!(target_in(&named), named);
    }
}
