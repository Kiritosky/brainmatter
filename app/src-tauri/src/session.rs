//! Lock state of the app: no vault yet, locked, or unlocked with an open
//! engine. Secrets and the engine exist only while unlocked.
//!
//! The session knows nothing about Tauri windows: it reports changes through
//! an [`Emit`] callback, which keeps it testable without a running app.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
};

use bm_core::{
    Engine, Vault, VaultKeys,
    model::{ActorKind, Ctx},
    time::now_ms,
    vault::{KdfParams, RecoveryKey, SecretKey, SlotKind},
};
use bm_macos::{biometrics, keychain, power::LockTrigger};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;

use crate::error::{ApiError, ApiResult};

const KEYCHAIN_SERVICE: &str =
    if cfg!(debug_assertions) { "app.brainmatter.dev" } else { "app.brainmatter" };
const READERS: usize = 4;
const SETTINGS_FILE: &str = "settings.json";
const MIN_PASSWORD: usize = 8;

/// Sends a named event with a JSON payload to the UI.
pub type Emit = Arc<dyn Fn(&'static str, serde_json::Value) + Send + Sync>;

/// Non-secret preferences, stored in plain JSON next to the vault so they
/// apply before unlocking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub auto_lock_minutes: u32,
    pub lock_on_sleep: bool,
    pub lock_on_screen_lock: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { auto_lock_minutes: 60, lock_on_sleep: true, lock_on_screen_lock: false }
    }
}

impl Settings {
    fn load(dir: &Path) -> Self {
        std::fs::read(dir.join(SETTINGS_FILE))
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default()
    }

    fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!("{SETTINGS_FILE}.tmp"));
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).expect("settings serialize"))?;
        std::fs::rename(tmp, dir.join(SETTINGS_FILE))
    }
}

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
    pub settings: Settings,
}

pub struct Session {
    dir: PathBuf,
    emit: Emit,
    kdf: KdfParams,
    state: Mutex<State>,
    settings: Mutex<Settings>,
    last_activity: AtomicI64,
}

impl Session {
    /// `kdf` is the cost for new vaults; pass [`KdfParams::recommended`]
    /// outside tests.
    pub fn new(dir: PathBuf, emit: Emit, kdf: KdfParams) -> Self {
        let state = if Vault::exists(&dir) { State::Locked } else { State::NoVault };
        let settings = Settings::load(&dir);
        Self {
            dir,
            emit,
            kdf,
            state: Mutex::new(state),
            settings: Mutex::new(settings),
            last_activity: AtomicI64::new(now_ms()),
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
            settings: self.settings.lock().clone(),
        }
    }

    // ------------------------------------------------------------ vault ---

    /// Creates the vault and unlocks it. Returns the recovery key, which the UI
    /// must show exactly once.
    pub fn create(&self, password: &str, user_name: &str) -> ApiResult<String> {
        check_password(password)?;
        let name = user_name.trim();
        if name.is_empty() {
            return Err(ApiError::new("invalid", "Bitte einen Namen angeben."));
        }
        if !matches!(*self.state.lock(), State::NoVault) {
            return Err(ApiError::new("vault_exists", "Es gibt bereits einen Tresor."));
        }
        let (vault, keys, recovery) = Vault::create(&self.dir, password, self.kdf.clone())?;
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
        u.vault.remove_slot(SlotKind::Keychain)?;
        Ok(())
    }

    /// Requires the current password, so an unattended unlocked Mac is not
    /// enough to take over the vault. Slow (Argon2 twice).
    pub fn change_password(&self, current: &str, new_password: &str) -> ApiResult<()> {
        check_password(new_password)?;
        self.verify_password(current)?;
        let mut state = self.state.lock();
        let State::Unlocked(u) = &mut *state else { return Err(ApiError::locked()) };
        u.vault.change_password(&u.keys, new_password)?;
        Ok(())
    }

