//! The local embedding model: multilingual-e5-small on candle (Metal).
//!
//! Chosen in spike 5 (PLAN.md §13): 4.7 ms per query at ~0.9 GB while loaded,
//! with f32 weights (f16 ruined the quality). Weights are not secret, so they
//! live unencrypted next to the vault. They are loaded on first use and unloaded
//! when idle, so the app stays small in the background.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicI64, Ordering},
};

use bm_core::{
    semantic::{Embedder, normalize},
    time::now_ms,
};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use hf_hub::{Repo, RepoType, api::Progress, api::sync::ApiBuilder};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};

const REPO: &str = "intfloat/multilingual-e5-small";
/// Pinned revision and hashes: a changed or tampered download is rejected.
const REVISION: &str = "614241f622f53c4eeff9890bdc4f31cfecc418b3";
const FILES: [(&str, &str); 3] = [
    ("config.json", "69137736cab8b8903a07fe8afaafdda25aac55415a12a55d1bffa9f581abf959"),
    ("tokenizer.json", "0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39"),
    ("model.safetensors", "1a55775f53449dac10a2bcbc312469fac40b96d53198c407081a831f81c98477"),
];
/// Total download size, for the UI.
pub const DOWNLOAD_BYTES: u64 = 470_641_600 + 17_082_730 + 655;
pub const MODEL_ID: &str = "multilingual-e5-small@614241f";
const BATCH: usize = 16;

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("Download fehlgeschlagen: {0}")]
    Download(String),
    #[error("{0} stimmt nicht mit der erwarteten Prüfsumme überein")]
    Corrupt(String),
    #[error("Modell konnte nicht geladen werden: {0}")]
    Load(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Where the model files live. Downloads are verified once, then marked.
#[derive(Debug, Clone)]
pub struct ModelStore {
    dir: PathBuf,
}

impl ModelStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn marker(&self) -> PathBuf {
        self.dir.join(format!("{}.verified", REVISION))
    }

    fn file(&self, name: &str) -> Option<PathBuf> {
        hf_hub::Cache::new(self.dir.clone())
            .repo(Repo::with_revision(REPO.into(), RepoType::Model, REVISION.into()))
            .get(name)
    }

    pub fn is_ready(&self) -> bool {
        self.marker().is_file() && FILES.iter().all(|(name, _)| self.file(name).is_some())
    }

    /// Downloads and verifies the model. `progress` receives bytes so far.
    /// Blocking; call from a worker thread.
    pub fn download(&self, progress: impl Fn(u64) + Send + Sync) -> Result<(), ModelError> {
        fs::create_dir_all(&self.dir)?;
        let api = ApiBuilder::new()
            .with_cache_dir(self.dir.clone())
            .with_progress(false)
            .build()
            .map_err(|e| ModelError::Download(e.to_string()))?;
        let repo = api.repo(Repo::with_revision(REPO.into(), RepoType::Model, REVISION.into()));

        let mut finished = 0u64;
        for (name, expected) in FILES {
            let path = match self.file(name) {
                Some(path) => path,
                None => repo
                    .download_with_progress(
                        name,
                        Report { base: finished, done: 0, progress: &progress },
                    )
                    .map_err(|e| ModelError::Download(e.to_string()))?,
            };
            if sha256(&path)? != expected {
                let _ = fs::remove_file(&path);
                return Err(ModelError::Corrupt(name.to_string()));
            }
            finished += fs::metadata(&path)?.len();
            progress(finished);
        }
        fs::write(self.marker(), REVISION)?;
        Ok(())
    }
}

struct Report<'a, F> {
    base: u64,
    done: u64,
    progress: &'a F,
}

impl<F: Fn(u64)> Progress for Report<'_, F> {
    fn init(&mut self, _size: usize, _filename: &str) {}

    fn update(&mut self, size: usize) {
        self.done += size as u64;
        (self.progress)(self.base + self.done);
    }

    fn finish(&mut self) {}
}

fn sha256(path: &Path) -> Result<String, ModelError> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    std::io::copy(&mut file, &mut hash)?;
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

struct Loaded {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
}

