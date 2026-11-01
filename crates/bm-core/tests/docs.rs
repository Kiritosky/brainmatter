//! Project documentation: the page tree, and mirroring folders read-only.

use std::fs;

use bm_core::{CoreError, Engine, Vault, ids::ProjectId, model::*, vault::KdfParams};

struct Fixture {
    _dir: tempfile::TempDir,
    engine: Engine,
    user: Ctx,
    project: ProjectId,
}

fn setup() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let (vault, keys, _) = Vault::create(dir.path(), "pw", KdfParams::insecure_fast()).unwrap();
    let engine = Engine::open(&vault.db_path(), &keys.db_key(), 2).unwrap();
    let user = Ctx::new(engine.ensure_actor(ActorKind::User, "Nutzer").unwrap().id);
    let input = NewProject { name: "P".into(), ..Default::default() };
    let project = engine.create_project(&user, input).unwrap().id;
    Fixture { _dir: dir, engine, user, project }
}

impl Fixture {
    fn page(&self, title: &str, parent: Option<&Node>) -> Node {
        self.engine
            .create_doc_page(
                &self.user,
                &self.project,
                title,
                "Text",
                parent.map(|p| &p.id),
                vec![],
            )
            .unwrap()
    }

    /// `(title, depth)` in reading order.
    fn outline(&self) -> Vec<(String, u32)> {
        let tree = self.engine.doc_tree(&self.project).unwrap();
        tree.into_iter().map(|p| (p.node.title, p.depth)).collect()
    }
}

fn expected(items: &[(&str, u32)]) -> Vec<(String, u32)> {
    items.iter().map(|(t, d)| (t.to_string(), *d)).collect()
}

#[test]
fn pages_form_a_tree_in_reading_order() {
    let f = setup();
    let a = f.page("Einleitung", None);
    let b = f.page("Anleitung", None);
    f.page("Schritt 1", Some(&b));
    let two = f.page("Schritt 2", Some(&b));
    f.page("Detail", Some(&two));
    f.page("Unter Einleitung", Some(&a));
    assert_eq!(
        f.outline(),
        expected(&[
            ("Einleitung", 0),
            ("Unter Einleitung", 1),
            ("Anleitung", 0),
            ("Schritt 1", 1),
            ("Schritt 2", 1),
            ("Detail", 2),
        ])
    );
}

#[test]
fn pages_can_be_reordered_and_moved_but_not_into_themselves() {
    let f = setup();
    let a = f.page("A", None);
    let b = f.page("B", None);
    let c = f.page("C", Some(&b));

    f.engine.place_doc_page(&f.user, &b.id, None, 0).unwrap();
    assert_eq!(f.outline(), expected(&[("B", 0), ("C", 1), ("A", 0)]));

    f.engine.place_doc_page(&f.user, &a.id, Some(&b.id), 0).unwrap();
    assert_eq!(f.outline(), expected(&[("B", 0), ("A", 1), ("C", 1)]));

    let cycle = f.engine.place_doc_page(&f.user, &b.id, Some(&c.id), 0);
    assert!(matches!(cycle, Err(CoreError::Invalid(_))));
    let own = f.engine.place_doc_page(&f.user, &b.id, Some(&b.id), 0);
    assert!(matches!(own, Err(CoreError::Invalid(_))));
    assert_eq!(f.outline(), expected(&[("B", 0), ("A", 1), ("C", 1)]));
}

#[test]
fn a_parent_must_be_in_the_same_project() {
    let f = setup();
    let other = f
        .engine
        .create_project(&f.user, NewProject { name: "Q".into(), ..Default::default() })
        .unwrap()
        .id;
    let foreign = f.engine.create_doc_page(&f.user, &other, "Fremd", "", None, vec![]).unwrap();
    let result = f.engine.create_doc_page(&f.user, &f.project, "X", "", Some(&foreign.id), vec![]);
    assert!(matches!(result, Err(CoreError::Invalid(_))));
    let note = f
        .engine
        .create_node(&f.user, NewNode { title: "Notiz".into(), ..Default::default() })
        .unwrap();
    let result = f.engine.create_doc_page(&f.user, &f.project, "Y", "", Some(&note.id), vec![]);
    assert!(matches!(result, Err(CoreError::Invalid(_))), "a parent must be a page");
}

