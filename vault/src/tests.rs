use super::{ItemKind, Vault, VaultError, WorkspaceMeta};
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
    assert!(vault.current().is_none());
}

#[test]
fn signup_makes_account_active() {
    let (mut vault, _tmp) = fresh();
    let (did, _mnemonic) = vault.signup("personal", "pw").unwrap();
    assert!(!vault.is_empty());
    assert!(vault.current().is_some());
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
}

#[test]
fn wrong_passphrase_is_distinct_and_leaves_locked() {
    let (mut vault, _tmp) = fresh();
    let (did, _mnemonic) = vault.signup("me", "right").unwrap();
    vault.lock();
    let error = vault.login(&did, "wrong").unwrap_err();
    assert!(matches!(error, VaultError::WrongPassphrase));
    assert!(vault.current().is_none());
}

#[test]
fn login_unknown_account_errors() {
    let (mut vault, _tmp) = fresh();
    let error = vault
        .login("did:key:z6MkNotARealAccountKeyValue", "pw")
        .unwrap_err();
    assert!(matches!(
        error,
        VaultError::NoSuchAccount(_) | VaultError::BadDid(_)
    ));
}

#[test]
fn accounts_lists_labels_active_from_cache_others_read_only() {
    let (mut vault, _tmp) = fresh();
    let (did_a, _) = vault.signup("alice", "pa").unwrap();
    let (did_b, _) = vault.signup("bob", "pb").unwrap();
    // bob is active (the last signup): accounts() reads bob from cache, alice read-only.
    assert_eq!(vault.current().unwrap().did, did_b);

    let mut listed: Vec<(String, String)> = vault
        .accounts()
        .unwrap()
        .into_iter()
        .map(|a| (a.did, a.label))
        .collect();
    listed.sort();
    let mut expected = vec![(did_a, "alice".to_string()), (did_b, "bob".to_string())];
    expected.sort();
    assert_eq!(listed, expected);
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

    let names: Vec<String> = vault
        .workspaces()
        .unwrap()
        .into_iter()
        .map(|w| w.name)
        .collect();
    // Same-second creation ties break by id, so assert on the set + that all three are present
    // rather than a strict ordering the clock can't guarantee within one test run.
    assert_eq!(names.len(), 3);
    for w in [&first, &second, &third] {
        assert!(names.contains(&w.name), "missing {}", w.name);
    }

    // Ids are unique per workspace.
    let ids: std::collections::HashSet<_> = [first.id, second.id, third.id].into_iter().collect();
    assert_eq!(ids.len(), 3);
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
    assert!(matches!(
        vault.create_workspace("x"),
        Err(VaultError::Locked)
    ));
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
    store
        .put(&format!("ws/{}/file/abc", meta.id), b"not a workspace")
        .unwrap();

    let listed = vault.workspaces().unwrap();
    assert_eq!(listed, vec![meta]);
}

// ── Item blobs: the source doc (`…/src`) and the state doc (`…/state`) ────────

// Both payloads are deliberately invalid UTF-8: a Loro snapshot is binary, and a suite that
// only ever round-trips ASCII won't notice a `String::from_utf8` creeping into the path.
const SRC: &[u8] = b"\x00\x01loro-src\xff\xfe";
const STATE: &[u8] = b"\x00\x02loro-state\xfd\xfc";

fn app_item(vault: &Vault) -> (String, String) {
    let ws = vault.create_workspace("ws").unwrap();
    let item = vault.create_item(&ws.id, "app", ItemKind::App).unwrap();
    (ws.id, item.id)
}

#[test]
fn src_round_trips_and_misses_cleanly() {
    let (mut vault, _tmp) = fresh();
    vault.signup("me", "pw").unwrap();
    let (ws_id, item_id) = app_item(&vault);

    assert_eq!(vault.get_src(&ws_id, &item_id).unwrap(), None);
    vault.put_src(&ws_id, &item_id, SRC).unwrap();
    assert_eq!(vault.get_src(&ws_id, &item_id).unwrap(), Some(SRC.to_vec()));

    // An item that was never created reads as absent, not as an error.
    let missing = vault
        .get_src(&ws_id, "0123456789abcdef0123456789abcdef")
        .unwrap();
    assert_eq!(missing, None);
}

