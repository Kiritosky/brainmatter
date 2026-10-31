//! Meaning-based features on top of the vector index: hybrid search, similar
//! entries, link suggestions and the background indexer.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use rusqlite::{OptionalExtension, params};

use super::{Engine, edges, projects, rows, search};
use crate::{
    error::Result,
    events::Event,
    ids::NodeId,
    model::*,
    semantic::{Embedder, content_hash, from_blob, passages, pool, to_blob},
    time::now_ms,
};

/// Passage-to-passage similarity from which two entries probably say the same
/// thing, and from which they are worth suggesting as a link. Measured with
/// `bm-embed --example calibrate`; see PLAN.md §17. multilingual-e5 scores sit
/// high and close together, so these are hints, never automatic actions.
pub const DUPLICATE_SCORE: f32 = 0.925;
pub const SUGGEST_SCORE: f32 = 0.88;
/// Query-to-passage scores overlap: right hits measured 0.80–0.91, wrong ones
/// up to 0.83. So a hit needs both this floor and to be close to the query's
/// best hit, which keeps 9 of 10 right hits and 7 of 90 wrong ones (PLAN.md §17).
const QUERY_FLOOR: f32 = 0.80;
const QUERY_MARGIN: f32 = 0.02;
/// Reciprocal rank fusion constant.
const RRF_K: f64 = 60.0;
const MAX_SUGGESTIONS: usize = 5;

/// Small, because locking the vault waits for the running batch.
const INDEX_BATCH: usize = 8;
const INDEX_DEBOUNCE: Duration = Duration::from_secs(2);
const INDEX_TICK: Duration = Duration::from_millis(500);

struct Stale {
    id: NodeId,
    title: String,
    body: String,
    hash: Vec<u8>,
}

impl Engine {
    /// Turns semantic features on for `model`: loads its stored vectors.
    pub fn load_vectors(&self, model: &str) -> Result<()> {
        let mut index = crate::semantic::VectorIndex::default();
        self.read(|c| {
            let mut stmt = c.prepare(
                "SELECT e.node_id, e.vec FROM embeddings e JOIN nodes n ON n.id = e.node_id
                 WHERE e.model = ?1 AND n.deleted_at IS NULL",
            )?;
            let rows =
                stmt.query_map([model], |r| Ok((r.get::<_, NodeId>(0)?, r.get::<_, Vec<u8>>(1)?)))?;
            for row in rows {
                let (id, blob) = row?;
                index.upsert(&id, &from_blob(&blob));
            }
            Ok(())
        })?;
        *self.vectors.write() = index;
        *self.vector_model.write() = Some(model.to_string());
        Ok(())
    }

    /// Turns semantic features off again. Stored vectors stay: they are still
    /// valid if the model comes back.
    pub fn unload_vectors(&self) {
        *self.vector_model.write() = None;
        *self.vectors.write() = Default::default();
    }

    /// Keeps the in-memory index in line with committed changes: trashed
    /// entries leave it, restored ones come back without re-embedding.
    pub(super) fn sync_vectors(&self, events: &[Event]) {
        let Some(model) = self.vector_model.read().clone() else { return };
        for event in events {
            match event {
                Event::NodeDeleted { id } => self.vectors.write().remove(id),
                Event::NodeUpserted { id } if self.vectors.read().get(id).is_none() => {
                    let blob: Result<Option<Vec<u8>>> = self.read(|c| {
                        Ok(c.query_row(
                            "SELECT vec FROM embeddings WHERE node_id = ?1 AND model = ?2",
                            params![id, model],
                            |r| r.get(0),
                        )
                        .optional()?)
                    });
                    if let Ok(Some(blob)) = blob {
                        self.vectors.write().upsert(id, &from_blob(&blob));
                    }
                }
                _ => {}
            }
        }
    }

    pub fn index_status(&self) -> Result<IndexStatus> {
        let total: i64 = self.read(|c| {
            Ok(c.query_row("SELECT count(*) FROM nodes WHERE deleted_at IS NULL", [], |r| {
                r.get(0)
            })?)
        })?;
        Ok(IndexStatus { indexed: self.vectors.read().len(), total: total as usize })
    }

