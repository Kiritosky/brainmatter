//! `brainmatter-mcp`: stdio bridge for MCP clients that cannot speak HTTP.
//!
//! Reads JSON-RPC messages line by line from stdin, forwards them to the
//! Streamable HTTP server inside the Brainmatter app and writes every response
//! to stdout. All logic (and all data) stays in the app; this holds no state
//! except the session id and the negotiated protocol version.
//!
//! Environment: `BRAINMATTER_TOKEN` (required), `BRAINMATTER_URL` (optional).

use std::{sync::Arc, time::Duration};

use futures_util::StreamExt;
use reqwest::{Client, Response, StatusCode, header};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{Mutex, RwLock},
};

const DEFAULT_URL: &str = "http://127.0.0.1:47474/mcp";
const APP_BUNDLE_ID: &str = "app.brainmatter";
const APP_START_WAIT: Duration = Duration::from_secs(15);
const SESSION_HEADER: &str = "mcp-session-id";
const VERSION_HEADER: &str = "mcp-protocol-version";

struct Bridge {
    http: Client,
    url: String,
    token: String,
    session: RwLock<Option<String>>,
    version: RwLock<Option<String>>,
    stdout: Mutex<tokio::io::Stdout>,
}

impl Bridge {
    async fn write(&self, message: &str) {
        let mut out = self.stdout.lock().await;
        let _ = out.write_all(message.as_bytes()).await;
        let _ = out.write_all(b"\n").await;
        let _ = out.flush().await;
    }

    async fn post(&self, line: &str) -> reqwest::Result<Response> {
        let mut req = self
            .http
            .post(&self.url)
            .bearer_auth(&self.token)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json, text/event-stream")
            .body(line.to_string());
        if let Some(id) = &*self.session.read().await {
            req = req.header(SESSION_HEADER, id);
        }
        if let Some(version) = &*self.version.read().await {
            req = req.header(VERSION_HEADER, version);
        }
        req.send().await
    }

    /// Posts, starting the app and retrying once if nothing is listening.
    async fn post_or_start_app(&self, line: &str) -> reqwest::Result<Response> {
        match self.post(line).await {
            Err(e) if e.is_connect() => {
                let _ = tokio::process::Command::new("open")
                    .args(["-g", "-b", APP_BUNDLE_ID])
                    .status()
                    .await;
                let deadline = tokio::time::Instant::now() + APP_START_WAIT;
                loop {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    match self.post(line).await {
                        Err(e) if e.is_connect() && tokio::time::Instant::now() < deadline => {}
                        other => return other,
                    }
                }
            }
            other => other,
        }
    }

    async fn forward(&self, line: String) {
        let request: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        let id = request.get("id").cloned();
        let response = match self.post_or_start_app(&line).await {
            Ok(r) => r,
            Err(e) => return self.fail(id, format!("Brainmatter is not reachable: {e}")).await,
        };

        if let Some(session) = response.headers().get(SESSION_HEADER).and_then(|v| v.to_str().ok())
        {
            *self.session.write().await = Some(session.to_string());
        }
        let status = response.status();
        if status == StatusCode::UNAUTHORIZED {
            return self.fail(id, "Invalid Brainmatter token (BRAINMATTER_TOKEN).".into()).await;
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return self.fail(id, format!("Brainmatter answered {status}: {body}")).await;
        }

        let is_stream = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/event-stream"));
        if is_stream {
            self.relay_events(response).await;
        } else {
            let body = response.text().await.unwrap_or_default();
            if !body.trim().is_empty() {
                self.deliver(body.trim()).await;
            }
        }
    }

    /// Parses Server-Sent Events; each `data:` payload is one JSON-RPC message.
    async fn relay_events(&self, response: Response) {
        let mut stream = response.bytes_stream();
        let mut buffer = Vec::new();
        while let Some(Ok(chunk)) = stream.next().await {
            buffer.extend_from_slice(&chunk);
            while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
                let raw: Vec<u8> = buffer.drain(..=end).collect();
                let line = String::from_utf8_lossy(&raw);
                if let Some(data) = line.trim_end().strip_prefix("data:") {
                    let data = data.trim();
                    if !data.is_empty() {
                        self.deliver(data).await;
                    }
                }
            }
        }
    }

    async fn deliver(&self, message: &str) {
        // Later requests must announce the version negotiated by `initialize`.
        if let Ok(value) = serde_json::from_str::<Value>(message)
            && let Some(version) = value.pointer("/result/protocolVersion").and_then(Value::as_str)
        {
            *self.version.write().await = Some(version.to_string());
        }
        self.write(message).await;
    }

    async fn fail(&self, id: Option<Value>, message: String) {
        eprintln!("brainmatter-mcp: {message}");
        // Notifications have no id and therefore no response.
        if let Some(id) = id {
            let error =
                json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": message}});
            self.write(&error.to_string()).await;
        }
    }
}

#[tokio::main]
async fn main() {
    let Ok(token) = std::env::var("BRAINMATTER_TOKEN") else {
        eprintln!(
            "brainmatter-mcp: set BRAINMATTER_TOKEN to a token from Brainmatter → Einstellungen → KI-Verbindungen"
        );
        std::process::exit(2);
    };
    let bridge = Arc::new(Bridge {
        http: Client::new(),
        url: std::env::var("BRAINMATTER_URL").unwrap_or_else(|_| DEFAULT_URL.to_string()),
        token,
        session: RwLock::new(None),
        version: RwLock::new(None),
        stdout: Mutex::new(tokio::io::stdout()),
    });

    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut tasks = tokio::task::JoinSet::new();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let is_initialize = serde_json::from_str::<Value>(&line)
            .is_ok_and(|v| v.get("method").and_then(Value::as_str) == Some("initialize"));
        if is_initialize {
            // Everything else needs the session id this request produces.
            bridge.forward(line).await;
        } else {
            // A tool waiting for Touch ID must not hold up other requests.
            let bridge = bridge.clone();
            tasks.spawn(async move { bridge.forward(line).await });
        }
    }
    // stdin closed: let in-flight requests finish, then exit.
    while tasks.join_next().await.is_some() {}
}
