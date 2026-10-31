//! Attachments: encrypted at rest, tamper-evident, undoable.

use std::{
    fs,
    path::{Path, PathBuf},
};

use bm_core::{
    CoreError, Engine, Vault,
    blobs::BlobStore,
    engine::MAX_ATTACHMENT_BYTES,
    model::*,
    vault::{KdfParams, SecretKey},
};

const SECRET: &[u8] = b"streng geheimer Inhalt der Datei";

struct Fixture {
    dir: tempfile::TempDir,
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
    Fixture { dir, engine, user }
}

impl Fixture {
    fn note(&self, title: &str) -> Node {
        let input = NewNode { title: title.into(), ..Default::default() };
        self.engine.create_node(&self.user, input).unwrap()
    }

    fn attach(&self, node: &Node, name: &str, bytes: &[u8]) -> Attachment {
        self.engine.add_attachment(&self.user, &node.id, name, None, bytes).unwrap()
    }

    fn last_change(&self) -> bm_core::ids::ChangeSetId {
        self.engine.change_sets(1, None).unwrap()[0].id.clone()
    }
}

fn blob_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for shard in fs::read_dir(dir.join("blobs")).unwrap() {
        for file in fs::read_dir(shard.unwrap().path()).unwrap() {
            out.push(file.unwrap().path());
        }
    }
    out
}

#[test]
fn content_is_encrypted_at_rest_and_comes_back_intact() {
    let f = setup();
    let n = f.note("Rechnung");
    let a = f.attach(&n, "rechnung.pdf", SECRET);
    assert_eq!(a.mime, "application/pdf");
    assert_eq!(a.size, SECRET.len() as i64);
    assert_eq!(f.engine.attachment_data(&a.id).unwrap().1, SECRET);
    assert_eq!(f.engine.node_detail(&n.id).unwrap().attachments, std::slice::from_ref(&a));

    let files = blob_files(f.dir.path());
    assert_eq!(files.len(), 1);
    let raw = fs::read(&files[0]).unwrap();
    assert!(!raw.windows(SECRET.len()).any(|w| w == SECRET), "plaintext on disk");
    assert!(!files[0].to_string_lossy().contains("rechnung"), "name reveals the file");
}

#[test]
fn identical_files_are_stored_once() {
    let f = setup();
    let n = f.note("A");
    let a = f.attach(&n, "eins.txt", SECRET);
    let b = f.attach(&n, "zwei.txt", SECRET);
    assert_eq!(a.blob_id, b.blob_id);
    assert_eq!(blob_files(f.dir.path()).len(), 1);
}

#[test]
fn tampered_or_swapped_files_are_rejected() {
    let f = setup();
    let n = f.note("A");
    let a = f.attach(&n, "a.txt", SECRET);
    let b = f.attach(&n, "b.txt", b"anderer Inhalt");
    let path = |id: &str| f.dir.path().join("blobs").join(&id[..2]).join(id);

    // One flipped bit.
    let original = fs::read(path(&a.blob_id)).unwrap();
    let mut flipped = original.clone();
    *flipped.last_mut().unwrap() ^= 1;
    fs::write(path(&a.blob_id), &flipped).unwrap();
    assert!(matches!(f.engine.attachment_data(&a.id), Err(CoreError::CorruptBlob(_))));

    // A valid file of the same vault under another name.
    fs::copy(path(&b.blob_id), path(&a.blob_id)).unwrap();
    assert!(matches!(f.engine.attachment_data(&a.id), Err(CoreError::CorruptBlob(_))));

    fs::write(path(&a.blob_id), &original).unwrap();
    assert_eq!(f.engine.attachment_data(&a.id).unwrap().1, SECRET);
}

#[test]
fn another_key_cannot_read_it() {
    let f = setup();
    let n = f.note("A");
    let a = f.attach(&n, "a.txt", SECRET);
    let other = BlobStore::new(f.dir.path().join("blobs"), &SecretKey::random());
    assert!(matches!(other.get(&a.blob_id), Err(CoreError::CorruptBlob(_))));
}

#[test]
fn removing_and_adding_can_be_undone() {
    let f = setup();
    let n = f.note("A");
    let a = f.attach(&n, "a.txt", SECRET);
    let added = f.last_change();

    f.engine.delete_attachment(&f.user, &a.id).unwrap();
    assert!(f.engine.node_detail(&n.id).unwrap().attachments.is_empty());
    assert!(f.engine.attachment_data(&a.id).is_err());

    f.engine.revert(&f.user, &f.last_change()).unwrap();
    assert_eq!(f.engine.attachment_data(&a.id).unwrap().1, SECRET);

    f.engine.revert(&f.user, &added).unwrap();
    assert!(f.engine.attachment(&a.id).is_err());
    assert!(f.engine.node_detail(&n.id).unwrap().attachments.is_empty());
}

#[test]
fn undoing_an_upload_conflicts_after_the_attachment_was_removed() {
    let f = setup();
    let n = f.note("A");
    let a = f.attach(&n, "a.txt", SECRET);
    let added = f.last_change();
    f.engine.delete_attachment(&f.user, &a.id).unwrap();
    assert!(matches!(f.engine.revert(&f.user, &added), Err(CoreError::Conflict(_))));
}

#[test]
fn merging_moves_attachments_and_undo_moves_them_back() {
    let f = setup();
    let keep = f.note("Behalten");
    let other = f.note("Duplikat");
    let a = f.attach(&other, "a.txt", SECRET);

    f.engine.merge_nodes(&f.user, &keep.id, std::slice::from_ref(&other.id)).unwrap();
    let moved = &f.engine.node_detail(&keep.id).unwrap().attachments;
    assert_eq!(moved.len(), 1);
    assert_eq!(moved[0].id, a.id);
    assert_eq!(f.engine.attachment_data(&a.id).unwrap().1, SECRET);

    f.engine.revert(&f.user, &f.last_change()).unwrap();
    assert!(f.engine.node_detail(&keep.id).unwrap().attachments.is_empty());
    assert_eq!(f.engine.node_detail(&other.id).unwrap().attachments[0].id, a.id);
}

#[test]
fn trashed_entries_hide_their_attachments_until_restored() {
    let f = setup();
    let n = f.note("A");
    let a = f.attach(&n, "a.txt", SECRET);
    f.engine.delete_node(&f.user, &n.id).unwrap();
    assert!(f.engine.attachment_data(&a.id).is_err());
    f.engine.revert(&f.user, &f.last_change()).unwrap();
    assert_eq!(f.engine.attachment_data(&a.id).unwrap().1, SECRET);
}

#[test]
fn rejects_files_it_cannot_store() {
    let f = setup();
    let n = f.note("A");
    let add = |name: &str, bytes: &[u8]| f.engine.add_attachment(&f.user, &n.id, name, None, bytes);
    assert!(matches!(add("..", SECRET), Err(CoreError::Invalid(_))));
    assert!(matches!(
        add("gross.bin", &vec![0; MAX_ATTACHMENT_BYTES + 1]),
        Err(CoreError::Invalid(_))
    ));
    assert_eq!(add("../../x.txt", SECRET).unwrap().filename, "x.txt");
}
