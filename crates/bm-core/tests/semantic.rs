//! Meaning-based features with a deterministic fake model: words hash into
//! buckets, and listed synonyms share a bucket.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use bm_core::{
    CoreError, Engine, Vault,
    engine::spawn_indexer,
    model::*,
    semantic::{Embedder, normalize},
    vault::KdfParams,
};

const SYNONYMS: &[(&str, &str)] = &[
    ("parallel", "async"),
    ("gleichzeitig", "runtime"),
    ("geteilt", "gemeinsam"),
    ("flott", "besonderheit"),
];

#[derive(Default)]
struct FakeModel {
    calls: AtomicUsize,
}

impl FakeModel {
    fn vector(text: &str) -> Vec<f32> {
        let mut v = vec![0.0f32; 256];
        for word in
            text.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty())
        {
            let word = SYNONYMS.iter().find(|(a, _)| *a == word).map_or(word, |(_, b)| *b);
            let mut h = DefaultHasher::new();
            word.hash(&mut h);
            v[(h.finish() % 256) as usize] += 1.0;
        }
        normalize(&mut v);
        v
    }
}

impl Embedder for FakeModel {
    fn model_id(&self) -> &str {
        "fake"
    }

    fn embed_passages(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        self.calls.fetch_add(texts.len(), Ordering::Relaxed);
        Ok(texts.iter().map(|t| Self::vector(t)).collect())
    }

    fn embed_query(&self, text: &str) -> Result<Vec<f32>, String> {
        Ok(Self::vector(text))
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    engine: Arc<Engine>,
    user: Ctx,
    model: Arc<FakeModel>,
}

fn setup() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let (vault, keys, _) = Vault::create(dir.path(), "pw", KdfParams::insecure_fast()).unwrap();
    let engine = Arc::new(Engine::open(&vault.db_path(), &keys.db_key(), 2).unwrap());
    let user = Ctx::new(engine.ensure_actor(ActorKind::User, "Lasse").unwrap().id);
    engine.load_vectors("fake").unwrap();
    Fixture { _dir: dir, engine, user, model: Arc::new(FakeModel::default()) }
}

fn note(title: &str, body: &str) -> NewNode {
    NewNode { title: title.into(), body: body.into(), ..Default::default() }
}

impl Fixture {
    fn add(&self, input: NewNode) -> Node {
        let n = self.engine.create_node(&self.user, input).unwrap();
        self.engine.index_now(self.model.as_ref(), std::slice::from_ref(&n.id)).unwrap();
        n
    }

    fn hybrid(&self, q: SearchQuery) -> Vec<SearchHit> {
        let v = FakeModel::vector(&q.text);
        self.engine.search_hybrid(&q, Some(&v)).unwrap()
    }
}

fn titles(hits: &[SearchHit]) -> Vec<&str> {
    hits.iter().map(|h| h.node.title.as_str()).collect()
}

#[test]
fn hybrid_search_finds_by_meaning_and_respects_filters() {
    let f = setup();
    let project = f
        .engine
        .create_project(&f.user, NewProject { name: "P".into(), ..Default::default() })
        .unwrap();
    f.add(NewNode { projects: vec![project.id.clone()], ..note("async runtime", "") });
    f.add(note("Sauerteig", "Mehl und Wasser"));

    // No shared word, so text search alone finds nothing.
    let q = SearchQuery::new("parallel gleichzeitig");
    assert!(f.engine.search(&q).unwrap().is_empty());
    let hits = f.hybrid(q.clone());
    assert_eq!(titles(&hits), ["async runtime"]);
    assert!(hits[0].semantic);

    // Word matches still count, and are not marked as meaning-only.
    let hits = f.hybrid(SearchQuery::new("Sauerteig"));
    assert_eq!(titles(&hits), ["Sauerteig"]);

    assert!(f.hybrid(SearchQuery { kinds: vec!["task".into()], ..q.clone() }).is_empty());
    assert!(f.hybrid(SearchQuery { tags: vec!["x".into()], ..q.clone() }).is_empty());
    assert_eq!(f.hybrid(SearchQuery { project: Some(project.id.clone()), ..q.clone() }).len(), 1);

    let other = f
        .engine
        .create_project(&f.user, NewProject { name: "O".into(), ..Default::default() })
        .unwrap();
    assert!(f.hybrid(SearchQuery { project: Some(other.id), ..q }).is_empty());

    // Unrelated queries stay empty instead of returning the nearest noise.
    assert!(f.hybrid(SearchQuery::new("Steuererklärung Quittung")).is_empty());
}

