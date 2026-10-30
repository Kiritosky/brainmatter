use std::sync::Arc;

use bm_core::Engine;

use crate::Client;

/// Where tools get the engine from. The app implements this on top of its
/// lock session; tests use [`StaticHub`].
pub trait Hub: Send + Sync + 'static {
    /// The open engine. If the vault is locked, the implementation may block
    /// to ask the user (e.g. Touch ID) on behalf of `client`. The error text
    /// is shown to the AI.
    fn engine(&self, client: &Client) -> Result<Arc<Engine>, String>;
}

/// A hub that always has the same engine (tests, headless use).
pub struct StaticHub(pub Arc<Engine>);

impl Hub for StaticHub {
    fn engine(&self, _client: &Client) -> Result<Arc<Engine>, String> {
        Ok(self.0.clone())
    }
}
