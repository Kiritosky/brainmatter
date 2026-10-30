//! Lock state of the app: no vault yet, locked, or unlocked with an open
//! engine. Secrets and the engine exist only while unlocked.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicI64, AtomicU32, Ordering},
    },
};

use bm_core::{
    Engine, Vault, VaultKeys,
    model::{ActorKind, Ctx},
    time::now_ms,
    vault::{KdfParams, RecoveryKey, SecretKey},
};
use bm_macos::{biometrics, keychain};
use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast::error::RecvError;

use crate::error::{ApiError, ApiResult};

const KEYCHAIN_SERVICE: &str =
    if cfg!(debug_assertions) { "app.brainmatter.dev" } else { "app.brainmatter" };
const READERS: usize = 4;

enum State {
    NoVault,
    Locked,
    Unlocked(Box<Unlocked>),
}

struct Unlocked {
    vault: Vault,
    keys: VaultKeys,
    engine: Arc<Engine>,
    ctx: Ctx,
    user_name: String,
    forwarder: tauri::async_runtime::JoinHandle<()>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    /// `"none"`, `"locked"` or `"unlocked"`.
    pub state: &'static str,
    pub touch_id_enrolled: bool,
    pub biometry_available: bool,
    pub user_name: Option<String>,
    pub auto_lock_minutes: u32,
}

pub struct Session {
    dir: PathBuf,
    app: AppHandle,
    state: Mutex<State>,
    last_activity: AtomicI64,
    auto_lock_minutes: AtomicU32,
}

impl Session {
    pub fn new(dir: PathBuf, app: AppHandle) -> Self {
        let state = if Vault::exists(&dir) { State::Locked } else { State::NoVault };
        Self {
            dir,
            app,
            state: Mutex::new(state),
            last_activity: AtomicI64::new(now_ms()),
            auto_lock_minutes: AtomicU32::new(60),
        }
    }

    pub fn status(&self) -> Status {
        let state = self.state.lock();
        let (name, user_name) = match &*state {
            State::NoVault => ("none", None),
            State::Locked => ("locked", None),
            State::Unlocked(u) => ("unlocked", Some(u.user_name.clone())),
        };
        let touch_id_enrolled = match &*state {
            State::Unlocked(u) => u.vault.keychain_account().is_some(),
            State::Locked => Vault::load(&self.dir).is_ok_and(|v| v.keychain_account().is_some()),
            State::NoVault => false,
        };
        Status {
            state: name,
            touch_id_enrolled,
            biometry_available: biometrics::available().is_ok(),
            user_name,
            auto_lock_minutes: self.auto_lock_minutes.load(Ordering::Relaxed),
        }
    }

    /// Creates the vault and unlocks it. Returns the recovery key, which the UI
    /// must show exactly once.
    pub fn create(&self, password: &str, user_name: &str) -> ApiResult<String> {
        if password.chars().count() < 8 {
            return Err(ApiError::new("invalid", "Das Passwort braucht mindestens 8 Zeichen."));
        }
        let name = user_name.trim();
        if name.is_empty() {
            return Err(ApiError::new("invalid", "Bitte einen Namen angeben."));
        }
        if !matches!(*self.state.lock(), State::NoVault) {
            return Err(ApiError::new("vault_exists", "Es gibt bereits einen Tresor."));
        }
        let (vault, keys, recovery) = Vault::create(&self.dir, password, KdfParams::recommended())?;
        let engine = Engine::open(&vault.db_path(), &keys.db_key(), READERS)?;
        engine.ensure_actor(ActorKind::User, name)?;
        self.install(vault, keys, Arc::new(engine))?;
        Ok(recovery.to_string())
    }

    /// Deliberately slow (Argon2id): call from a blocking thread.
    pub fn unlock_password(&self, password: &str) -> ApiResult<()> {
        let vault = self.locked_vault()?;
        let keys = vault.unlock_with_password(password)?;
        self.open(vault, keys)?;
        self.refresh_touch_id();
        Ok(())
    }

