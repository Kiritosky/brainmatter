//! A compact binary form of [`GraphSnapshot`] for the app's web view.
//!
//! As JSON, a snapshot of 50,000 entries and 150,000 links is 45 MB, and writing,
//! sending and parsing it takes about half a second. Here every string is stored
//! once and everything else in columns that JavaScript reads as typed arrays.
//! `app/src/lib/graph-wire.ts` reads it.
//!
//! All numbers are little-endian. Each section starts at a multiple of 8 bytes.
//!
//! | Section | Content |
//! |---|---|
//! | Header | 8 × u32: magic, version, nodes, edges, strings, text bytes, view, 0 |
//!
//! A string may appear more than once in the text; readers only follow the numbers.
//! | String ends | u32 per string: where it ends in the text, in UTF-16 units |
//! | Text | all strings as UTF-8, one after the other |
//! | Node numbers | f64 columns `x`, `y` (NaN for none), `updated_at`; f32 column `rank` |
//! | Node fields | u32 columns `id`, `title`, `kind`, `created_by` (strings), `degree`, `cluster`, flags (1 reviewed, 2 pinned) |
//! | Node lists | u32: where each node's projects end, the project strings, where each node's tags end, the tag strings |
//! | Edges | u32 columns `id`, `relation`, `origin` (strings), `src`, `dst` (node positions) |

use std::collections::HashMap;

use crate::model::{GraphEdge, GraphSnapshot};

pub const MAGIC: u32 = u32::from_le_bytes(*b"BMG1");
pub const VERSION: u32 = 1;

/// The strings, numbered in order. Those that repeat, such as kinds and projects,
/// are stored once; ids and titles are nearly all different and skip the lookup.
#[derive(Default)]
struct Strings<'a> {
    index: HashMap<&'a str, u32>,
    list: Vec<&'a str>,
}

impl<'a> Strings<'a> {
    fn add(&mut self, s: &'a str) -> u32 {
        if let Some(i) = self.index.get(s) {
            return *i;
        }
        let i = self.push(s);
        self.index.insert(s, i);
        i
    }

    fn push(&mut self, s: &'a str) -> u32 {
        self.list.push(s);
        self.list.len() as u32 - 1
    }
}

#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn u32s(&mut self, values: impl IntoIterator<Item = u32>) {
        for v in values {
            self.0.extend_from_slice(&v.to_le_bytes());
        }
        self.pad();
    }

    fn f64s(&mut self, values: impl IntoIterator<Item = f64>) {
        for v in values {
            self.0.extend_from_slice(&v.to_le_bytes());
        }
        self.pad();
    }

    fn f32s(&mut self, values: impl IntoIterator<Item = f32>) {
        for v in values {
            self.0.extend_from_slice(&v.to_le_bytes());
        }
        self.pad();
    }

    fn pad(&mut self) {
        while !self.0.len().is_multiple_of(8) {
            self.0.push(0);
        }
    }
}

/// Where each list ends in the flat sequence of all lists' items.
fn ends<T>(lists: impl Iterator<Item = T>, len: impl Fn(&T) -> usize) -> Vec<u32> {
    let mut end = 0u32;
    lists
        .map(|l| {
            end += len(&l) as u32;
            end
        })
        .collect()
}

