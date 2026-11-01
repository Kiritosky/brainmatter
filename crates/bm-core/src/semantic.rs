//! Embedding-based retrieval: the model interface and an in-memory vector index.
//!
//! The vectors live encrypted in SQLite and are loaded into RAM while unlocked.
//! Up to ~100k entries, a brute-force scan is fast enough and exact.

use std::collections::HashMap;

use sha2::{Digest, Sha256};

use crate::ids::NodeId;

/// A sentence-embedding model. Implementations live outside this crate, so the
/// core stays free of ML dependencies.
pub trait Embedder: Send + Sync {
    /// Identifies the model; vectors from other models are ignored.
    fn model_id(&self) -> &str;
    fn embed_passages(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String>;
    fn embed_query(&self, text: &str) -> Result<Vec<f32>, String>;
}

/// Longer entries are embedded in pieces, since the model reads ~512 tokens.
const CHUNK_CHARS: usize = 1500;
const MAX_CHUNKS: usize = 8;

/// The texts to embed for an entry: its title with each chunk of its body.
pub fn passages(title: &str, body: &str) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    for paragraph in body.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        if !current.is_empty() && current.chars().count() + paragraph.chars().count() > CHUNK_CHARS
        {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(paragraph);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks.truncate(MAX_CHUNKS);
    if chunks.is_empty() {
        return vec![title.to_string()];
    }
    chunks.into_iter().map(|c| format!("{title}\n{c}")).collect()
}

/// Mean of unit vectors, normalized again: one vector per entry.
pub fn pool(vectors: &[Vec<f32>]) -> Vec<f32> {
    let Some(first) = vectors.first() else { return Vec::new() };
    let mut out = vec![0.0f32; first.len()];
    for v in vectors {
        for (o, x) in out.iter_mut().zip(v) {
            *o += x;
        }
    }
    normalize(&mut out);
    out
}

pub fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
}

/// Detects whether an entry's text changed since it was embedded.
pub fn content_hash(title: &str, body: &str) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(title.as_bytes());
    h.update([0]);
    h.update(body.as_bytes());
    h.finalize().to_vec()
}

pub(crate) fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

pub(crate) fn from_blob(b: &[u8]) -> Vec<f32> {
    b.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect()
}

/// Below this, the mean says more about the few entries than about the model.
const MIN_FOR_CENTRE: usize = 20;

/// Unit vectors in one contiguous matrix, for cache-friendly scans.
#[derive(Default)]
pub(crate) struct VectorIndex {
    dim: usize,
    ids: Vec<NodeId>,
    slots: HashMap<NodeId, usize>,
    data: Vec<f32>,
    /// The mean of all vectors, and each vector's dot product with it. Kept so a
    /// centred search costs the same scan as a plain one.
    centre: Vec<f32>,
    centre_dots: Vec<f32>,
    changes_since_centre: usize,
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

impl VectorIndex {
    pub(crate) fn len(&self) -> usize {
        self.ids.len()
    }

    /// Whether [`Self::nearest_centred`] has a mean worth trusting.
    pub(crate) fn can_centre(&self) -> bool {
        self.len() >= MIN_FOR_CENTRE
    }

    /// The mean barely moves with a few changes, so it waits for enough of them.
    pub(crate) fn centre_is_stale(&self) -> bool {
        self.changes_since_centre >= (self.len() / 20).max(16)
            || (self.centre.is_empty() && !self.ids.is_empty())
    }

    pub(crate) fn refresh_centre(&mut self) {
        self.changes_since_centre = 0;
        if self.ids.is_empty() {
            self.centre.clear();
            self.centre_dots.clear();
            return;
        }
        let mut centre = vec![0.0f32; self.dim];
        for v in self.data.chunks_exact(self.dim) {
            for (c, x) in centre.iter_mut().zip(v) {
                *c += x;
            }
        }
        let n = self.ids.len() as f32;
        centre.iter_mut().for_each(|c| *c /= n);
        self.centre_dots = self.data.chunks_exact(self.dim).map(|v| dot(v, &centre)).collect();
        self.centre = centre;
    }

