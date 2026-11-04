//! Errors as the UI sees them: a stable `code` to branch on, a German message
//! to show, and candidates for ambiguous or unknown references.

use crate::i18n::tr;
use bm_core::{CoreError, VaultError, mirror::MirrorError, model::NodeBrief};
use bm_macos::keychain::KeychainError;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<NodeBrief>,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), candidates: Vec::new() }
    }

    pub fn locked() -> Self {
        Self::new("locked", tr("Brainmatter ist gesperrt.", "Brainmatter is locked."))
    }

    pub fn internal(e: impl std::fmt::Display) -> Self {
        tracing::error!("internal error: {e}");
        Self::new("internal", format!("Interner Fehler: {e}"))
    }
}

impl From<CoreError> for ApiError {
    fn from(e: CoreError) -> Self {
        match e {
            CoreError::NotFound { kind, reference, suggestions } => Self {
                code: "not_found",
                message: match crate::i18n::german() {
                    true => format!("{kind} „{reference}“ nicht gefunden"),
                    false => format!("{kind} “{reference}” not found"),
                },
                candidates: suggestions,
            },
            CoreError::Ambiguous { reference, candidates } => Self {
                code: "ambiguous",
                message: match crate::i18n::german() {
                    true => format!("„{reference}“ ist mehrdeutig"),
                    false => format!("“{reference}” is ambiguous"),
                },
                candidates,
            },
            CoreError::Invalid(m) => Self::new("invalid", m),
            CoreError::Conflict(m) => Self::new("conflict", m),
            CoreError::CorruptBlob(_) => Self::new(
                "corrupt",
                tr(
                    "Die Datei ist beschädigt oder wurde verändert.",
                    "The file is damaged or was tampered with.",
                ),
            ),
            other => Self::internal(other),
        }
    }
}

impl From<VaultError> for ApiError {
    fn from(e: VaultError) -> Self {
        match e {
            VaultError::WrongPassword => {
                Self::new("wrong_password", tr("Falsches Passwort.", "Wrong password."))
            }
            VaultError::WrongRecoveryKey => Self::new(
                "wrong_recovery_key",
                tr(
                    "Der Wiederherstellungsschlüssel passt nicht.",
                    "The recovery key does not match.",
                ),
            ),
            VaultError::SlotMissing(_) | VaultError::WrongSlotKey(_) => {
                Self::new("touch_id_reenroll", "Touch ID muss neu eingerichtet werden.")
            }
            VaultError::AlreadyExists(_) => Self::new(
                "vault_exists",
                tr("Es gibt bereits einen Tresor.", "There is a vault already."),
            ),
            other => Self::internal(other),
        }
    }
}

impl From<MirrorError> for ApiError {
    fn from(e: MirrorError) -> Self {
        match e {
            MirrorError::OtherVault => Self::new(
                "other_vault",
                tr(
                    "In diesem Ordner liegt das Backup eines anderen Tresors. Bitte einen anderen wählen.",
                    "This folder holds the backup of another vault. Please choose another one.",
                ),
            ),
            MirrorError::SameFolder => Self::new(
                "invalid",
                tr(
                    "Das ist der Ordner des Tresors selbst. Bitte einen anderen Ort wählen.",
                    "This is the vault's own folder. Please choose another place.",
                ),
            ),
            MirrorError::Empty => Self::new(
                "not_a_backup",
                tr(
                    "In diesem Ordner liegt noch kein Backup.",
                    "There is no backup in this folder yet.",
                ),
            ),
            MirrorError::Damaged => Self::new(
                "corrupt",
                tr(
                    "Keines der Backups in diesem Ordner ist lesbar.",
                    "None of the backups in this folder can be read.",
                ),
            ),
            MirrorError::VaultExists => Self::new(
                "vault_exists",
                tr("Es gibt bereits einen Tresor.", "There is a vault already."),
            ),
            MirrorError::Vault(e) => e.into(),
            MirrorError::Core(e) => e.into(),
            MirrorError::Io(e) => Self::new(
                "io",
                match crate::i18n::german() {
                    true => format!("In den Ordner lässt sich nicht schreiben: {e}"),
                    false => format!("The folder cannot be written to: {e}"),
                },
            ),
        }
    }
}

impl From<KeychainError> for ApiError {
    fn from(e: KeychainError) -> Self {
        match e {
            KeychainError::NotAccessible => Self::new(
                "touch_id_reenroll",
                tr(
                    "Nach einem Update bitte einmal mit Passwort entsperren; Touch ID wird danach neu eingerichtet.",
                    "After an update, please unlock once with your password; Touch ID is set up again afterwards.",
                ),
            ),
            other => Self::internal(other),
        }
    }
}
