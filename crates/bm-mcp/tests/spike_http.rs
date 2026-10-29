//! Spike 2: Streamable HTTP MCP server with bearer auth and Origin rejection,
//! exercised by the official rmcp client.

use std::{sync::Arc, time::Instant};

use bm_mcp::{Actor, TokenStore};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ClientConfig},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use tokio_util::sync::CancellationToken;

const TOKEN: &str = "bm_test_token";

async fn start() -> (String, CancellationToken) {
    let mut tokens = TokenStore::default();
    tokens.insert(TOKEN, Actor { id: "a1".into(), name: "Claude Code".into() });

    let ct = CancellationToken::new();
    let app = bm_mcp::router(Arc::new(tokens), ct.child_token());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let shutdown = ct.clone();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move { shutdown.cancelled_owned().await })
            .await
            .unwrap();
    });
    (format!("http://{addr}/mcp"), ct)
}

fn init_body() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "curl", "version": "0" }
        }
    })
}

#[tokio::test]
async fn client_lists_and_calls_tools() -> anyhow::Result<()> {
    let (url, ct) = start().await;

    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(url.as_str()).auth_header(TOKEN),
    );
    let client = ClientConfig::default().serve(transport).await?;

    let info = client.peer_info().expect("server info");
    assert!(info.instructions.as_deref().unwrap_or("").contains("Search before you create"));

    let tools = client.list_all_tools().await?;
    assert!(tools.iter().any(|t| t.name == "ping"));

    let mut timings = Vec::new();
    for _ in 0..50 {
        let start = Instant::now();
        let res = client.call_tool(CallToolRequestParams::new("ping")).await?;
        timings.push(start.elapsed());
        let text = serde_json::to_string(&res.content)?;
        assert!(text.contains("Claude Code"), "{text}");
    }
    timings.sort();
    let p95 = timings[timings.len() * 95 / 100];
    eprintln!("ping roundtrip p50={:?} p95={:?}", timings[timings.len() / 2], p95);
    assert!(p95.as_millis() < 15, "p95 {p95:?}");

    client.cancel().await?;
    ct.cancel();
    Ok(())
}

#[tokio::test]
async fn rejects_missing_token_and_browser_origins() {
    let (url, ct) = start().await;
    let http = reqwest::Client::new();
    let post = || {
        http.post(&url).header("accept", "application/json, text/event-stream").json(&init_body())
    };

    let res = post().send().await.unwrap();
    assert_eq!(res.status(), 401);

    let res = post().bearer_auth("wrong").send().await.unwrap();
    assert_eq!(res.status(), 401);

    let res =
        post().bearer_auth(TOKEN).header("origin", "https://evil.example").send().await.unwrap();
    assert_eq!(res.status(), 403, "browser origin must be rejected");

    let res = post().bearer_auth(TOKEN).send().await.unwrap();
    assert!(res.status().is_success(), "valid token: {}", res.status());

    ct.cancel();
}