    /// Entries whose text changed since embedding, optionally only those
    /// touched after `since` (ms).
    fn stale(&self, model: &str, since: Option<i64>) -> Result<Vec<Stale>> {
        self.read(|c| {
            let mut stmt = c.prepare(
                "SELECT n.id, n.title, n.body, e.content_hash FROM nodes n
                 LEFT JOIN embeddings e ON e.node_id = n.id AND e.model = ?1
                 WHERE n.deleted_at IS NULL AND n.updated_at >= ?2",
            )?;
            let rows = stmt.query_map(params![model, since.unwrap_or(0)], |r| {
                Ok((
                    r.get::<_, NodeId>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<Vec<u8>>>(3)?,
                ))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (id, title, body, stored) = row?;
                let hash = content_hash(&title, &body);
                if stored.as_deref() != Some(hash.as_slice()) {
                    out.push(Stale { id, title, body, hash });
                }
            }
            Ok(out)
        })
    }

    fn store_vector(&self, id: &NodeId, model: &str, vector: &[f32], hash: &[u8]) -> Result<()> {
        self.writer.lock().execute(
            "INSERT INTO embeddings (node_id, model, dim, vec, content_hash) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (node_id) DO UPDATE SET model = excluded.model, dim = excluded.dim,
                 vec = excluded.vec, content_hash = excluded.content_hash",
            params![id, model, vector.len() as i64, to_blob(vector), hash],
        )?;
        // It may have been trashed while the model was busy.
        if self.read(|c| rows::live_node(c, id).map(|_| ())).is_ok() {
            self.vectors.write().upsert(id, vector);
        }
        Ok(())
    }

    /// Embeds specific entries right away, e.g. to report similar ones in the
    /// same response. Already current entries are skipped.
    pub fn index_now(&self, embedder: &dyn Embedder, ids: &[NodeId]) -> Result<()> {
        let model = embedder.model_id().to_string();
        let mut stale = Vec::new();
        for id in ids {
            let node = self.read(|c| rows::live_node(c, id))?;
            let hash = content_hash(&node.title, &node.body);
            let stored: Option<Vec<u8>> = self.read(|c| {
                Ok(c.query_row(
                    "SELECT content_hash FROM embeddings WHERE node_id = ?1 AND model = ?2",
                    params![id, model],
                    |r| r.get(0),
                )
                .optional()?)
            })?;
            if stored.as_deref() != Some(hash.as_slice()) {
                stale.push(Stale { id: node.id, title: node.title, body: node.body, hash });
            }
        }
        self.embed_batch(embedder, &stale)
    }

    fn embed_batch(&self, embedder: &dyn Embedder, entries: &[Stale]) -> Result<()> {
        let model = embedder.model_id();
        for entry in entries {
            let texts = passages(&entry.title, &entry.body);
            let vectors = embedder.embed_passages(&texts).map_err(crate::CoreError::Invalid)?;
            self.store_vector(&entry.id, model, &pool(&vectors), &entry.hash)?;
        }
        Ok(())
    }

    /// Text and meaning search combined by reciprocal rank fusion. Without a
    /// query vector this is plain text search.
    pub fn search_hybrid(
        &self,
        q: &SearchQuery,
        query_vector: Option<&[f32]>,
    ) -> Result<Vec<SearchHit>> {
        let Some(vector) = query_vector.filter(|_| !q.text.trim().is_empty()) else {
            return self.search(q);
        };
        self.read(|c| {
            let limit = q.limit.clamp(1, 200);
            let mut wide = q.clone();
            wide.limit = (limit * 3).min(200);
            let text_hits = search::search(c, &wide)?;

            let allowed = |id: &NodeId| -> Result<Option<Node>> {
                let Ok(node) = rows::live_node(c, id) else { return Ok(None) };
                if !q.kinds.is_empty() && !q.kinds.contains(&node.kind) {
                    return Ok(None);
                }
                if !q.tags.is_empty()
                    && !q
                        .tags
                        .iter()
                        .any(|t| node.tags.contains(&t.trim_start_matches('#').to_lowercase()))
                {
                    return Ok(None);
                }
                if let Some(project) = &q.project {
                    let scope: HashSet<_> = if q.include_subprojects {
                        projects::subtree(c, project)?.into_iter().collect()
                    } else {
                        HashSet::from([project.clone()])
                    };
                    if !rows::projects_of(c, id)?.iter().any(|p| scope.contains(&p.id)) {
                        return Ok(None);
                    }
                }
                Ok(Some(node))
            };

            let mut scores: HashMap<NodeId, f64> = HashMap::new();
            let mut hits: HashMap<NodeId, SearchHit> = HashMap::new();
            for (rank, hit) in text_hits.into_iter().enumerate() {
                *scores.entry(hit.node.id.clone()).or_default() +=
                    1.0 / (RRF_K + rank as f64 + 1.0);
                hits.insert(hit.node.id.clone(), hit);
            }
            let nearest = self.vectors.read().nearest(vector, (limit * 8).max(100));
            let mut rank = 0usize;
            let mut cut = QUERY_FLOOR;
            for (id, score) in nearest {
                if score < cut || rank >= limit * 3 {
                    break;
                }
                let Some(node) = allowed(&id)? else { continue };
                // The best hit within the filters, not an excluded one, sets the bar.
                if rank == 0 {
                    cut = cut.max(score - QUERY_MARGIN);
                }
                *scores.entry(id.clone()).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
                rank += 1;
                hits.entry(id).and_modify(|h| h.semantic = true).or_insert_with(|| SearchHit {
                    snippet: node.body.chars().take(160).collect(),
                    node: NodeBrief { id: node.id, title: node.title, kind: node.kind },
                    score: 0.0,
                    semantic: true,
                });
            }

            let mut merged: Vec<SearchHit> = hits
                .into_iter()
                .map(|(id, mut hit)| {
                    hit.score = scores[&id];
                    hit
                })
                .collect();
            merged.sort_by(|a, b| {
                b.score.total_cmp(&a.score).then_with(|| a.node.title.cmp(&b.node.title))
            });
            merged.truncate(limit);
            Ok(merged)
        })
    }

    /// Entries similar in meaning to `id`, best first. Computed on demand: a
    /// scan takes milliseconds and can never be out of date.
    pub fn similar_nodes(&self, id: &NodeId, limit: usize, min_score: f32) -> Result<Vec<Similar>> {
        let Some(vector) = self.vectors.read().get(id).map(<[f32]>::to_vec) else {
            return Ok(Vec::new());
        };
        let nearest = self.vectors.read().nearest(&vector, limit + 1);
        self.read(|c| {
            let mut out = Vec::new();
            for (other, score) in nearest {
                if &other == id || score < min_score {
                    continue;
                }
                if let Some(node) = rows::brief(c, &other)? {
                    out.push(Similar { node, score, likely_duplicate: score >= DUPLICATE_SCORE });
                }
            }
            Ok(out)
        })
    }

    /// Link suggestions for an entry: similar, not linked yet, not dismissed.
    pub fn link_suggestions(&self, id: &NodeId) -> Result<Vec<Similar>> {
        let candidates = self.similar_nodes(id, MAX_SUGGESTIONS + 20, SUGGEST_SCORE)?;
        self.read(|c| {
            let linked: HashSet<NodeId> = edges::active_edges_of(c, id)?
                .into_iter()
                .map(|e| if &e.src == id { e.dst } else { e.src })
                .collect();
            let mut dismissed = c.prepare_cached(
                "SELECT 1 FROM link_suggestions WHERE status = 'dismissed'
                 AND ((src = ?1 AND dst = ?2) OR (src = ?2 AND dst = ?1))",
            )?;
            let mut out = Vec::new();
            for s in candidates {
                if !linked.contains(&s.node.id) && !dismissed.exists(params![id, s.node.id])? {
                    out.push(s);
                }
                if out.len() == MAX_SUGGESTIONS {
                    break;
                }
            }
            Ok(out)
        })
    }

    /// Stops suggesting this pair, in both directions.
    pub fn dismiss_suggestion(&self, a: &NodeId, b: &NodeId) -> Result<()> {
        self.writer.lock().execute(
            "INSERT INTO link_suggestions (src, dst, score, status, created_at)
             VALUES (?1, ?2, 0, 'dismissed', ?3)
             ON CONFLICT (src, dst) DO UPDATE SET status = 'dismissed'",
            params![a, b, now_ms()],
        )?;
        Ok(())
    }

    /// Creates the suggested link.
    pub fn accept_suggestion(
        &self,
        ctx: &Ctx,
        src: &NodeId,
        dst: &NodeId,
        relation: Option<&str>,
    ) -> Result<Edge> {
        let input = NewEdge {
            src: src.clone(),
            dst: dst.clone(),
            relation: relation.map(str::to_string),
            weight: None,
            note: None,
        };
        Ok(self
            .write(ctx, "Vorgeschlagene Verbindung angenommen", |w| {
                edges::link(w, &input, EdgeOrigin::Suggestion)
            })?
            .0)
    }

    /// Entries without any link, newest first.
    pub fn orphans(&self, limit: usize) -> Result<Vec<NodeBrief>> {
        self.read(|c| {
            let mut stmt = c.prepare_cached(
                "SELECT n.id, n.title, n.kind FROM nodes n
                 WHERE n.deleted_at IS NULL AND n.kind != 'template'
                   AND NOT EXISTS (SELECT 1 FROM edges e WHERE e.deleted_at IS NULL AND (e.src = n.id OR e.dst = n.id))
                 ORDER BY n.updated_at DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map([limit as i64], rows::brief_from_row)?.collect::<rusqlite::Result<_>>()?;
            Ok(rows)
        })
    }

    /// AI-written entries the user has not looked at, newest first.
    pub fn unreviewed(&self, limit: usize) -> Result<Vec<NodeBrief>> {
        self.read(|c| {
            let mut stmt = c.prepare_cached(
                "SELECT id, title, kind FROM nodes WHERE deleted_at IS NULL AND reviewed = 0
                 ORDER BY updated_at DESC LIMIT ?1",
            )?;
            let rows = stmt
                .query_map([limit as i64], rows::brief_from_row)?
                .collect::<rusqlite::Result<_>>()?;
            Ok(rows)
        })
    }
}

/// Keeps embeddings current in the background. Dropping the handle stops it;
/// it also ends when the engine goes away (vault locked).
pub struct Indexer {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Indexer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            // Locking waits here, so don't sit out the indexer's pause.
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}

pub fn spawn_indexer(engine: &Arc<Engine>, embedder: Arc<dyn Embedder>) -> Result<Indexer> {
    engine.load_vectors(embedder.model_id())?;
    let weak = Arc::downgrade(engine);
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let thread = std::thread::Builder::new()
        .name("bm-indexer".into())
        .spawn(move || index_loop(weak, embedder, flag))
        .expect("spawn indexer thread");
    Ok(Indexer { stop, thread: Some(thread) })
}

fn index_loop(engine: Weak<Engine>, embedder: Arc<dyn Embedder>, stop: Arc<AtomicBool>) {
    let model = embedder.model_id().to_string();
    let mut indexed_generation = None; // first round: everything
    let mut last_scan: Option<i64> = None;
    let mut seen_generation = None;
    let mut changed_at = Instant::now();

    while !stop.load(Ordering::Relaxed) {
        let Some(engine) = engine.upgrade() else { return };
        let generation = engine.generation.load(Ordering::Relaxed);
        if seen_generation != Some(generation) {
            if seen_generation.is_some() {
                changed_at = Instant::now();
            }
            seen_generation = Some(generation);
        }
        let due = indexed_generation.is_none()
            || (changed_at.elapsed() >= INDEX_DEBOUNCE && indexed_generation != Some(generation));
        if !due {
            drop(engine);
            std::thread::park_timeout(INDEX_TICK);
            continue;
        }

        let started = now_ms();
        // Slight overlap: a clock tick must not hide a change. Reading the
        // generation before the scan means later changes trigger another round.
        let scan_from = last_scan.map(|t| t - 1000);
        let stale = match engine.stale(&model, scan_from) {
            Ok(stale) => stale,
            Err(e) => {
                tracing::warn!("indexer scan failed: {e}");
                drop(engine);
                std::thread::park_timeout(INDEX_DEBOUNCE);
                continue;
            }
        };
        let mut failed = false;
        for batch in stale.chunks(INDEX_BATCH) {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            if let Err(e) = engine.embed_batch(embedder.as_ref(), batch) {
                tracing::warn!("indexer embedding failed: {e}");
                failed = true;
                break;
            }
            if let Ok(status) = engine.index_status() {
                let _ = engine
                    .events
                    .send(Event::IndexUpdated { indexed: status.indexed, total: status.total });
            }
        }
        if failed {
            drop(engine);
            std::thread::park_timeout(INDEX_DEBOUNCE * 5);
            continue;
        }
        indexed_generation = Some(generation);
        last_scan = Some(started);
        drop(engine);
        std::thread::park_timeout(INDEX_TICK);
    }
}
