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

    fn embedder(&self) -> Option<Arc<dyn Embedder>> {
        self.1.clone()
    }
}
