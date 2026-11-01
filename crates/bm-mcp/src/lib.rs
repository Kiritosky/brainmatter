//! MCP server exposing the Brainmatter knowledge graph over Streamable HTTP.

mod auth;
mod clients;
mod format;
mod hub;
mod schema;
mod server;
mod tools;

pub use clients::{Client, ClientRegistry, RegistryError, Scope};
pub use hub::{Hub, StaticHub};
pub use server::BrainmatterMcp;

use std::sync::Arc;

use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tokio_util::sync::CancellationToken;

pub const DEFAULT_ADDR: &str = "127.0.0.1:47474";

/// Builds the `/mcp` router. Every request must carry a valid client token;
/// any request with a browser `Origin` header is rejected (DNS-rebinding and
/// drive-by protection: legitimate MCP clients never send one).
pub fn router(
    registry: Arc<ClientRegistry>,
    hub: Arc<dyn Hub>,
    shutdown: CancellationToken,
) -> axum::Router {
    let config = StreamableHttpServerConfig::default()
        .with_allowed_hosts(["127.0.0.1", "localhost"])
        .enforce_origin_validation()
        .with_cancellation_token(shutdown);

    let service: StreamableHttpService<BrainmatterMcp, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(BrainmatterMcp::new(hub.clone())),
            Default::default(),
            config,
        );

    axum::Router::new()
        .nest_service("/mcp", service)
        .layer(axum::middleware::from_fn_with_state(registry, auth::require_bearer))
}

/// Binds `addr` and serves until `shutdown` fires. Returns once bound failed
/// or the server stopped.
pub async fn serve(
    addr: &str,
    registry: Arc<ClientRegistry>,
    hub: Arc<dyn Hub>,
    shutdown: CancellationToken,
) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("MCP listening on http://{}/mcp", listener.local_addr()?);
    let app = router(registry, hub, shutdown.child_token());
    axum::serve(listener, app)
        .with_graceful_shutdown(async move { shutdown.cancelled_owned().await })
        .await
}