    pub(crate) fn get(&self, id: &NodeId) -> Option<&[f32]> {
        let slot = *self.slots.get(id)?;
        Some(&self.data[slot * self.dim..(slot + 1) * self.dim])
    }

    pub(crate) fn upsert(&mut self, id: &NodeId, vector: &[f32]) {
        if self.dim == 0 {
            self.dim = vector.len();
        }
        if vector.len() != self.dim {
            return;
        }
        let centre_dot = if self.centre.is_empty() { 0.0 } else { dot(vector, &self.centre) };
        match self.slots.get(id) {
            Some(&slot) => {
                self.data[slot * self.dim..(slot + 1) * self.dim].copy_from_slice(vector);
                if let Some(d) = self.centre_dots.get_mut(slot) {
                    *d = centre_dot;
                }
            }
            None => {
                self.slots.insert(id.clone(), self.ids.len());
                self.ids.push(id.clone());
                self.data.extend_from_slice(vector);
                if !self.centre.is_empty() {
                    self.centre_dots.push(centre_dot);
                }
            }
        }
        self.changes_since_centre += 1;
    }

    pub(crate) fn remove(&mut self, id: &NodeId) {
        let Some(slot) = self.slots.remove(id) else { return };
        let last = self.ids.len() - 1;
        if slot != last {
            let moved = self.ids[last].clone();
            self.data.copy_within(last * self.dim..(last + 1) * self.dim, slot * self.dim);
            self.slots.insert(moved.clone(), slot);
            self.ids[slot] = moved;
        }
        self.ids.pop();
        self.data.truncate(last * self.dim);
        if !self.centre_dots.is_empty() {
            self.centre_dots.swap_remove(slot);
        }
        self.changes_since_centre += 1;
    }

    /// Like [`Self::nearest`], by cosine after subtracting the mean of all vectors.
    ///
    /// Everything this model embeds points in much the same direction, so raw
    /// scores sit close together and long averaged documents come out near any
    /// query. Without that shared part, only what is specific to a text counts.
    pub(crate) fn nearest_centred(&self, query: &[f32], k: usize) -> Vec<(NodeId, f32)> {
        if self.dim == 0
            || query.len() != self.dim
            || k == 0
            || self.centre.len() != self.dim
            || self.centre_dots.len() != self.ids.len()
        {
            return Vec::new();
        }
        let centre_sq = dot(&self.centre, &self.centre);
        let query_dot = dot(query, &self.centre);
        let query_norm = (dot(query, query) - 2.0 * query_dot + centre_sq).max(1e-9).sqrt();
        let mut scored: Vec<(usize, f32)> = self
            .data
            .chunks_exact(self.dim)
            .zip(&self.centre_dots)
            .enumerate()
            .map(|(i, (v, centre_dot))| {
                // |v| is 1, so |v - centre|² needs no second pass over the vector.
                let norm = (1.0 - 2.0 * centre_dot + centre_sq).max(1e-9).sqrt();
                let score = (dot(v, query) - centre_dot - query_dot + centre_sq) / (norm * query_norm);
                (i, score)
            })
            .collect();
        let k = k.min(scored.len());
        if k < scored.len() {
            scored.select_nth_unstable_by(k, |a, b| b.1.total_cmp(&a.1));
            scored.truncate(k);
        }
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.into_iter().map(|(i, s)| (self.ids[i].clone(), s)).collect()
    }

