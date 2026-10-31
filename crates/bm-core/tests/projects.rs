//! Nested projects: any depth, in order, and a view shows its whole subtree.

use bm_core::{Engine, Vault, ids::ProjectId, model::*, vault::KdfParams};

fn setup() -> (tempfile::TempDir, Engine, Ctx) {
    let dir = tempfile::tempdir().unwrap();
    let (vault, keys, _) = Vault::create(dir.path(), "pw", KdfParams::insecure_fast()).unwrap();
    let engine = Engine::open(&vault.db_path(), &keys.db_key(), 2).unwrap();
    let user = Ctx::new(engine.ensure_actor(ActorKind::User, "Nutzer").unwrap().id);
    (dir, engine, user)
}

fn project(e: &Engine, ctx: &Ctx, name: &str, parent: Option<&ProjectId>) -> ProjectId {
    let input = NewProject { name: name.into(), parent: parent.cloned(), ..Default::default() };
    e.create_project(ctx, input).unwrap().id
}

#[test]
fn projects_nest_to_any_depth_and_views_include_everything_below() {
    let (_dir, e, ctx) = setup();
    let mut chain = vec![project(&e, &ctx, "Ebene 0", None)];
    for level in 1..6 {
        let parent = chain.last().unwrap().clone();
        chain.push(project(&e, &ctx, &format!("Ebene {level}"), Some(&parent)));
    }
    let other = project(&e, &ctx, "Anderes", None);

    let deepest = e
        .create_node(
            &ctx,
            NewNode {
                title: "Ganz unten".into(),
                projects: vec![chain[5].clone()],
                ..Default::default()
            },
        )
        .unwrap();

    for (level, id) in chain.iter().enumerate() {
        let view = e.graph_snapshot(Some(id)).unwrap();
        let ids: Vec<_> = view.nodes.iter().map(|n| &n.id).collect();
        assert_eq!(ids, [&deepest.id], "level {level} shows what is below it");
    }
    assert!(e.graph_snapshot(Some(&other)).unwrap().nodes.is_empty());

    // Parents come directly before their children, which the sidebar relies on.
    let names: Vec<_> = e.projects().unwrap().into_iter().map(|p| p.project.name).collect();
    let position = |n: &str| names.iter().position(|x| x == n).unwrap();
    for level in 1..6 {
        assert!(position(&format!("Ebene {}", level - 1)) < position(&format!("Ebene {level}")));
    }
}
