//! Brainmatter core: domain model, encrypted storage, graph, search.
//!
//! This crate has no UI or network dependencies; the Tauri app and the MCP
//! server are thin layers on top of it.

pub mod store;

pub use store::{Db, DbKey, StoreError};
