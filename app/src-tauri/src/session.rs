//! Lock state of the app: no vault yet, locked, or unlocked with an open
//! engine. Secrets and the engine exist only while unlocked.
//!
//! The session knows nothing about Tauri windows: it reports changes through
//! an [`Emit`] callback, which keeps it testable without a running app.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicI64, Ordering},
    },
    time::{Duration, Instant},
};

use bm_core::{
    Engine, Vault, VaultKeys,
    backups::{self, BackupInfo},
    blobs::BlobStore,
    engine::{Indexer, spawn_indexer},
    model::{ActorKind, Ctx},
    semantic::Embedder,
    time::now_ms,
    vault::{KdfParams, RecoveryKey, SecretKey, SlotKind},
};
use bm_embed::{DOWNLOAD_BYTES, E5Embedder, ModelStore};
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
/// How long an AI request waits for the user to answer the Touch ID prompt.
const UNLOCK_WAIT: Duration = Duration::from_secs(30);
/// After a dismissed prompt, this client does not trigger another one for a while.
const PROMPT_COOLDOWN_MS: i64 = 5 * 60 * 1000;
/// The model takes ~0.9 GB while loaded, so it leaves memory when unused.
const MODEL_IDLE_MS: i64 = 5 * 60 * 1000;
/// Shorter queries are prefixes of a word being typed; meaning adds only noise.
const MIN_SEMANTIC_QUERY: usize = 3;
const PROGRESS_INTERVAL_MS: i64 = 150;
const BACKUP_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;
/// How long a restore waits for running commands to let go of the database.
const RESTORE_WAIT: Duration = Duration::from_secs(5);

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
    pub auto_backup: bool,
    pub theme: Theme,
    pub quick_capture: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_lock_minutes: 60,
            lock_on_sleep: true,
            lock_on_screen_lock: false,
            auto_backup: true,
            theme: Theme::System,
            quick_capture: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    System,
    Light,
    Dark,
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
    /// First, so it stops before the engine goes away.
    indexer: Option<Indexer>,
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

enum Download {
    Idle,
    Running(u64),
    Failed(String),
}

/// The settings page's view of the backups.
#[derive(Debug, Clone, Serialize)]
pub struct BackupStatus {
    pub running: bool,
    /// Why the last automatic backup failed.
    pub error: Option<String>,
    pub backups: Vec<BackupInfo>,
}

/// The optional embedding model, for the settings page.
#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    /// `"missing"`, `"downloading"` or `"ready"`.
    pub state: &'static str,
    pub downloaded: u64,
    pub size: u64,
    pub error: Option<String>,
    /// Entries embedded so far, and entries overall (while unlocked).
    pub indexed: usize,
    pub total: usize,
}

pub struct Session {
    dir: PathBuf,
    emit: Emit,
    kdf: KdfParams,
    state: Mutex<State>,
    settings: Mutex<Settings>,
    last_activity: AtomicI64,
    /// A Touch ID prompt for an AI client is on screen.
    prompting: AtomicBool,
    /// Client name → no new prompt before this time (ms).
    prompt_blocked_until: Mutex<HashMap<String, i64>>,
    models: ModelStore,
    embedder: Arc<E5Embedder>,
    download: Mutex<Download>,
    backup_running: AtomicBool,
    backup_error: Mutex<Option<String>>,
    restoring: AtomicBool,
}

