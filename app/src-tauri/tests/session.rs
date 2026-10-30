//! Session lifecycle without a running Tauri app. Touch ID is not covered:
//! it needs a finger and touches the real Keychain.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use bm_core::{model::NewNode, vault::KdfParams};
use bm_macos::power::LockTrigger;
use brainmatter::session::{Emit, Session, Settings};

struct Fixture {
    dir: tempfile::TempDir,
    session: Session,
    events: Arc<Mutex<Vec<String>>>,
}

fn session_in(dir: &std::path::Path, events: &Arc<Mutex<Vec<String>>>) -> Session {
    let sink = events.clone();
    let emit: Emit = Arc::new(move |name, _payload| sink.lock().unwrap().push(name.to_string()));
    Session::new(dir.to_owned(), emit, KdfParams::insecure_fast())
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let session = session_in(dir.path(), &events);
    Fixture { dir, session, events }
}

fn code<T>(r: Result<T, brainmatter::error::ApiError>) -> &'static str {
    match r {
        Ok(_) => panic!("expected an error"),
        Err(e) => e.code,
    }
}

#[test]
fn create_lock_unlock_roundtrip() {
    let f = fixture();
    assert_eq!(f.session.status().state, "none");
    assert_eq!(code(f.session.create("kurz", "Lasse")), "invalid");
    assert_eq!(code(f.session.create("lang-genug", "  ")), "invalid");

    let recovery = f.session.create("lang-genug", "Lasse").unwrap();
    assert_eq!(recovery.replace('-', "").len(), 36);
    let status = f.session.status();
    assert_eq!((status.state, status.user_name.as_deref()), ("unlocked", Some("Lasse")));
    assert_eq!(code(f.session.create("lang-genug", "Lasse")), "vault_exists");

    let (engine, ctx) = f.session.engine().unwrap();
    let node =
        engine.create_node(&ctx, NewNode { title: "Bleibt".into(), ..Default::default() }).unwrap();
    drop(engine);

    f.session.lock();
    assert_eq!(f.session.status().state, "locked");
    assert_eq!(code(f.session.engine()), "locked");
    assert_eq!(code(f.session.unlock_password("falsch-falsch")), "wrong_password");

    f.session.unlock_password("lang-genug").unwrap();
    let (engine, _) = f.session.engine().unwrap();
    assert_eq!(engine.node(&node.id).unwrap().unwrap().title, "Bleibt");
    assert_eq!(code(f.session.unlock_password("lang-genug")), "already_unlocked");

    // Recovery key works as a second way in.
    f.session.lock();
    assert_eq!(code(f.session.unlock_recovery("XXXX-XXXX")), "wrong_recovery_key");
    f.session.unlock_recovery(&recovery.to_lowercase()).unwrap();
    assert!(f.session.is_unlocked());
}

#[test]
fn reopening_the_app_finds_a_locked_vault() {
    let f = fixture();
    f.session.create("lang-genug", "Lasse").unwrap();
    f.session.lock();
    let again = session_in(f.dir.path(), &f.events);
    assert_eq!(again.status().state, "locked");
    again.unlock_password("lang-genug").unwrap();
    assert_eq!(again.status().user_name.as_deref(), Some("Lasse"));
}

#[test]
fn password_change_and_recovery_rotation_need_the_current_password() {
    let f = fixture();
    let old_recovery = f.session.create("lang-genug", "Lasse").unwrap();

    assert_eq!(
        code(f.session.change_password("falsch-falsch", "neues-passwort")),
        "wrong_password"
    );
    assert_eq!(code(f.session.change_password("lang-genug", "kurz")), "invalid");
    f.session.change_password("lang-genug", "neues-passwort").unwrap();

    assert_eq!(code(f.session.rotate_recovery_key("lang-genug")), "wrong_password");
    let new_recovery = f.session.rotate_recovery_key("neues-passwort").unwrap();
    assert_ne!(new_recovery, old_recovery);

    f.session.lock();
    assert_eq!(code(f.session.unlock_password("lang-genug")), "wrong_password");
    assert_eq!(code(f.session.unlock_recovery(&old_recovery)), "wrong_recovery_key");
    f.session.unlock_recovery(&new_recovery).unwrap();
}

#[test]
fn settings_persist_and_drive_system_locks() {
    let f = fixture();
    assert_eq!(f.session.settings(), Settings::default());
    let saved = f
        .session
        .update_settings(Settings {
            auto_lock_minutes: 0,
            lock_on_sleep: true,
            lock_on_screen_lock: true,
        })
        .unwrap();
    assert_eq!(saved.auto_lock_minutes, 1, "clamped to at least one minute");
    assert_eq!(session_in(f.dir.path(), &f.events).settings(), saved);

    f.session.create("lang-genug", "Lasse").unwrap();
    f.session.on_system_event(LockTrigger::ScreenLocked);
    assert!(!f.session.is_unlocked());

    f.session.unlock_password("lang-genug").unwrap();
    f.session
        .update_settings(Settings { lock_on_sleep: false, lock_on_screen_lock: false, ..saved })
        .unwrap();
    f.session.on_system_event(LockTrigger::SystemSleep);
    assert!(f.session.is_unlocked(), "lock on sleep was turned off");

    // Not idle long enough for auto-lock.
    f.session.maybe_auto_lock();
    assert!(f.session.is_unlocked());
}

#[test]
fn emits_session_and_domain_events() {
    let f = fixture();
    f.session.create("lang-genug", "Lasse").unwrap();
    let (engine, ctx) = f.session.engine().unwrap();
    engine.create_node(&ctx, NewNode { title: "Live".into(), ..Default::default() }).unwrap();

    // The forwarder runs on the async runtime; give it a moment.
    for _ in 0..50 {
        if f.events.lock().unwrap().iter().any(|e| e == "bm:event") {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    f.session.lock();
    let events = f.events.lock().unwrap().clone();
    assert_eq!(events.first().map(String::as_str), Some("bm:session"));
    assert!(events.iter().any(|e| e == "bm:event"), "{events:?}");
    assert_eq!(events.last().map(String::as_str), Some("bm:session"), "lock is announced");
}