#[test]
fn deleted_parents_leave_their_children_readable_and_undo_restores_them() {
    let f = setup();
    let parent = f.page("Eltern", None);
    f.page("Kind", Some(&parent));
    f.engine.delete_node(&f.user, &parent.id).unwrap();
    assert_eq!(f.outline(), expected(&[("Kind", 0)]));
    f.engine.restore_node(&f.user, &parent.id).unwrap();
    assert_eq!(f.outline(), expected(&[("Eltern", 0), ("Kind", 1)]));

    f.engine.delete_node(&f.user, &parent.id).unwrap();
    f.engine.purge_node(&f.user, &parent.id).unwrap();
    assert_eq!(f.outline(), expected(&[("Kind", 0)]));
}

#[test]
fn pages_stay_out_of_the_graph_but_can_be_searched() {
    let f = setup();
    f.engine
        .create_doc_page(&f.user, &f.project, "Handbuch", "einzigartigeswort", None, vec![])
        .unwrap();
    assert!(f.engine.graph_snapshot(None).unwrap().nodes.is_empty());
    assert!(f.engine.orphans(10).unwrap().is_empty());
    assert_eq!(f.engine.projects().unwrap()[0].node_count, 0, "the count means graph entries");
    let q = SearchQuery {
        text: "einzigartigeswort".into(),
        project: Some(f.project.clone()),
        include_subprojects: true,
        kinds: vec![DOC_KIND.to_string()],
        tags: vec![],
        limit: 5,
    };
    assert_eq!(f.engine.search(&q).unwrap()[0].node.title, "Handbuch");
}

fn write(root: &std::path::Path, path: &str, text: &str) {
    let full = root.join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, text).unwrap();
}

#[test]
fn a_mirrored_folder_follows_its_files() {
    let f = setup();
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "README.md", "# Start\nErste Fassung");
    write(folder.path(), "guide/a.md", "# Kapitel A");
    let path = folder.path().to_str().unwrap();
    let source = f.engine.add_doc_source(&f.user, &f.project, path, None).unwrap();

    let first = f.engine.sync_doc_source(&f.user, &source, folder.path()).unwrap();
    assert_eq!((first.added, first.updated, first.removed), (3, 0, 0));
    assert_eq!(f.outline(), expected(&[("Start", 0), ("guide", 0), ("Kapitel A", 1)]));
    assert_eq!(f.engine.doc_sources(&f.project).unwrap()[0].pages, 3);

    // Nothing changed: no work, and no noise in the history.
    let before = f.engine.change_sets(100, None).unwrap().len();
    let again = f.engine.sync_doc_source(&f.user, &source, folder.path()).unwrap();
    assert_eq!((again.added, again.updated, again.removed, again.unchanged), (0, 0, 0, 3));
    assert_eq!(f.engine.change_sets(100, None).unwrap().len(), before);

    write(folder.path(), "README.md", "# Neuer Titel\nZweite Fassung");
    fs::remove_file(folder.path().join("guide/a.md")).unwrap();
    write(folder.path(), "neu.md", "# Neu");
    let changed = f.engine.sync_doc_source(&f.user, &source, folder.path()).unwrap();
    assert_eq!((changed.added, changed.updated), (1, 1));
    assert_eq!(changed.removed, 2, "the file and its folder page");
    assert_eq!(f.outline(), expected(&[("Neuer Titel", 0), ("Neu", 0)]));

    // A file that returns gets its page back, not a second one.
    write(folder.path(), "guide/a.md", "# Kapitel A wieder");
    let back = f.engine.sync_doc_source(&f.user, &source, folder.path()).unwrap();
    assert_eq!(back.added, 2);
    let titles: Vec<_> = f.outline().into_iter().map(|(t, _)| t).collect();
    assert_eq!(titles.iter().filter(|t| t.contains("Kapitel A")).count(), 1, "{titles:?}");
}