    pub fn unlock_recovery(&self, key: &str) -> ApiResult<()> {
        let key = RecoveryKey::parse(key).ok_or_else(|| {
            ApiError::new("wrong_recovery_key", "Der Schlüssel ist unvollständig oder vertippt.")
        })?;
        let vault = self.locked_vault()?;
        let keys = vault.unlock_with_recovery(&key)?;
        self.open(vault, keys)?;
        self.refresh_touch_id();
        Ok(())
    }

    /// Shows the Touch ID prompt and blocks until the user responds.
    pub fn unlock_touch_id(&self, reason: &str) -> ApiResult<()> {
        let vault = self.locked_vault()?;
        let account = vault
            .keychain_account()
            .ok_or_else(|| {
                ApiError::new("touch_id_not_enrolled", "Touch ID ist nicht eingerichtet.")
            })?
            .to_string();
        biometrics::authenticate(reason)
            .map_err(|e| ApiError::new("touch_id_cancelled", e.to_string()))?;
        let secret = match keychain::load_secret(KEYCHAIN_SERVICE, &account) {
            Ok(Some(secret)) => secret,
            Ok(None) => return Err(keychain::KeychainError::NotAccessible.into()),
            Err(e) => {
                // Written by an older build: unreadable for us, so drop it.
                let _ = keychain::delete_secret(KEYCHAIN_SERVICE, &account);
                return Err(e.into());
            }
        };
        let key =
            SecretKey::from_slice(&secret).ok_or_else(|| ApiError::internal("bad keychain key"))?;
        let keys = vault.unlock_with_keychain_key(&key)?;
        self.open(vault, keys)
    }

    pub fn enroll_touch_id(&self) -> ApiResult<()> {
        biometrics::available()
            .map_err(|e| ApiError::new("touch_id_unavailable", e.to_string()))?;
        let mut state = self.state.lock();
        let State::Unlocked(u) = &mut *state else { return Err(ApiError::locked()) };
        enroll(u)
    }

    pub fn disable_touch_id(&self) -> ApiResult<()> {
        let mut state = self.state.lock();
        let State::Unlocked(u) = &mut *state else { return Err(ApiError::locked()) };
        if let Some(account) = u.vault.keychain_account() {
            let _ = keychain::delete_secret(KEYCHAIN_SERVICE, account);
        }
        u.vault.remove_slot(bm_core::vault::SlotKind::Keychain)?;
        Ok(())
    }

    pub fn change_password(&self, new_password: &str) -> ApiResult<()> {
        if new_password.chars().count() < 8 {
            return Err(ApiError::new("invalid", "Das Passwort braucht mindestens 8 Zeichen."));
        }
        let mut state = self.state.lock();
        let State::Unlocked(u) = &mut *state else { return Err(ApiError::locked()) };
        u.vault.change_password(&u.keys, new_password)?;
        Ok(())
    }

    /// Drops the engine and all key material.
    pub fn lock(&self) {
        let mut state = self.state.lock();
        if let State::Unlocked(u) = std::mem::replace(&mut *state, State::Locked) {
            u.forwarder.abort();
            drop(u);
            drop(state);
            tracing::info!("vault locked");
            let _ = self.app.emit("bm:session", self.status());
        }
    }

    /// Engine and acting context for one command; counts as user activity.
    pub fn engine(&self) -> ApiResult<(Arc<Engine>, Ctx)> {
        let state = self.state.lock();
        let State::Unlocked(u) = &*state else { return Err(ApiError::locked()) };
        self.last_activity.store(now_ms(), Ordering::Relaxed);
        Ok((u.engine.clone(), u.ctx.clone()))
    }

    pub fn set_auto_lock_minutes(&self, minutes: u32) {
        self.auto_lock_minutes.store(minutes.clamp(1, 24 * 60), Ordering::Relaxed);
        self.last_activity.store(now_ms(), Ordering::Relaxed);
    }

