use bm_core::{
    Db, Vault, VaultError,
    vault::{KdfParams, RecoveryKey, SecretKey, SlotKind},
};

fn create(dir: &std::path::Path) -> (Vault, bm_core::VaultKeys, RecoveryKey) {
    Vault::create(dir, "korrekt-pferd-batterie", KdfParams::insecure_fast()).unwrap()
}

#[test]
fn create_and_unlock_with_every_slot() {
    let dir = tempfile::tempdir().unwrap();
    let (mut vault, keys, recovery) = create(dir.path());
    assert!(Vault::exists(dir.path()));

    // Data written with the creation keys is readable after any unlock path.
    {
        let db = Db::open(&vault.db_path(), &keys.db_key()).unwrap();
        db.conn()
            .execute(
                "INSERT INTO actors (id, kind, name, created_at) VALUES ('u', 'user', 'L', 0)",
                [],
            )
            .unwrap();
    }
    let db_path = vault.db_path();
    let read_actor = |keys: &bm_core::VaultKeys| {
        let db = Db::open(&db_path, &keys.db_key()).unwrap();
        db.conn().query_row("SELECT name FROM actors", [], |r| r.get::<_, String>(0)).unwrap()
    };

    let loaded = Vault::load(dir.path()).unwrap();
    assert_eq!(loaded.vault_id(), vault.vault_id());
    let by_password = loaded.unlock_with_password("korrekt-pferd-batterie").unwrap();
    assert_eq!(read_actor(&by_password), "L");

    let parsed = RecoveryKey::parse(&recovery.to_string()).unwrap();
    assert_eq!(read_actor(&loaded.unlock_with_recovery(&parsed).unwrap()), "L");

    let (account, slot_key) = vault.enroll_keychain(&keys).unwrap();
    assert!(account.starts_with("vault-"));
    let reloaded = Vault::load(dir.path()).unwrap();
    assert_eq!(reloaded.keychain_account(), Some(account.as_str()));
    assert_eq!(read_actor(&reloaded.unlock_with_keychain_key(&slot_key).unwrap()), "L");
    assert!(matches!(
        reloaded.unlock_with_keychain_key(&SecretKey::random()),
        Err(VaultError::WrongSlotKey(SlotKind::Keychain))
    ));
}

#[test]
fn wrong_secrets_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (vault, _, _) = create(dir.path());
    assert!(matches!(vault.unlock_with_password("falsch"), Err(VaultError::WrongPassword)));

    let other = RecoveryKey::parse(&{
        let d2 = tempfile::tempdir().unwrap();
        create(d2.path()).2.to_string()
    })
    .unwrap();
    assert!(matches!(vault.unlock_with_recovery(&other), Err(VaultError::WrongRecoveryKey)));
    assert!(matches!(
        Vault::create(dir.path(), "x", KdfParams::insecure_fast()),
        Err(VaultError::AlreadyExists(_))
    ));
}

#[test]
fn change_password_and_rotate_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let (mut vault, keys, old_recovery) = create(dir.path());
    drop(Db::open(&vault.db_path(), &keys.db_key()).unwrap());

    vault.change_password(&keys, "neues passwort").unwrap();
    let loaded = Vault::load(dir.path()).unwrap();
    assert!(matches!(
        loaded.unlock_with_password("korrekt-pferd-batterie"),
        Err(VaultError::WrongPassword)
    ));
    let unlocked = loaded.unlock_with_password("neues passwort").unwrap();
    // Same data key: the existing database still opens.
    Db::open(&vault.db_path(), &unlocked.db_key()).unwrap();

    let new_recovery = vault.rotate_recovery_key(&keys).unwrap();
    let loaded = Vault::load(dir.path()).unwrap();
    assert!(loaded.unlock_with_recovery(&old_recovery).is_err());
    assert!(loaded.unlock_with_recovery(&new_recovery).is_ok());
}

#[test]
fn tampered_header_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let (vault, _, _) = create(dir.path());
    let path = dir.path().join("vault.header");

    // Swap the vault id: AAD no longer matches, so no slot opens.
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replace(vault.vault_id(), "00000000-0000-7000-8000-000000000000"))
        .unwrap();
    let loaded = Vault::load(dir.path()).unwrap();
    assert!(matches!(
        loaded.unlock_with_password("korrekt-pferd-batterie"),
        Err(VaultError::WrongPassword)
    ));

    std::fs::write(&path, b"{ not json").unwrap();
    assert!(matches!(Vault::load(dir.path()), Err(VaultError::Corrupt(_))));
}
