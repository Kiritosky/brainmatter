//! Spike 5: local embedding models — latency, throughput and DE/EN retrieval
//! quality on a tiny knowledge-base-like corpus.
//!
//! `cargo run --release -p embed-bench -- e5small|gemma`

use std::{path::PathBuf, time::Instant};

use anyhow::Context;
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};

mod common;
use common::{NOTES, QUERIES, cosine, rss_mb};

struct Profile {
    model: EmbeddingModel,
    query_prefix: &'static str,
    passage_prefix: &'static str,
}

fn profile(name: &str) -> anyhow::Result<Profile> {
    Ok(match name {
        "e5small" => Profile {
            model: EmbeddingModel::MultilingualE5Small,
            query_prefix: "query: ",
            passage_prefix: "passage: ",
        },
        "e5base" => Profile {
            model: EmbeddingModel::MultilingualE5Base,
            query_prefix: "query: ",
            passage_prefix: "passage: ",
        },
        "gemma" => Profile {
            model: EmbeddingModel::EmbeddingGemma300MQ,
            query_prefix: "task: search result | query: ",
            passage_prefix: "title: none | text: ",
        },
        "paraml" => Profile {
            model: EmbeddingModel::ParaphraseMLMiniLML12V2Q,
            query_prefix: "",
            passage_prefix: "",
        },
        other => anyhow::bail!("unknown model {other}"),
    })
}

fn main() -> anyhow::Result<()> {
    let name = std::env::args().nth(1).unwrap_or_else(|| "e5small".into());
    let p = profile(&name)?;
    let cache = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/models");

    let t = Instant::now();
    let mut opts = TextInitOptions::new(p.model.clone())
        .with_cache_dir(cache)
        .with_show_download_progress(false);
    if let Ok(n) = std::env::var("THREADS") {
        opts = opts.with_intra_threads(n.parse()?);
    }
    let batch_size = std::env::var("BATCH").ok().map(|b| b.parse()).transpose()?;
    let mut model = TextEmbedding::try_new(opts).context("load model")?;
    println!("[{name}] load: {:?}, rss {} MB", t.elapsed(), rss_mb());

    let passages: Vec<String> = NOTES.iter().map(|n| format!("{}{n}", p.passage_prefix)).collect();
    let note_vecs = model.embed(&passages, None)?;
    println!("[{name}] dim: {}, rss {} MB", note_vecs[0].len(), rss_mb());

    // Throughput: 256 note-sized passages.
    let batch: Vec<String> = (0..256).map(|i| passages[i % passages.len()].clone()).collect();
    let t = Instant::now();
    model.embed(&batch, batch_size)?;
    println!(
        "[{name}] batch: {:.2} ms/note, rss {} MB",
        t.elapsed().as_secs_f64() * 1000.0 / 256.0,
        rss_mb()
    );

    // Latency: single short query, as when the AI calls `search`.
    let mut lat = Vec::new();
    for _ in 0..30 {
        let t = Instant::now();
        model.embed([format!("{}tokio Nebenläufigkeit", p.query_prefix)], None)?;
        lat.push(t.elapsed());
    }
    lat.sort();
    println!("[{name}] query latency p50: {:?} p95: {:?}", lat[15], lat[28]);

    let mut hits = 0;
    for (q, expected) in QUERIES {
        let qv = &model.embed([format!("{}{q}", p.query_prefix)], None)?[0];
        let mut ranked: Vec<(usize, f32)> =
            note_vecs.iter().enumerate().map(|(i, v)| (i, cosine(qv, v))).collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        let ok = ranked[0].0 == *expected;
        hits += ok as usize;
        println!(
            "  {} {q:40} → #{} ({:.3}, margin {:.3})",
            if ok { "✓" } else { "✗" },
            ranked[0].0,
            ranked[0].1,
            ranked[0].1 - ranked[1].1
        );
    }
    println!("[{name}] top-1: {hits}/{}", QUERIES.len());
    Ok(())
}
