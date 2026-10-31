//! The vault: a directory holding the plaintext `vault.header` (KDF parameters
//! and wrapped keys, no content) and the SQLCipher database.
//!
//! A random 256-bit data key (DEK) encrypts everything. It is stored wrapped
//! once per *slot*: password (Argon2id), recovery key, and optionally a key
//! kept in the macOS Keychain for Touch ID. Changing the password only
//! re-wraps the DEK. See PLAN.md §5.

mod crypto;
mod recovery;

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

pub(crate) use crypto::random_fill;
pub use crypto::{KdfParams, SecretKey};
pub use recovery::RecoveryKey;

use crate::store::DbKey;

const HEADER_FILE: &str = "vault.header";
const DB_FILE: &str = "brainmatter.db";
const FORMAT: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("no vault at {0}")]
    NotFound(PathBuf),
    #[error("a vault already exists at {0}")]
    AlreadyExists(PathBuf),
    #[error("wrong password")]
    WrongPassword,
    #[error("wrong recovery key")]
    WrongRecoveryKey,
    #[error("the {0:?} slot is not set up")]
    SlotMissing(SlotKind),
    #[error("the {0:?} slot key does not match")]
    WrongSlotKey(SlotKind),
    #[error("vault header is corrupt: {0}")]
    Corrupt(String),
    #[error("unsupported vault format {0}")]
    UnsupportedFormat(u32),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotKind {
    Password,
    Recovery,
    Keychain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Slot {
    kind: SlotKind,
    /// Argon2id parameters (password slot only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kdf: Option<KdfParams>,
    /// Keychain account holding the slot key (keychain slot only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    keychain_account: Option<String>,
    wrapped: crypto::Wrapped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Header {
    format: u32,
    vault_id: String,
    created_at: i64,
    slots: Vec<Slot>,
}

/// The unlocked data key. Hand out derived subkeys, never the DEK itself.
pub struct VaultKeys {
    dek: SecretKey,
}

impl VaultKeys {
    pub fn db_key(&self) -> DbKey {
        DbKey::from_bytes(*self.dek.derive("brainmatter/db/v1").as_bytes())
    }

    pub fn blob_key(&self) -> SecretKey {
        self.dek.derive("brainmatter/blob/v1")
    }
}

pub struct Vault {
    dir: PathBuf,
    header: Header,
}

impl Vault {
    pub fn exists(dir: &Path) -> bool {
        dir.join(HEADER_FILE).is_file()
    }

    /// Creates a new vault. The returned recovery key must be shown to the
    /// user exactly once; it cannot be retrieved later.
    pub fn create(
        dir: &Path,
        password: &str,
        kdf: KdfParams,
    ) -> Result<(Vault, VaultKeys, RecoveryKey), VaultError> {
        if Self::exists(dir) {
            return Err(VaultError::AlreadyExists(dir.to_owned()));
        }
        fs::create_dir_all(dir)?;

        let dek = SecretKey::random();
        let recovery = RecoveryKey::generate();
        let vault_id = uuid::Uuid::now_v7().to_string();

        let mut vault = Vault {
            dir: dir.to_owned(),
            header: Header {
                format: FORMAT,
                vault_id,
                created_at: crate::time::now_ms(),
                slots: Vec::new(),
            },
        };
        let password_kek = kdf.derive(password)?;
        vault.put_slot(SlotKind::Password, &password_kek, &dek, Some(kdf), None);
        vault.put_slot(SlotKind::Recovery, &recovery.kek(), &dek, None, None);
        vault.save()?;
        Ok((vault, VaultKeys { dek }, recovery))
    }

    pub fn load(dir: &Path) -> Result<Vault, VaultError> {
        let path = dir.join(HEADER_FILE);
        let raw = fs::read(&path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => VaultError::NotFound(dir.to_owned()),
            _ => e.into(),
        })?;
        let header: Header =
            serde_json::from_slice(&raw).map_err(|e| VaultError::Corrupt(e.to_string()))?;
        if header.format != FORMAT {
            return Err(VaultError::UnsupportedFormat(header.format));
        }
        Ok(Vault { dir: dir.to_owned(), header })
    }

    pub fn vault_id(&self) -> &str {
        &self.header.vault_id
    }

    pub fn db_path(&self) -> PathBuf {
        self.dir.join(DB_FILE)
    }

    pub fn has_slot(&self, kind: SlotKind) -> bool {
        self.slot(kind).is_some()
    }

    /// Keychain account name of the Touch ID slot, if enrolled.
    pub fn keychain_account(&self) -> Option<&str> {
        self.slot(SlotKind::Keychain)?.keychain_account.as_deref()
    }

    /// Deliberately slow (Argon2id). Run off the UI thread.
    pub fn unlock_with_password(&self, password: &str) -> Result<VaultKeys, VaultError> {
        let slot =
            self.slot(SlotKind::Password).ok_or(VaultError::SlotMissing(SlotKind::Password))?;
        let kdf = slot.kdf.as_ref().ok_or_else(|| VaultError::Corrupt("password kdf".into()))?;
        let kek = kdf.derive(password)?;
        self.open_slot(SlotKind::Password, &kek).ok_or(VaultError::WrongPassword)
    }

    pub fn unlock_with_recovery(&self, key: &RecoveryKey) -> Result<VaultKeys, VaultError> {
        self.open_slot(SlotKind::Recovery, &key.kek()).ok_or(VaultError::WrongRecoveryKey)
    }

    pub fn unlock_with_keychain_key(&self, key: &SecretKey) -> Result<VaultKeys, VaultError> {
        if !self.has_slot(SlotKind::Keychain) {
            return Err(VaultError::SlotMissing(SlotKind::Keychain));
        }
        self.open_slot(SlotKind::Keychain, key).ok_or(VaultError::WrongSlotKey(SlotKind::Keychain))
    }

    pub fn change_password(
        &mut self,
        keys: &VaultKeys,
        new_password: &str,
    ) -> Result<(), VaultError> {
        let kdf = self
            .slot(SlotKind::Password)
            .and_then(|s| s.kdf.as_ref())
            .map(KdfParams::reseeded)
            .unwrap_or_else(KdfParams::recommended);
        let kek = kdf.derive(new_password)?;
        self.put_slot(SlotKind::Password, &kek, &keys.dek, Some(kdf), None);
        self.save()
    }

    /// Replaces the recovery key; the old one stops working.
    pub fn rotate_recovery_key(&mut self, keys: &VaultKeys) -> Result<RecoveryKey, VaultError> {
        let recovery = RecoveryKey::generate();
        self.put_slot(SlotKind::Recovery, &recovery.kek(), &keys.dek, None, None);
        self.save()?;
        Ok(recovery)
    }

    /// Enrolls Touch ID. Returns the Keychain account name and the random slot
    /// key the caller must store there. Replaces any previous enrollment.
    pub fn enroll_keychain(&mut self, keys: &VaultKeys) -> Result<(String, SecretKey), VaultError> {
        let key = SecretKey::random();
        let account = format!("vault-{}-{}", self.header.vault_id, uuid::Uuid::now_v7().simple());
        self.put_slot(SlotKind::Keychain, &key, &keys.dek, None, Some(account.clone()));
        self.save()?;
        Ok((account, key))
    }

    pub fn remove_slot(&mut self, kind: SlotKind) -> Result<(), VaultError> {
        if kind == SlotKind::Password {
            return Err(VaultError::Corrupt("the password slot cannot be removed".into()));
        }
        self.header.slots.retain(|s| s.kind != kind);
        self.save()
    }

    fn slot(&self, kind: SlotKind) -> Option<&Slot> {
        self.header.slots.iter().find(|s| s.kind == kind)
    }

    /// Binds each wrapped key to this vault and slot, so slots cannot be
    /// swapped between vaults or kinds.
    fn aad(&self, kind: SlotKind) -> String {
        format!("brainmatter/v{FORMAT}/{}/{kind:?}", self.header.vault_id)
    }

    fn open_slot(&self, kind: SlotKind, kek: &SecretKey) -> Option<VaultKeys> {
        let slot = self.slot(kind)?;
        crypto::unwrap(kek, &slot.wrapped, &self.aad(kind)).map(|dek| VaultKeys { dek })
    }

    fn put_slot(
        &mut self,
        kind: SlotKind,
        kek: &SecretKey,
        dek: &SecretKey,
        kdf: Option<KdfParams>,
        keychain_account: Option<String>,
    ) {
        let wrapped = crypto::wrap(kek, dek, &self.aad(kind));
        self.header.slots.retain(|s| s.kind != kind);
        self.header.slots.push(Slot { kind, kdf, keychain_account, wrapped });
    }

    /// Atomic replace: write a temp file, fsync, rename.
    fn save(&self) -> Result<(), VaultError> {
        let path = self.dir.join(HEADER_FILE);
        let tmp = self.dir.join(format!("{HEADER_FILE}.tmp"));
        let json = serde_json::to_vec_pretty(&self.header).expect("header serializes");
        {
            let mut opts = fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
            let mut f = opts.open(&tmp)?;
            f.write_all(&json)?;
            f.sync_all()?;
        }
        fs::rename(&tmp, &path)?;
        Ok(())
    }
}
