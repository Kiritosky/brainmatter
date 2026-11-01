//! File links: who may read them, undo, and leaving no trace when erased.

use bm_core::{CoreError, Engine, Vault, model::*, vault::KdfParams};

struct Fixture {
    _dir: tempfile::TempDir,
    engine: Engine,
    user: Ctx,
    ai: Ctx,
}

fn setup() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let (vault, keys, _) = Vault::create(dir.path(), "pw", KdfParams::insecure_fast()).unwrap();
    let engine = Engine::open(&vault.db_path(), &keys.db_key(), 2).unwrap();
    let user = Ctx::new(engine.ensure_actor(ActorKind::User, "Nutzer").unwrap().id);
    let ai = Ctx::new(engine.ensure_actor(ActorKind::Ai, "KI").unwrap().id);
    Fixture { _dir: dir, engine, user, ai }
}

impl Fixture {
    fn note(&self, title: &str) -> Node {
        let input = NewNode { title: title.into(), ..Default::default() };
        self.engine.create_node(&self.user, input).unwrap()
    }

    fn links(&self, n: &Node) -> Vec<FileLink> {
        self.engine.node_detail(&n.id).unwrap().file_links
    }

    fn undo_last(&self) {
        let last = self.engine.change_sets(1, None).unwrap()[0].id.clone();
        self.engine.revert(&self.user, &last).unwrap();
    }
}

#[test]
fn only_the_user_gets_links_ai_clients_may_read() {
    let f = setup();
    let n = f.note("A");
    let by_user = f.engine.add_file_link(&f.user, &n.id, "/a/spec.md", None, None).unwrap();
    assert!(by_user.approved_for_ai);
    assert_eq!(by_user.label, "spec.md");

    let by_ai = f.engine.add_file_link(&f.ai, &n.id, "/etc/passwd", None, None).unwrap();
    assert!(!by_ai.approved_for_ai, "an AI must not be able to approve its own link");

    f.engine.set_file_link_approved(&f.user, &by_ai.id, true).unwrap();
    assert!(f.engine.file_link(&by_ai.id).unwrap().approved_for_ai);
}

#[test]
fn linking_a_path_again_cannot_launder_an_approval() {
    let f = setup();
    let n = f.note("A");
    let first = f.engine.add_file_link(&f.ai, &n.id, "/a/x.md", None, None).unwrap();
    let second = f.engine.add_file_link(&f.user, &n.id, "/a/x.md/", None, None).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(f.links(&n).len(), 1);
    assert!(!second.approved_for_ai, "an explicit approval stays explicit");
}

#[test]
fn bad_paths_are_rejected() {
    let f = setup();
    let n = f.note("A");
    for bad in ["relativ/x.md", "~/x.md", "/a/../../etc/passwd"] {
        let result = f.engine.add_file_link(&f.user, &n.id, bad, None, None);
        assert!(matches!(result, Err(CoreError::Invalid(_))), "{bad}");
    }
}

#[test]
fn adding_removing_and_approving_can_be_undone() {
    let f = setup();
    let n = f.note("A");
    let link = f.engine.add_file_link(&f.ai, &n.id, "/a/x.md", None, None).unwrap();

    f.engine.set_file_link_approved(&f.user, &link.id, true).unwrap();
    f.undo_last();
    assert!(!f.engine.file_link(&link.id).unwrap().approved_for_ai);

    f.engine.delete_file_link(&f.user, &link.id).unwrap();
    assert!(f.links(&n).is_empty());
    f.undo_last();
    assert_eq!(f.links(&n).len(), 1);
}

#[test]
fn a_moved_file_does_not_block_undo_or_bring_back_the_old_path() {
    let f = setup();
    let n = f.note("A");
    let link = f.engine.add_file_link(&f.user, &n.id, "/a/x.md", None, Some(b"alt")).unwrap();
    f.engine.set_file_link_approved(&f.user, &link.id, false).unwrap();

    f.engine.set_file_link_location(&link.id, "/b/x.md", Some(b"neu")).unwrap();
    assert_eq!(f.engine.file_link_bookmark(&link.id).unwrap().unwrap(), b"neu");

    f.undo_last();
    let now = f.engine.file_link(&link.id).unwrap();
    assert!(now.approved_for_ai);
    assert_eq!(now.path, "/b/x.md");
}

#[test]
fn merging_moves_links_and_trash_hides_them() {
    let f = setup();
    let keep = f.note("Behalten");
    let other = f.note("Duplikat");
    let link = f.engine.add_file_link(&f.user, &other.id, "/a/x.md", None, None).unwrap();

    f.engine.merge_nodes(&f.user, &keep.id, std::slice::from_ref(&other.id)).unwrap();
    assert_eq!(f.links(&keep)[0].id, link.id);

    f.engine.delete_node(&f.user, &keep.id).unwrap();
    assert!(f.engine.file_link(&link.id).is_err(), "a trashed entry's link is not readable");
    f.engine.restore_node(&f.user, &keep.id).unwrap();
    assert!(f.engine.file_link(&link.id).is_ok());
}

#[test]
fn erasing_an_entry_erases_its_links() {
    let f = setup();
    let n = f.note("A");
    let link = f.engine.add_file_link(&f.user, &n.id, "/geheim/pfad.md", None, None).unwrap();
    f.engine.delete_node(&f.user, &n.id).unwrap();
    f.engine.purge_node(&f.user, &n.id).unwrap();
    assert!(f.engine.file_link(&link.id).is_err());
    let summaries: Vec<String> =
        f.engine.change_sets(100, None).unwrap().into_iter().map(|c| c.summary).collect();
    assert!(!summaries.iter().any(|s| s.contains("pfad.md")), "{summaries:?}");
}
