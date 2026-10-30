//! Registered AI clients and their tokens.
//!
//! Stored as JSON next to the vault (not inside it) so tokens can be checked
//! while the vault is locked: only an authenticated client may trigger the
//! Touch ID unlock prompt. Tokens are 256-bit random strings; the file holds
//! only their SHA-256 hashes, which reveal nothing.

use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use bm_core::ids::ProjectId;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Persist `last_seen_at` at most this often.
const LAST_SEEN_FLUSH_MS: i64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Read,
    Write,
    Delete,
}

/// A client as stored (with token hash).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stored {
    id: String,
    name: String,
    token_hash: String,
    scopes: Vec<Scope>,
    #[serde(default)]
    projects: Vec<ProjectId>,
    created_at: i64,
    #[serde(default)]
    last_seen_at: Option<i64>,
}

/// A client as the rest of the app sees it (no secrets).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Client {
    pub id: String,
    /// Display name; also the name of its actor in the knowledge graph.
    pub name: String,
    pub scopes: Vec<Scope>,
    /// Allowed projects (including their subprojects). Empty: everything.
    pub projects: Vec<ProjectId>,
    pub created_at: i64,
    pub last_seen_at: Option<i64>,
}

impl Client {
    pub fn can(&self, scope: Scope) -> bool {
        self.scopes.contains(&scope)
    }
}

impl From<&Stored> for Client {
    fn from(s: &Stored) -> Self {
        Self {
            id: s.id.clone(),
            name: s.name.clone(),
            scopes: s.scopes.clone(),
            projects: s.projects.clone(),
            created_at: s.created_at,
            last_seen_at: s.last_seen_at,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("a client named „{0}“ already exists")]
    DuplicateName(String),
    #[error("client name must not be empty")]
    EmptyName,
    #[error("client not found")]
    NotFound,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct ClientRegistry {
    path: Option<PathBuf>,
    clients: RwLock<Vec<Stored>>,
    last_flush: RwLock<i64>,
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn hash(token: &str) -> String {
    Sha256::digest(token.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

impl ClientRegistry {
    /// Loads (or starts) the registry stored at `path`.
    pub fn open(path: &Path) -> Self {
        let clients = std::fs::read(path)
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default();
        Self {
            path: Some(path.to_owned()),
            clients: RwLock::new(clients),
            last_flush: RwLock::new(0),
        }
    }

    /// Not persisted; for tests.
    pub fn in_memory() -> Self {
        Self { path: None, clients: RwLock::new(Vec::new()), last_flush: RwLock::new(0) }
    }

    pub fn list(&self) -> Vec<Client> {
        self.clients.read().iter().map(Client::from).collect()
    }

    /// Registers a client. The returned token is shown once and never stored.
    pub fn create(
        &self,
        name: &str,
        scopes: &[Scope],
        projects: &[ProjectId],
    ) -> Result<(Client, String), RegistryError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(RegistryError::EmptyName);
        }
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).expect("OS random number generator failed");
        let token = format!("bm_{}", URL_SAFE_NO_PAD.encode(bytes));

        let mut scopes = scopes.to_vec();
        scopes.sort_by_key(|s| *s as u8);
        scopes.dedup();
        let stored = Stored {
            id: uuid::Uuid::now_v7().to_string(),
            name: name.to_string(),
            token_hash: hash(&token),
            scopes,
            projects: projects.to_vec(),
            created_at: now_ms(),
            last_seen_at: None,
        };
        {
            let mut clients = self.clients.write();
            if clients.iter().any(|c| c.name.eq_ignore_ascii_case(name)) {
                return Err(RegistryError::DuplicateName(name.to_string()));
            }
            clients.push(stored.clone());
        }
        self.save()?;
        Ok((Client::from(&stored), token))
    }

    pub fn revoke(&self, id: &str) -> Result<(), RegistryError> {
        let removed = {
            let mut clients = self.clients.write();
            let before = clients.len();
            clients.retain(|c| c.id != id);
            before != clients.len()
        };
        if !removed {
            return Err(RegistryError::NotFound);
        }
        self.save()
    }

    /// The client owning `token`, recording it as seen.
    pub fn authenticate(&self, token: &str) -> Option<Client> {
        let h = hash(token);
        let now = now_ms();
        let client = {
            let mut clients = self.clients.write();
            let c = clients.iter_mut().find(|c| c.token_hash == h)?;
            c.last_seen_at = Some(now);
            Client::from(&*c)
        };
        let due = now - *self.last_flush.read() > LAST_SEEN_FLUSH_MS;
        if due {
            *self.last_flush.write() = now;
            let _ = self.save();
        }
        Some(client)
    }

    fn save(&self) -> Result<(), RegistryError> {
        let Some(path) = &self.path else { return Ok(()) };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(&*self.clients.read()).expect("clients serialize");
        let tmp = path.with_extension("json.tmp");
        {
            use std::io::Write;
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).truncate(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
            let mut f = opts.open(&tmp)?;
            f.write_all(&json)?;
            f.sync_all()?;
        }
        std::fs::rename(tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_authenticate_revoke_and_persist() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clients.json");
        let reg = ClientRegistry::open(&path);
        let (client, token) =
            reg.create("Claude Code", &[Scope::Write, Scope::Read, Scope::Read], &[]).unwrap();
        assert!(token.starts_with("bm_") && token.len() > 40);
        assert_eq!(client.scopes, [Scope::Read, Scope::Write]);
        assert!(matches!(
            reg.create("claude code", &[Scope::Read], &[]),
            Err(RegistryError::DuplicateName(_))
        ));

        assert_eq!(reg.authenticate(&token).unwrap().id, client.id);
        assert!(reg.authenticate("bm_wrong").is_none());

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains(&token), "token must never be stored");

        let reopened = ClientRegistry::open(&path);
        assert_eq!(reopened.authenticate(&token).unwrap().name, "Claude Code");
        reopened.revoke(&client.id).unwrap();
        assert!(ClientRegistry::open(&path).authenticate(&token).is_none());
    }
}
