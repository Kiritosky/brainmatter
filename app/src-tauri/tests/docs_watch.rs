//! Mirrored documentation follows its files while the vault is open, and stops when
//! it locks.

use std::{
    fs,
    sync::Arc,
    time::{Duration, Instant},
};

use bm_core::{model::NewProject, vault::KdfParams};
use brainmatter::session::{Emit, Session};

const PASSWORD: &str = "lang-genug";

fn pages(session: &Session, project: &bm_core::ids::ProjectId) -> Vec<String> {
    let (engine, _) = session.engine().unwrap();
    let mut titles: Vec<String> =
        engine.doc_tree(project).unwrap().into_iter().map(|p| p.node.title).collect();
    titles.sort();
    titles
}

fn wait_for(what: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if what() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn a_save_in_the_folder_shows_up_without_asking() {
    let dir = tempfile::tempdir().unwrap();
    let emit: Emit = Arc::new(|_, _| {});
    let session = Session::new(dir.path().to_owned(), emit, KdfParams::insecure_fast());
    session.create(PASSWORD, "Lasse").unwrap();
    let docs = tempfile::tempdir().unwrap();
    fs::write(docs.path().join("README.md"), "# Start\nHallo").unwrap();

    let project = {
        let (engine, ctx) = session.engine().unwrap();
        let input = NewProject { name: "Repo".into(), ..Default::default() };
        let project = engine.create_project(&ctx, input).unwrap().id;
        let path = docs.path().to_str().unwrap();
        let id = engine.add_doc_source(&ctx, &project, path, None).unwrap();
        engine.sync_doc_source(&ctx, &id, docs.path()).unwrap();
        project
    };
    session.watch_docs();
    assert_eq!(pages(&session, &project), ["Start"]);

    fs::write(docs.path().join("Plan.md"), "# Plan\nErst das").unwrap();
    assert!(wait_for(|| pages(&session, &project) == ["Plan", "Start"]), "a new file");
    fs::write(docs.path().join("Plan.md"), "# Plan B\nDann das").unwrap();
    assert!(wait_for(|| pages(&session, &project) == ["Plan B", "Start"]), "a changed file");
    // Build output and the like stay out of it.
    fs::create_dir_all(docs.path().join("target")).unwrap();
    fs::write(docs.path().join("target/x.md"), "# Gebaut").unwrap();
    fs::remove_file(docs.path().join("Plan.md")).unwrap();
    assert!(wait_for(|| pages(&session, &project) == ["Start"]), "a removed file");
    {
        // Syncs of one source close together are one change, as edits of one entry are.
        let (engine, _) = session.engine().unwrap();
        let changes = engine.change_sets(10, None).unwrap();
        let synced = changes.iter().filter(|c| c.summary.starts_with("Doku aus")).count();
        assert_eq!(synced, 1, "{changes:?}");
    }

    session.lock();
    fs::write(docs.path().join("Gesperrt.md"), "# Gesperrt").unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    session.unlock_password(PASSWORD).unwrap();
    assert_eq!(pages(&session, &project), ["Start"], "nothing is read while locked");
    // After unlocking, it watches again.
    fs::write(docs.path().join("Gesperrt.md"), "# Wieder da").unwrap();
    assert!(wait_for(|| pages(&session, &project) == ["Start", "Wieder da"]));
}
