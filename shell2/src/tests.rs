//! The persistence seam: `resolver` and `persist` against a real `Vault`.
//!
//! Both ends of this chain are already covered elsewhere — `app_host` proves `flush` exports
//! the right bytes at the right moment against a stub `put`, and `vault` proves a blob written
//! under a key comes back sealed and intact. What neither can see is the *join*: that the two
//! halves agree on which `(workspace, item, name)` triple a doc lives at, and that a read
//! failure stays a failure. Every bug this file can catch is a bug in that agreement.

use super::{persist, resolver};
use app_host::{LuaApp, LuaMsg, Resolve, Wake};
use loro::{LoroDoc, LoroText, LoroValue, ValueOrContainer};
use std::rc::Rc;
use tempfile::TempDir;
use vault::{ItemKind, Vault};

/// No window to repaint — the poke itself is covered in `app_host`.
fn noop_wake() -> Wake {
    std::sync::Arc::new(|| {})
}

/// An unlocked vault holding one workspace with one app item.
fn fresh() -> (Vault, TempDir, String, String) {
    let tmp = TempDir::new().unwrap();
    let mut vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    vault.signup("me", "pw").unwrap();
    let (ws, item) = app_item(&vault);
    (vault, tmp, ws, item)
}

fn app_item(vault: &Vault) -> (String, String) {
    let ws = vault.create_workspace("ws").unwrap();
    let item = vault.create_item(&ws.id, "app", ItemKind::App).unwrap();
    (ws.id, item.id)
}

/// An app that opens `board` and writes one field at module scope — the smallest thing that
/// gives `flush` something to do. `:set` puts scalars into a root map, which is the whole of
/// the Lua write path today.
///
/// If the write never lands the doc stays clean, `flush` writes nothing, and every assertion
/// below fails on a missing key rather than on a wrong value. That is deliberate: `LuaApp::open`
/// captures a Lua error instead of returning it, so a silent no-op is the failure mode to guard.
fn writer(resolve: Resolve, title: &str) -> LuaApp<LuaMsg> {
    let src = LoroDoc::new();
    let files = src.get_map("files");
    let main = files.insert_container("main.lua", LoroText::new()).unwrap();
    main.insert(
        0,
        &format!(
            r#"
            local b = doc:open("board")
            b:set({{"meta"}}, doc.map{{ title = "{title}" }})
            return function() return ui.text{{ b.meta.title }} end
            "#
        ),
    )
    .unwrap();
    src.commit();
    LuaApp::open(src, resolve, noop_wake(), Rc::new(|m| m)).unwrap()
}

/// `meta.title` inside a snapshot, as the vault would hand it back.
fn title_in(snapshot: &[u8]) -> Option<String> {
    let doc = LoroDoc::new();
    doc.import(snapshot).ok()?;
    match doc.get_map("meta").get("title")? {
        ValueOrContainer::Value(LoroValue::String(s)) => Some(s.to_string()),
        _ => None,
    }
}

/// The whole chain in one test: Lua writes, `flush` exports, `persist` seals it into the vault
/// under the doc's own name, and `resolver` hands back those same bytes.
#[test]
fn a_doc_round_trips_through_the_vault() {
    let (vault, _tmp, ws, item) = fresh();

    let mut app = writer(resolver(&vault, &ws, &item), "saved");
    app.flush(persist(&vault, &ws, &item)).unwrap();

    // The doc name is the last path segment, not the item id — `board`, not `item`.
    let stored = vault
        .get_doc(&ws, &item, "board")
        .unwrap()
        .expect("flush should have written the board");
    assert_eq!(title_in(&stored).as_deref(), Some("saved"));

    let read_back = resolver(&vault, &ws, &item)("board")
        .expect("a healthy vault must not error")
        .expect("the doc persist just wrote must resolve");
    assert_eq!(read_back, stored);
}

/// The ordinary first run. `Ok(None)` has to stay distinct from both an error and an empty
/// doc, because it is the one case where opening empty is correct.
#[test]
fn a_never_saved_doc_reads_as_absent() {
    let (vault, _tmp, ws, item) = fresh();
    assert_eq!(resolver(&vault, &ws, &item)("board").unwrap(), None);
}