impl Loaded {
    fn load(store: &ModelStore) -> Result<Self, ModelError> {
        let path =
            |name: &str| store.file(name).ok_or_else(|| ModelError::Load(format!("{name} fehlt")));
        let load = |e: String| ModelError::Load(e);
        let device = Device::new_metal(0).unwrap_or(Device::Cpu);
        let config: Config = serde_json::from_slice(&fs::read(path("config.json")?)?)
            .map_err(|e| load(e.to_string()))?;
        let mut tokenizer =
            Tokenizer::from_file(path("tokenizer.json")?).map_err(|e| load(e.to_string()))?;
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,
            ..Default::default()
        }));
        tokenizer
            .with_truncation(Some(TruncationParams { max_length: 512, ..Default::default() }))
            .map_err(|e| load(e.to_string()))?;
        // SAFETY: the file was verified against a pinned hash and nothing
        // modifies it while mapped.
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[path("model.safetensors")?], DType::F32, &device)
        }
        .map_err(|e| load(e.to_string()))?;
        let model = BertModel::load(vb, &config).map_err(|e| load(e.to_string()))?;
        Ok(Self { model, tokenizer, device })
    }

    /// Mean-pooled, L2-normalized embeddings.
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        let e = |x: candle_core::Error| x.to_string();
        let mut out = Vec::with_capacity(texts.len());
        for batch in texts.chunks(BATCH) {
            let enc =
                self.tokenizer.encode_batch(batch.to_vec(), true).map_err(|x| x.to_string())?;
            let stack = |f: &dyn Fn(&tokenizers::Encoding) -> &[u32]| -> Result<Tensor, String> {
                let rows = enc
                    .iter()
                    .map(|en| Tensor::new(f(en), &self.device))
                    .collect::<candle_core::Result<Vec<_>>>()
                    .map_err(e)?;
                Tensor::stack(&rows, 0).map_err(e)
            };
            let ids = stack(&|en| en.get_ids())?;
            let mask = stack(&|en| en.get_attention_mask())?;
            let types = ids.zeros_like().map_err(e)?;
            let hidden = self.model.forward(&ids, &types, Some(&mask)).map_err(e)?;
            let weights = mask.to_dtype(hidden.dtype()).map_err(e)?.unsqueeze(2).map_err(e)?;
            let summed = hidden.broadcast_mul(&weights).map_err(e)?.sum(1).map_err(e)?;
            let counts = weights.sum(1).map_err(e)?;
            let mean = summed.broadcast_div(&counts).map_err(e)?;
            let rows: Vec<Vec<f32>> = mean.to_dtype(DType::F32).map_err(e)?.to_vec2().map_err(e)?;
            for mut row in rows {
                normalize(&mut row);
                out.push(row);
            }
        }
        Ok(out)
    }
}

/// Loads the model on first use; [`Self::unload_if_idle`] frees it again.
pub struct E5Embedder {
    store: ModelStore,
    loaded: Mutex<Option<Loaded>>,
    last_used: AtomicI64,
}

impl E5Embedder {
    pub fn new(store: ModelStore) -> Self {
        Self { store, loaded: Mutex::new(None), last_used: AtomicI64::new(0) }
    }

    fn with_model<T>(&self, f: impl FnOnce(&Loaded) -> Result<T, String>) -> Result<T, String> {
        let mut guard = self.loaded.lock();
        if guard.is_none() {
            *guard = Some(Loaded::load(&self.store).map_err(|e| e.to_string())?);
            tracing::info!("embedding model loaded");
        }
        self.last_used.store(now_ms(), Ordering::Relaxed);
        f(guard.as_ref().expect("just loaded"))
    }

    pub fn unload_if_idle(&self, idle_ms: i64) {
        if now_ms() - self.last_used.load(Ordering::Relaxed) > idle_ms
            && let Some(mut guard) = self.loaded.try_lock()
            && guard.take().is_some()
        {
            tracing::info!("embedding model unloaded");
        }
    }
}

impl Embedder for E5Embedder {
    fn model_id(&self) -> &str {
        MODEL_ID
    }

    fn embed_passages(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        let prefixed: Vec<String> = texts.iter().map(|t| format!("passage: {t}")).collect();
        self.with_model(|m| m.embed(&prefixed))
    }

    fn embed_query(&self, text: &str) -> Result<Vec<f32>, String> {
        let prefixed = vec![format!("query: {text}")];
        self.with_model(|m| m.embed(&prefixed))?
            .pop()
            .ok_or_else(|| "empty embedding result".to_string())
    }
}
