//! Brainmatter core: domain model, encrypted storage, graph, search.
//!
//! This crate has no UI or network dependencies; the Tauri app and the MCP
//! server are thin layers on top of it.

pub mod engine;
pub mod error;
pub mod events;
pub mod ids;
pub mod model;
pub mod store;
pub mod time;
pub mod vault;
pub mod wikilinks;

pub use engine::Engine;
pub use error::{CoreError, Result};
pub use events::Event;
pub use store::{Db, DbKey, StoreError};
pub use vault::{Vault, VaultError, VaultKeys};
