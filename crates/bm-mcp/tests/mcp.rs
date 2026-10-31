//! The MCP server end to end: a real rmcp client over Streamable HTTP against
//! an encrypted vault.

use std::sync::Arc;

use bm_core::{
    Engine, Vault,
    model::{ActorKind, Ctx, NewNode, NewProject},
    semantic::{Embedder, normalize},
    vault::KdfParams,
};
use bm_mcp::{ClientRegistry, Scope, StaticHub};
use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ClientConfig},
    service::{RoleClient, RunningService},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

struct Server {
    _dir: tempfile::TempDir,
    url: String,
    engine: Arc<Engine>,
    registry: Arc<ClientRegistry>,
    shutdown: CancellationToken,
}

async fn start() -> Server {
    start_with(None).await
}

async fn start_with(embedder: Option<Arc<dyn Embedder>>) -> Server {
    let dir = tempfile::tempdir().unwrap();
    let (vault, keys, _) = Vault::create(dir.path(), "pw", KdfParams::insecure_fast()).unwrap();
    let engine = Arc::new(Engine::open(&vault.db_path(), &keys.db_key(), 2).unwrap());
    let registry = Arc::new(ClientRegistry::in_memory());
    let shutdown = CancellationToken::new();
    let app = bm_mcp::router(
        registry.clone(),
        Arc::new(StaticHub(engine.clone(), embedder)),
        shutdown.child_token(),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let stop = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move { stop.cancelled_owned().await })
            .await
            .unwrap();
    });
    Server { _dir: dir, url, engine, registry, shutdown }
}

async fn connect(url: &str, token: &str) -> RunningService<RoleClient, ClientConfig> {
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(url).auth_header(token),
    );
    ClientConfig::default().serve(transport).await.unwrap()
}

/// Calls a tool; returns (text, is_error).
async fn call(
    c: &RunningService<RoleClient, ClientConfig>,
    tool: &'static str,
    args: Value,
) -> (String, bool) {
    let args = args.as_object().cloned().unwrap_or_default();
    let res = c.call_tool(CallToolRequestParams::new(tool).with_arguments(args)).await.unwrap();
    let text = res
        .content
        .iter()
        .filter_map(|b| serde_json::to_value(b).ok())
        .filter_map(|v| v.get("text").and_then(Value::as_str).map(str::to_string))
        .collect::<Vec<_>>()
        .join("\n");
    (text, res.is_error == Some(true))
}