/// The regression test for `.ok().flatten()`.
///
/// A vault that cannot be read must surface an error, never `None`. Collapsing the two makes an
/// unreadable vault look like a board nobody has saved yet — the app opens empty, and the first
/// flush writes that emptiness over the real data. Harmless before persistence existed; silent
/// data loss now.
#[test]
fn a_read_failure_is_an_error_not_an_absent_doc() {
    let (mut vault, _tmp, ws, item) = fresh();
    let mut app = writer(resolver(&vault, &ws, &item), "real data");
    app.flush(persist(&vault, &ws, &item)).unwrap();

    vault.lock();

    let got = resolver(&vault, &ws, &item)("board");
    assert!(
        got.is_err(),
        "a locked vault resolved to {got:?} — the real board is still on disk, and treating \
         this as `no doc yet` is what lets the next flush overwrite it"
    );
    // The write half has to refuse for the same reason, rather than silently dropping bytes.
    assert!(persist(&vault, &ws, &item)("board", b"snapshot").is_err());
}

/// What `OpenApp::ws_id` exists for. Two apps in the same workspace both keep a doc called
/// `board`; the ids are the only thing keeping them apart, and nothing else in the process
/// remembers `ws_id` once `open_tab` has returned.
#[test]
fn docs_are_scoped_to_their_item() {
    let (vault, _tmp, ws, item_a) = fresh();
    let item_b = vault.create_item(&ws, "other", ItemKind::App).unwrap().id;

    let mut a = writer(resolver(&vault, &ws, &item_a), "board A");
    a.flush(persist(&vault, &ws, &item_a)).unwrap();
    let mut b = writer(resolver(&vault, &ws, &item_b), "board B");
    b.flush(persist(&vault, &ws, &item_b)).unwrap();

    let read = |item: &str| {
        let bytes = resolver(&vault, &ws, item)("board").unwrap().unwrap();
        title_in(&bytes).unwrap()
    };
    assert_eq!(read(&item_a), "board A");
    assert_eq!(read(&item_b), "board B");
}

/// A doc named on one item must not be visible from another workspace holding the same item id.
/// `doc_key` interpolates both, so this only fails if one of them stops being part of the key.
#[test]
fn docs_are_scoped_to_their_workspace() {
    let (vault, _tmp, ws_a, item) = fresh();
    let ws_b = vault.create_workspace("other").unwrap().id;

    let mut app = writer(resolver(&vault, &ws_a, &item), "saved");
    app.flush(persist(&vault, &ws_a, &item)).unwrap();

    assert_eq!(resolver(&vault, &ws_b, &item)("board").unwrap(), None);
}

/// Sealed to the account key, not to the session. This is the assertion `a_card_survives_a_restart`
/// makes against a stub resolver — repeated here against the disk it actually has to survive.
#[test]
fn a_doc_survives_a_vault_reopen() {
    let tmp = TempDir::new().unwrap();
    let mut vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    let (did, _mnemonic) = vault.signup("me", "pw").unwrap();
    let (ws, item) = app_item(&vault);

    let mut app = writer(resolver(&vault, &ws, &item), "survives");
    app.flush(persist(&vault, &ws, &item)).unwrap();
    drop(app);
    drop(vault);

    let mut reopened = Vault::open(tmp.path().to_path_buf()).unwrap();
    reopened.login(&did, "pw").unwrap();
    let bytes = resolver(&reopened, &ws, &item)("board")
        .unwrap()
        .expect("the board should still be on disk");
    assert_eq!(title_in(&bytes).as_deref(), Some("survives"));
}

/// `flush` is the only thing that decides *whether* to write, so the seam must not write on its
/// own. A doc that has not changed since its last save leaves the vault untouched.
#[test]
fn a_clean_doc_writes_nothing() {
    let (vault, _tmp, ws, item) = fresh();
    let mut app = writer(resolver(&vault, &ws, &item), "saved");
    app.flush(persist(&vault, &ws, &item)).unwrap();
    let first = vault.get_doc(&ws, &item, "board").unwrap().unwrap();

    app.flush(persist(&vault, &ws, &item)).unwrap();
    let second = vault.get_doc(&ws, &item, "board").unwrap().unwrap();

    // Byte equality is the point: a re-export would reseal with a fresh nonce and differ even
    // though the doc did not change.
    assert_eq!(first, second, "a clean doc was written again");
}
