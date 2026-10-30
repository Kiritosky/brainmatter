//! Errors as the UI sees them: a stable `code` to branch on, a German message
//! to show, and candidates for ambiguous or unknown references.

use bm_core::{CoreError, VaultError, model::NodeBrief};
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
        Self::new("locked", "Brainmatter ist gesperrt.")
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
                message: format!("{kind} „{reference}“ nicht gefunden"),
                candidates: suggestions,
            },
            CoreError::Ambiguous { reference, candidates } => Self {
                code: "ambiguous",
                message: format!("„{reference}“ ist mehrdeutig"),
                candidates,
            },
            CoreError::Invalid(m) => Self::new("invalid", m),
            CoreError::Conflict(m) => Self::new("conflict", m),
            other => Self::internal(other),
        }
    }
}

impl From<VaultError> for ApiError {
    fn from(e: VaultError) -> Self {
        match e {
            VaultError::WrongPassword => Self::new("wrong_password", "Falsches Passwort."),
            VaultError::WrongRecoveryKey => {
                Self::new("wrong_recovery_key", "Der Wiederherstellungsschlüssel passt nicht.")
            }
            VaultError::SlotMissing(_) | VaultError::WrongSlotKey(_) => {
                Self::new("touch_id_reenroll", "Touch ID muss neu eingerichtet werden.")
            }
            VaultError::AlreadyExists(_) => {
                Self::new("vault_exists", "Es gibt bereits einen Tresor.")
            }
            other => Self::internal(other),
        }
    }
}

impl From<KeychainError> for ApiError {
    fn from(e: KeychainError) -> Self {
        match e {
            KeychainError::NotAccessible => Self::new(
                "touch_id_reenroll",
                "Nach einem Update bitte einmal mit Passwort entsperren; Touch ID wird danach neu eingerichtet.",
            ),
            other => Self::internal(other),
        }
    }
}
