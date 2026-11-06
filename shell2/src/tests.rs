//! The persistence seam: `resolver` and `persist` against a real `Vault`.
//!
//! Both ends of this chain are already covered elsewhere — `app_host` proves `flush` exports
//! the right bytes at the right moment against a stub `put`, and `vault` proves a blob written
//! under a key comes back sealed and intact. What neither can see is the *join*: that the two
//! halves agree on which `(workspace, item, name)` triple a doc lives at, and that a read
//! failure stays a failure. Every bug this file can catch is a bug in that agreement.

use super::{persist, resolver, screenshot_spec};
use app_host::{DocKey, LuaApp, LuaMsg, Resolve, Wake};
use loro::{LoroDoc, LoroText, LoroValue, ValueOrContainer};
use std::rc::Rc;
use tempfile::TempDir;
use vault::{ASSET_DOC, ItemKind, Vault, WorkspaceMeta};

/// No window to repaint — the poke itself is covered in `app_host`.
fn board() -> DocKey {
    DocKey::Own("board".to_string())
}

fn noop_wake() -> Wake {
    std::sync::Arc::new(|| {})
}

#[test]
fn folder_import_button_follows_build_capability() {
    fn has_upload(node: &runtime::ElInfo) -> bool {
        node.id.as_deref() == Some("add_item") || node.children.iter().any(has_upload)
    }
    let tmp = TempDir::new().unwrap();
    let vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    let mut screen = crate::item::ItemsScreen::new(&vault, WorkspaceMeta {
        id: "0123456789abcdef0123456789abcdef".into(),
        name: "test".into(), created: 0,
    });
    assert_eq!(has_upload(&screen.view(None, None).info()),
        cfg!(all(feature = "folder-picker", not(target_os = "android"))));
    #[cfg(not(all(feature = "folder-picker", not(target_os = "android"))))]
    assert_eq!(screen.upload(&vault).unwrap_err(), "folder import is unavailable in this build");
    // The mutable binding is used only in builds without a picker.
    let _ = &mut screen;
}

pub(super) fn screen_rects(view: impl Fn() -> runtime::El<crate::Msg>, width: f32) -> Vec<runtime::ElRect> {
    screen_rects_at(view, (width, 720.0))
}

fn screen_rects_at(view: impl Fn() -> runtime::El<crate::Msg>, viewport: (f32, f32)) -> Vec<runtime::ElRect> {
    struct ScreenView<F>(F);
    impl<F: Fn() -> runtime::El<crate::Msg>> runtime::App for ScreenView<F> {
        type Msg = crate::Msg;
        fn view(&self) -> runtime::El<Self::Msg> { (self.0)() }
        fn update(&mut self, _: Self::Msg) {}
    }
    runtime::Headless::new(ScreenView(view), viewport).rects()
}

pub(super) fn assert_fits(rect: &runtime::ElRect, width: f32) {
    assert!(rect.w > 0.0 && rect.h > 0.0, "{rect:?}");
    assert!(rect.x >= -0.1 && rect.x + rect.w <= width + 0.1, "{rect:?}, viewport {width}");
    assert!(rect.y >= -0.1 && rect.y + rect.h <= 720.1, "{rect:?}");
}

#[test]
fn auth_and_empty_workspace_panels_fit_phones_and_keep_desktop_widths() {
    let (vault, _tmp, _, _) = fresh();
    let login = crate::login::LoginScreen::new(&vault);
    let signup = crate::signup::SignupForm::default();
    let empty = TempDir::new().unwrap();
    let empty_vault = Vault::open(empty.path().to_path_buf()).unwrap();
    let spaces = crate::space::SpaceScreen::new(&empty_vault);
    for width in [320.0_f32, 360.0, 393.0, 900.0] {
        for (rects, fields, cap) in [
            (screen_rects(|| login.view(), width), vec!["passphrase", "login"], 460.0),
            (screen_rects(|| signup.view(), width), vec!["label", "password", "submit"], 360.0),
            (screen_rects(|| spaces.view(None, None), width), vec!["workspace_name"], 420.0),
        ] {
            for id in fields {
                let rect = rects.iter().find(|r| r.id == id).unwrap();
                assert_fits(rect, width);
                assert!((rect.w - (width - 32.0).min(cap)).abs() < 0.1, "{rect:?}");
                assert!(rect.x >= 15.9);
            }
        }
    }
}