    /// The `k` most similar entries by cosine, best first.
    pub(crate) fn nearest(&self, query: &[f32], k: usize) -> Vec<(NodeId, f32)> {
        if self.dim == 0 || query.len() != self.dim || k == 0 {
            return Vec::new();
        }
        let mut scored: Vec<(usize, f32)> = self
            .data
            .chunks_exact(self.dim)
            .enumerate()
            .map(|(i, v)| (i, v.iter().zip(query).map(|(a, b)| a * b).sum()))
            .collect();
        let k = k.min(scored.len());
        if k < scored.len() {
            scored.select_nth_unstable_by(k, |a, b| b.1.total_cmp(&a.1));
            scored.truncate(k);
        }
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.into_iter().map(|(i, s)| (self.ids[i].clone(), s)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> NodeId {
        NodeId::from_raw(s)
    }

    #[test]
    fn index_finds_nearest_and_survives_removal() {
        let mut ix = VectorIndex::default();
        ix.upsert(&id("x"), &[1.0, 0.0]);
        ix.upsert(&id("y"), &[0.0, 1.0]);
        ix.upsert(&id("z"), &[0.6, 0.8]);
        let hits = ix.nearest(&[1.0, 0.0], 2);
        assert_eq!(hits[0].0, id("x"));
        assert_eq!(hits[1].0, id("z"));

        ix.remove(&id("x"));
        assert_eq!(ix.len(), 2);
        assert!(ix.get(&id("x")).is_none());
        assert_eq!(ix.get(&id("z")).unwrap(), &[0.6, 0.8]);
        assert_eq!(ix.nearest(&[1.0, 0.0], 5)[0].0, id("z"));

        ix.upsert(&id("y"), &[1.0, 0.0]);
        assert_eq!(ix.nearest(&[1.0, 0.0], 1)[0].0, id("y"));
    }

    fn unit(v: [f32; 3]) -> Vec<f32> {
        let mut v = v.to_vec();
        normalize(&mut v);
        v
    }

    #[test]
    fn centred_scores_match_the_definition_and_survive_changes() {
        let mut ix = VectorIndex::default();
        let vectors = [unit([1.0, 0.1, 0.0]), unit([1.0, 0.0, 0.1]), unit([1.0, -0.1, 0.0]), unit([1.0, 0.0, -0.1])];
        for (i, v) in vectors.iter().enumerate() {
            ix.upsert(&id(&format!("n{i}")), v);
        }
        ix.refresh_centre();
        ix.remove(&id("n0"));
        ix.upsert(&id("n0"), &vectors[0]);

        // By hand: subtract the mean from both sides, then take the cosine.
        let query = unit([1.0, 0.3, 0.0]);
        let centre: Vec<f32> = (0..3).map(|d| vectors.iter().map(|v| v[d]).sum::<f32>() / 4.0).collect();
        let minus = |v: &[f32]| {
            let mut out: Vec<f32> = v.iter().zip(&centre).map(|(a, c)| a - c).collect();
            normalize(&mut out);
            out
        };
        let expected = dot(&minus(&query), &minus(&vectors[0]));
        let hits = ix.nearest_centred(&query, 4);
        assert_eq!(hits[0].0, id("n0"));
        assert!((hits[0].1 - expected).abs() < 1e-3, "{} vs {expected}", hits[0].1);
    }

    #[test]
    fn the_centre_waits_for_enough_changes() {
        let mut ix = VectorIndex::default();
        ix.upsert(&id("first"), &unit([1.0, 0.0, 0.0]));
        assert!(ix.centre_is_stale(), "there is no centre yet");
        ix.refresh_centre();
        assert!(!ix.centre_is_stale());
        for i in 0..16 {
            ix.upsert(&id(&format!("n{i}")), &unit([1.0, i as f32, 0.0]));
        }
        assert!(ix.centre_is_stale());
    }

    #[test]
    fn passages_split_long_bodies_and_keep_the_title() {
        assert_eq!(passages("Nur Titel", " "), ["Nur Titel"]);
        assert_eq!(passages("T", "a\n\nb"), ["T\na\n\nb"]);
        let long = vec!["x".repeat(1000); 3].join("\n\n");
        let p = passages("T", &long);
        assert_eq!(p.len(), 3);
        assert!(p.iter().all(|c| c.starts_with("T\n")));
        let huge = vec!["y".repeat(1600); 20].join("\n\n");
        assert_eq!(passages("T", &huge).len(), MAX_CHUNKS);
    }

    #[test]
    fn blobs_roundtrip_and_pooling_normalizes() {
        let v = vec![0.25f32, -1.5, 3.0];
        assert_eq!(from_blob(&to_blob(&v)), v);
        let p = pool(&[vec![1.0, 0.0], vec![0.0, 1.0]]);
        assert!((p[0] - p[1]).abs() < 1e-6 && (p[0] * p[0] + p[1] * p[1] - 1.0).abs() < 1e-5);
    }
}
