//! Engine load test against the PLAN.md §9 targets.
//!
//! `cargo run --release -p bm-core --example bench_engine [-- 50000]`

use std::time::{Duration, Instant};

use bm_core::{Engine, Vault, model::*, vault::KdfParams};

const WORDS: &[&str] = &[
    "rust",
    "tokio",
    "async",
    "graph",
    "projekt",
    "notiz",
    "idee",
    "datenbank",
    "suche",
    "vektor",
    "nebenläufigkeit",
    "speicher",
    "sicherheit",
    "schlüssel",
    "modell",
    "agent",
    "werkzeug",
    "oberfläche",
    "layout",
    "kante",
    "knoten",
    "thema",
    "cluster",
    "zeit",
    "plan",
    "release",
    "bug",
    "test",
    "design",
    "api",
    "protokoll",
    "server",
    "client",
    "token",
    "cache",
    "index",
];

fn rng(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

/// Natural-language-like corpus: 5000 words, Zipf-distributed (s = 1.07).
struct Corpus {
    words: Vec<String>,
    cdf: Vec<f64>,
}

impl Corpus {
    fn new() -> Self {
        const SYL: &[&str] = &[
            "ka", "ro", "mi", "te", "lu", "sa", "no", "vi", "da", "pe", "zu", "fo", "ri", "ba",
            "gle", "tor",
        ];
        let mut words: Vec<String> = WORDS.iter().map(|w| w.to_string()).collect();
        let mut i = 0u64;
        while words.len() < 5000 {
            let (a, b, c) = (i % 16, (i / 16) % 16, (i / 256) % 16);
            words.push(format!("{}{}{}", SYL[a as usize], SYL[b as usize], SYL[c as usize]));
            i += 1;
        }
        // Domain words get mid-frequency ranks, like real topic terms.
        words.rotate_left(WORDS.len());
        words.splice(40..40, WORDS.iter().map(|w| w.to_string()));
        words.truncate(5000);
        let mut acc = 0.0;
        let cdf = (1..=words.len())
            .map(|r| {
                acc += 1.0 / (r as f64).powf(1.07);
                acc
            })
            .collect();
        Self { words, cdf }
    }

    fn word(&self, seed: &mut u64) -> &str {
        let x = (rng(seed) as f64 / u64::MAX as f64) * self.cdf.last().unwrap();
        &self.words[self.cdf.partition_point(|&c| c < x).min(self.words.len() - 1)]
    }

    fn sentence(&self, seed: &mut u64, n: usize) -> String {
        (0..n).map(|_| self.word(seed)).collect::<Vec<_>>().join(" ")
    }
}

fn pct(mut v: Vec<Duration>) -> String {
    v.sort();
    let at = |p: f64| v[((v.len() as f64 * p) as usize).min(v.len() - 1)];
    format!("p50 {:?} p95 {:?} max {:?}", at(0.5), at(0.95), v[v.len() - 1])
}

fn main() {
    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(50_000);
    let dir = tempfile::tempdir().unwrap();
    let (vault, keys, _) = Vault::create(dir.path(), "pw", KdfParams::insecure_fast()).unwrap();
    let engine = Engine::open(&vault.db_path(), &keys.db_key(), 4).unwrap();
    let ai = Ctx::new(engine.ensure_actor(ActorKind::Ai, "Bench").unwrap().id);
    let project = engine
        .create_project(&ai, NewProject { name: "Bench".into(), ..Default::default() })
        .unwrap();

    let corpus = Corpus::new();
    let mut seed = 0x9e3779b97f4a7c15u64;
    let t = Instant::now();
    let mut created = 0;
    let mut batch_times = Vec::new();
    while created < n {
        let size = 200.min(n - created);
        let nodes = (0..size)
            .map(|i| CaptureNode {
                key: Some(format!("k{i}")),
                node: NewNode {
                    title: format!("{} {}", corpus.sentence(&mut seed, 3), created + i),
                    body: corpus.sentence(&mut seed, 60),
                    tags: vec![WORDS[(rng(&mut seed) % 8) as usize].into()],
                    ..Default::default()
                },
            })
            .collect();
        let links = (0..size * 2)
            .map(|_| CaptureLink {
                from: format!("k{}", rng(&mut seed) % size as u64),
                to: format!("k{}", rng(&mut seed) % size as u64),
                relation: None,
                note: None,
            })
            .filter(|l| l.from != l.to)
            .collect();
        let bt = Instant::now();
        engine.capture(&ai, Capture { nodes, links, project: Some(project.id.clone()) }).unwrap();
        batch_times.push(bt.elapsed());
        created += size;
    }
    let total = t.elapsed();
    println!(
        "insert: {n} nodes (+~2 links each) in {total:.2?} → {:.0} nodes/s; capture(200) {}",
        n as f64 / total.as_secs_f64(),
        pct(batch_times)
    );
    let db_mb = std::fs::metadata(vault.db_path()).map(|m| m.len()).unwrap_or(0) / 1_000_000;
    println!("db size: {db_mb} MB");

    // Where does FTS time go? Probe raw SQL on a separate connection.
    {
        let db = bm_core::Db::open(&vault.db_path(), &keys.db_key()).unwrap();
        let c = db.conn();
        let worst = format!("\"{}\"*", corpus.words[0]);
        let probe = |label: &str, sql: &str| {
            let mut v = Vec::new();
            let mut rows = 0i64;
            for _ in 0..20 {
                let t = Instant::now();
                let mut stmt = c.prepare_cached(sql).unwrap();
                rows = stmt.query_map([worst.as_str()], |_| Ok(())).unwrap().count() as i64;
                v.push(t.elapsed());
            }
            println!("probe {label:28} rows {rows:6}  {}", pct(v));
        };
        println!("probe term: most frequent word {worst}");
        probe("match only", "SELECT rowid FROM nodes_fts WHERE nodes_fts MATCH ?1");
        probe(
            "order by rank limit 20",
            "SELECT rowid FROM nodes_fts WHERE nodes_fts MATCH ?1 ORDER BY rank LIMIT 20",
        );
        probe(
            "+ join nodes",
            "SELECT f.rowid FROM nodes_fts f JOIN nodes n ON n.seq = f.rowid WHERE nodes_fts MATCH ?1 AND n.deleted_at IS NULL ORDER BY f.rank LIMIT 20",
        );
        probe(
            "exact term (no prefix)",
            "SELECT rowid FROM nodes_fts WHERE nodes_fts MATCH replace(?1, '*', '') ORDER BY rank LIMIT 20",
        );
    }

    // Typical queries: topic words plus mid-frequency vocabulary.
    let queries = [
        "tokio",
        "graph suche",
        "nebenläufigkeit speicher",
        "sicherheit schlüssel modell",
        "api",
        &corpus.words[300],
        &corpus.words[1200],
    ];
    let mut times = Vec::new();
    for i in 0..200 {
        let q = SearchQuery::new(queries[i % queries.len()].to_string());
        let t = Instant::now();
        let hits = engine.search(&q).unwrap();
        times.push(t.elapsed());
        assert!(!hits.is_empty());
    }
    println!("search (20 hits):            {}", pct(times));

    let mut times = Vec::new();
    for i in 0..200 {
        let q = SearchQuery {
            project: Some(project.id.clone()),
            tags: vec!["rust".into()],
            ..SearchQuery::new(queries[i % queries.len()].to_string())
        };
        let t = Instant::now();
        engine.search(&q).unwrap();
        times.push(t.elapsed());
    }
    println!("search + project + tag:      {}", pct(times));

    let mut times = Vec::new();
    for _ in 0..50 {
        let t = Instant::now();
        engine
            .search(&SearchQuery::new(format!("{} {}", corpus.words[0], corpus.words[1])))
            .unwrap();
        times.push(t.elapsed());
    }
    println!("worst case (2 top words):    {}", pct(times));

    let t = Instant::now();
    let snap = engine.graph_snapshot(None).unwrap();
    println!(
        "graph_snapshot ({} nodes, {} edges, with PageRank + clusters): {:?}",
        snap.nodes.len(),
        snap.edges.len(),
        t.elapsed()
    );
    let clusters = snap.nodes.iter().map(|n| n.cluster).max().unwrap_or(0) + 1;
    println!("clusters: {clusters}");

    let sample = engine.search(&SearchQuery { limit: 200, ..SearchQuery::new("") }).unwrap();
    let mut times = Vec::new();
    for hit in &sample {
        let t = Instant::now();
        engine.node_detail(&hit.node.id).unwrap();
        times.push(t.elapsed());
    }
    println!("node_detail:                 {}", pct(times));

    let mut times = Vec::new();
    for hit in &sample {
        let t = Instant::now();
        engine.resolve_node(&hit.node.title).unwrap();
        times.push(t.elapsed());
    }
    println!("resolve by title:            {}", pct(times));

    let mut times = Vec::new();
    for hit in sample.iter().take(100) {
        let t = Instant::now();
        engine
            .update_node(
                &ai,
                &hit.node.id,
                NodePatch { body: Some(BodyEdit::Append("Nachtrag".into())), ..Default::default() },
            )
            .unwrap();
        times.push(t.elapsed());
    }
    println!("update_node:                 {}", pct(times));
}