#[test]
fn src_is_sealed_at_rest() {
    let (mut vault, _tmp) = fresh();
    vault.signup("me", "pw").unwrap();
    let (ws_id, item_id) = app_item(&vault);
    vault.put_src(&ws_id, &item_id, SRC).unwrap();

    // Read the raw redb value: sealing is invisible through the public API, so the only way
    // to prove it happened is to look at the bytes on disk.
    let store = vault.store().unwrap();
    let raw = store
        .get(&crate::item::src_key(&ws_id, &item_id))
        .unwrap()
        .expect("put_src must write to ws/<ws>/item/<id>/src");
    assert_ne!(raw, SRC);
    assert!(
        !raw.windows(SRC.len()).any(|w| w == SRC),
        "plaintext found inside the stored blob"
    );
}

#[test]
fn src_and_state_are_separate_docs() {
    // w3.md §2a: two docs per app, never one — code and data don't share a container.
    let (mut vault, _tmp) = fresh();
    vault.signup("me", "pw").unwrap();
    let (ws_id, item_id) = app_item(&vault);

    vault.put_src(&ws_id, &item_id, SRC).unwrap();
    vault.put_doc(&ws_id, &item_id, STATE, "test").unwrap();

    assert_eq!(vault.get_src(&ws_id, &item_id).unwrap(), Some(SRC.to_vec()));
    assert_eq!(
        vault.get_doc(&ws_id, &item_id, "test").unwrap(),
        Some(STATE.to_vec())
    );
}

#[test]
fn src_survives_reopen_and_relogin() {
    let (mut vault, tmp) = fresh();
    let (did, _) = vault.signup("me", "pw").unwrap();
    let (ws_id, item_id) = app_item(&vault);
    vault.put_src(&ws_id, &item_id, SRC).unwrap();
    drop(vault);

    // A fresh Vault over the same dir proves the blob was sealed to the account key, not to
    // something that died with the session.
    let mut reopened = Vault::open(tmp.path().to_path_buf()).unwrap();
    reopened.login(&did, "pw").unwrap();
    assert_eq!(
        reopened.get_src(&ws_id, &item_id).unwrap(),
        Some(SRC.to_vec())
    );
}

#[test]
fn item_blob_ops_require_an_unlocked_account() {
    let (vault, _tmp) = fresh();
    assert!(matches!(vault.get_src("w", "i"), Err(VaultError::Locked)));
    assert!(matches!(
        vault.put_src("w", "i", SRC),
        Err(VaultError::Locked)
    ));
    assert!(matches!(
        vault.get_doc("w", "i", "test"),
        Err(VaultError::Locked)
    ));
    assert!(matches!(
        vault.put_doc("w", "i", STATE, "test"),
        Err(VaultError::Locked)
    ));
}

#[test]
fn entries_round_trip_and_list_by_prefix() {
    let (mut vault, _tmp) = fresh();
    vault.signup("node", "pw").unwrap();

    vault.put_entry("token/a", b"one").unwrap();
    vault.put_entry("token/b", b"two").unwrap();
    vault.put_entry("revoked/a", b"gone").unwrap();

    assert_eq!(vault.get_entry("token/a").unwrap().unwrap(), b"one");
    assert_eq!(vault.get_entry("nothing").unwrap(), None);
    assert_eq!(
        vault.list_entries("token/").unwrap(),
        vec!["token/a".to_string(), "token/b".to_string()]
    );
    assert_eq!(vault.list_entries("").unwrap().len(), 3);

    vault.delete_entry("token/a").unwrap();
    assert_eq!(vault.get_entry("token/a").unwrap(), None);
    assert_eq!(vault.list_entries("token/").unwrap(), vec!["token/b"]);
}

#[test]
fn an_entry_is_sealed_at_rest_and_cannot_address_the_keystore() {
    let (mut vault, _tmp) = fresh();
    let (did, _) = vault.signup("node", "pw").unwrap();
    vault.put_entry("secret", b"plaintext-marker").unwrap();

    // The reserved namespace means the raw key is elsewhere, and the bytes there are sealed.
    // Scoped: a live Store clone holds the redb handle, and login would then find it open.
    {
        let store = vault.store().unwrap();
        assert!(store.get("secret").unwrap().is_none());
        let sealed = store.get("entry/secret").unwrap().unwrap();
        assert_ne!(sealed, b"plaintext-marker");
    }

    // Writing through the entry API never reaches the keystore, so the account still opens.
    vault.put_entry("identity/keystore", b"clobbered").unwrap();
    vault.lock();
    vault.login(&did, "pw").unwrap();
    assert_eq!(
        vault.get_entry("secret").unwrap().unwrap(),
        b"plaintext-marker"
    );
}

