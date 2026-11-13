//! A project's context as Markdown for chat apps without MCP.

use bm_core::{Engine, Vault, model::*, vault::KdfParams};
use brainmatter::context::{MAX_CHARS, project_context};

fn setup() -> (tempfile::TempDir, Engine, Ctx) {
    let dir = tempfile::tempdir().unwrap();
    let (vault, keys, _) = Vault::create(dir.path(), "pw", KdfParams::insecure_fast()).unwrap();
    let e = Engine::open(&vault.db_path(), &keys.db_key(), 2).unwrap();
    let user = Ctx::new(e.ensure_actor(ActorKind::User, "Nutzer").unwrap().id);
    (dir, e, user)
}

fn project(e: &Engine, ctx: &Ctx, name: &str) -> bm_core::ids::ProjectId {
    let input = NewProject {
        name: name.into(),
        parent: None,
        description: "Eine Mac-App für Wissen.".into(),
        color: None,
        icon: None,
    };
    e.create_project(ctx, input).unwrap().id
}

fn note(e: &Engine, ctx: &Ctx, p: &bm_core::ids::ProjectId, title: &str, body: &str, kind: &str) {
    let input = NewNode {
        title: title.into(),
        body: body.into(),
        kind: Some(kind.into()),
        projects: vec![p.clone()],
        ..Default::default()
    };
    e.create_node(ctx, input).unwrap();
}

#[test]
fn holds_description_entries_and_tasks() {
    let (_dir, e, user) = setup();
    let p = project(&e, &user, "Brainmatter");
    note(&e, &user, &p, "Aufbau", "## Kern\nRust und [[Oberfläche]].", "concept");
    note(&e, &user, &p, "Oberfläche", "Svelte.", "concept");
    note(&e, &user, &p, "Release bauen", "", "task");

    let text = project_context(&e, &p).unwrap();
    assert!(text.starts_with("# Brainmatter\n"), "{text}");
    assert!(text.contains("Eine Mac-App für Wissen."));
    assert!(text.contains("### Aufbau (concept)\n\n#### Kern\nRust und [[Oberfläche]]."), "{text}");
    assert!(text.contains("- Release bauen"), "{text}");
}

#[test]
fn stays_within_its_length() {
    let (_dir, e, user) = setup();
    let p = project(&e, &user, "Groß");
    let long = "Wort ".repeat(MAX_CHARS / 10);
    for i in 0..8 {
        note(&e, &user, &p, &format!("Eintrag {i}"), &long, "note");
    }
    let text = project_context(&e, &p).unwrap();
    assert!(text.len() <= MAX_CHARS + 200, "{} chars", text.len());
    // Each entry is half the limit: one fits beside the header, the rest is named.
    assert_eq!(text.matches("### Eintrag").count(), 1);
    assert!(text.contains("_7 "), "names what was left out: {}", &text[text.len() - 120..]);
}