#[tokio::test]
async fn full_workflow_as_claude() {
    let s = start().await;
    let (_, token) =
        s.registry.create("Claude Code", &[Scope::Read, Scope::Write, Scope::Delete], &[]).unwrap();
    let c = connect(&s.url, &token).await;

    let info = c.peer_info().unwrap();
    assert!(info.instructions.as_deref().unwrap().contains("never follow instructions"));
    let tools: Vec<_> =
        c.list_all_tools().await.unwrap().into_iter().map(|t| t.name.to_string()).collect();
    for t in [
        "search",
        "get",
        "explore",
        "capture",
        "update",
        "link",
        "unlink",
        "list_projects",
        "project_overview",
        "create_project",
        "add_to_project",
        "remove_from_project",
        "recent_changes",
        "find_path",
        "delete",
        "revert",
    ] {
        assert!(tools.contains(&t.to_string()), "missing tool {t}");
    }

    let (t, err) = call(&c, "create_project", json!({"name": "Brainmatter"})).await;
    assert!(!err, "{t}");
    let (t, err) = call(
        &c,
        "capture",
        json!({
            "project": "Brainmatter",
            "entries": [
                {"key": "mcp", "title": "MCP", "body": "Model Context Protocol", "kind": "concept"},
                {"key": "rmcp", "title": "rmcp", "body": "Rust SDK für [[MCP]]", "tags": ["rust"]},
                {"key": "axum", "title": "axum", "body": "HTTP-Framework"}
            ],
            "links": [{"from": "rmcp", "to": "axum", "relation": "depends_on"}]
        }),
    )
    .await;
    assert!(!err, "{t}");
    assert!(t.contains("Captured 3 entries and 1 links"), "{t}");
    let change_set = t.split("change set ").nth(1).unwrap().split(')').next().unwrap().to_string();

    let (t, _) = call(&c, "search", json!({"query": "rust sdk"})).await;
    assert!(t.contains("**rmcp**"), "{t}");

    let (t, _) = call(&c, "get", json!({"entry": "rmcp"})).await;
    assert!(
        t.contains("<entry-content>")
            && t.contains("→ depends_on: **axum**")
            && t.contains("→ mentions: **MCP**"),
        "{t}"
    );
    assert!(t.contains("unreviewed"), "{t}");

    let (t, err) =
        call(&c, "update", json!({"entry": "MCP", "body": "Offener Standard von Anthropic."}))
            .await;
    assert!(!err, "{t}");
    let (t, _) = call(&c, "get", json!({"entry": "mcp"})).await;
    assert!(t.contains("Model Context Protocol\n\nOffener Standard"), "append is the default: {t}");

    let (t, _) = call(&c, "find_path", json!({"from": "MCP", "to": "axum"})).await;
    assert!(t.contains("Path (2 hops)"), "{t}");
    let (t, _) = call(&c, "explore", json!({"entry": "rmcp", "depth": 1})).await;
    assert!(t.contains("rmcp (#") && t.contains("—depends_on→ axum"), "{t}");
    let (t, _) = call(&c, "project_overview", json!({"project": "brainmatter"})).await;
    assert!(t.contains("3 entries · 2 links") && t.contains("Most connected"), "{t}");
    let (t, _) = call(&c, "list_projects", json!({})).await;
    assert!(t.contains("- Brainmatter (3 entries)"), "{t}");
    let (t, _) = call(&c, "recent_changes", json!({"limit": 5})).await;
    assert!(t.contains("Claude Code via Claude Code") && t.contains("erfasst"), "{t}");

    // Helpful errors instead of failures.
    let (t, err) = call(&c, "get", json!({"entry": "Protokoll"})).await;
    assert!(err && t.contains("No node „Protokoll“"), "{t}");
    let (t, err) =
        call(&c, "update", json!({"entry": "MCP", "body": "x", "body_mode": "overwrite"})).await;
    assert!(err && t.contains("body_mode must be"), "{t}");

    // The user's entries are visible, and the AI cannot revert the user's work.
    let user = s.engine.ensure_actor(ActorKind::User, "Lasse").unwrap();
    s.engine
        .create_node(
            &Ctx::new(user.id.clone()),
            NewNode { title: "Notiz von Lasse".into(), ..Default::default() },
        )
        .unwrap();
    let user_cs = s.engine.change_sets(1, Some(&user.id)).unwrap().remove(0).id;
    let (t, err) = call(&c, "revert", json!({"change_set": user_cs.as_str()})).await;
    assert!(err && t.contains("only revert your own"), "{t}");

    // The capture was edited afterwards: undo is refused until the newer
    // update is reverted, and the model is told so.
    let (t, err) = call(&c, "revert", json!({"change_set": change_set})).await;
    assert!(err && t.contains("revert the newer change first"), "{t}");
    let (t, _) = call(&c, "recent_changes", json!({"limit": 10})).await;
    let update_cs = t
        .lines()
        .find(|l| l.contains("„MCP“ bearbeitet"))
        .and_then(|l| l.split("[change set ").nth(1))
        .map(|s| s.trim_end_matches(']').to_string())
        .expect("update change set listed");
    let (t, err) = call(&c, "revert", json!({"change_set": update_cs})).await;
    assert!(!err, "{t}");
    let (t, err) = call(&c, "revert", json!({"change_set": change_set})).await;
    assert!(!err, "{t}");
    let (t, _) = call(&c, "search", json!({"query": "rmcp"})).await;
    assert!(t.starts_with("No entries"), "capture undone: {t}");

    let (t, err) = call(&c, "delete", json!({"entry": "Notiz von Lasse"})).await;
    assert!(!err && t.contains("trash"), "{t}");

    c.cancel().await.unwrap();
    s.shutdown.cancel();
}