    /// Issues a new recovery key; the old one stops working.
    pub fn rotate_recovery_key(&self, current: &str) -> ApiResult<String> {
        self.verify_password(current)?;
        let mut state = self.state.lock();
        let State::Unlocked(u) = &mut *state else { return Err(ApiError::locked()) };
        Ok(u.vault.rotate_recovery_key(&u.keys)?.to_string())
    }

    /// Drops the engine and all key material.
    pub fn lock(&self) {
        let mut state = self.state.lock();
        if let State::Unlocked(u) = std::mem::replace(&mut *state, State::Locked) {
            u.forwarder.abort();
            drop(u);
            drop(state);
            tracing::info!("vault locked");
            self.emit_status();
        }
    }

    // --------------------------------------------------------- activity ---

    /// Engine and acting context for one command; counts as user activity.
    pub fn engine(&self) -> ApiResult<(Arc<Engine>, Ctx)> {
        let state = self.state.lock();
        let State::Unlocked(u) = &*state else { return Err(ApiError::locked()) };
        self.last_activity.store(now_ms(), Ordering::Relaxed);
        Ok((u.engine.clone(), u.ctx.clone()))
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().clone()
    }

    pub fn update_settings(&self, mut settings: Settings) -> ApiResult<Settings> {
        settings.auto_lock_minutes = settings.auto_lock_minutes.clamp(1, 24 * 60);
        settings.save(&self.dir).map_err(ApiError::internal)?;
        *self.settings.lock() = settings.clone();
        self.last_activity.store(now_ms(), Ordering::Relaxed);
        Ok(settings)
    }

    /// Called periodically; locks after the configured idle time.
    pub fn maybe_auto_lock(&self) {
        let idle_ms = now_ms() - self.last_activity.load(Ordering::Relaxed);
        let limit_ms = self.settings.lock().auto_lock_minutes as i64 * 60_000;
        if idle_ms > limit_ms && self.is_unlocked() {
            tracing::info!("auto-lock after {} min idle", idle_ms / 60_000);
            self.lock();
        }
    }

    pub fn on_system_event(&self, trigger: LockTrigger) {
        let settings = self.settings.lock().clone();
        let wanted = match trigger {
            LockTrigger::SystemSleep => settings.lock_on_sleep,
            LockTrigger::ScreenLocked => settings.lock_on_screen_lock,
        };
        if wanted && self.is_unlocked() {
            tracing::info!("locking on {trigger:?}");
            self.lock();
        }
    }

    pub fn is_unlocked(&self) -> bool {
        matches!(*self.state.lock(), State::Unlocked(_))
    }

    // --------------------------------------------------------- internals ---

    fn emit_status(&self) {
        let status = serde_json::to_value(self.status()).expect("status serializes");
        (self.emit)("bm:session", status);
    }

    fn verify_password(&self, password: &str) -> ApiResult<()> {
        Vault::load(&self.dir)?.unlock_with_password(password)?;
        Ok(())
    }

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
        let emit = self.emit.clone();
        let forwarder = tauri::async_runtime::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => emit("bm:event", serde_json::to_value(&event).expect("event")),
                    Err(RecvError::Lagged(_)) => emit("bm:resync", serde_json::Value::Null),
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
        self.emit_status();
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

fn check_password(password: &str) -> ApiResult<()> {
    if password.chars().count() < MIN_PASSWORD {
        return Err(ApiError::new("invalid", "Das Passwort braucht mindestens 8 Zeichen."));
    }
    Ok(())
}

fn enroll(u: &mut Unlocked) -> ApiResult<()> {
    let previous = u.vault.keychain_account().map(str::to_string);
    let (account, key) = u.vault.enroll_keychain(&u.keys)?;
    if let Err(e) = keychain::store_secret(KEYCHAIN_SERVICE, &account, key.as_bytes()) {
        // Never leave a slot whose key is not in the Keychain.
        let _ = u.vault.remove_slot(SlotKind::Keychain);
        return Err(e.into());
    }
    if let Some(old) = previous.filter(|old| *old != account) {
        let _ = keychain::delete_secret(KEYCHAIN_SERVICE, &old);
    }
    Ok(())
}
