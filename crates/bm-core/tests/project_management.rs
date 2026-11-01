//! Renaming, moving and deleting projects.

use bm_core::{CoreError, Engine, Vault, ids::ProjectId, model::*, vault::KdfParams};

struct Fixture {
    _dir: tempfile::TempDir,
    e: Engine,
    user: Ctx,
}

fn setup() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let (vault, keys, _) = Vault::create(dir.path(), "pw", KdfParams::insecure_fast()).unwrap();
    let e = Engine::open(&vault.db_path(), &keys.db_key(), 2).unwrap();
    let user = Ctx::new(e.ensure_actor(ActorKind::User, "Nutzer").unwrap().id);
    Fixture { _dir: dir, e, user }
}

impl Fixture {
    fn project(&self, name: &str, parent: Option<&ProjectId>) -> ProjectId {
        let input = NewProject { name: name.into(), parent: parent.cloned(), ..Default::default() };
        self.e.create_project(&self.user, input).unwrap().id
    }

    fn entry(&self, title: &str, kind: &str, projects: &[&ProjectId]) -> Node {
        let input = NewNode {
            title: title.into(),
            kind: Some(kind.into()),
            projects: projects.iter().map(|p| (*p).clone()).collect(),
            ..Default::default()
        };
        self.e.create_node(&self.user, input).unwrap()
    }

    /// `name` indented by depth, in sidebar order.
    fn tree(&self) -> Vec<String> {
        let all = self.e.projects().unwrap();
        let depth = |mut p: Option<ProjectId>| {
            let mut d = 0;
            while let Some(id) = p {
                p = all
                    .iter()
                    .find(|o| o.project.id == id)
                    .and_then(|o| o.project.parent_id.clone());
                d += 1;
            }
            d
        };
        all.iter()
            .map(|p| {
                format!("{}{}", " ".repeat(depth(p.project.parent_id.clone())), p.project.name)
            })
            .collect()
    }

    fn undo_last(&self) {
        let last = self.e.change_sets(1, None).unwrap()[0].id.clone();
        self.e.revert(&self.user, &last).unwrap();
    }

    fn graph_titles(&self) -> Vec<String> {
        let mut t: Vec<_> =
            self.e.graph_snapshot(None).unwrap().nodes.into_iter().map(|n| n.title).collect();
        t.sort();
        t
    }

    fn trash_titles(&self) -> Vec<String> {
        let mut t: Vec<_> = self.e.trash(50).unwrap().into_iter().map(|n| n.node.title).collect();
        t.sort();
        t
    }
}

#[test]
fn renaming_can_be_undone_and_needs_a_name() {
    let f = setup();
    let p = f.project("Alt", None);
    f.e.update_project(&f.user, &p, Some(" Neu "), None).unwrap();
    assert_eq!(f.tree(), ["Neu"]);
    assert!(matches!(
        f.e.update_project(&f.user, &p, Some("  "), None),
        Err(CoreError::Invalid(_))
    ));
    f.undo_last();
    assert_eq!(f.tree(), ["Alt"]);
}

#[test]
fn a_project_moves_with_its_subprojects() {
    let f = setup();
    let a = f.project("A", None);
    let b = f.project("B", None);
    let child = f.project("B1", Some(&b));
    f.project("B2", Some(&child));

    f.e.place_project(&f.user, &b, Some(&a), 0).unwrap();
    assert_eq!(f.tree(), ["A", " B", "  B1", "   B2"]);

    f.undo_last();
    assert_eq!(f.tree(), ["A", "B", " B1", "  B2"]);

    // Back out to the top level, at a chosen position.
    f.e.place_project(&f.user, &child, None, 0).unwrap();
    assert_eq!(f.tree(), ["B1", " B2", "A", "B"]);
}