    /// Called periodically; locks after the configured idle time.
    pub fn maybe_auto_lock(&self) {
        let idle_ms = now_ms() - self.last_activity.load(Ordering::Relaxed);
        let limit_ms = self.auto_lock_minutes.load(Ordering::Relaxed) as i64 * 60_000;
        if idle_ms > limit_ms && matches!(*self.state.lock(), State::Unlocked(_)) {
            tracing::info!("auto-lock after {} min idle", idle_ms / 60_000);
            self.lock();
        }
    }

    // --------------------------------------------------------- internals ---

    /// Fresh header from disk; Argon2 then runs without holding the state lock.
    fn locked_vault(&self) -> ApiResult<Vault> {
        match *self.state.lock() {
            State::Locked => Ok(Vault::load(&self.dir)?),
            State::Unlocked(_) => Err(ApiError::new("already_unlocked", "Bereits entsperrt.")),
            State::NoVault => Err(ApiError::new("no_vault", "Es gibt noch keinen Tresor.")),
        }
    }

    fn open(&self, vault: Vault, keys: VaultKeys) -> ApiResult<()> {
        let engine = Engine::open(&vault.db_path(), &keys.db_key(), READERS)?;
        self.install(vault, keys, Arc::new(engine))
    }

    fn install(&self, vault: Vault, keys: VaultKeys, engine: Arc<Engine>) -> ApiResult<()> {
        let user =
            engine.primary_user()?.ok_or_else(|| ApiError::internal("vault has no user actor"))?;
        let mut rx = engine.subscribe();
        let app = self.app.clone();
        let forwarder = tauri::async_runtime::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let _ = app.emit("bm:event", &event);
                    }
                    Err(RecvError::Lagged(_)) => {
                        let _ = app.emit("bm:resync", ());
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        });
        let unlocked = Unlocked {
            vault,
            keys,
            engine,
            ctx: Ctx::new(user.id).with_tool("app"),
            user_name: user.name,
            forwarder,
        };
        *self.state.lock() = State::Unlocked(Box::new(unlocked));
        self.last_activity.store(now_ms(), Ordering::Relaxed);
        tracing::info!("vault unlocked");
        let _ = self.app.emit("bm:session", self.status());
        Ok(())
    }

    /// After a password/recovery unlock: if Touch ID was enrolled but its
    /// Keychain item is gone or belongs to an older build, enroll again.
    fn refresh_touch_id(&self) {
        let mut state = self.state.lock();
        let State::Unlocked(u) = &mut *state else { return };
        let Some(account) = u.vault.keychain_account().map(str::to_string) else { return };
        let healthy = matches!(
            keychain::load_secret(KEYCHAIN_SERVICE, &account),
            Ok(Some(ref s)) if SecretKey::from_slice(s).is_some_and(|k| u.vault.unlock_with_keychain_key(&k).is_ok())
        );
        if !healthy {
            let _ = keychain::delete_secret(KEYCHAIN_SERVICE, &account);
            match enroll(u) {
                Ok(()) => tracing::info!("touch id re-enrolled"),
                Err(e) => tracing::warn!("touch id re-enroll failed: {}", e.message),
            }
        }
    }
}

fn enroll(u: &mut Unlocked) -> ApiResult<()> {
    let previous = u.vault.keychain_account().map(str::to_string);
    let (account, key) = u.vault.enroll_keychain(&u.keys)?;
    if let Err(e) = keychain::store_secret(KEYCHAIN_SERVICE, &account, key.as_bytes()) {
        // Never leave a slot whose key is not in the Keychain.
        let _ = u.vault.remove_slot(bm_core::vault::SlotKind::Keychain);
        return Err(e.into());
    }
    if let Some(old) = previous.filter(|old| *old != account) {
        let _ = keychain::delete_secret(KEYCHAIN_SERVICE, &old);
    }
    Ok(())
}