#[test]
fn login_account_list_yields_height_without_squeezing_the_form() {
    let (vault, _tmp, _, _) = fresh();
    let login = crate::login::LoginScreen::new(&vault);
    for height in [320.0, 450.0, 500.0, 720.0] {
        let rects = screen_rects_at(|| login.view(), (393.0, height));
        for (id, expected) in [("passphrase", 40.0), ("login", 44.0), ("add_account", 32.0)] {
            let rect = rects.iter().find(|r| r.id == id).unwrap();
            assert!(rect.y >= 0.0 && rect.y + rect.h <= height, "{rect:?} at {height}");
            assert!((rect.h - expected).abs() < 0.1, "{rect:?}");
        }
    }
}

#[test]
fn workspace_header_wraps_without_squeezing_controls_or_cards() {
    let (vault, _tmp, ws_id, _) = fresh();
    let spaces = crate::space::SpaceScreen::new(&vault);
    for width in [320.0, 360.0, 393.0, 900.0] {
        let rects = screen_rects(|| spaces.view(None, None), width);
        for id in ["claim_node", "add_space", &ws_id] {
            assert_fits(rects.iter().find(|r| r.id == id).unwrap(), width);
        }
        let card = rects.iter().find(|r| r.id == ws_id).unwrap();
        assert!((card.w - 240.0).abs() < 0.1, "{card:?}");
        let add = rects.iter().find(|r| r.id == "add_space").unwrap();
        let claim = rects.iter().find(|r| r.id == "claim_node").unwrap();
        // Beside the join button when there is room, on the next row when there isn't — never
        // over it. (Since every account hosts itself the join button is a short "join", so a
        // phone width no longer forces the wrap it did when a node label stood there.)
        let beside = add.y == claim.y && add.x >= claim.x + claim.w;
        let below = add.y >= claim.y + claim.h;
        assert!(beside || below, "header squeezed at {width}: {rects:?}");
        if width == 900.0 {
            assert!(beside, "header wrapped at {width}: {rects:?}");
        }
    }
}

#[test]
fn screenshot_dimensions_are_bounded_and_paired() {
    assert_eq!(screenshot_spec(None, None, None), Ok((None, None)));
    assert_eq!(
        screenshot_spec(Some(320.0), Some(240.0), Some(2.0)),
        Ok((Some((320.0, 240.0)), Some(2.0)))
    );
    assert!(screenshot_spec(Some(320.0), None, None).is_err());
    assert!(screenshot_spec(Some(-1.0), Some(240.0), None).is_err());
    assert!(screenshot_spec(None, None, Some(4.1)).is_err());
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

    let read_back = resolver(&vault, &ws, &item)(&board())
        .expect("a healthy vault must not error")
        .expect("the doc persist just wrote must resolve");
    assert_eq!(read_back, stored);
}

fn hero() -> DocKey {
    DocKey::Asset("hero".to_string())
}

/// `writer`, writing to the workspace asset `hero` instead of its own board.
fn asset_writer(resolve: Resolve, title: &str) -> LuaApp<LuaMsg> {
    let src = LoroDoc::new();
    let main = src
        .get_map("files")
        .insert_container("main.lua", LoroText::new())
        .unwrap();
    main.insert(
        0,
        &format!(
            r#"
            local h = asset:open("hero")
            h:set({{"meta"}}, doc.map{{ title = "{title}" }})
            return function() return ui.text{{ "writer" }} end
            "#
        ),
    )
    .unwrap();
    src.commit();
    LuaApp::open(src, resolve, noop_wake(), Rc::new(|m| m)).unwrap()
}

/// The point of an asset: one app writes it, a different app reads it. Its first save creates
/// the asset item, and its bytes live there — not under the app that wrote them.
#[test]
fn an_asset_one_app_saves_another_app_reads() {
    let (vault, _tmp, ws, writer_app) = fresh();
    let reader_app = vault.create_item(&ws, "game", ItemKind::App).unwrap().id;
    assert_eq!(resolver(&vault, &ws, &reader_app)(&hero()).unwrap(), None);

    let mut app = asset_writer(resolver(&vault, &ws, &writer_app), "drawn");
    app.flush(persist(&vault, &ws, &writer_app)).unwrap();

    let asset = vault
        .item_named(&ws, "hero", ItemKind::Asset)
        .unwrap()
        .expect("the first save creates the asset");
    let stored = vault.get_doc(&ws, &asset.id, ASSET_DOC).unwrap().unwrap();
    assert_eq!(title_in(&stored).as_deref(), Some("drawn"));
    assert_eq!(vault.get_doc(&ws, &writer_app, "hero").unwrap(), None);

    let read = resolver(&vault, &ws, &reader_app)(&hero())
        .unwrap()
        .unwrap();
    assert_eq!(read, stored);
}

