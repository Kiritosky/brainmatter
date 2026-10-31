//! MCP server on a throwaway vault, for testing real clients without the app:
//!
//! ```sh
//! cargo run -p bm-mcp --example dev_server          # prints a token
//! claude mcp add --transport http brainmatter http://127.0.0.1:47476/mcp \
//!     --header "Authorization: Bearer <token>"
//! ```

use std::sync::Arc;

use bm_core::{Engine, Vault, vault::KdfParams};
use bm_mcp::{ClientRegistry, Scope, StaticHub};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let dir = tempfile::tempdir()?;
    let (vault, keys, _) = Vault::create(dir.path(), "dev", KdfParams::insecure_fast())?;
    let engine = Arc::new(Engine::open(&vault.db_path(), &keys.db_key(), 2)?);
    let registry = Arc::new(ClientRegistry::in_memory());
    let (_, token) =
        registry.create("Dev Client", &[Scope::Read, Scope::Write, Scope::Delete], &[])?;
    println!("TOKEN {token}");
    bm_mcp::serve(
        "127.0.0.1:47476",
        registry,
        Arc::new(StaticHub(engine, None)),
        Default::default(),
    )
    .await?;
    Ok(())
}