#[test]
fn mirrored_pages_cannot_be_changed_here() {
    let f = setup();
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "a.md", "# Seite");
    let path = folder.path().to_str().unwrap();
    let source = f.engine.add_doc_source(&f.user, &f.project, path, None).unwrap();
    f.engine.sync_doc_source(&f.user, &source, folder.path()).unwrap();
    let page = f.engine.doc_tree(&f.project).unwrap().remove(0);
    let id = page.node.id;
    assert!(page.read_only);
    assert!(f.engine.node_detail(&id).unwrap().read_only);

    let edit = NodePatch { body: Some(BodyEdit::Replace("x".into())), ..Default::default() };
    assert!(matches!(f.engine.update_node(&f.user, &id, edit), Err(CoreError::Conflict(_))));
    assert!(matches!(f.engine.delete_node(&f.user, &id), Err(CoreError::Conflict(_))));
    let other = f.page("Andere", None);
    let merge = f.engine.merge_nodes(&f.user, &other.id, std::slice::from_ref(&id));
    assert!(matches!(merge, Err(CoreError::Conflict(_))));
    assert!(f.engine.place_doc_page(&f.user, &id, None, 0).is_err());

    // Marking it as read is no change to its content.
    let seen = NodePatch { reviewed: Some(true), ..Default::default() };
    f.engine.update_node(&f.user, &id, seen).unwrap();
}

#[test]
fn an_unreachable_source_leaves_its_pages_alone() {
    let f = setup();
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "a.md", "# Seite");
    let path = folder.path().to_str().unwrap();
    let source = f.engine.add_doc_source(&f.user, &f.project, path, None).unwrap();
    f.engine.sync_doc_source(&f.user, &source, folder.path()).unwrap();

    let gone = folder.path().join("gibt-es-nicht");
    assert!(f.engine.sync_doc_source(&f.user, &source, &gone).is_err());
    assert_eq!(f.outline().len(), 1, "a disconnected drive must not erase the docs");
}

#[test]
fn removing_a_source_trashes_its_pages_and_keeps_the_files() {
    let f = setup();
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "a.md", "# Seite");
    let path = folder.path().to_str().unwrap();
    let source = f.engine.add_doc_source(&f.user, &f.project, path, None).unwrap();
    assert_eq!(f.engine.add_doc_source(&f.user, &f.project, path, None).unwrap(), source);
    f.engine.sync_doc_source(&f.user, &source, folder.path()).unwrap();

    f.engine.remove_doc_source(&f.user, &source).unwrap();
    assert!(f.outline().is_empty());
    assert!(f.engine.doc_sources(&f.project).unwrap().is_empty());
    assert!(folder.path().join("a.md").exists());
    assert!(f.engine.sync_doc_source(&f.user, &source, folder.path()).is_err());
}

#[test]
fn pages_carry_their_tags() {
    let f = setup();
    let tags = vec!["setup".to_string(), "api".to_string()];
    f.engine.create_doc_page(&f.user, &f.project, "Tagged", "Text", None, tags).unwrap();
    f.page("Plain", None);
    let tree = f.engine.doc_tree(&f.project).unwrap();
    let tagged = tree.iter().find(|p| p.node.title == "Tagged").unwrap();
    assert_eq!(tagged.tags, ["api", "setup"]);
    assert!(tree.iter().find(|p| p.node.title == "Plain").unwrap().tags.is_empty());
}

#[test]
fn a_mirrored_page_can_be_tagged_and_keeps_its_tags_through_a_sync() {
    let f = setup();
    let folder = tempfile::tempdir().unwrap();
    write(folder.path(), "guide.md", "# Guide\n\nAlt");
    let path = folder.path().to_str().unwrap();
    let source = f.engine.add_doc_source(&f.user, &f.project, path, None).unwrap();
    f.engine.sync_doc_source(&f.user, &source, folder.path()).unwrap();
    let id = f.engine.doc_tree(&f.project).unwrap()[0].node.id.clone();

    let patch = NodePatch { tags_add: vec!["handbuch".into()], ..Default::default() };
    f.engine.update_node(&f.user, &id, patch).unwrap();

    write(folder.path(), "guide.md", "# Guide\n\nNeu");
    f.engine.sync_doc_source(&f.user, &source, folder.path()).unwrap();
    let page = f.engine.doc_tree(&f.project).unwrap().remove(0);
    assert_eq!(page.tags, ["handbuch"]);
    assert!(f.engine.node_detail(&id).unwrap().node.body.contains("Neu"));
}