/// A second save finds the asset the first one made rather than minting another.
#[test]
fn saving_an_asset_twice_keeps_one_item() {
    let (vault, _tmp, ws, item) = fresh();
    for title in ["one", "two"] {
        let mut app = asset_writer(resolver(&vault, &ws, &item), title);
        app.flush(persist(&vault, &ws, &item)).unwrap();
    }
    let assets = vault.items(&ws).unwrap();
    let heroes = assets.iter().filter(|i| i.kind == ItemKind::Asset).count();
    assert_eq!(heroes, 1);
}

/// The ordinary first run. `Ok(None)` has to stay distinct from both an error and an empty
/// doc, because it is the one case where opening empty is correct.
#[test]
fn a_never_saved_doc_reads_as_absent() {
    let (vault, _tmp, ws, item) = fresh();
    assert_eq!(resolver(&vault, &ws, &item)(&board()).unwrap(), None);
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

    let got = resolver(&vault, &ws, &item)(&board());
    assert!(
        got.is_err(),
        "a locked vault resolved to {got:?} — the real board is still on disk, and treating \
         this as `no doc yet` is what lets the next flush overwrite it"
    );
    // The write half has to refuse for the same reason, rather than silently dropping bytes.
    assert!(persist(&vault, &ws, &item)(&board(), b"snapshot").is_err());
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
        let bytes = resolver(&vault, &ws, item)(&board()).unwrap().unwrap();
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

    assert_eq!(resolver(&vault, &ws_b, &item)(&board()).unwrap(), None);
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
    let bytes = resolver(&reopened, &ws, &item)(&board())
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

// ── bridge answers: the read-only family, without standing up a window ──────────────────

use super::{AuthOutcome, Screen, answer, finish_auth, resolve_did, unlocks};

#[test]
fn a_screen_move_is_an_unlock_only_from_the_lock_screen_onto_spaces() {
    let (vault, _tmp, _ws, _item) = fresh();
    let login = Screen::Login(crate::login::LoginScreen::new(&vault));
    let phrase = Screen::Mnemonic(crate::mnemonic::Mnemonic { words: String::new(), accepted: true });
    let spaces = Screen::Spaces(SpaceScreen::new(&vault));
    let signup = Screen::Signup(crate::signup::SignupForm::default());
    // The lock screen's login and a signup's last step land on Spaces with the vault open:
    // what starts the node, the listener and the endpoint when no bridge did the unlock.
    assert!(unlocks(&login, Some(&spaces)));
    assert!(unlocks(&phrase, Some(&spaces)));
    // Staying put, adding an account, or a screen that was never locked: not an unlock.
    assert!(!unlocks(&login, None));
    assert!(!unlocks(&login, Some(&signup)));
    assert!(!unlocks(&spaces, Some(&spaces)));
    assert!(!unlocks(&signup, Some(&spaces)));
}
use osvauld_rpc::{Request, Response};

#[test]
fn bridge_answers_ping_and_lists_a_fresh_vaults_accounts() {
    let tmp = TempDir::new().unwrap();
    let vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    assert!(matches!(answer(&vault, Request::Ping), Response::Ok { result } if result == "pong"));
    // A fresh vault has zero accounts — that is a successful empty list, not an error.
    assert!(matches!(
        answer(&vault, Request::ListAccounts),
        Response::Ok { .. }
    ));
}

#[test]
fn bridge_workspaces_refuse_while_locked_and_list_once_open() {
    let tmp = TempDir::new().unwrap();
    let mut vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    assert!(matches!(
        answer(&vault, Request::ListWorkspaces),
        Response::Err { .. }
    ));
    vault.signup("me", "correct horse").unwrap();
    vault.create_workspace("personal").unwrap();
    let back = answer(&vault, Request::ListWorkspaces);
    match back {
        Response::Ok { result } => assert!(result.to_string().contains("personal")),
        _ => panic!("expected an ok listing"),
    }
}

#[test]
fn bridge_unwired_families_err_honestly() {
    let (vault, _tmp, _ws, _item) = fresh();
    let back = answer(
        &vault,
        Request::Click {
            item_id: "i-9".into(),
            el_id: "7f".into(),
        },
    );
    assert!(matches!(back, Response::Err { message } if message.contains("not wired")));
}

#[test]
fn resolve_did_accepts_a_label_or_a_did_and_refuses_a_stranger() {
    let (vault, _tmp, _ws, _item) = fresh();
    let did = vault.accounts().unwrap()[0].did.clone();
    assert_eq!(resolve_did(&vault, "me").unwrap(), did);
    assert_eq!(resolve_did(&vault, &did).unwrap(), did);
    assert!(resolve_did(&vault, "nobody").is_err());
}

#[test]
fn finish_auth_answers_and_transitions() {
    let (mut vault, _tmp, _ws, _item) = fresh();
    // Signup success: the mnemonic rides the response (never a second time), the window
    // lands on Spaces — the script is the mnemonic's reader, so no mnemonic screen.
    let job = std::sync::Arc::new(std::sync::Mutex::new(Some(
        Vault::prepare_signup("new", "pw").map_err(|e| e.to_string()),
    )));
    let (resp, next) = finish_auth(&mut vault, AuthOutcome::Signup(job));
    assert!(
        matches!(resp, Response::Ok { ref result } if result["mnemonic"].as_str().unwrap().contains(' '))
    );
    assert!(matches!(next, Some(Screen::Spaces(_))));
    // Unlock failure: an error, and the screen does not move.
    let job = std::sync::Arc::new(std::sync::Mutex::new(Some(Err("bad passphrase".into()))));
    let (resp, next) = finish_auth(&mut vault, AuthOutcome::Unlock(job));
    assert!(matches!(resp, Response::Err { .. }));
    assert!(matches!(next, None));
}

// ── slice 4: workspaces/items over the bridge ──────────────────────────────────────────

use super::{
    ItemsScreen, SpaceScreen, find_item, kind_from_str, refresh_after_item, refresh_after_workspace,
};

#[test]
fn kind_parse_is_exact() {
    assert!(matches!(kind_from_str("app").unwrap(), ItemKind::App));
    assert!(matches!(kind_from_str("doc").unwrap(), ItemKind::Doc));
    assert!(
        kind_from_str("App").is_err(),
        "kinds are lower-case tags, not case-insensitive"
    );
}

#[test]
fn find_item_resolves_an_id_across_workspaces() {
    let (vault, _tmp, ws, item) = fresh();
    let wi = find_item(&vault, &item).unwrap();
    assert_eq!(
        (wi.id.as_str(), wi.ws_id.as_str()),
        (item.as_str(), ws.as_str())
    );
    assert!(find_item(&vault, "missing").is_err());
}

#[test]
fn a_bridge_write_rebuilds_only_the_screen_that_shows_it() {
    let (vault, _tmp, ws, _item) = fresh();
    // Spaces showing → a new workspace appears in the snapshot after refresh.
    let mut screen = Screen::Spaces(SpaceScreen::new(&vault));
    vault.create_workspace("second").unwrap();
    refresh_after_workspace(&mut screen, &vault);
    match &screen {
        Screen::Spaces(s) => assert_eq!(
            s.spaces().len(),
            2,
            "snapshot must include the new workspace"
        ),
        _ => panic!("refresh must keep the Spaces screen"),
    }
    // Items showing the touched ws → rebuilt with the new item.
    let meta = vault
        .workspaces()
        .unwrap()
        .into_iter()
        .find(|w| w.id == ws)
        .unwrap();
    let mut screen = Screen::Items(ItemsScreen::new(&vault, meta));
    vault.create_item(&ws, "tally", ItemKind::App).unwrap();
    refresh_after_item(&mut screen, &vault, &ws);
    // A different ws (or another screen entirely) is untouched.
    let other = vault.create_workspace("elsewhere").unwrap();
    refresh_after_item(&mut screen, &vault, &other.id);
    assert!(
        matches!(screen, Screen::Items(_)),
        "an unrelated write must not rebuild this screen"
    );
}

#[test]
fn refresh_shows_a_workspace_adopted_while_the_empty_state_was_showing() {
    // A joinee's exact starting state: zero workspaces of their own, so `SpaceScreen::view()`
    // is showing its empty "press ⏎ to create" branch — the same branch that never went away
    // once `JoinItem` adopted a workspace behind it without a matching refresh call.
    let tmp = TempDir::new().unwrap();
    let mut vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    vault.signup("bob", "pw").unwrap();
    let mut screen = Screen::Spaces(SpaceScreen::new(&vault));
    match &screen {
        Screen::Spaces(s) => assert!(s.spaces().is_empty()),
        _ => unreachable!(),
    }

    // `adopt_workspace`, not `create_workspace` — the exact write `join_item` makes.
    vault
        .adopt_workspace(&WorkspaceMeta {
            id: "a".repeat(32),
            name: "joined".to_string(),
            created: 1,
        })
        .unwrap();
    refresh_after_workspace(&mut screen, &vault);

    match &screen {
        Screen::Spaces(s) => assert_eq!(s.spaces().len(), 1, "the adopted workspace must show"),
        _ => panic!("refresh must keep the Spaces screen"),
    }
}
