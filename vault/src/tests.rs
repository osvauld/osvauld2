use super::{Vault, VaultError};
use tempfile::TempDir;

fn fresh() -> (Vault, TempDir) {
    let tmp = TempDir::new().unwrap();
    let vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    (vault, tmp)
}

#[test]
fn empty_vault_has_no_accounts() {
    let (vault, _tmp) = fresh();
    assert!(vault.is_empty());
    assert!(vault.accounts().unwrap().is_empty());
    assert!(vault.identity().is_none());
    assert!(vault.current().is_none());
}

#[test]
fn signup_makes_account_active() {
    let (mut vault, _tmp) = fresh();
    let (did, _mnemonic) = vault.signup("personal", "pw").unwrap();
    assert!(!vault.is_empty());
    assert!(vault.identity().is_some());
    let current = vault.current().unwrap();
    assert_eq!(current.did, did);
    assert_eq!(current.label, "personal");
}

#[test]
fn login_round_trips_across_reopen() {
    let (mut vault, tmp) = fresh();
    let (did, _mnemonic) = vault.signup("me", "pw").unwrap();
    drop(vault);

    // A fresh Vault over the same dir proves the keystore persisted to disk.
    let mut reopened = Vault::open(tmp.path().to_path_buf()).unwrap();
    assert!(!reopened.is_empty());
    reopened.login(&did, "pw").unwrap();
    assert_eq!(reopened.current().unwrap().did, did);
    assert_eq!(reopened.identity().unwrap().did(), did);
}

#[test]
fn wrong_passphrase_is_distinct_and_leaves_locked() {
    let (mut vault, _tmp) = fresh();
    let (did, _mnemonic) = vault.signup("me", "right").unwrap();
    vault.lock();
    let error = vault.login(&did, "wrong").unwrap_err();
    assert!(matches!(error, VaultError::WrongPassphrase));
    assert!(vault.identity().is_none());
}

#[test]
fn login_unknown_account_errors() {
    let (mut vault, _tmp) = fresh();
    let error = vault.login("did:key:z6MkNotARealAccountKeyValue", "pw").unwrap_err();
    assert!(matches!(error, VaultError::NoSuchAccount(_) | VaultError::BadDid(_)));
}

#[test]
fn accounts_lists_labels_active_from_cache_others_read_only() {
    let (mut vault, _tmp) = fresh();
    let (did_a, _) = vault.signup("alice", "pa").unwrap();
    let (did_b, _) = vault.signup("bob", "pb").unwrap();
    // bob is active (the last signup): accounts() reads bob from cache, alice read-only.
    assert_eq!(vault.current().unwrap().did, did_b);

    let mut listed: Vec<(String, String)> =
        vault.accounts().unwrap().into_iter().map(|a| (a.did, a.label)).collect();
    listed.sort();
    let mut expected = vec![
        (did_a, "alice".to_string()),
        (did_b, "bob".to_string()),
    ];
    expected.sort();
    assert_eq!(listed, expected);
}

#[test]
fn switch_changes_active_account() {
    let (mut vault, _tmp) = fresh();
    let (did_a, _) = vault.signup("alice", "pa").unwrap();
    let (did_b, _) = vault.signup("bob", "pb").unwrap();

    vault.switch(&did_a, "pa").unwrap();
    assert_eq!(vault.current().unwrap().did, did_a);

    // A failed switch logs out (lock + login), by design.
    let error = vault.switch(&did_b, "nope").unwrap_err();
    assert!(matches!(error, VaultError::WrongPassphrase));
    assert!(vault.identity().is_none());
}
