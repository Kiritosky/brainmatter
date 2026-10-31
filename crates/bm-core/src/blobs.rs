//! Encrypted attachment files (PLAN.md §3). A file is named after a keyed hash
//! of its content, which deduplicates and reveals nothing about the content.
//!
//! Files are never removed here: the trash and undo can still need them.

use std::{
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};

use crate::{
    error::{CoreError, Result},
    vault::{SecretKey, random_fill},
};

const FORMAT_V1: u8 = 1;
const NONCE_LEN: usize = 24;
const ID_LEN: usize = 64;

pub struct BlobStore {
    dir: PathBuf,
    data_key: SecretKey,
    name_key: SecretKey,
}

impl BlobStore {
    /// `key` is [`crate::VaultKeys::blob_key`].
    pub fn new(dir: impl Into<PathBuf>, key: &SecretKey) -> Self {
        Self {
            dir: dir.into(),
            data_key: key.derive("brainmatter/blob/data/v1"),
            name_key: key.derive("brainmatter/blob/name/v1"),
        }
    }

    fn path(&self, id: &str) -> Result<PathBuf> {
        if id.len() != ID_LEN || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(CoreError::Invalid(format!("not a blob id: {id}")));
        }
        Ok(self.dir.join(&id[..2]).join(id))
    }

    /// Stores `bytes` and returns the blob's id. Durable once this returns.
    pub fn put(&self, bytes: &[u8]) -> Result<String> {
        let id = blake3::keyed_hash(self.name_key.as_bytes(), bytes).to_hex().to_string();
        let path = self.path(&id)?;
        if path.is_file() {
            return Ok(id);
        }
        let mut nonce = [0u8; NONCE_LEN];
        random_fill(&mut nonce);
        let sealed =
            self.cipher().encrypt(&XNonce::from(nonce), Payload { msg: bytes, aad: id.as_bytes() });
        let sealed = sealed.map_err(|_| CoreError::Invalid("encryption failed".into()))?;

        let parent = path.parent().expect("blob path has a parent");
        create_private_dir(&self.dir)?;
        create_private_dir(parent)?;
        let tmp = parent.join(format!("{id}.{}.tmp", uuid::Uuid::now_v7()));
        let mut file =
            fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
        let written = file
            .write_all(&[FORMAT_V1])
            .and_then(|()| file.write_all(&nonce))
            .and_then(|()| file.write_all(&sealed))
            .and_then(|()| file.sync_all());
        if let Err(e) = written.and_then(|()| fs::rename(&tmp, &path)) {
            let _ = fs::remove_file(&tmp);
            return Err(e.into());
        }
        Ok(id)
    }

    /// Reads and authenticates a blob; tampered files are an error.
    pub fn get(&self, id: &str) -> Result<Vec<u8>> {
        let raw = fs::read(self.path(id)?)?;
        let corrupt = || CoreError::CorruptBlob(id.to_string());
        let (format, rest) = raw.split_first().ok_or_else(corrupt)?;
        let (nonce, sealed) = rest.split_first_chunk::<NONCE_LEN>().ok_or_else(corrupt)?;
        if *format != FORMAT_V1 {
            return Err(corrupt());
        }
        self.cipher()
            .decrypt(&XNonce::from(*nonce), Payload { msg: sealed, aad: id.as_bytes() })
            .map_err(|_| corrupt())
    }

    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(&(*self.data_key.as_bytes()).into())
    }
}

fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    if !dir.is_dir() {
        fs::create_dir_all(dir)?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