#[test]
fn what_all_entries_share_does_not_make_them_hits() {
    let f = setup();
    // Like the real model, where every text shares a large part of its direction.
    let shared = "gemeinsam ".repeat(4);
    for i in 0..24 {
        f.add(note(&format!("Eintrag{i}"), &format!("{shared} wort{i}")));
    }
    let special = f.add(note("Besonders", &format!("{shared} besonderheit")));

    // Raw cosine rates every entry about 0.97 against this.
    let hits = f.hybrid(SearchQuery::new("geteilt"));
    assert!(hits.is_empty(), "{:?}", titles(&hits));

    let hits = f.hybrid(SearchQuery::new("flott"));
    assert_eq!(titles(&hits), [special.title.as_str()]);
    assert!(hits[0].semantic);
}

#[test]
fn similar_entries_suggestions_and_duplicates() {
    let f = setup();
    let a = f.add(note(
        "Tokio Runtime Rust Async",
        "Scheduler Tasks Threads Futures Waker Pinning Poll Reactor",
    ));
    let b = f.add(note(
        "Tokio Runtime Rust Async",
        "Scheduler Tasks Threads Futures Waker Pinning Poll Reactor",
    ));
    let c = f.add(note(
        "Tokio Runtime Rust Async",
        "Scheduler Tasks Threads Executor Waker Pinning Poll Reactor",
    ));
    let far = f.add(note("Sauerteig", "Mehl Wasser Salz"));

    let similar = f.engine.similar_nodes(&a.id, 5, 0.5).unwrap();
    assert_eq!(similar[0].node.id, b.id);
    assert!(similar[0].likely_duplicate);
    assert!(similar.iter().all(|s| s.node.id != far.id && s.node.id != a.id));

    let ids = |id| -> Vec<_> {
        f.engine.link_suggestions(id).unwrap().into_iter().map(|s| s.node.id).collect()
    };
    let suggested = ids(&a.id);
    assert!(suggested.contains(&b.id) && suggested.contains(&c.id));

    // Linked entries are not suggested, whichever way the link points.
    let edge = f.engine.accept_suggestion(&f.user, &c.id, &a.id, None).unwrap();
    assert_eq!(edge.origin, EdgeOrigin::Suggestion);
    assert!(!ids(&a.id).contains(&c.id));

    // Dismissing works in both directions and is permanent.
    f.engine.dismiss_suggestion(&b.id, &a.id).unwrap();
    assert!(!ids(&a.id).contains(&b.id));
    assert!(!ids(&b.id).contains(&a.id));
    f.engine.dismiss_suggestion(&b.id, &a.id).unwrap();
}

