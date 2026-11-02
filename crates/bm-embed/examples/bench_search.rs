//! Measures search by meaning on a real vault, without a window. Finishes the index
//! first if it is not complete, and says how fast that went.
//!
//! `cargo run --release -p bm-embed --example bench_search -- <vault dir> <password>`
//!
//! The vault is one made by `bm-core`'s `seed_vault`, with the model in `<dir>/models`.

use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use bm_core::{Engine, Vault, engine::spawn_indexer, model::SearchQuery, semantic::Embedder};
use bm_embed::{E5Embedder, ModelStore};

const FIRST: &[&str] = &[
    "daten",
    "server",
    "projekt",
    "memory",
    "layout",
    "notiz",
    "graphen",
    "verbindung",
    "index",
    "testplan",
];
const SECOND: &[&str] =
    &["code", "idee", "bank", "such", "plan", "mory", "tiz", "phen", "out", "jekt"];

/// Long enough for the rest of an index of 50,000, short enough to notice a stuck one.
const MAX_INDEXING: Duration = Duration::from_secs(45 * 60);

fn quantile(sorted: &[f64], q: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

fn report(what: &str, times: &mut [f64]) {
    times.sort_by(f64::total_cmp);
    println!(
        "{what:14} p50 {:7.1} ms   p95 {:7.1} ms   max {:7.1} ms",
        quantile(times, 0.5),
        quantile(times, 0.95),
        times[times.len() - 1]
    );
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(dir), Some(password)) = (args.next(), args.next()) else {
        eprintln!("usage: bench_search <vault dir> <password>");
        std::process::exit(2);
    };
    let dir = Path::new(&dir);
    let vault = Vault::load(dir).expect("vault");
    let keys = vault.unlock_with_password(&password).expect("password");
    let engine = Arc::new(Engine::open(&vault.db_path(), &keys.db_key(), 4).expect("engine"));

    let embedder = Arc::new(E5Embedder::new(ModelStore::new(dir.join("models"))));
    let indexer = spawn_indexer(&engine, embedder.clone()).expect("indexer");

    let started = Instant::now();
    let at_start = engine.index_status().expect("status").indexed;
    let mut status = engine.index_status().expect("status");
    println!("index: {} of {}", status.indexed, status.total);
    while status.indexed < status.total && started.elapsed() < MAX_INDEXING {
        std::thread::sleep(Duration::from_secs(5));
        status = engine.index_status().expect("status");
    }
    let done = status.indexed - at_start;
    if done > 0 {
        let seconds = started.elapsed().as_secs_f64();
        println!("indexed {done} more in {seconds:.0} s, {:.1} per second", done as f64 / seconds);
    }
    println!("index: {} of {}", status.indexed, status.total);
    drop(indexer);

    let loading = Instant::now();
    let _ = embedder.embed_query("datenbank verbindung").expect("model");
    println!("model ready after {:.1} s", loading.elapsed().as_secs_f64());

    let (mut embedding, mut words, mut searching, mut together) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut hits = 0;
    for a in FIRST {
        for b in SECOND {
            let text = format!("{a} {b}");
            let query = SearchQuery {
                limit: 20,
                include_subprojects: true,
                text: text.clone(),
                ..Default::default()
            };
            let words_start = Instant::now();
            engine.search(&query).expect("words");
            words.push(words_start.elapsed().as_secs_f64() * 1e3);

            let start = Instant::now();
            let vector = embedder.embed_query(&text).expect("query");
            let embedded = start.elapsed();
            engine.search_hybrid(&query, Some(&vector)).expect("search");
            let all = start.elapsed();
            embedding.push(embedded.as_secs_f64() * 1e3);
            searching.push((all - embedded).as_secs_f64() * 1e3);

            // As the app does it: the words are looked up while the query vector is made.
            let start = Instant::now();
            let found = engine
                .search_hybrid_with(&query, || embedder.embed_query(&text).ok())
                .expect("search");
            together.push(start.elapsed().as_secs_f64() * 1e3);
            hits += found.len();
        }
    }
    println!("{} searches, {} hits on average", together.len(), hits / together.len());
    report("query vector", &mut embedding);
    report("words alone", &mut words);
    report("search", &mut searching);
    report("as in the app", &mut together);
}