impl Session {
    /// `kdf` is the cost for new vaults; pass [`KdfParams::recommended`]
    /// outside tests.
    pub fn new(dir: PathBuf, emit: Emit, kdf: KdfParams) -> Self {
        let state = if Vault::exists(&dir) { State::Locked } else { State::NoVault };
        let settings = Settings::load(&dir);
        let models = ModelStore::new(dir.join("models"));
        Self {
            embedder: Arc::new(E5Embedder::new(models.clone())),
            models,
            download: Mutex::new(Download::Idle),
            backup_running: AtomicBool::new(false),
            backup_error: Mutex::new(None),
            restoring: AtomicBool::new(false),
            dir,
            emit,
            kdf,
            state: Mutex::new(state),
            settings: Mutex::new(settings),
            last_activity: AtomicI64::new(now_ms()),
            prompting: AtomicBool::new(false),
            prompt_blocked_until: Mutex::new(HashMap::new()),
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
        let engine = open_engine(&self.dir, &vault, &keys)?;
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
            self.embedder.unload();
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

    /// The open engine for an AI client. Deliberately does not count as user
    /// activity: a busy AI must not keep an unattended vault unlocked.
    fn open_engine(&self) -> Option<Arc<Engine>> {
        match &*self.state.lock() {
            State::Unlocked(u) => Some(u.engine.clone()),
            _ => None,
        }
    }

    /// Like [`Self::open_engine`], but if the vault is locked, asks the user
    /// with a Touch ID prompt naming the client. Blocks up to [`UNLOCK_WAIT`].
    /// Only one prompt is ever open; a dismissed prompt silences that client
    /// for [`PROMPT_COOLDOWN_MS`]. The error text is shown to the AI.
    pub fn engine_for_client(self: &Arc<Self>, client: &str) -> Result<Arc<Engine>, String> {
        const LOCKED: &str =
            "Brainmatter is locked. Ask the user to unlock it in the app, then retry.";
        if let Some(engine) = self.open_engine() {
            return Ok(engine);
        }
        if !self.status().touch_id_enrolled {
            return Err(LOCKED.into());
        }
        let blocked = self.prompt_blocked_until.lock().get(client).is_some_and(|t| *t > now_ms());
        if blocked {
            return Err(LOCKED.into());
        }

        if !self.prompting.swap(true, Ordering::SeqCst) {
            let session = self.clone();
            let name = client.to_string();
            std::thread::spawn(move || {
                let reason = format!("„{name}“ möchte auf Brainmatter zugreifen");
                if let Err(e) = session.unlock_touch_id(&reason) {
                    tracing::info!("unlock for {name} refused: {}", e.code);
                    session.prompt_blocked_until.lock().insert(name, now_ms() + PROMPT_COOLDOWN_MS);
                }
                session.prompting.store(false, Ordering::SeqCst);
            });
        }

        let deadline = Instant::now() + UNLOCK_WAIT;
        while Instant::now() < deadline {
            if let Some(engine) = self.open_engine() {
                return Ok(engine);
            }
            if !self.prompting.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(LOCKED.into())
    }

    // ---------------------------------------------------------- backups ---

    fn backup_dir(&self) -> PathBuf {
        self.dir.join("backups")
    }

    pub fn backup_status(&self) -> ApiResult<BackupStatus> {
        Ok(BackupStatus {
            running: self.backup_running.load(Ordering::SeqCst),
            error: self.backup_error.lock().clone(),
            backups: backups::list(&self.backup_dir())?,
        })
    }

    /// Slow for large vaults: call from a blocking thread. Only one at a time.
    pub fn backup_now(&self) -> ApiResult<BackupInfo> {
        let (db, key) = {
            let state = self.state.lock();
            let State::Unlocked(u) = &*state else { return Err(ApiError::locked()) };
            (u.vault.db_path(), u.keys.db_key())
        };
        if self.backup_running.swap(true, Ordering::SeqCst) {
            return Err(ApiError::new("busy", "Es läuft bereits ein Backup."));
        }
        let result = backups::create(&db, &key, &self.backup_dir());
        self.backup_running.store(false, Ordering::SeqCst);
        *self.backup_error.lock() = result.as_ref().err().map(ToString::to_string);
        Ok(result?)
    }

    /// Called periodically: starts a backup in the background if there are
    /// changes the newest backup does not have and it is a day old. Unchanged
    /// days add nothing, and would push older states out of the retention.
    pub fn maybe_backup(self: &Arc<Self>) {
        if !self.settings.lock().auto_backup || self.backup_running.load(Ordering::SeqCst) {
            return;
        }
        let Some(engine) = self.open_engine() else { return };
        let last = backups::list(&self.backup_dir()).ok().and_then(|b| b.first().map(|b| b.at));
        let changed = engine.change_sets(1, None).ok().and_then(|c| c.first().map(|c| c.at));
        let due = match (last, changed) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(last), Some(changed)) => changed > last && now_ms() - last >= BACKUP_INTERVAL_MS,
        };
        if due {
            let session = self.clone();
            std::thread::spawn(move || match session.backup_now() {
                Ok(b) => tracing::info!("backup written: {} bytes", b.size),
                Err(e) => tracing::warn!("backup failed: {}", e.message),
            });
        }
    }

    /// Replaces all data with a backup and locks. The current state is saved as
    /// a backup first, so this can be undone. Asks for the password because it
    /// is the most far-reaching action in the app. Slow: blocking thread.
    pub fn restore_backup(&self, name: &str, password: &str) -> ApiResult<()> {
        if self.backup_running.load(Ordering::SeqCst) {
            return Err(ApiError::new("busy", "Es läuft gerade ein Backup."));
        }
        let vault = Vault::load(&self.dir)?;
        let keys = vault.unlock_with_password(password)?;
        // Checked before locking, so a bad backup does not interrupt the user.
        let source = self.backup_dir().join(name);
        if !backups::list(&self.backup_dir())?.iter().any(|b| b.name == name) {
            return Err(ApiError::new("not_found", "Dieses Backup gibt es nicht mehr."));
        }
        backups::verify(&source, &keys.db_key())
            .map_err(|_| ApiError::new("corrupt", "Dieses Backup ist beschädigt."))?;

        // Files of an open database must not be swapped underneath it, and
        // running commands or AI calls can still hold the engine.
        let engine = Arc::downgrade(&self.open_engine().ok_or_else(ApiError::locked)?);
        // Keeps everyone out while the files change: the UI is about to show the
        // lock screen, and an AI request could ask for Touch ID.
        self.restoring.store(true, Ordering::SeqCst);
        self.lock();
        let deadline = Instant::now() + RESTORE_WAIT;
        let result = loop {
            if engine.strong_count() == 0 {
                break backups::restore(&vault.db_path(), &keys.db_key(), &self.backup_dir(), name)
                    .map_err(ApiError::from);
            }
            if Instant::now() >= deadline {
                break Err(ApiError::new(
                    "busy",
                    "Die Datenbank war noch in Benutzung und wurde nicht ersetzt. Bitte noch einmal versuchen.",
                ));
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        self.restoring.store(false, Ordering::SeqCst);
        // The dialog is gone by now, so this is where the user can read it.
        *self.backup_error.lock() = result.as_ref().err().map(|e| e.message.clone());
        result.map(|_| tracing::info!("restored a backup"))
    }

    // ------------------------------------------------------------ model ---

    /// The model for AI clients, once it is downloaded.
    pub fn embedder(&self) -> Option<Arc<dyn Embedder>> {
        self.models.is_ready().then(|| self.embedder.clone() as Arc<dyn Embedder>)
    }

    /// The vector for a search as you type. Never waits for the model to load:
    /// until it is in memory, search works by words alone.
    pub fn query_vector(&self, text: &str) -> Option<Vec<f32>> {
        if text.trim().chars().count() < MIN_SEMANTIC_QUERY || !self.models.is_ready() {
            return None;
        }
        if !self.embedder.is_loaded() {
            self.embedder.warm();
            return None;
        }
        self.embedder.embed_query(text).map_err(|e| tracing::warn!("query failed: {e}")).ok()
    }

    pub fn model_status(&self) -> ModelStatus {
        let (state, downloaded, error) = match &*self.download.lock() {
            Download::Running(bytes) => ("downloading", *bytes, None),
            Download::Failed(e) => ("missing", 0, Some(e.clone())),
            Download::Idle if self.models.is_ready() => ("ready", DOWNLOAD_BYTES, None),
            Download::Idle => ("missing", 0, None),
        };
        let index = self.open_engine().and_then(|e| e.index_status().ok());
        ModelStatus {
            state,
            downloaded,
            size: DOWNLOAD_BYTES,
            error,
            indexed: index.as_ref().map_or(0, |i| i.indexed),
            total: index.as_ref().map_or(0, |i| i.total),
        }
    }

    /// Downloads in the background and reports through `bm:model`. Only the
    /// user starts this: it is the app's only outgoing connection.
    pub fn download_model(self: &Arc<Self>) {
        {
            let mut download = self.download.lock();
            if matches!(*download, Download::Running(_)) {
                return;
            }
            *download = Download::Running(0);
        }
        self.emit_model();
        let session = self.clone();
        std::thread::spawn(move || {
            let last = AtomicI64::new(0);
            let result = session.models.download(|bytes| {
                *session.download.lock() = Download::Running(bytes);
                let now = now_ms();
                if now - last.load(Ordering::Relaxed) >= PROGRESS_INTERVAL_MS {
                    last.store(now, Ordering::Relaxed);
                    session.emit_model();
                }
            });
            *session.download.lock() = match result {
                Ok(()) => Download::Idle,
                Err(e) => {
                    tracing::warn!("model download failed: {e}");
                    Download::Failed(e.to_string())
                }
            };
            session.start_indexer();
            session.emit_model();
        });
    }

    pub fn remove_model(&self) -> ApiResult<()> {
        if matches!(*self.download.lock(), Download::Running(_)) {
            return Err(ApiError::new("invalid", "Der Download läuft noch."));
        }
        if let State::Unlocked(u) = &mut *self.state.lock() {
            u.indexer = None;
            u.engine.unload_vectors();
        }
        self.embedder.unload();
        self.models.remove().map_err(ApiError::internal)?;
        *self.download.lock() = Download::Idle;
        self.emit_model();
        Ok(())
    }

    /// Called periodically.
    pub fn unload_idle_model(&self) {
        self.embedder.unload_if_idle(MODEL_IDLE_MS);
    }

    fn spawn_indexer(&self, engine: &Arc<Engine>) -> Option<Indexer> {
        if !self.models.is_ready() {
            return None;
        }
        spawn_indexer(engine, self.embedder.clone())
            .map_err(|e| tracing::warn!("indexer not started: {e}"))
            .ok()
    }

    fn start_indexer(&self) {
        let Some(engine) = self.open_engine() else { return };
        let indexer = self.spawn_indexer(&engine);
        if let State::Unlocked(u) = &mut *self.state.lock()
            && Arc::ptr_eq(&u.engine, &engine)
        {
            u.indexer = indexer;
        }
    }

    fn emit_model(&self) {
        (self.emit)("bm:model", serde_json::to_value(self.model_status()).expect("model status"));
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
        if self.restoring.load(Ordering::SeqCst) {
            return Err(ApiError::new("busy", "Das Backup wird gerade wiederhergestellt."));
        }
        match *self.state.lock() {
            State::Locked => Ok(Vault::load(&self.dir)?),
            State::Unlocked(_) => Err(ApiError::new("already_unlocked", "Bereits entsperrt.")),
            State::NoVault => Err(ApiError::new("no_vault", "Es gibt noch keinen Tresor.")),
        }
    }

    fn open(&self, vault: Vault, keys: VaultKeys) -> ApiResult<()> {
        let engine = open_engine(&self.dir, &vault, &keys)?;
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
            indexer: self.spawn_indexer(&engine),
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

fn open_engine(dir: &Path, vault: &Vault, keys: &VaultKeys) -> ApiResult<Engine> {
    let blobs = BlobStore::new(dir.join("blobs"), &keys.blob_key());
    Ok(Engine::open(&vault.db_path(), &keys.db_key(), READERS)?.with_blobs(blobs))
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
