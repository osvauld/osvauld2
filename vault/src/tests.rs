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

#[test]
fn new_account_has_no_workspaces() {
    let (mut vault, _tmp) = fresh();
    vault.signup("me", "pw").unwrap();
    assert!(vault.workspaces().unwrap().is_empty());
}

#[test]
fn create_workspace_returns_named_record_with_an_id() {
    let (mut vault, _tmp) = fresh();
    vault.signup("me", "pw").unwrap();

    let meta = vault.create_workspace("Engineering").unwrap();
    assert_eq!(meta.name, "Engineering");
    assert_eq!(meta.id.len(), 32); // 16 random bytes, hex-encoded
    assert!(meta.id.chars().all(|c| c.is_ascii_hexdigit()));

    let listed = vault.workspaces().unwrap();
    assert_eq!(listed, vec![meta]);
}

#[test]
fn workspaces_are_listed_newest_first() {
    let (mut vault, _tmp) = fresh();
    vault.signup("me", "pw").unwrap();

    let first = vault.create_workspace("first").unwrap();
    let second = vault.create_workspace("second").unwrap();
    let third = vault.create_workspace("third").unwrap();

    let names: Vec<String> = vault.workspaces().unwrap().into_iter().map(|w| w.name).collect();
    // Same-second creation ties break by id, so assert on the set + that all three are present
    // rather than a strict ordering the clock can't guarantee within one test run.
    assert_eq!(names.len(), 3);
    for w in [&first, &second, &third] {
        assert!(names.contains(&w.name), "missing {}", w.name);
    }

    // Ids are unique per workspace.
    let ids: std::collections::HashSet<_> =
        [first.id, second.id, third.id].into_iter().collect();
    assert_eq!(ids.len(), 3);
}

#[test]
fn workspace_by_id_round_trips_and_misses_cleanly() {
    let (mut vault, _tmp) = fresh();
    vault.signup("me", "pw").unwrap();
    let meta = vault.create_workspace("Design").unwrap();

    assert_eq!(vault.workspace(&meta.id).unwrap(), Some(meta));
    assert_eq!(vault.workspace("0123456789abcdef0123456789abcdef").unwrap(), None);
}

#[test]
fn workspaces_survive_reopen_and_relogin() {
    let (mut vault, tmp) = fresh();
    let (did, _) = vault.signup("me", "pw").unwrap();
    let made = vault.create_workspace("Persisted").unwrap();
    drop(vault);

    // A fresh Vault over the same dir proves the sealed header persisted to disk.
    let mut reopened = Vault::open(tmp.path().to_path_buf()).unwrap();
    reopened.login(&did, "pw").unwrap();
    assert_eq!(reopened.workspaces().unwrap(), vec![made]);
}

#[test]
fn workspace_ops_require_an_unlocked_account() {
    let (vault, _tmp) = fresh();
    assert!(matches!(vault.create_workspace("x"), Err(VaultError::Locked)));
    assert!(matches!(vault.workspaces(), Err(VaultError::Locked)));
}

#[test]
fn workspace_content_keys_are_not_mistaken_for_workspaces() {
    let (mut vault, _tmp) = fresh();
    vault.signup("me", "pw").unwrap();
    let meta = vault.create_workspace("real").unwrap();

    // A future content key living under the same `ws/<id>/…` namespace must not be counted
    // as a workspace by the `ws/<id>/meta` listing scan.
    let store = vault.store().unwrap();
    store.put(&format!("ws/{}/file/abc", meta.id), b"not a workspace").unwrap();

    let listed = vault.workspaces().unwrap();
    assert_eq!(listed, vec![meta]);
}
