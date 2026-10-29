//! MCP server exposing the Brainmatter knowledge graph over Streamable HTTP.

mod auth;
mod server;

pub use auth::{Actor, TokenStore};
pub use server::BrainmatterMcp;

use std::sync::Arc;

use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tokio_util::sync::CancellationToken;

pub const DEFAULT_ADDR: &str = "127.0.0.1:47474";

/// Builds the `/mcp` router. Every request must carry a valid bearer token;
/// any request with a browser `Origin` header is rejected (DNS-rebinding and
/// drive-by protection: legitimate MCP clients never send one).
pub fn router(tokens: Arc<TokenStore>, shutdown: CancellationToken) -> axum::Router {
    let config = StreamableHttpServerConfig::default()
        .with_allowed_hosts(["127.0.0.1", "localhost"])
        .enforce_origin_validation()
        .with_cancellation_token(shutdown);

    let service: StreamableHttpService<BrainmatterMcp, LocalSessionManager> =
        StreamableHttpService::new(|| Ok(BrainmatterMcp::new()), Default::default(), config);

    axum::Router::new()
        .nest_service("/mcp", service)
        .layer(axum::middleware::from_fn_with_state(tokens, auth::require_bearer))
}
