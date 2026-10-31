//! The trash: restoring brings an entry back with its links; erasing leaves
//! nothing behind, which is checked in the database itself.

use std::{fs, path::Path};

use bm_core::{
    CoreError, Db, Engine, Vault, VaultKeys, blobs::BlobStore, model::*, vault::KdfParams,
};

const SECRET: &str = "GEHEIMWORT";

struct Fixture {
    dir: tempfile::TempDir,
    vault: Vault,
    keys: VaultKeys,
    engine: Engine,
    user: Ctx,
}

fn setup() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let (vault, keys, _) = Vault::create(dir.path(), "pw", KdfParams::insecure_fast()).unwrap();
    let engine = Engine::open(&vault.db_path(), &keys.db_key(), 2)
        .unwrap()
        .with_blobs(BlobStore::new(dir.path().join("blobs"), &keys.blob_key()));
    let user = Ctx::new(engine.ensure_actor(ActorKind::User, "Lasse").unwrap().id);
    Fixture { dir, vault, keys, engine, user }
}

impl Fixture {
    fn note(&self, title: &str, body: &str) -> Node {
        let input = NewNode { title: title.into(), body: body.into(), ..Default::default() };
        self.engine.create_node(&self.user, input).unwrap()
    }

    fn link(&self, a: &Node, b: &Node) {
        let input = NewEdge {
            src: a.id.clone(),
            dst: b.id.clone(),
            relation: None,
            weight: None,
            note: None,
        };
        self.engine.link(&self.user, input).unwrap();
    }

    fn degree(&self, n: &Node) -> usize {
        let d = self.engine.node_detail(&n.id).unwrap();
        d.outgoing.len() + d.incoming.len()
    }

    fn hits(&self, text: &str) -> usize {
        let q = SearchQuery {
            text: text.into(),
            project: None,
            include_subprojects: true,
            kinds: vec![],
            tags: vec![],
            limit: 10,
        };
        self.engine.search(&q).unwrap().len()
    }

    /// Rows anywhere in the database that still contain `needle`.
    fn rows_containing(&self, needle: &str) -> Vec<String> {
        let db = Db::open(&self.vault.db_path(), &self.keys.db_key()).unwrap();
        let conn = db.conn();
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table'
                 AND name NOT LIKE 'nodes_fts_%' AND name NOT LIKE 'sqlite_%'",
            )
            .unwrap();
        let tables: Vec<String> =
            stmt.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        let mut found = Vec::new();
        for table in tables {
            let columns: Vec<String> = conn
                .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
                .unwrap()
                .query_map([], |r| r.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            for column in columns {
                let sql = format!(
                    "SELECT count(*) FROM \"{table}\" WHERE CAST(\"{column}\" AS TEXT) LIKE ?1"
                );
                let n: i64 =
                    conn.query_row(&sql, [format!("%{needle}%")], |r| r.get(0)).unwrap_or(0);
                if n > 0 {
                    found.push(format!("{table}.{column}"));
                }
            }
        }
        found
    }
}

fn blob_count(dir: &Path) -> usize {
    let Ok(shards) = fs::read_dir(dir.join("blobs")) else { return 0 };
    shards.map(|s| fs::read_dir(s.unwrap().path()).unwrap().count()).sum()
}

#[test]
fn restoring_brings_back_the_entry_with_its_links() {
    let f = setup();
    let a = f.note("Alpha", "suchbar");
    let b = f.note("Beta", "");
    f.link(&a, &b);
    f.engine.delete_node(&f.user, &a.id).unwrap();
    assert_eq!(f.engine.trash(10).unwrap()[0].node.title, "Alpha");
    assert_eq!(f.hits("suchbar"), 0);
    assert_eq!(f.degree(&b), 0);

    let back = f.engine.restore_node(&f.user, &a.id).unwrap();
    assert_eq!(back.deleted_at, None);
    assert!(f.engine.trash(10).unwrap().is_empty());
    assert_eq!(f.hits("suchbar"), 1);
    assert_eq!(f.degree(&b), 1);
}

#[test]
fn a_link_to_an_entry_still_in_the_trash_stays_gone() {
    let f = setup();
    let a = f.note("Alpha", "");
    let b = f.note("Beta", "");
    f.link(&a, &b);
    f.engine.delete_node(&f.user, &b.id).unwrap();
    f.engine.delete_node(&f.user, &a.id).unwrap();

    f.engine.restore_node(&f.user, &a.id).unwrap();
    assert_eq!(f.degree(&a), 0, "its other end is still in the trash");
}

#[test]
fn links_removed_earlier_do_not_come_back() {
    let f = setup();
    let a = f.note("Alpha", "");
    let b = f.note("Beta", "");
    f.link(&a, &b);
    f.engine.unlink(&f.user, &a.id, &b.id, None).unwrap();
    f.engine.delete_node(&f.user, &a.id).unwrap();

    f.engine.restore_node(&f.user, &a.id).unwrap();
    assert_eq!(f.degree(&a), 0);
}

