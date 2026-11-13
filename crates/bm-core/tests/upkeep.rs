//! Upkeep: reads, paths that are out of date, and links to entries that do not exist.

use std::{fs, path::Path, thread::sleep, time::Duration};

use bm_core::{Engine, Vault, ids::ProjectId, model::*, time::now_ms, vault::KdfParams};

fn setup() -> (tempfile::TempDir, Engine, Ctx) {
    let dir = tempfile::tempdir().unwrap();
    let (vault, keys, _) = Vault::create(dir.path(), "pw", KdfParams::insecure_fast()).unwrap();
    let e = Engine::open(&vault.db_path(), &keys.db_key(), 2).unwrap();
    let user = Ctx::new(e.ensure_actor(ActorKind::User, "Nutzer").unwrap().id);
    (dir, e, user)
}

fn note(e: &Engine, ctx: &Ctx, title: &str, body: &str, projects: &[&ProjectId]) -> Node {
    let input = NewNode {
        title: title.into(),
        body: body.into(),
        projects: projects.iter().map(|p| (*p).clone()).collect(),
        ..Default::default()
    };
    e.create_node(ctx, input).unwrap()
}

fn project(e: &Engine, ctx: &Ctx, name: &str, parent: Option<&ProjectId>) -> ProjectId {
    let input = NewProject {
        name: name.into(),
        parent: parent.cloned(),
        description: String::new(),
        color: None,
        icon: None,
    };
    e.create_project(ctx, input).unwrap().id
}

fn titles(nodes: &[NodeBrief]) -> Vec<&str> {
    nodes.iter().map(|n| n.title.as_str()).collect()
}

fn write(root: &Path, path: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "").unwrap();
}

#[test]
fn reads_count_once_per_reader_and_are_no_change() {
    let (_dir, e, user) = setup();
    let ai = e.ensure_actor(ActorKind::Ai, "Claude Code").unwrap();
    let n = note(&e, &user, "Gelesen", "", &[]);
    assert_eq!(e.node_detail(&n.id).unwrap().reads, None);
    let changes = e.change_sets(50, None).unwrap().len();

    e.note_read(&n.id, &ai.id).unwrap();
    e.note_read(&n.id, &ai.id).unwrap();
    let reads = e.node_detail(&n.id).unwrap().reads.unwrap();
    assert_eq!((reads.count, reads.last_by.as_str()), (1, "Claude Code"));

    e.note_read(&n.id, &user.actor).unwrap();
    let reads = e.node_detail(&n.id).unwrap().reads.unwrap();
    assert_eq!((reads.count, reads.last_by.as_str()), (2, "Nutzer"));
    assert_eq!(e.change_sets(50, None).unwrap().len(), changes, "reading is no change");

    // Counted entries can still be erased for good.
    e.delete_node(&user, &n.id).unwrap();
    e.purge_node(&user, &n.id).unwrap();
}

#[test]
fn unread_entries_count_only_from_when_reads_are_counted() {
    let (_dir, e, user) = setup();
    let ai = e.ensure_actor(ActorKind::Ai, "KI").unwrap();
    let read = note(&e, &user, "Gelesen", "", &[]);
    note(&e, &user, "Ungelesen", "", &[]);
    note(&e, &user, "Ein Skill", "", &[]);
    let skill = e.resolve_node("Ein Skill").unwrap();
    e.update_node(&user, &skill, NodePatch { kind: Some("skill".into()), ..Default::default() })
        .unwrap();
    sleep(Duration::from_millis(5));
    e.note_read(&read.id, &ai.id).unwrap();

    let since = e.reads_since().unwrap();
    assert!(since > 0 && since <= now_ms());
    assert!(e.unread_since(since, 50).unwrap().is_empty(), "nothing before counting began");

    // Everything is older than a cutoff in the future; the read one came last.
    let later = now_ms() + 60_000;
    assert_eq!(titles(&e.unread_since(later, 50).unwrap()), ["Ungelesen", "Gelesen"]);
    assert_eq!(titles(&e.unread_since(later, 1).unwrap()), ["Ungelesen"]);
}