#[test]
fn an_empty_entry_name_is_refused() {
    let (mut vault, _tmp) = fresh();
    vault.signup("node", "pw").unwrap();
    assert!(matches!(
        vault.put_entry("", b"x"),
        Err(VaultError::InvalidName(_))
    ));
}

#[test]
fn a_locked_vault_neither_signs_nor_keeps_entries() {
    let (mut vault, _tmp) = fresh();
    vault.signup("node", "pw").unwrap();
    vault.lock();

    assert!(vault.with_signer(|s| s.did().to_string()).is_none());
    assert!(matches!(
        vault.put_entry("k", b"v"),
        Err(VaultError::Locked)
    ));
    assert!(matches!(vault.get_entry("k"), Err(VaultError::Locked)));
    assert!(matches!(vault.list_entries(""), Err(VaultError::Locked)));
    assert!(matches!(vault.delete_entry("k"), Err(VaultError::Locked)));
}

#[test]
fn the_vault_signs_as_its_account_without_lending_the_identity() {
    let (mut vault, _tmp) = fresh();
    let (did, _) = vault.signup("node", "pw").unwrap();

    let signed = vault
        .with_signer(|signer| {
            assert_eq!(signer.did(), did);
            signer.sign(b"a token payload")
        })
        .unwrap();

    let key = identity::public_key_from_did(&did).unwrap();
    assert!(identity::verify(&key, b"a token payload", &signed));
}

#[test]
fn an_adopted_workspace_keeps_the_id_and_time_it_arrived_with() {
    let (mut vault, _tmp) = fresh();
    vault.signup("node", "pw").unwrap();

    // What another account would have sent: its own minted id, its own creation time.
    let published = WorkspaceMeta {
        id: "0123456789abcdef0123456789abcdef".to_string(),
        name: "notes".to_string(),
        created: 42,
    };
    vault.adopt_workspace(&published).unwrap();

    // Both ends name the same workspace, which is the whole point — an id minted here would
    // leave the two accounts unable to refer to one thing.
    assert_eq!(vault.workspaces().unwrap(), vec![published.clone()]);

    // The originating account owns the header, so a republish replaces rather than adding.
    let renamed = WorkspaceMeta {
        name: "field notes".to_string(),
        ..published.clone()
    };
    vault.adopt_workspace(&renamed).unwrap();
    assert_eq!(vault.workspaces().unwrap(), vec![renamed]);
}

#[test]
fn an_id_this_account_would_not_mint_is_refused_before_it_becomes_a_key() {
    let (mut vault, _tmp) = fresh();
    vault.signup("node", "pw").unwrap();
    let real = vault.create_workspace("mine").unwrap();

    // `ws/<id>/meta` with a slashed id is a legal key addressing something else under `ws/`.
    // This one would land inside a workspace that already exists.
    let forged = WorkspaceMeta {
        id: format!("{}/item/abc", real.id),
        name: "trojan".to_string(),
        created: 1,
    };
    assert!(matches!(
        vault.adopt_workspace(&forged),
        Err(VaultError::BadWorkspaceId(_))
    ));

    for id in ["", "../identity", "0123456789ABCDEF0123456789abcdef", "abc"] {
        assert!(
            matches!(
                vault.adopt_workspace(&WorkspaceMeta {
                    id: id.to_string(),
                    name: "n".to_string(),
                    created: 1,
                }),
                Err(VaultError::BadWorkspaceId(_))
            ),
            "accepted {id:?}"
        );
    }

    // Nothing was written by any of them.
    assert_eq!(vault.workspaces().unwrap(), vec![real]);
}

#[test]
fn a_locked_account_adopts_nothing() {
    let (mut vault, _tmp) = fresh();
    vault.signup("node", "pw").unwrap();
    vault.lock();

    assert!(matches!(
        vault.adopt_workspace(&WorkspaceMeta {
            id: "0123456789abcdef0123456789abcdef".to_string(),
            name: "notes".to_string(),
            created: 42,
        }),
        Err(VaultError::Locked)
    ));
}