#[test]
fn restoring_can_be_undone_and_needs_a_trashed_entry() {
    let f = setup();
    let a = f.note("Alpha", "");
    assert!(matches!(f.engine.restore_node(&f.user, &a.id), Err(CoreError::Invalid(_))));
    f.engine.delete_node(&f.user, &a.id).unwrap();
    f.engine.restore_node(&f.user, &a.id).unwrap();

    let last = f.engine.change_sets(1, None).unwrap()[0].id.clone();
    f.engine.revert(&f.user, &last).unwrap();
    assert_eq!(f.engine.trash(10).unwrap().len(), 1);
}

#[test]
fn erasing_leaves_nothing_behind() {
    let f = setup();
    let keep = f.note("Bleibt", "");
    let title = format!("{SECRET} Titel");
    let doomed = f.note(&title, &format!("Text mit {SECRET}"));
    let patch = NodePatch {
        body: Some(BodyEdit::Replace(format!("Neuer Text mit {SECRET}"))),
        tags_add: vec![SECRET.to_lowercase()],
        ..Default::default()
    };
    f.engine.update_node(&f.user, &doomed.id, patch).unwrap();
    f.link(&doomed, &keep);
    let project = f
        .engine
        .create_project(&f.user, NewProject { name: "P".into(), ..Default::default() })
        .unwrap();
    f.engine.add_to_project(&f.user, &project.id, std::slice::from_ref(&doomed.id)).unwrap();
    f.engine.add_attachment(&f.user, &doomed.id, "geheim.txt", None, b"Inhalt").unwrap();
    assert!(!f.rows_containing(SECRET).is_empty(), "the check itself must be able to find it");

    assert!(
        matches!(f.engine.purge_node(&f.user, &doomed.id), Err(CoreError::Invalid(_))),
        "only from the trash"
    );
    f.engine.delete_node(&f.user, &doomed.id).unwrap();
    let before = f.engine.change_sets(100, None).unwrap().len();
    f.engine.purge_node(&f.user, &doomed.id).unwrap();

    assert_eq!(f.rows_containing(SECRET), Vec::<String>::new());
    assert_eq!(f.rows_containing(doomed.id.as_str()), Vec::<String>::new());
    assert_eq!(f.rows_containing("geheim.txt"), Vec::<String>::new());
    assert_eq!(blob_count(f.dir.path()), 0, "the attachment file is gone");
    assert_eq!(f.hits(SECRET), 0);
    assert!(f.engine.trash(10).unwrap().is_empty());
    assert!(f.engine.node_detail(&doomed.id).is_err());
    assert_eq!(f.degree(&keep), 0);
    assert_eq!(f.engine.change_sets(100, None).unwrap().len(), before, "erasing logs nothing");
}

#[test]
fn undoing_older_changes_still_works_after_erasing() {
    let f = setup();
    let keep = f.note("Bleibt", "alt");
    let doomed = f.note("Weg", "");
    f.link(&doomed, &keep);
    let linked = f.engine.change_sets(1, None).unwrap()[0].id.clone();
    let patch = NodePatch { body: Some(BodyEdit::Replace("neu".into())), ..Default::default() };
    f.engine.update_node(&f.user, &keep.id, patch).unwrap();
    let edited = f.engine.change_sets(1, None).unwrap()[0].id.clone();

    f.engine.delete_node(&f.user, &doomed.id).unwrap();
    f.engine.purge_node(&f.user, &doomed.id).unwrap();

    f.engine.revert(&f.user, &edited).unwrap();
    assert_eq!(f.engine.node_detail(&keep.id).unwrap().node.body, "alt");
    // Its only content was the erased link, so there is nothing left to undo.
    assert!(matches!(f.engine.revert(&f.user, &linked), Err(CoreError::Conflict(_))));
}

#[test]
fn a_file_another_entry_uses_survives_erasing() {
    let f = setup();
    let a = f.note("A", "");
    let b = f.note("B", "");
    f.engine.add_attachment(&f.user, &a.id, "x.txt", None, b"gleich").unwrap();
    let kept = f.engine.add_attachment(&f.user, &b.id, "y.txt", None, b"gleich").unwrap();

    f.engine.delete_node(&f.user, &a.id).unwrap();
    f.engine.purge_node(&f.user, &a.id).unwrap();
    assert_eq!(f.engine.attachment_data(&kept.id).unwrap().1, b"gleich");
}

#[test]
fn erasing_works_for_entries_named_in_other_texts() {
    let f = setup();
    let target = f.note("Ziel", "");
    let source = f.note("Quelle", "Siehe [[Ziel]]");
    assert_eq!(f.degree(&source), 1);
    f.engine.delete_node(&f.user, &target.id).unwrap();
    f.engine.purge_node(&f.user, &target.id).unwrap();
    assert_eq!(f.degree(&source), 0);

    // The text still names the title, so a new entry with it links up again.
    let again = f.note("Ziel", "");
    assert_eq!(f.degree(&again), 1);
}