#[test]
fn indexer_keeps_up_and_restored_entries_need_no_new_embedding() {
    let f = setup();
    let early = f.engine.create_node(&f.user, note("Vorher da", "")).unwrap();
    let _indexer = spawn_indexer(&f.engine, f.model.clone()).unwrap();

    let wait = |what: &str, ok: &dyn Fn() -> bool| {
        let started = Instant::now();
        while !ok() {
            assert!(started.elapsed() < Duration::from_secs(15), "timed out: {what}");
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    wait("existing entries", &|| f.engine.index_status().unwrap().indexed == 1);

    let later = f.engine.create_node(&f.user, note("Später dazu", "")).unwrap();
    wait("new entries", &|| f.engine.index_status().unwrap().indexed == 2);

    // Edited text gets a new vector.
    let calls = f.model.calls.load(Ordering::Relaxed);
    f.engine
        .update_node(
            &f.user,
            &later.id,
            NodePatch { body: Some(BodyEdit::Replace("Neuer Text".into())), ..Default::default() },
        )
        .unwrap();
    wait("edit", &|| f.model.calls.load(Ordering::Relaxed) > calls);

    // Trash removes it from the index, undo brings it back without the model.
    f.engine.delete_node(&f.user, &early.id).unwrap();
    assert_eq!(f.engine.index_status().unwrap(), IndexStatus { indexed: 1, total: 1 });
    let cs = f.engine.change_sets(1, None).unwrap().remove(0).id;
    let calls = f.model.calls.load(Ordering::Relaxed);
    f.engine.revert(&f.user, &cs).unwrap();
    assert_eq!(f.engine.index_status().unwrap(), IndexStatus { indexed: 2, total: 2 });
    std::thread::sleep(Duration::from_millis(2800));
    assert_eq!(f.model.calls.load(Ordering::Relaxed), calls, "nothing to re-embed");
}

#[test]
fn merge_moves_everything_and_can_be_undone() {
    let f = setup();
    let p = f
        .engine
        .create_project(&f.user, NewProject { name: "P".into(), ..Default::default() })
        .unwrap();
    let keep = f.add(NewNode { tags: vec!["a".into()], ..note("Tokio", "Laufzeit.") });
    let dup = f.add(NewNode {
        tags: vec!["b".into()],
        projects: vec![p.id.clone()],
        ..note("Tokio Runtime", "Work Stealing.")
    });
    let other = f.add(note("Axum", "Baut auf [[Tokio Runtime]] auf."));
    let third = f.add(note("Hyper", ""));
    f.engine
        .link(
            &f.user,
            NewEdge {
                src: dup.id.clone(),
                dst: third.id.clone(),
                relation: Some("depends_on".into()),
                weight: None,
                note: None,
            },
        )
        .unwrap();
    // Would become a self-link and must disappear.
    f.engine
        .link(
            &f.user,
            NewEdge {
                src: keep.id.clone(),
                dst: dup.id.clone(),
                relation: None,
                weight: None,
                note: None,
            },
        )
        .unwrap();

    let merged = f.engine.merge_nodes(&f.user, &keep.id, std::slice::from_ref(&dup.id)).unwrap();
    assert_eq!(merged.body, "Laufzeit.\n\nWork Stealing.");
    assert_eq!(merged.tags, ["a", "b"]);
    assert!(f.engine.node(&dup.id).unwrap().unwrap().deleted_at.is_some());

    let d = f.engine.node_detail(&keep.id).unwrap();
    assert_eq!(d.projects[0].id, p.id);
    assert!(d.outgoing.iter().any(|e| e.other.id == third.id && e.edge.relation == "depends_on"));
    assert!(d.incoming.iter().any(|e| e.other.id == other.id));
    assert!(d.outgoing.iter().chain(&d.incoming).all(|e| e.other.id != keep.id), "no self links");

    // The former wikilink edge survives later edits of the linking entry.
    f.engine
        .update_node(
            &f.user,
            &other.id,
            NodePatch { title: Some("Axum Framework".into()), ..Default::default() },
        )
        .unwrap();
    f.engine
        .update_node(
            &f.user,
            &other.id,
            NodePatch { body: Some(BodyEdit::Append("Mehr Text.".into())), ..Default::default() },
        )
        .unwrap();
    assert!(
        f.engine.node_detail(&keep.id).unwrap().incoming.iter().any(|e| e.other.id == other.id)
    );

    let merge_cs = f
        .engine
        .change_sets(50, None)
        .unwrap()
        .into_iter()
        .find(|c| c.summary.contains("zusammengeführt"))
        .unwrap();
    // Undo newest first, as the conflict rule demands.
    for cs in f.engine.change_sets(50, None).unwrap() {
        f.engine.revert(&f.user, &cs.id).unwrap();
        if cs.id == merge_cs.id {
            break;
        }
    }
    let restored = f.engine.node_detail(&dup.id).unwrap();
    assert_eq!(restored.projects[0].id, p.id);
    assert_eq!(f.engine.node(&keep.id).unwrap().unwrap().body, "Laufzeit.");
    assert!(restored.outgoing.iter().any(|e| e.other.id == third.id));

    assert!(matches!(
        f.engine.merge_nodes(&f.user, &keep.id, std::slice::from_ref(&keep.id)),
        Err(CoreError::Invalid(_))
    ));
}

#[test]
fn rank_clusters_themes_and_review_lists() {
    let f = setup();
    let ai = Ctx::new(f.engine.ensure_actor(ActorKind::Ai, "KI").unwrap().id);
    let p = f
        .engine
        .create_project(&f.user, NewProject { name: "P".into(), ..Default::default() })
        .unwrap();
    let mk = |title: &str, body: &str| {
        f.engine
            .create_node(&f.user, NewNode { projects: vec![p.id.clone()], ..note(title, body) })
            .unwrap()
    };
    // Two tight groups joined by one bridge. Created in reverse so links resolve.
    mk("Rust", "");
    mk("Tokio", "[[Rust]]");
    mk("Axum", "[[Rust]] [[Tokio]]");
    let hub = mk("Hyper", "[[Rust]] [[Tokio]] [[Axum]]");
    mk("Mehl", "");
    mk("Wasser", "[[Mehl]]");
    mk("Salz", "[[Mehl]] [[Wasser]]");
    mk("Brot", "[[Mehl]] [[Wasser]] [[Salz]] [[Hyper]]");
    let lonely = mk("Einsam", "");

    let snap = f.engine.graph_snapshot(Some(&p.id)).unwrap();
    let node = |t: &str| snap.nodes.iter().find(|n| n.title == t).unwrap();
    assert_eq!(node("Rust").cluster, node("Tokio").cluster);
    assert_eq!(node("Mehl").cluster, node("Brot").cluster);
    assert_ne!(node("Rust").cluster, node("Mehl").cluster);
    assert_ne!(node("Einsam").cluster, node("Rust").cluster);
    assert!(node("Hyper").rank > node("Einsam").rank);

    let o = f.engine.project_overview(&p.id).unwrap();
    assert_eq!(o.themes.len(), 2);
    assert!(o.hubs[0].1 >= 3);
    let _ = hub;

    assert_eq!(f.engine.orphans(10).unwrap()[0].id, lonely.id);
    let written = f.engine.create_node(&ai, note("Von der KI", "")).unwrap();
    assert_eq!(f.engine.unreviewed(10).unwrap()[0].id, written.id);
}

#[test]
fn common_words_do_not_drown_out_rare_ones() {
    let f = setup();
    for i in 0..20 {
        f.engine.create_node(&f.user, note(&format!("Notiz {i}"), "haeufig wort")).unwrap();
    }
    f.engine.create_node(&f.user, note("Besonders", "selten haeufig")).unwrap();
    f.engine.create_node(&f.user, note("Nur selten", "selten")).unwrap();

    // The common word is dropped, so the other rare match is found too.
    let hits = f.engine.search(&SearchQuery::new("haeufig selten")).unwrap();
    let mut got = titles(&hits);
    got.sort();
    assert_eq!(got, ["Besonders", "Nur selten"]);

    // A rare word nobody has means no results, not "everything with haeufig".
    assert!(f.engine.search(&SearchQuery::new("haeufig gibtsnicht")).unwrap().is_empty());
    // Only common words: the rarest of them decides.
    assert_eq!(f.engine.search(&SearchQuery::new("haeufig wort")).unwrap().len(), 20);
}
