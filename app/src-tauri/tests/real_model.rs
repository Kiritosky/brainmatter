//! The whole semantic path with the real model on Metal: verifying the
//! download, background indexing, search by meaning, duplicates, locking.
//!
//! Needs the weights, which `cargo run --release -p bm-embed --example calibrate`
//! puts into target/models:
//! `cargo test --release -p brainmatter --test real_model -- --ignored --nocapture`

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use bm_core::{
    model::{NewNode, SearchQuery},
    vault::KdfParams,
};
use brainmatter::session::{Emit, Session};

const TIMEOUT: Duration = Duration::from_secs(120);

fn wait_for(what: &str, done: impl Fn() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < TIMEOUT, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("{what}: {:?}", start.elapsed());
}

fn query(text: &str) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        project: None,
        include_subprojects: true,
        kinds: vec![],
        tags: vec![],
        limit: 10,
    }
}

#[test]
#[ignore = "needs the downloaded model"]
fn real_model_from_download_to_lock() {
    let weights =
        std::fs::canonicalize(format!("{}/../../target/models", env!("CARGO_MANIFEST_DIR")))
            .expect("model files in target/models");
    let dir = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(&weights, dir.path().join("models")).unwrap();

    let emit: Emit = Arc::new(|_, _| {});
    let session = Arc::new(Session::new(dir.path().to_owned(), emit, KdfParams::insecure_fast()));
    session.create("correct horse battery", "Lasse").unwrap();

    // Cached files: this only verifies the hashes, then starts the indexer.
    session.download_model();
    wait_for("verified", || session.model_status().state == "ready");

    let (engine, ctx) = session.engine().unwrap();
    let entries = [
        ("Tokio", "Asynchrone Laufzeit für Rust mit Work-Stealing-Scheduler."),
        (
            "Tokio Runtime",
            "Tokio ist die asynchrone Rust-Laufzeitumgebung und nutzt einen Work-Stealing-Scheduler.",
        ),
        ("Sauerteig", "Roggenmehl und Wasser mischen, fünf Tage täglich füttern."),
        ("Marathon-Plan", "16 Wochen, lange Läufe am Sonntag."),
        ("Steuererklärung", "Belege für Fahrtkosten und Arbeitszimmer sammeln."),
    ];
    let ids: Vec<_> = entries
        .iter()
        .map(|(title, body)| {
            let input =
                NewNode { title: (*title).into(), body: (*body).into(), ..Default::default() };
            engine.create_node(&ctx, input).unwrap().id
        })
        .collect();
    wait_for("indexed", || session.model_status().indexed == entries.len());

    // Typing never waits: without the model in memory, there is no vector yet.
    wait_for("model in memory", || session.query_vector("Brot selber backen").is_some());
    let vector = session.query_vector("Brot selber backen").unwrap();
    let start = Instant::now();
    let hits = engine.search_hybrid(&query("Brot selber backen"), Some(&vector)).unwrap();
    println!("hybrid search: {:?}", start.elapsed());
    for hit in &hits {
        println!("  {} (semantic: {})", hit.node.title, hit.semantic);
    }
    assert_eq!(hits.first().map(|h| h.node.title.as_str()), Some("Sauerteig"));
    assert!(hits[0].semantic, "shares no word with the query");
    assert!(
        !hits.iter().any(|h| h.node.title == "Marathon-Plan" || h.node.title == "Tokio"),
        "unrelated entries are noise here"
    );

    let suggestions = engine.link_suggestions(&ids[0]).unwrap();
    for s in &suggestions {
        println!("  suggested: {} {:.3} dup={}", s.node.title, s.score, s.likely_duplicate);
    }
    assert!(suggestions.iter().any(|s| s.node.title == "Tokio Runtime"));

    // Locking must not hang on the indexer, and the model leaves memory.
    let start = Instant::now();
    session.lock();
    println!("lock: {:?}", start.elapsed());
    assert!(start.elapsed() < Duration::from_millis(400), "lock took {:?}", start.elapsed());

    session.unlock_password("correct horse battery").unwrap();
    assert_eq!(session.model_status().indexed, entries.len(), "vectors survive the lock");

    session.remove_model().unwrap();
    assert_eq!(session.model_status().state, "missing");
    assert!(weights.is_dir(), "removal only drops the link in this test");
    assert!(session.query_vector("Brot selber backen").is_none());
    let (engine, _) = session.engine().unwrap();
    assert!(engine.link_suggestions(&ids[0]).unwrap().is_empty());
    assert!(!engine.search(&query("Tokio")).unwrap().is_empty(), "word search still works");
}
