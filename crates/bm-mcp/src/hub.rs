use std::sync::Arc;

use bm_core::{Engine, semantic::Embedder};

use crate::Client;

/// Where tools get the engine from. The app implements this on top of its
/// lock session; tests use [`StaticHub`].
pub trait Hub: Send + Sync + 'static {
    /// The open engine. If the vault is locked, the implementation may block
    /// to ask the user (e.g. Touch ID) on behalf of `client`. The error text
    /// is shown to the AI.
    fn engine(&self, client: &Client) -> Result<Arc<Engine>, String>;

    /// The engine if the vault is unlocked; never asks the user anything. For
    /// requests clients make on their own, such as listing prompts on connect.
    fn open_engine(&self) -> Option<Arc<Engine>>;

    /// Where a bookmarked file is now. Platform dependent; without it, links keep
    /// the path they were created with.
    fn resolve_bookmark(&self, _bookmark: &[u8]) -> Option<String> {
        None
    }

    /// The embedding model, once the user has set it up. Without one, search
    /// falls back to full text and similarity hints are skipped.
    fn embedder(&self) -> Option<Arc<dyn Embedder>> {
        None
    }
}

/// A hub that always has the same engine (tests, headless use).
pub struct StaticHub(pub Arc<Engine>, pub Option<Arc<dyn Embedder>>);

impl Hub for StaticHub {
    fn engine(&self, _client: &Client) -> Result<Arc<Engine>, String> {
        Ok(self.0.clone())
    }

    fn open_engine(&self) -> Option<Arc<Engine>> {
        Some(self.0.clone())
    }

    fn embedder(&self) -> Option<Arc<dyn Embedder>> {
        self.1.clone()
    }
}
