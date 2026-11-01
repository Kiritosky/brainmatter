//! What the MCP interface costs an AI in tokens. Context is what clients pay for,
//! so this is measured rather than assumed: run it with
//! `cargo test -p bm-mcp --test token_cost -- --nocapture`.
//!
//! Tokens are estimated as characters / 4, which is close for mixed prose and code.

use std::sync::Arc;

use bm_core::{
    Engine, Vault,
    model::{ActorKind, Ctx, NewEdge, NewNode, NewProject},
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

const ENTRIES: usize = 300;
const BODY: &str = "Dieser Eintrag beschreibt eine Entscheidung im Projekt. Wir haben die Variante \
    mit dem lokalen Index gewählt, weil sie ohne Server auskommt und auch offline funktioniert. \
    Nachteile: Der Index muss beim Start geladen werden, und große Vaults brauchen mehr Speicher. \
    Offen ist noch, wie wir mit Konflikten umgehen, wenn zwei Geräte denselben Eintrag ändern.";

/// Raising one of these should be a decision, not something that happens unnoticed.
const BUDGETS: &[(&str, usize)] = &[
    ("all tool definitions", 3000),
    ("server instructions", 350),
    ("search (default)", 450),
    ("project_overview", 450),
    ("recent_changes (default)", 400),
    ("capture, response for one entry", 40),
];

async fn call(
    c: &RunningService<RoleClient, ClientConfig>,
    tool: &'static str,
    args: Value,
) -> String {
    let args = args.as_object().cloned().unwrap_or_default();
    let res = c.call_tool(CallToolRequestParams::new(tool).with_arguments(args)).await.unwrap();
    res.content
        .iter()
        .filter_map(|b| serde_json::to_value(b).ok())
        .filter_map(|v| v.get("text").and_then(Value::as_str).map(str::to_string))
        .collect::<Vec<_>>()
        .join("\n")
}

fn tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

#[tokio::test]
async fn the_interface_stays_within_its_budget() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let dir = tempfile::tempdir().unwrap();
    let (vault, keys, _) = Vault::create(dir.path(), "pw", KdfParams::insecure_fast()).unwrap();
    let engine = Arc::new(Engine::open(&vault.db_path(), &keys.db_key(), 2).unwrap());
    let user = Ctx::new(engine.ensure_actor(ActorKind::User, "Nutzer").unwrap().id);
    let project = engine
        .create_project(&user, NewProject { name: "Projekt".into(), ..Default::default() })
        .unwrap();
    let mut previous = None;
    for i in 0..ENTRIES {
        let input = NewNode {
            title: format!("Entscheidung {i}"),
            body: format!("{BODY} Index {i}."),
            kind: Some("decision".into()),
            tags: vec!["architektur".into()],
            projects: vec![project.id.clone()],
            ..Default::default()
        };
        let node = engine.create_node(&user, input).unwrap();
        if let Some(prev) = previous.replace(node.id.clone()) {
            let edge =
                NewEdge { src: prev, dst: node.id, relation: None, weight: None, note: None };
            engine.link(&user, edge).unwrap();
        }
    }

    let registry = Arc::new(ClientRegistry::in_memory());
    let (_, token) =
        registry.create("KI", &[Scope::Read, Scope::Write, Scope::Delete], &[]).unwrap();
    let shutdown = CancellationToken::new();
    let app =
        bm_mcp::router(registry, Arc::new(StaticHub(engine.clone(), None)), shutdown.child_token());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let stop = shutdown.clone();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move { stop.cancelled_owned().await })
            .await
            .unwrap();
    });
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(url).auth_header(token),
    );
    let c = ClientConfig::default().serve(transport).await.unwrap();

    let mut report: Vec<(&str, usize)> = Vec::new();
    let tools = c.list_all_tools().await.unwrap();
    report.push(("all tool definitions", tokens(&serde_json::to_string(&tools).unwrap())));
    let info = c.peer_info().unwrap().instructions.clone().unwrap_or_default();
    report.push(("server instructions", tokens(&info)));
    let mut each: Vec<(String, usize)> = tools
        .iter()
        .map(|t| (t.name.to_string(), tokens(&serde_json::to_string(t).unwrap())))
        .collect();
    each.sort_by_key(|(_, size)| std::cmp::Reverse(*size));

    let search = call(&c, "search", json!({"query": "Konflikte Index"})).await;
    report.push(("search (default)", tokens(&search)));
    let get = call(&c, "get", json!({"entry": "Entscheidung 150"})).await;
    report.push(("get one entry", tokens(&get)));
    let explore = call(&c, "explore", json!({"entry": "Entscheidung 150"})).await;
    report.push(("explore (default)", tokens(&explore)));
    let overview = call(&c, "project_overview", json!({"project": "Projekt"})).await;
    report.push(("project_overview", tokens(&overview)));
    let recent = call(&c, "recent_changes", json!({})).await;
    report.push(("recent_changes (default)", tokens(&recent)));

    let handbook = format!("# Installation\n\n{BODY}\n\n# Nutzung\n\n{BODY}\n\n# Fehler\n\n{BODY}");
    for title in ["Handbuch", "Architektur", "Betrieb"] {
        let page =
            json!({"project": "Projekt", "title": title, "body": handbook, "tags": ["handbuch"]});
        call(&c, "write_doc", page).await;
    }
    let docs = call(&c, "list_docs", json!({"project": "Projekt"})).await;
    report.push(("list_docs, three pages", tokens(&docs)));
    let section = call(&c, "read_doc", json!({"page": "Handbuch", "section": "nutzung"})).await;
    report.push(("read_doc, one section", tokens(&section)));
    let whole = call(&c, "read_doc", json!({"page": "Handbuch"})).await;
    report.push(("read_doc, whole page", tokens(&whole)));

    let one =
        json!({"entries": [{"title": "Neu", "body": BODY, "kind": "note"}], "project": "Projekt"});
    report.push(("capture, request for one entry", tokens(&one.to_string())));
    let captured = call(&c, "capture", one).await;
    report.push(("capture, response for one entry", tokens(&captured)));
    let ten: Vec<Value> =
        (0..10).map(|i| json!({"title": format!("Stapel {i}"), "body": BODY})).collect();
    let many = call(&c, "capture", json!({"entries": ten, "project": "Projekt"})).await;
    report.push(("capture, response for ten entries", tokens(&many)));
    let linked = call(&c, "link", json!({"from": "Neu", "to": "Entscheidung 1"})).await;
    report.push(("link, response", tokens(&linked)));
    let updated = call(&c, "update", json!({"entry": "Neu", "append": "Noch ein Satz."})).await;
    report.push(("update, response", tokens(&updated)));

    println!("\nestimated tokens with {ENTRIES} entries of about {} characters:", BODY.len());
    for (name, size) in &report {
        println!("{size:>7}  {name}");
    }
    println!("\nlargest tool definitions:");
    for (name, size) in each.iter().take(8) {
        println!("{size:>7}  {name}");
    }
    let sample = tools.iter().find(|t| t.name == "search").unwrap();
    println!("\n--- sample: definition\n{}", serde_json::to_string(sample).unwrap());
    println!("\n--- sample: search\n{}", search.chars().take(700).collect::<String>());
    println!("\n--- sample: capture response\n{captured}");

    // The schema no longer spells out `null`, but clients may still send it.
    let explicit = call(&c, "search", json!({"query": "Konflikte", "limit": null})).await;
    assert!(explicit.starts_with("10 results"), "{explicit}");

    for (name, budget) in BUDGETS {
        let (_, size) = report.iter().find(|(n, _)| n == name).unwrap();
        assert!(size <= budget, "{name} costs {size} tokens, budget is {budget}");
    }

    c.cancel().await.unwrap();
    shutdown.cancel();
}