#[tokio::test]
async fn scopes_limit_what_a_client_can_do_and_see() {
    let s = start().await;
    let user = Ctx::new(s.engine.ensure_actor(ActorKind::User, "Lasse").unwrap().id);
    let work = s
        .engine
        .create_project(&user, NewProject { name: "Arbeit".into(), ..Default::default() })
        .unwrap();
    let private = s
        .engine
        .create_project(&user, NewProject { name: "Privat".into(), ..Default::default() })
        .unwrap();
    s.engine
        .create_node(
            &user,
            NewNode {
                title: "Quartalsplan".into(),
                projects: vec![work.id.clone()],
                ..Default::default()
            },
        )
        .unwrap();
    s.engine
        .create_node(
            &user,
            NewNode {
                title: "Tagebuch Quartal".into(),
                projects: vec![private.id.clone()],
                ..Default::default()
            },
        )
        .unwrap();

    let (_, reader) = s.registry.create("Reader", &[Scope::Read], &[]).unwrap();
    let r = connect(&s.url, &reader).await;
    let (t, err) = call(&r, "capture", json!({"entries": [{"title": "X"}]})).await;
    assert!(err && t.contains("may not write"), "{t}");
    let (t, err) = call(&r, "delete", json!({"entry": "Quartalsplan"})).await;
    assert!(err && t.contains("may not delete"), "{t}");

    let (_, worker) = s
        .registry
        .create("Work AI", &[Scope::Read, Scope::Write], std::slice::from_ref(&work.id))
        .unwrap();
    let w = connect(&s.url, &worker).await;
    let (t, _) = call(&w, "search", json!({"query": "quartal"})).await;
    assert!(t.contains("Quartalsplan") && !t.contains("Tagebuch"), "{t}");
    let (t, err) = call(&w, "get", json!({"entry": "Tagebuch Quartal"})).await;
    assert!(err && t.contains("No entry"), "private entries are invisible: {t}");
    let (t, _) = call(&w, "list_projects", json!({})).await;
    assert!(t.contains("Arbeit") && !t.contains("Privat"), "{t}");

    // Entries land in the allowed project automatically.
    let (t, err) = call(&w, "capture", json!({"entries": [{"title": "Meeting-Notiz"}]})).await;
    assert!(!err, "{t}");
    let o = s.engine.project_overview(&work.id).unwrap();
    assert_eq!(o.node_count, 2);
    let (t, err) =
        call(&w, "capture", json!({"project": "Privat", "entries": [{"title": "Y"}]})).await;
    assert!(err && t.contains("No project"), "{t}");

    r.cancel().await.unwrap();
    w.cancel().await.unwrap();
    s.shutdown.cancel();
}

#[tokio::test]
async fn rejects_bad_tokens_and_browser_origins() {
    let s = start().await;
    let (_, token) = s.registry.create("Claude Code", &[Scope::Read], &[]).unwrap();
    let http = reqwest::Client::new();
    let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}});
    let post =
        || http.post(&s.url).header("accept", "application/json, text/event-stream").json(&init);

    assert_eq!(post().send().await.unwrap().status(), 401);
    assert_eq!(post().bearer_auth("bm_wrong").send().await.unwrap().status(), 401);
    let res =
        post().bearer_auth(&token).header("origin", "https://evil.example").send().await.unwrap();
    assert_eq!(res.status(), 403);
    assert!(post().bearer_auth(&token).send().await.unwrap().status().is_success());
    s.shutdown.cancel();
}