#[test]
fn siblings_can_be_reordered() {
    let f = setup();
    let a = f.project("A", None);
    f.project("B", None);
    f.project("C", None);
    f.e.place_project(&f.user, &a, None, 2).unwrap();
    assert_eq!(f.tree(), ["B", "C", "A"]);
    f.e.place_project(&f.user, &a, None, 1).unwrap();
    assert_eq!(f.tree(), ["B", "A", "C"]);
    f.e.place_project(&f.user, &a, None, 99).unwrap();
    assert_eq!(f.tree(), ["B", "C", "A"]);
}

#[test]
fn a_project_cannot_go_inside_itself() {
    let f = setup();
    let a = f.project("A", None);
    let b = f.project("B", Some(&a));
    let c = f.project("C", Some(&b));
    for target in [&a, &b, &c] {
        assert!(matches!(
            f.e.place_project(&f.user, &a, Some(target), 0),
            Err(CoreError::Invalid(_))
        ));
    }
    assert_eq!(f.tree(), ["A", " B", "  C"]);
}

#[test]
fn deleting_keeps_entries_unless_asked_and_undo_restores_everything() {
    let f = setup();
    let keep = f.project("Bleibt", None);
    let gone = f.project("Weg", None);
    let sub = f.project("Unter", Some(&gone));
    f.entry("Nur hier", "note", &[&gone]);
    f.entry("Nur unten", "note", &[&sub]);
    f.entry("Auch woanders", "note", &[&gone, &keep]);

    let preview = f.e.project_delete_preview(&gone).unwrap();
    assert_eq!(
        (preview.subprojects, preview.entries_only_here, preview.entries_elsewhere),
        (1, 2, 1)
    );

    let trashed = f.e.delete_project(&f.user, &gone, false).unwrap();
    assert_eq!(trashed, 0);
    assert_eq!(f.tree(), ["Bleibt"]);
    assert_eq!(f.graph_titles(), ["Auch woanders", "Nur hier", "Nur unten"], "entries stay");
    let id =
        f.e.graph_snapshot(None)
            .unwrap()
            .nodes
            .iter()
            .find(|n| n.title == "Auch woanders")
            .unwrap()
            .id
            .clone();
    let projects = f.e.node_detail(&id).unwrap().projects;
    assert_eq!(projects.len(), 1, "only the live project is shown");

    f.undo_last();
    assert_eq!(f.tree(), ["Bleibt", "Weg", " Unter"]);
    assert_eq!(f.e.graph_snapshot(Some(&gone)).unwrap().nodes.len(), 3);
}

#[test]
fn deleting_with_entries_spares_those_in_other_projects() {
    let f = setup();
    let keep = f.project("Bleibt", None);
    let gone = f.project("Weg", None);
    f.entry("Nur hier", "note", &[&gone]);
    f.entry("Auch woanders", "note", &[&gone, &keep]);

    assert_eq!(f.e.delete_project(&f.user, &gone, true).unwrap(), 1);
    assert_eq!(f.graph_titles(), ["Auch woanders"]);
    assert_eq!(f.trash_titles(), ["Nur hier"]);

    f.undo_last();
    assert_eq!(f.graph_titles(), ["Auch woanders", "Nur hier"]);
    assert!(f.trash_titles().is_empty());
}

#[test]
fn a_project_template_does_not_turn_global_when_its_project_goes() {
    let f = setup();
    let gone = f.project("Weg", None);
    f.entry("Projektvorlage", TEMPLATE_KIND, &[&gone]);
    f.entry("Global", TEMPLATE_KIND, &[]);
    f.e.create_doc_page(&f.user, &gone, "Handbuch", "", None).unwrap();

    let preview = f.e.project_delete_preview(&gone).unwrap();
    assert_eq!((preview.documents, preview.entries_only_here), (2, 0));

    f.e.delete_project(&f.user, &gone, false).unwrap();
    let names: Vec<_> = f.e.templates(None).unwrap().into_iter().map(|t| t.node.title).collect();
    assert_eq!(names, ["Global"]);
    assert_eq!(f.trash_titles(), ["Handbuch", "Projektvorlage"]);

    f.undo_last();
    assert_eq!(f.e.templates(None).unwrap().len(), 2);
    assert_eq!(f.e.doc_tree(&gone).unwrap().len(), 1);
}
