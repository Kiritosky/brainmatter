use crate::{model::NodeBrief, store::StoreError};

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("{kind} not found: {reference}")]
    NotFound { kind: &'static str, reference: String, suggestions: Vec<NodeBrief> },
    #[error("'{reference}' matches {} entries", candidates.len())]
    Ambiguous { reference: String, candidates: Vec<NodeBrief> },
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl CoreError {
    pub(crate) fn not_found(kind: &'static str, reference: impl Into<String>) -> Self {
        Self::NotFound { kind, reference: reference.into(), suggestions: Vec::new() }
    }
}

pub type Result<T, E = CoreError> = std::result::Result<T, E>;