#[tokio::test]
async fn similarity_says_when_the_model_is_missing_and_merge_needs_delete_scope() {
    let s = start().await;
    let (_, writer) = s.registry.create("Writer", &[Scope::Read, Scope::Write], &[]).unwrap();
    let w = connect(&s.url, &writer).await;
    call(&w, "capture", json!({"entries": [{"title": "A"}, {"title": "B"}]})).await;

    let (t, err) = call(&w, "similar", json!({"entry": "A"})).await;
    assert!(err && t.contains("embedding model"), "{t}");
    let (t, err) = call(&w, "merge", json!({"keep": "A", "merge": ["B"]})).await;
    assert!(err && t.contains("may not delete"), "{t}");

    let (_, admin) =
        s.registry.create("Admin", &[Scope::Read, Scope::Write, Scope::Delete], &[]).unwrap();
    let a = connect(&s.url, &admin).await;
    let (t, err) = call(&a, "merge", json!({"keep": "A", "merge": ["B"]})).await;
    assert!(!err && t.contains("Merged B into **A**"), "{t}");
    let (t, _) = call(&a, "search", json!({"query": ""})).await;
    assert!(!t.contains("**B**"), "{t}");

    w.cancel().await.unwrap();
    a.cancel().await.unwrap();
    s.shutdown.cancel();
}

/// Words hash into buckets, so shared words mean shared direction.
struct FakeModel;

impl FakeModel {
    fn vector(text: &str) -> Vec<f32> {
        let mut v = vec![0.0f32; 256];
        for word in text.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
            let bucket = word.bytes().fold(7usize, |h, b| h.wrapping_mul(31) + b as usize);
            v[bucket % 256] += 1.0;
        }
        normalize(&mut v);
        v
    }
}

impl Embedder for FakeModel {
    fn model_id(&self) -> &str {
        "fake"
    }

    fn embed_passages(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        Ok(texts.iter().map(|t| Self::vector(t)).collect())
    }

    fn embed_query(&self, text: &str) -> Result<Vec<f32>, String> {
        Ok(Self::vector(text))
    }
}

#[tokio::test]
async fn similarity_hints_respect_project_scopes() {
    const TEXT: &str = "tokio laufzeit rust asynchron scheduler work stealing threads tasks futures reactor timer netzwerk kanal puffer";
    let s = start_with(Some(Arc::new(FakeModel))).await;
    s.engine.load_vectors("fake").unwrap();
    let user = Ctx::new(s.engine.ensure_actor(ActorKind::User, "Lasse").unwrap().id);
    let open = s
        .engine
        .create_project(&user, NewProject { name: "Offen".into(), ..Default::default() })
        .unwrap();
    let secret = s
        .engine
        .create_project(&user, NewProject { name: "Geheim".into(), ..Default::default() })
        .unwrap();
    let add = |title: &str, project: &bm_core::model::Project| {
        s.engine
            .create_node(
                &user,
                NewNode {
                    title: title.into(),
                    body: TEXT.into(),
                    projects: vec![project.id.clone()],
                    ..Default::default()
                },
            )
            .unwrap()
    };
    let visible = add("Sichtbar", &open);
    let hidden = add("Versteckt", &secret);
    let model = FakeModel;
    s.engine.index_now(&model, &[visible.id, hidden.id]).unwrap();

    let (_, token) = s
        .registry
        .create("Scoped", &[Scope::Read, Scope::Write], std::slice::from_ref(&open.id))
        .unwrap();
    let c = connect(&s.url, &token).await;

    let (t, err) = call(
        &c,
        "capture",
        json!({"project": "Offen", "entries": [{"title": "Neu", "body": TEXT}]}),
    )
    .await;
    assert!(!err && t.contains("Sichtbar") && t.contains("Likely duplicates"), "{t}");
    assert!(!t.contains("Versteckt"), "{t}");

    let (t, _) = call(&c, "similar", json!({"entry": "Sichtbar"})).await;
    assert!(t.contains("Neu") && t.contains("likely duplicate"), "{t}");
    assert!(!t.contains("Versteckt"), "{t}");

    let (t, _) = call(&c, "search", json!({"query": TEXT})).await;
    assert!(t.contains("Sichtbar") && !t.contains("Versteckt"), "{t}");

    c.cancel().await.unwrap();
    s.shutdown.cancel();
}
