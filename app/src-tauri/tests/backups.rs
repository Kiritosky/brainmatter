//! Backup and restore through the session, including what must not happen:
//! a wrong password, a bad name, and swapping files under a running command.

use std::sync::Arc;

use bm_core::{model::NewNode, vault::KdfParams};
use brainmatter::session::{Emit, Session};

const PASSWORD: &str = "lang-genug";

fn setup() -> (tempfile::TempDir, Session) {
    let dir = tempfile::tempdir().unwrap();
    let emit: Emit = Arc::new(|_, _| {});
    let session = Session::new(dir.path().to_owned(), emit, KdfParams::insecure_fast());
    session.create(PASSWORD, "Lasse").unwrap();
    (dir, session)
}

fn add(session: &Session, title: &str) {
    let (engine, ctx) = session.engine().unwrap();
    let input = NewNode { title: title.into(), ..Default::default() };
    engine.create_node(&ctx, input).unwrap();
}

fn titles(session: &Session) -> Vec<String> {
    let (engine, _) = session.engine().unwrap();
    let mut t: Vec<_> =
        engine.graph_snapshot(None).unwrap().nodes.into_iter().map(|n| n.title).collect();
    t.sort();
    t
}

#[test]
fn restore_needs_the_password_locks_and_can_be_undone() {
    let (_dir, s) = setup();
    add(&s, "Alt");
    let old = s.backup_now().unwrap();
    add(&s, "Neu");

    let wrong = s.restore_backup(&old.name, "falsch").unwrap_err();
    assert_eq!(wrong.code, "wrong_password");
    assert!(s.is_unlocked(), "a wrong password changes nothing");
    assert_eq!(titles(&s), ["Alt", "Neu"]);

    s.restore_backup(&old.name, PASSWORD).unwrap();
    assert!(!s.is_unlocked(), "restoring locks");
    s.unlock_password(PASSWORD).unwrap();
    assert_eq!(titles(&s), ["Alt"]);

    let saved = s.backup_status().unwrap().backups[0].clone();
    s.restore_backup(&saved.name, PASSWORD).unwrap();
    s.unlock_password(PASSWORD).unwrap();
    assert_eq!(titles(&s), ["Alt", "Neu"], "the state before the restore was saved");
}

#[test]
fn unknown_backups_are_refused_without_locking() {
    let (_dir, s) = setup();
    add(&s, "Eintrag");
    for name in ["backup-1.db", "../vault.header"] {
        assert_eq!(s.restore_backup(name, PASSWORD).unwrap_err().code, "not_found");
    }
    assert!(s.is_unlocked());
}

#[test]
fn a_command_holding_the_engine_blocks_the_swap_but_loses_nothing() {
    let (_dir, s) = setup();
    add(&s, "Alt");
    let old = s.backup_now().unwrap();
    add(&s, "Neu");

    let (held, _) = s.engine().unwrap();
    let busy = s.restore_backup(&old.name, PASSWORD).unwrap_err();
    assert_eq!(busy.code, "busy");
    assert!(s.backup_status().unwrap().error.is_some(), "readable after the dialog is gone");
    drop(held);

    s.unlock_password(PASSWORD).unwrap();
    assert_eq!(titles(&s), ["Alt", "Neu"]);
}

#[test]
fn locked_vaults_are_not_backed_up() {
    let (_dir, s) = setup();
    add(&s, "Eintrag");
    s.lock();
    assert_eq!(s.backup_now().unwrap_err().code, "locked");
}

#[test]
fn a_second_place_gets_every_backup_and_makes_the_vault_on_a_new_mac() {
    let (_dir, s) = setup();
    add(&s, "Wichtig");
    let drive = tempfile::tempdir().unwrap();
    let status = s.set_backup_copy(Some(drive.path().to_owned())).unwrap();
    let copy = status.copy.unwrap();
    assert!(copy.reachable);
    assert!(copy.newest.is_some(), "the first backup is taken and copied at once");
    std::thread::sleep(std::time::Duration::from_millis(5));
    s.backup_now().unwrap();
    let copy = s.backup_status().unwrap().copy.unwrap();
    assert_eq!(copy.newest, Some(s.backup_status().unwrap().backups[0].at));
    assert_eq!(copy.error, None);
    // Saving other settings from the UI leaves the place alone.
    let mut settings = s.settings();
    settings.backup_copy = None;
    assert!(s.update_settings(settings).unwrap().backup_copy.is_some());

    let emit: Emit = Arc::new(|_, _| {});
    let mac = tempfile::tempdir().unwrap();
    let new = Session::new(mac.path().to_owned(), emit, KdfParams::insecure_fast());
    let wrong = new.adopt_backup(drive.path(), "falsch-falsch").unwrap_err();
    assert_eq!(wrong.code, "wrong_password");
    assert!(!new.is_unlocked());
    new.adopt_backup(drive.path(), PASSWORD).unwrap();
    assert!(new.is_unlocked());
    assert_eq!(titles(&new), ["Wichtig"]);
    assert!(new.settings().backup_copy.is_some(), "copying goes on to the same place");
    assert_eq!(new.adopt_backup(drive.path(), PASSWORD).unwrap_err().code, "vault_exists");
}

#[test]
fn a_place_with_another_vault_is_not_taken() {
    let (_a, first) = setup();
    let (_b, second) = setup();
    let drive = tempfile::tempdir().unwrap();
    first.set_backup_copy(Some(drive.path().to_owned())).unwrap();
    let refused = second.set_backup_copy(Some(drive.path().to_owned())).unwrap_err();
    assert_eq!(refused.code, "other_vault");
    assert!(second.backup_status().unwrap().copy.is_none());
    let nothing = tempfile::tempdir().unwrap();
    let emit: Emit = Arc::new(|_, _| {});
    let mac = tempfile::tempdir().unwrap();
    let new = Session::new(mac.path().to_owned(), emit, KdfParams::insecure_fast());
    assert_eq!(new.adopt_backup(nothing.path(), PASSWORD).unwrap_err().code, "not_a_backup");
}
