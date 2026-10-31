//! Backups: encrypted on disk, restorable, and restoring can be undone.

use std::{fs, path::PathBuf};

use bm_core::{Engine, Vault, VaultKeys, backups, model::*, vault::KdfParams};

const SECRET: &str = "GEHEIMWORT-IM-EINTRAG";

struct Fixture {
    dir: tempfile::TempDir,
    db: PathBuf,
    keys: VaultKeys,
}

impl Fixture {
    fn backups(&self) -> PathBuf {
        self.dir.path().join("backups")
    }

    fn with_engine<T>(&self, f: impl FnOnce(&Engine, &Ctx) -> T) -> T {
        let engine = Engine::open(&self.db, &self.keys.db_key(), 1).unwrap();
        let user = engine.ensure_actor(ActorKind::User, "Lasse").unwrap();
        f(&engine, &Ctx::new(user.id))
    }

    fn add(&self, title: &str) {
        self.with_engine(|e, ctx| {
            let input = NewNode { title: title.into(), body: SECRET.into(), ..Default::default() };
            e.create_node(ctx, input).unwrap();
        });
    }

    fn titles(&self) -> Vec<String> {
        self.with_engine(|e, _| {
            let mut t: Vec<_> =
                e.graph_snapshot(None).unwrap().nodes.into_iter().map(|n| n.title).collect();
            t.sort();
            t
        })
    }
}

fn setup() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let (vault, keys, _) = Vault::create(dir.path(), "pw", KdfParams::insecure_fast()).unwrap();
    let db = vault.db_path();
    Fixture { dir, db, keys }
}

#[test]
fn backups_are_encrypted_on_disk() {
    let f = setup();
    f.add("Eintrag");
    let b = backups::create(&f.db, &f.keys.db_key(), &f.backups()).unwrap();

    let raw = fs::read(f.backups().join(&b.name)).unwrap();
    assert!(raw.len() > 1000);
    assert!(!raw.starts_with(b"SQLite format 3"), "backup has a plaintext SQLite header");
    assert!(!raw.windows(SECRET.len()).any(|w| w == SECRET.as_bytes()), "plaintext in backup");
    backups::verify(&f.backups().join(&b.name), &f.keys.db_key()).unwrap();
}

#[test]
fn another_key_cannot_read_a_backup() {
    let f = setup();
    f.add("Eintrag");
    let b = backups::create(&f.db, &f.keys.db_key(), &f.backups()).unwrap();

    let other_dir = tempfile::tempdir().unwrap();
    let (_, other, _) = Vault::create(other_dir.path(), "x", KdfParams::insecure_fast()).unwrap();
    assert!(backups::verify(&f.backups().join(&b.name), &other.db_key()).is_err());
}

#[test]
fn damaged_backups_are_detected_and_never_restored() {
    let f = setup();
    for i in 0..50 {
        f.add(&format!("Eintrag {i}"));
    }
    let b = backups::create(&f.db, &f.keys.db_key(), &f.backups()).unwrap();
    let path = f.backups().join(&b.name);
    let mut raw = fs::read(&path).unwrap();
    let middle = raw.len() / 2;
    raw[middle..middle + 64].fill(0);
    fs::write(&path, raw).unwrap();

    assert!(backups::verify(&path, &f.keys.db_key()).is_err());
    assert!(backups::restore(&f.db, &f.keys.db_key(), &f.backups(), &b.name).is_err());
    assert_eq!(f.titles().len(), 50, "the database is untouched");
}

#[test]
fn restoring_brings_back_the_old_state_and_can_be_undone() {
    let f = setup();
    f.add("Alt");
    let old = backups::create(&f.db, &f.keys.db_key(), &f.backups()).unwrap();
    f.add("Neu");
    assert_eq!(f.titles(), ["Alt", "Neu"]);

    let saved = backups::restore(&f.db, &f.keys.db_key(), &f.backups(), &old.name).unwrap();
    assert_eq!(f.titles(), ["Alt"]);

    // The state before the restore is itself a backup.
    backups::restore(&f.db, &f.keys.db_key(), &f.backups(), &saved.name).unwrap();
    assert_eq!(f.titles(), ["Alt", "Neu"]);
}

#[test]
fn restore_rejects_names_outside_the_backup_folder() {
    let f = setup();
    f.add("Eintrag");
    for name in ["../vault.header", "/etc/passwd", "backup-1.db/../../x", "backup-999.db"] {
        assert!(backups::restore(&f.db, &f.keys.db_key(), &f.backups(), name).is_err(), "{name}");
    }
    assert_eq!(f.titles(), ["Eintrag"]);
}

#[test]
fn a_running_engine_keeps_working_during_a_backup() {
    let f = setup();
    f.add("Vorher");
    f.with_engine(|e, ctx| {
        let b = backups::create(&f.db, &f.keys.db_key(), &f.backups()).unwrap();
        let input = NewNode { title: "Danach".into(), ..Default::default() };
        e.create_node(ctx, input).unwrap();
        assert!(b.size > 0);
    });
    assert_eq!(f.titles(), ["Danach", "Vorher"]);
}

#[test]
fn listing_ignores_unfinished_files() {
    let f = setup();
    f.add("Eintrag");
    let b = backups::create(&f.db, &f.keys.db_key(), &f.backups()).unwrap();
    fs::write(f.backups().join("backup-1.db.tmp"), b"halb").unwrap();
    fs::write(f.backups().join("notizen.txt"), b"fremd").unwrap();
    assert_eq!(backups::list(&f.backups()).unwrap(), [b]);
    assert!(backups::list(&f.dir.path().join("gibt-es-nicht")).unwrap().is_empty());
}