#[test]
fn paths_are_checked_inside_the_projects_folder() {
    let (dir, e, user) = setup();
    let repo = dir.path().join("repo");
    write(&repo, "src/kept.rs");
    let code = project(&e, &user, "Code", None);
    let sub = project(&e, &user, "Teil", Some(&code));
    let other = project(&e, &user, "Anderes", None);
    let body = "Steht in `src/kept.rs`, früher in src/gone.rs.";
    let inside = note(&e, &user, "Im Ordner", body, &[&sub]);
    let outside = note(&e, &user, "Ohne Ordner", body, &[&other]);
    assert!(e.node_detail(&inside.id).unwrap().path_issues.is_empty(), "no folder yet");

    assert!(e.set_project_folder(&code, Some("relativ/pfad")).is_err());
    e.set_project_folder(&code, Some(repo.to_str().unwrap())).unwrap();
    let folder = e.projects().unwrap().into_iter().find(|p| p.project.id == code).unwrap().folder;
    assert_eq!(folder.as_deref(), repo.to_str());

    let gone = PathIssue { path: "src/gone.rs".into(), missing: true };
    assert_eq!(e.node_detail(&inside.id).unwrap().path_issues, std::slice::from_ref(&gone));
    assert!(e.node_detail(&outside.id).unwrap().path_issues.is_empty());

    // A file changed after the entry counts as changed.
    sleep(Duration::from_millis(20));
    write(&repo, "src/kept.rs");
    let changed = PathIssue { path: "src/kept.rs".into(), missing: false };
    let outdated = e.outdated(50).unwrap();
    assert_eq!(outdated.len(), 1);
    assert_eq!(outdated[0].node.title, "Im Ordner");
    assert_eq!(outdated[0].issues, [changed.clone(), gone.clone()]);

    // Confirmed as current: gone files no longer count, only later changes do.
    e.confirm_current(&inside.id).unwrap();
    assert!(e.outdated(50).unwrap().is_empty());
    sleep(Duration::from_millis(20));
    write(&repo, "src/kept.rs");
    assert_eq!(e.node_detail(&inside.id).unwrap().path_issues, [changed]);

    // Editing the entry makes gone files count again.
    let patch = NodePatch { body: Some(BodyEdit::Append(" Neu.".into())), ..Default::default() };
    e.update_node(&user, &inside.id, patch).unwrap();
    assert_eq!(e.node_detail(&inside.id).unwrap().path_issues, [gone]);

    e.set_project_folder(&code, None).unwrap();
    assert!(e.outdated(50).unwrap().is_empty());
}

#[test]
fn links_to_missing_entries_are_listed_until_one_exists() {
    let (_dir, e, user) = setup();
    note(&e, &user, "Da", "", &[]);
    note(&e, &user, "Erster", "Siehe [[Noch offen]], [[noch offen|alias]] und [[Da]].", &[]);
    note(&e, &user, "Zweiter", "Auch [[Noch offen]] und [[Einmal]]. `[[Code]]`", &[]);

    let missing = e.missing_links(50).unwrap();
    let found: Vec<(&str, Vec<&str>)> =
        missing.iter().map(|m| (m.title.as_str(), titles(&m.sources))).collect();
    assert_eq!(found, [("Noch offen", vec!["Erster", "Zweiter"]), ("Einmal", vec!["Zweiter"])]);
    assert_eq!(e.missing_links(1).unwrap().len(), 1);

    let created = note(&e, &user, "noch offen", "", &[]);
    assert_eq!(
        titles(
            &e.missing_links(50)
                .unwrap()
                .iter()
                .flat_map(|m| m.sources.clone())
                .collect::<Vec<_>>()
        ),
        ["Zweiter"]
    );
    assert_eq!(e.node_detail(&created.id).unwrap().incoming.len(), 2, "links filled in");
}
