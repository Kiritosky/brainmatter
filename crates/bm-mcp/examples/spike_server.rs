//! Standalone MCP server for manual testing with real clients:
//!
//! ```sh
//! BM_TOKEN=secret cargo run -p bm-mcp --example spike_server
//! claude mcp add --transport http brainmatter http://127.0.0.1:47474/mcp \
//!     --header "Authorization: Bearer secret"
//! ```

use std::sync::Arc;

use bm_mcp::{Actor, TokenStore};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let token = std::env::var("BM_TOKEN").unwrap_or_else(|_| "bm_dev".into());
    let mut tokens = TokenStore::default();
    tokens.insert(&token, Actor { id: "dev".into(), name: "Dev Client".into() });

    let ct = CancellationToken::new();
    let app = bm_mcp::router(Arc::new(tokens), ct.child_token());
    let listener = tokio::net::TcpListener::bind(bm_mcp::DEFAULT_ADDR).await?;
    tracing::info!("MCP listening on http://{}/mcp", listener.local_addr()?);

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            ct.cancel();
        })
        .await?;
    Ok(())
}
