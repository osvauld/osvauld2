//! T6–T8 of docs/design/search.md §0: the chat demo's real source, a real vault, the real
//! flush seam. Writes land on the doc cores directly — the way the bridge and a peer write —
//! so what is under test is everything *after* a write: flush, plan, index, query.

use super::*;
use crate::{persist, resolver};
use app_host::{LuaApp, LuaMsg};
use loro::{LoroDoc, LoroMap, LoroText};
use std::rc::Rc;
use std::sync::Arc;
use tempfile::TempDir;
use vault::{ItemKind, Vault};

const CHAT: &[&str] = &["main.lua", "model.lua", "theme.lua", "index.lua"];

struct Fixture {
    vault: Vault,
    _tmp: TempDir,
    did: String,
    ws: String,
    item: String,
}

fn fixture() -> Fixture {
    let tmp = TempDir::new().unwrap();
    let mut vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    vault.signup("me", "pw").unwrap();
    let did = vault.current().unwrap().did;
    let ws = vault.create_workspace("ws").unwrap().id;
    let item = vault.create_item(&ws, "chat", ItemKind::App).unwrap().id;
    Fixture {
        vault,
        _tmp: tmp,
        did,
        ws,
        item,
    }
}

fn chat_source() -> LoroDoc {
    let src = LoroDoc::new();
    let files = src.get_map("files");
    for name in CHAT {
        let body = std::fs::read_to_string(format!("../demo_apps/chat/{name}")).unwrap();
        files
            .insert_container(*name, LoroText::new())
            .unwrap()
            .insert(0, &body)
            .unwrap();
    }
    src.commit();
    src
}

/// The chat app, opened with its index wired the way `open_tab` wires it.
struct Open {
    app: LuaApp<LuaMsg>,
    index: Shared,
}

fn open(f: &Fixture) -> Open {
    let mut app = LuaApp::open(
        chat_source(),
        resolver(&f.vault, &f.ws, &f.item),
        Arc::new(|| {}),
        Rc::new(|m| m),
    )
    .unwrap();
    let index = Rc::new(RefCell::new(ItemIndex::open(&f.vault, &f.ws, &f.item).unwrap()));
    attach(&index, &mut app).unwrap();
    Open { app, index }
}

impl Open {
    /// What the shell does after every update: save, then index what was saved.
    fn flush(&mut self, f: &Fixture) {
        let mut dirtied = Vec::new();
        let mut put = persist(&f.vault, &f.ws, &f.item);
        self.app
            .flush(|name, bytes| {
                dirtied.push(name.to_string());
                put(name, bytes)
            })
            .unwrap();
        index_dirty(&self.index, &self.app, &dirtied).unwrap();
    }

    fn find(&self, q: &str) -> Vec<String> {
        let mut ids: Vec<String> = self
            .index
            .borrow()
            .query(q, 20)
            .unwrap()
            .into_iter()
            .map(|h| h.id)
            .collect();
        ids.sort();
        ids
    }

    fn runs(&self) -> usize {
        self.index.borrow().fields_runs
    }

    fn send(&self, channel: &str, id: &str, text: &str) {
        self.app
            .with_doc(&format!("channel:{channel}"), |doc| {
                let list = doc.get_movable_list("messages");
                let m = list.insert_container(list.len(), LoroMap::new()).unwrap();
                m.insert("id", id).unwrap();
                m.insert("author", "me").unwrap();
                m.insert("text", text).unwrap();
                m.insert("sent_at", 10.0).unwrap();
                doc.commit();
            })
            .expect("channel doc is open");
    }

    fn message(&self, channel: &str, id: &str) -> (usize, LoroMap) {
        self.app
            .with_doc(&format!("channel:{channel}"), |doc| {
                let list = doc.get_movable_list("messages");
                (0..list.len())
                    .find_map(|i| {
                        let m = list.get(i)?.into_container().ok()?.into_map().ok()?;
                        let is = m.get("id")?.into_value().ok()?.into_string().ok()?;
                        (is.as_str() == id).then_some((i, m))
                    })
                    .unwrap()
            })
            .unwrap()
    }
}

// ── T6: the chain ────────────────────────────────────────────────────────────

#[test]
fn seeded_messages_are_searchable_once_flushed() {
    let f = fixture();
    let mut o = open(&f);
    o.flush(&f);
    assert_eq!(o.find("deploy"), ["seed-1"]);
    assert_eq!(o.find("author:abe"), ["seed-2"]);
}

#[test]
fn a_write_an_edit_and_a_delete_each_reach_the_index() {
    let f = fixture();
    let mut o = open(&f);
    o.flush(&f);

    o.send("general", "m1", "marigold rollout");
    o.flush(&f);
    assert_eq!(o.find("marigold"), ["m1"]);

    let (_, m) = o.message("general", "m1");
    m.insert("text", "friday rollout").unwrap();
    o.app.with_doc("channel:general", |d| d.commit());
    o.flush(&f);
    assert!(o.find("marigold").is_empty(), "the edited-away word still matches");
    assert_eq!(o.find("friday"), ["m1"]);

    let (i, _) = o.message("general", "m1");
    o.app.with_doc("channel:general", |d| {
        d.get_movable_list("messages").delete(i, 1).unwrap();
        d.commit();
    });
    o.flush(&f);
    assert!(o.find("rollout").is_empty(), "a deleted message still matches");
}

