//! Favorites, tag counts and what filters need from the graph.

use bm_core::{Engine, Vault, model::*, vault::KdfParams};

fn setup() -> (tempfile::TempDir, Engine, Ctx) {
    let dir = tempfile::tempdir().unwrap();
    let (vault, keys, _) = Vault::create(dir.path(), "pw", KdfParams::insecure_fast()).unwrap();
    let e = Engine::open(&vault.db_path(), &keys.db_key(), 2).unwrap();
    let user = Ctx::new(e.ensure_actor(ActorKind::User, "Nutzer").unwrap().id);
    (dir, e, user)
}

fn note(e: &Engine, ctx: &Ctx, title: &str, tags: &[&str], kind: Option<&str>) -> Node {
    let input = NewNode {
        title: title.into(),
        tags: tags.iter().map(|t| t.to_string()).collect(),
        kind: kind.map(str::to_string),
        ..Default::default()
    };
    e.create_node(ctx, input).unwrap()
}

fn favorite_titles(e: &Engine) -> Vec<String> {
    e.favorites().unwrap().into_iter().map(|n| n.title).collect()
}

#[test]
fn favorites_follow_their_entries() {
    let (_dir, e, user) = setup();
    let b = note(&e, &user, "Beta", &[], None);
    let a = note(&e, &user, "alpha", &[], None);
    e.set_favorite(&b.id, true).unwrap();
    e.set_favorite(&a.id, true).unwrap();
    e.set_favorite(&a.id, true).unwrap();
    assert_eq!(favorite_titles(&e), ["alpha", "Beta"]);

    e.set_favorite(&a.id, false).unwrap();
    assert_eq!(favorite_titles(&e), ["Beta"]);

    e.delete_node(&user, &b.id).unwrap();
    assert!(favorite_titles(&e).is_empty(), "the trash hides them");
    assert!(e.set_favorite(&b.id, true).is_err(), "a trashed entry cannot become one");
    e.restore_node(&user, &b.id).unwrap();
    assert_eq!(favorite_titles(&e), ["Beta"], "and restoring brings them back");

    e.delete_node(&user, &b.id).unwrap();
    e.purge_node(&user, &b.id).unwrap();
    assert!(favorite_titles(&e).is_empty());
}

#[test]
fn tags_are_counted_over_graph_entries_only() {
    let (_dir, e, user) = setup();
    note(&e, &user, "A", &["rust", "wichtig"], None);
    let b = note(&e, &user, "B", &["rust"], None);
    note(&e, &user, "Vorlage", &["rust"], Some(TEMPLATE_KIND));
    let counts = |e: &Engine| {
        e.tag_counts().unwrap().into_iter().map(|t| (t.tag, t.count)).collect::<Vec<_>>()
    };
    assert_eq!(counts(&e), [("rust".to_string(), 2), ("wichtig".to_string(), 1)]);

    e.delete_node(&user, &b.id).unwrap();
    assert_eq!(counts(&e), [("rust".to_string(), 1), ("wichtig".to_string(), 1)]);
}

#[test]
fn the_graph_carries_what_filters_need() {
    let (_dir, e, user) = setup();
    note(&e, &user, "A", &["zwei", "eins"], Some("task"));
    let snap = e.graph_snapshot(None).unwrap();
    assert_eq!(snap.nodes[0].tags, ["eins", "zwei"]);
    assert_eq!(snap.nodes[0].kind, "task");

    let ai = e.ensure_actor(ActorKind::Ai, "KI").unwrap();
    let names: Vec<_> = e.actors().unwrap().into_iter().map(|a| a.name).collect();
    assert_eq!(names, ["Nutzer", "KI"]);
    assert_eq!(snap.nodes[0].created_by, e.actors().unwrap()[0].id);
    assert_ne!(ai.id, snap.nodes[0].created_by);
}