pub fn encode(snapshot: &GraphSnapshot) -> Vec<u8> {
    let nodes = &snapshot.nodes;
    let edges = &snapshot.edges;
    let mut strings = Strings::default();
    let view = strings.add(&snapshot.view);
    let position: HashMap<&str, u32> =
        nodes.iter().enumerate().map(|(i, n)| (n.id.as_str(), i as u32)).collect();

    let node_ids: Vec<u32> = nodes.iter().map(|n| strings.push(n.id.as_str())).collect();
    let titles: Vec<u32> = nodes.iter().map(|n| strings.push(&n.title)).collect();
    let kinds: Vec<u32> = nodes.iter().map(|n| strings.add(&n.kind)).collect();
    let authors: Vec<u32> = nodes.iter().map(|n| strings.add(n.created_by.as_str())).collect();
    let projects: Vec<u32> =
        nodes.iter().flat_map(|n| &n.projects).map(|p| strings.add(p.as_str())).collect();
    let tags: Vec<u32> = nodes.iter().flat_map(|n| &n.tags).map(|t| strings.add(t)).collect();
    // Edges whose ends are outside the snapshot cannot be drawn; the snapshot never has any.
    let edges: Vec<&GraphEdge> = edges
        .iter()
        .filter(|e| position.contains_key(e.src.as_str()) && position.contains_key(e.dst.as_str()))
        .collect();
    let edge_ids: Vec<u32> = edges.iter().map(|e| strings.push(e.id.as_str())).collect();
    let relations: Vec<u32> = edges.iter().map(|e| strings.add(&e.relation)).collect();
    let origins: Vec<u32> = edges.iter().map(|e| strings.add(e.origin.as_str())).collect();

    let text: String = strings.list.concat();
    let mut out = Writer(Vec::with_capacity(text.len() + nodes.len() * 72 + edges.len() * 20));
    out.u32s([
        MAGIC,
        VERSION,
        nodes.len() as u32,
        edges.len() as u32,
        strings.list.len() as u32,
        text.len() as u32,
        view,
        0,
    ]);
    let mut end = 0u32;
    out.u32s(strings.list.iter().map(|s| {
        end += s.encode_utf16().count() as u32;
        end
    }));
    out.0.extend_from_slice(text.as_bytes());
    out.pad();

    out.f64s(nodes.iter().map(|n| n.x.unwrap_or(f64::NAN)));
    out.f64s(nodes.iter().map(|n| n.y.unwrap_or(f64::NAN)));
    out.f64s(nodes.iter().map(|n| n.updated_at as f64));
    out.f32s(nodes.iter().map(|n| n.rank));
    out.u32s(node_ids);
    out.u32s(titles);
    out.u32s(kinds);
    out.u32s(authors);
    out.u32s(nodes.iter().map(|n| n.degree as u32));
    out.u32s(nodes.iter().map(|n| n.cluster));
    out.u32s(nodes.iter().map(|n| u32::from(n.reviewed) | (u32::from(n.pinned) << 1)));
    out.u32s(ends(nodes.iter(), |n| n.projects.len()));
    out.u32s(projects);
    out.u32s(ends(nodes.iter(), |n| n.tags.len()));
    out.u32s(tags);

    out.u32s(edge_ids);
    out.u32s(relations);
    out.u32s(origins);
    out.u32s(edges.iter().map(|e| position[e.src.as_str()]));
    out.u32s(edges.iter().map(|e| position[e.dst.as_str()]));
    out.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ids::{ActorId, EdgeId, NodeId, ProjectId},
        model::{EdgeOrigin, GraphNode},
    };

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    fn node(id: &str, title: &str) -> GraphNode {
        GraphNode {
            id: NodeId::from_raw(id),
            title: title.into(),
            kind: "note".into(),
            reviewed: true,
            updated_at: 1_793_600_000_000,
            created_by: ActorId::from_raw("me"),
            projects: vec![ProjectId::from_raw("p")],
            tags: vec![],
            degree: 1,
            rank: 0.5,
            cluster: 0,
            x: Some(1.5),
            y: None,
            pinned: false,
        }
    }

    #[test]
    fn strings_are_stored_once_and_counted_in_utf16() {
        let snapshot = GraphSnapshot {
            view: "all".into(),
            nodes: vec![node("a", "Über 😀"), node("b", "note")],
            edges: vec![
                GraphEdge {
                    id: EdgeId::from_raw("e"),
                    src: NodeId::from_raw("a"),
                    dst: NodeId::from_raw("b"),
                    relation: "related_to".into(),
                    origin: EdgeOrigin::Manual,
                },
                // Leads outside the snapshot, so it is left out.
                GraphEdge {
                    id: EdgeId::from_raw("x"),
                    src: NodeId::from_raw("a"),
                    dst: NodeId::from_raw("gone"),
                    relation: "related_to".into(),
                    origin: EdgeOrigin::Manual,
                },
            ],
        };
        let bytes = encode(&snapshot);
        assert_eq!(u32_at(&bytes, 0), MAGIC);
        assert_eq!(u32_at(&bytes, 8), 2, "nodes");
        assert_eq!(u32_at(&bytes, 12), 1, "edges");
        // all, a, b, Über 😀, note (the second title), note (the kind, stored again since
        // titles are not looked up), me, p, e, related_to, manual
        assert_eq!(u32_at(&bytes, 16), 11, "strings");
        let text = "allabÜber 😀notenotemeperelated_tomanual";
        assert_eq!(u32_at(&bytes, 20) as usize, text.len());
        // The emoji takes two UTF-16 units, so "Über 😀" ends at 3 + 2 + 7.
        assert_eq!(u32_at(&bytes, 32 + 3 * 4), 12);
        assert_eq!(bytes.len() % 8, 0);
        // app/src/lib/graph-wire.test.ts writes the same snapshot and must get these bytes.
        let hash = bytes.iter().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(*b)));
        assert_eq!((bytes.len(), hash), (304, 533_818_328));
    }
}
