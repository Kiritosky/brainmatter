//! Spike 5b: multilingual-e5-small on candle (pure Rust, Metal GPU).
//!
//! `cargo run --release -p embed-bench --bin candle -- [metal|cpu] [f32|f16]`

#[path = "../common.rs"]
mod common;

use std::{path::PathBuf, time::Instant};

use anyhow::{Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use common::{NOTES, QUERIES, cosine, rss_mb};
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};

const REPO: &str = "intfloat/multilingual-e5-small";

struct Embedder {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
}

impl Embedder {
    fn load(device: Device, dtype: DType) -> Result<Self> {
        let cache = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/models");
        let api = hf_hub::api::sync::ApiBuilder::new()
            .with_cache_dir(cache)
            .with_progress(false)
            .build()?;
        let repo = api.model(REPO.to_string());
        let config: Config =
            serde_json::from_str(&std::fs::read_to_string(repo.get("config.json")?)?)?;
        let weights = repo.get("model.safetensors")?;

        let mut tokenizer =
            Tokenizer::from_file(repo.get("tokenizer.json")?).map_err(anyhow::Error::msg)?;
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,
            ..Default::default()
        }));
        tokenizer
            .with_truncation(Some(TruncationParams { max_length: 512, ..Default::default() }))
            .map_err(anyhow::Error::msg)?;

        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[weights], dtype, &device)? };
        let model = BertModel::load(vb, &config).context("load bert")?;
        Ok(Self { model, tokenizer, device })
    }

    /// Mean-pooled, L2-normalised embeddings.
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let enc = self.tokenizer.encode_batch(texts.to_vec(), true).map_err(anyhow::Error::msg)?;
        let ids: Vec<Tensor> = enc
            .iter()
            .map(|e| Tensor::new(e.get_ids(), &self.device))
            .collect::<candle_core::Result<_>>()?;
        let mask: Vec<Tensor> = enc
            .iter()
            .map(|e| Tensor::new(e.get_attention_mask(), &self.device))
            .collect::<candle_core::Result<_>>()?;
        let ids = Tensor::stack(&ids, 0)?;
        let mask = Tensor::stack(&mask, 0)?;
        let type_ids = ids.zeros_like()?;

        let hidden = self.model.forward(&ids, &type_ids, Some(&mask))?;
        let mask_f = mask.to_dtype(hidden.dtype())?.unsqueeze(2)?;
        let summed = hidden.broadcast_mul(&mask_f)?.sum(1)?;
        let counts = mask_f.sum(1)?;
        let mean = summed.broadcast_div(&counts)?;
        let norm = mean.sqr()?.sum_keepdim(1)?.sqrt()?;
        let out = mean.broadcast_div(&norm)?.to_dtype(DType::F32)?;
        Ok(out.to_vec2()?)
    }
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let device = match args.next().as_deref() {
        Some("cpu") => Device::Cpu,
        _ => Device::new_metal(0)?,
    };
    let dtype = match args.next().as_deref() {
        Some("f16") => DType::F16,
        _ => DType::F32,
    };
    let tag = format!("candle {} {:?}", if device.is_metal() { "metal" } else { "cpu" }, dtype);

    let rss0 = rss_mb();
    let t = Instant::now();
    let e = Embedder::load(device, dtype)?;
    println!("[{tag}] load: {:?}, rss {} MB (base {rss0} MB)", t.elapsed(), rss_mb());

    let passages: Vec<String> = NOTES.iter().map(|n| format!("passage: {n}")).collect();
    let note_vecs = e.embed(&passages)?;
    println!("[{tag}] dim: {}, rss {} MB", note_vecs[0].len(), rss_mb());

    let batch: Vec<String> = (0..256).map(|i| passages[i % passages.len()].clone()).collect();
    let t = Instant::now();
    for chunk in batch.chunks(32) {
        e.embed(chunk)?;
    }
    println!(
        "[{tag}] batch: {:.2} ms/note, rss {} MB",
        t.elapsed().as_secs_f64() * 1000.0 / 256.0,
        rss_mb()
    );

    let q = vec!["query: tokio Nebenläufigkeit".to_string()];
    let mut lat = Vec::new();
    for _ in 0..30 {
        let t = Instant::now();
        e.embed(&q)?;
        lat.push(t.elapsed());
    }
    lat.sort();
    println!("[{tag}] query latency p50: {:?} p95: {:?}", lat[15], lat[28]);

    let mut hits = 0;
    for (q, expected) in QUERIES {
        let qv = &e.embed(&[format!("query: {q}")])?[0];
        let best = note_vecs
            .iter()
            .enumerate()
            .map(|(i, v)| (i, cosine(qv, v)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        hits += (best.0 == *expected) as usize;
    }
    println!("[{tag}] top-1: {hits}/{}", QUERIES.len());
    Ok(())
}