#[test]
fn search_query_in_the_app_reaches_the_same_index() {
    let f = fixture();
    let mut o = open(&f);
    o.flush(&f);
    // The app's own search box: `search.query` from Lua, through the hook `attach` installed.
    let hits = (o.app.search_fn().unwrap())("deploy", 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "seed-1");
    assert_eq!(hits[0].doc, "channel:general");
}

// ── T7: incremental ──────────────────────────────────────────────────────────

#[test]
fn only_what_changed_re_runs_fields() {
    let f = fixture();
    let mut o = open(&f);
    o.flush(&f);
    let base = o.runs();
    assert_eq!(base, 2, "the two seeds, once each");

    o.send("general", "m1", "one");
    o.flush(&f);
    assert_eq!(o.runs() - base, 1, "a new message runs fields once");

    let (_, m) = o.message("general", "seed-1");
    m.insert("text", "the deploy is red").unwrap();
    o.app.with_doc("channel:general", |d| d.commit());
    o.flush(&f);
    assert_eq!(o.runs() - base, 2, "an edit runs fields for that message only");

    o.flush(&f);
    assert_eq!(o.runs() - base, 2, "a flush with nothing new runs nothing");
}

#[test]
fn a_doc_the_spec_does_not_cover_is_never_planned() {
    let f = fixture();
    let mut o = open(&f);
    o.flush(&f);
    let before = o.runs();
    o.app.with_doc("chat", |d| {
        d.get_movable_list("channels")
            .insert_container(2, LoroMap::new())
            .unwrap()
            .insert("id", "random")
            .unwrap();
        d.commit();
    });
    o.flush(&f);
    assert_eq!(o.runs(), before);
}

#[test]
fn an_edited_index_lua_re_indexes_everything_once() {
    let f = fixture();
    let mut o = open(&f);
    o.flush(&f);
    let before = o.runs();
    let src = o.app.read_source_file("index.lua").unwrap();
    o.app
        .write_source_file("index.lua", &src.replace("body = m.text", "title = m.text"))
        .unwrap();
    o.flush(&f);
    assert_eq!(o.runs() - before, 2, "every record, once");
    assert_eq!(o.find("deploy"), ["seed-1"]);
}

#[test]
fn a_broken_index_lua_is_reported_and_the_app_keeps_running() {
    let f = fixture();
    let mut o = open(&f);
    o.flush(&f);
    o.app.write_source_file("index.lua", "return {").unwrap();
    o.flush(&f);
    let console = o.app.console(20).join("\n");
    assert!(console.contains("index.lua"), "{console}");
    // The last good index still answers.
    assert_eq!(o.find("deploy"), ["seed-1"]);
}

// ── T8: catch-up ─────────────────────────────────────────────────────────────

#[test]
fn a_restart_finds_everything_and_re_runs_nothing() {
    let f = fixture();
    {
        let mut o = open(&f);
        o.send("general", "m1", "marigold");
        o.flush(&f);
    }
    let o = open(&f);
    assert_eq!(o.find("marigold"), ["m1"]);
    assert_eq!(o.runs(), 0);
}

#[test]
fn a_doc_written_while_the_app_was_closed_is_indexed_when_it_opens() {
    let f = fixture();
    {
        let mut o = open(&f);
        o.flush(&f);
    }
    // A peer's channel the app has never opened, landing straight in the vault.
    let peer = LoroDoc::new();
    let m = peer
        .get_movable_list("messages")
        .insert_container(0, LoroMap::new())
        .unwrap();
    m.insert("id", "p1").unwrap();
    m.insert("text", "hello from a peer").unwrap();
    peer.commit();
    f.vault
        .put_doc(&f.ws, &f.item, &peer.export(loro::ExportMode::Snapshot).unwrap(), "channel:design")
        .unwrap();

    let o = open(&f);
    assert_eq!(o.find("peer"), ["p1"]);
    assert_eq!(o.runs(), 1, "only the new record");
}

#[test]
fn a_test_tab_gets_a_working_index_that_touches_no_vault() {
    let f = fixture();
    let mut app = LuaApp::open(chat_source(), Rc::new(|_| Ok(None)), Arc::new(|| {}), Rc::new(|m| m)).unwrap();
    let index = Rc::new(RefCell::new(ItemIndex::in_memory().unwrap()));
    attach(&index, &mut app).unwrap();
    let mut dirtied = Vec::new();
    app.flush(|name, _| {
        dirtied.push(name.to_string());
        Ok(())
    })
    .unwrap();
    index_dirty(&index, &app, &dirtied).unwrap();
    assert_eq!(index.borrow().query("deploy", 5).unwrap()[0].id, "seed-1");
    assert!(f.vault.list_entries("search/").unwrap().is_empty());
    let _ = &f.did;
}
