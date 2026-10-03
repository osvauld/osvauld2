//! T4–T5 of docs/design/search.md §0: the `index.lua` contract and its sandbox.

use crate::index::{IndexSpec, Plan};
use loro::{LoroDoc, LoroMap, LoroValue};

const CHAT: &str = r#"
return {
	doc = "channel:*",
	each = { "messages" },
	key = function(m) return m.id end,
	fields = function(_id, m, _doc)
		return { body = m.text, facet = { author = m.author }, time = m.sent_at }
	end,
	rank = "recent",
}
"#;

/// Cards in a map keyed by id, and a join: the column's *name* is indexed, which is exactly
/// what the "ids, not names" rule warns against — here on purpose, to prove a join re-runs.
const BOARD: &str = r#"
return {
	doc = "board",
	each = { "cards" },
	fields = function(id, c, doc)
		return { title = c.title, body = c.notes, facet = { column = doc.columns[c.col] } }
	end,
}
"#;

fn chat(msgs: &[(&str, &str, &str, f64)]) -> LoroDoc {
    let doc = LoroDoc::new();
    let list = doc.get_movable_list("messages");
    for (i, (id, author, text, t)) in msgs.iter().enumerate() {
        let m = list.insert_container(i, LoroMap::new()).unwrap();
        m.insert("id", *id).unwrap();
        m.insert("author", *author).unwrap();
        m.insert("text", *text).unwrap();
        m.insert("sent_at", *t).unwrap();
    }
    doc.commit();
    doc
}

fn board() -> LoroDoc {
    let doc = LoroDoc::new();
    let cols = doc.get_map("columns");
    cols.insert("c-todo", "Todo").unwrap();
    cols.insert("c-done", "Done").unwrap();
    let cards = doc.get_map("cards");
    for (id, title, col) in [("k1", "wire the bridge", "c-todo"), ("k2", "ship it", "c-done")] {
        let c = cards.insert_container(id, LoroMap::new()).unwrap();
        c.insert("title", title).unwrap();
        c.insert("notes", "").unwrap();
        c.insert("col", col).unwrap();
    }
    doc.commit();
    doc
}

fn upserted(plan: &Plan) -> Vec<&str> {
    let mut ids: Vec<&str> = plan.upserts.iter().map(|(id, _)| id.as_str()).collect();
    ids.sort();
    ids
}

fn set_text(doc: &LoroDoc, i: usize, text: &str) {
    let list = doc.get_movable_list("messages");
    let m = list.get(i).unwrap().into_container().unwrap().into_map().unwrap();
    m.insert("text", text).unwrap();
    doc.commit();
}

// ── T4: the contract ─────────────────────────────────────────────────────────

#[test]
fn a_pattern_covers_a_family_of_docs_and_a_name_covers_one() {
    let chat = IndexSpec::load(CHAT).unwrap();
    assert!(chat.covers("channel:general"));
    assert!(!chat.covers("chat"));
    assert!(!chat.covers("channel")); // the prefix must be followed by something
    let board = IndexSpec::load(BOARD).unwrap();
    assert!(board.covers("board"));
    assert!(!board.covers("boards"));
}

#[test]
fn rank_is_read_and_defaults_to_relevance() {
    assert!(IndexSpec::load(CHAT).unwrap().recent());
    assert!(!IndexSpec::load(BOARD).unwrap().recent());
}

#[test]
fn a_list_collection_is_keyed_by_key_and_fields_shape_each_record() {
    let spec = IndexSpec::load(CHAT).unwrap();
    let doc = chat(&[("m1", "anu", "the deploy is green", 1.0), ("m2", "abe", "lunch?", 2.0)]);
    let plan = spec.plan("channel:general", &doc, None).unwrap();
    assert_eq!(upserted(&plan), ["m1", "m2"]);
    assert_eq!(plan.fields_runs, 2);
    assert!(plan.deletes.is_empty() && plan.errors.is_empty());
    let (_, f) = plan.upserts.iter().find(|(id, _)| id == "m1").unwrap();
    assert_eq!(f.body.as_deref(), Some("the deploy is green"));
    assert_eq!(f.facets.get("author").map(String::as_str), Some("anu"));
    assert_eq!(f.time, Some(1.0));
    assert_eq!(f.title, None);
}

#[test]
fn a_map_collection_is_keyed_by_its_keys_and_fields_can_join() {
    let spec = IndexSpec::load(BOARD).unwrap();
    let plan = spec.plan("board", &board(), None).unwrap();
    assert_eq!(upserted(&plan), ["k1", "k2"]);
    let (_, f) = plan.upserts.iter().find(|(id, _)| id == "k1").unwrap();
    assert_eq!(f.title.as_deref(), Some("wire the bridge"));
    assert_eq!(f.facets.get("column").map(String::as_str), Some("Todo"));
}

#[test]
fn an_unchanged_doc_plans_nothing() {
    let spec = IndexSpec::load(CHAT).unwrap();
    let doc = chat(&[("m1", "anu", "hello", 1.0), ("m2", "abe", "world", 2.0)]);
    let first = spec.plan("channel:general", &doc, None).unwrap();
    let again = spec.plan("channel:general", &doc, Some(&first.prints)).unwrap();
    assert!(again.upserts.is_empty() && again.deletes.is_empty());
    assert_eq!(again.fields_runs, 0);
}

#[test]
fn an_edit_re_runs_fields_for_that_record_only() {
    let spec = IndexSpec::load(CHAT).unwrap();
    let doc = chat(&[("m1", "anu", "hello", 1.0), ("m2", "abe", "world", 2.0)]);
    let first = spec.plan("channel:general", &doc, None).unwrap();
    set_text(&doc, 1, "everyone");
    let plan = spec.plan("channel:general", &doc, Some(&first.prints)).unwrap();
    assert_eq!(upserted(&plan), ["m2"]);
    assert_eq!(plan.fields_runs, 1);
    assert_eq!(plan.upserts[0].1.body.as_deref(), Some("everyone"));
}

#[test]
fn a_concurrent_insert_shifting_positions_re_runs_nothing_old() {
    let spec = IndexSpec::load(CHAT).unwrap();
    let doc = chat(&[("m1", "anu", "hello", 1.0), ("m2", "abe", "world", 2.0)]);
    let first = spec.plan("channel:general", &doc, None).unwrap();
    let list = doc.get_movable_list("messages");
    let m = list.insert_container(0, LoroMap::new()).unwrap();
    m.insert("id", "m0").unwrap();
    m.insert("text", "first!").unwrap();
    doc.commit();
    let plan = spec.plan("channel:general", &doc, Some(&first.prints)).unwrap();
    assert_eq!(upserted(&plan), ["m0"]);
}

#[test]
fn a_removed_record_is_planned_as_a_delete() {
    let spec = IndexSpec::load(CHAT).unwrap();
    let doc = chat(&[("m1", "anu", "hello", 1.0), ("m2", "abe", "world", 2.0)]);
    let first = spec.plan("channel:general", &doc, None).unwrap();
    doc.get_movable_list("messages").delete(0, 1).unwrap();
    doc.commit();
    let plan = spec.plan("channel:general", &doc, Some(&first.prints)).unwrap();
    assert_eq!(plan.deletes, ["m1"]);
    assert!(plan.upserts.is_empty());
}

#[test]
fn a_change_outside_the_collection_re_runs_every_record() {
    let spec = IndexSpec::load(BOARD).unwrap();
    let doc = board();
    let first = spec.plan("board", &doc, None).unwrap();
    doc.get_map("columns").insert("c-todo", "Backlog").unwrap();
    doc.commit();
    let plan = spec.plan("board", &doc, Some(&first.prints)).unwrap();
    assert_eq!(upserted(&plan), ["k1", "k2"]);
    assert_eq!(plan.fields_runs, 2);
    let (_, f) = plan.upserts.iter().find(|(id, _)| id == "k1").unwrap();
    assert_eq!(f.facets.get("column").map(String::as_str), Some("Backlog"));
}

#[test]
fn fields_returning_nil_drops_the_record() {
    let spec = IndexSpec::load(
        r#"return {
			doc = "channel:*", each = { "messages" }, key = function(m) return m.id end,
			fields = function(_, m) if m.text == "" then return nil end return { body = m.text } end,
		}"#,
    )
    .unwrap();
    let doc = chat(&[("m1", "anu", "hello", 1.0)]);
    let first = spec.plan("channel:x", &doc, None).unwrap();
    assert_eq!(upserted(&first), ["m1"]);
    set_text(&doc, 0, "");
    let plan = spec.plan("channel:x", &doc, Some(&first.prints)).unwrap();
    assert!(plan.upserts.is_empty());
    assert_eq!(plan.deletes, ["m1"]);
}

#[test]
fn a_doc_without_the_collection_yet_has_no_records() {
    let spec = IndexSpec::load(CHAT).unwrap();
    let plan = spec.plan("channel:new", &LoroDoc::new(), None).unwrap();
    assert!(plan.upserts.is_empty() && plan.deletes.is_empty() && plan.errors.is_empty());
}

#[test]
fn a_list_without_key_is_an_error_naming_the_fix() {
    let spec = IndexSpec::load(
        r#"return { doc = "c", each = { "messages" }, fields = function(_, m) return { body = m.text } end }"#,
    )
    .unwrap();
    let err = spec.plan("c", &chat(&[("m1", "a", "t", 1.0)]), None).unwrap_err();
    assert!(err.contains("key"), "{err}");
}

#[test]
fn a_record_whose_key_is_missing_is_reported_and_skipped() {
    let spec = IndexSpec::load(CHAT).unwrap();
    let doc = chat(&[("m1", "anu", "hello", 1.0)]);
    let m = doc
        .get_movable_list("messages")
        .insert_container(1, LoroMap::new())
        .unwrap();
    m.insert("text", "no id here").unwrap();
    doc.commit();
    let plan = spec.plan("channel:x", &doc, None).unwrap();
    assert_eq!(upserted(&plan), ["m1"]);
    assert_eq!(plan.errors.len(), 1, "{:?}", plan.errors);
}

// ── T5: the sandbox ──────────────────────────────────────────────────────────

#[test]
fn a_spec_is_strict_about_its_own_shape() {
    for (src, needle) in [
        (r#"return { doc = "d", each = { "x" } }"#, "fields"),
        (r#"return { each = { "x" }, fields = function() end }"#, "doc"),
        (r#"return { doc = "d", fields = function() end }"#, "each"),
        (r#"return { doc = "d", each = { "x" }, fields = function() end, feilds = 1 }"#, "feilds"),
        (r#"return { doc = "d", each = { "x" }, fields = function() end, rank = "best" }"#, "rank"),
        (r#"return 42"#, "table"),
        (r#"error("boom")"#, "boom"),
    ] {
        let err = IndexSpec::load(src).err().unwrap_or_else(|| panic!("{src} loaded"));
        assert!(err.contains(needle), "{src}: {err}");
    }
}

#[test]
fn a_misspelt_field_is_an_error_for_that_record_not_silence() {
    let spec = IndexSpec::load(
        r#"return {
			doc = "c", each = { "messages" }, key = function(m) return m.id end,
			fields = function(_, m) return { bdy = m.text } end,
		}"#,
    )
    .unwrap();
    let plan = spec.plan("c", &chat(&[("m1", "a", "t", 1.0)]), None).unwrap();
    assert!(plan.upserts.is_empty());
    assert!(plan.errors[0].contains("bdy"), "{:?}", plan.errors);
}

#[test]
fn the_index_vm_cannot_reach_ui_docs_or_the_os() {
    for body in [
        "ui.text({ 'x' })",
        "doc:open('board')",
        "os.remove('x')",
        "io.open('x')",
        "gfx.solid('#fff')",
    ] {
        let src = format!(
            r#"return {{ doc = "c", each = {{ "messages" }}, key = function(m) return m.id end,
				fields = function(_, m) {body}; return {{ body = m.text }} end }}"#
        );
        let spec = IndexSpec::load(&src).unwrap();
        let plan = spec.plan("c", &chat(&[("m1", "a", "t", 1.0)]), None).unwrap();
        assert!(plan.upserts.is_empty(), "{body} ran");
        assert_eq!(plan.errors.len(), 1, "{body}");
    }
}

#[test]
fn records_are_read_only_copies() {
    // Writing to the record table must not reach the doc — it is a copy, and the doc is unchanged.
    let spec = IndexSpec::load(
        r#"return { doc = "c", each = { "messages" }, key = function(m) return m.id end,
			fields = function(_, m, d) m.text = "mutated"; d.messages = nil; return { body = m.text } end }"#,
    )
    .unwrap();
    let doc = chat(&[("m1", "a", "original", 1.0)]);
    spec.plan("c", &doc, None).unwrap();
    let v = doc.get_deep_value();
    let LoroValue::Map(root) = v else { panic!() };
    let LoroValue::List(msgs) = &root["messages"] else { panic!() };
    let LoroValue::Map(m) = &msgs[0] else { panic!() };
    assert_eq!(m["text"], LoroValue::String("original".into()));
}

#[test]
fn a_runaway_record_is_stopped_and_the_rest_still_index() {
    let spec = IndexSpec::load(
        r#"return { doc = "c", each = { "messages" }, key = function(m) return m.id end,
			fields = function(id, m) if id == "m1" then while true do end end return { body = m.text } end }"#,
    )
    .unwrap();
    let doc = chat(&[("m1", "a", "loops", 1.0), ("m2", "b", "fine", 2.0)]);
    let plan = spec.plan("c", &doc, None).unwrap();
    assert_eq!(upserted(&plan), ["m2"]);
    assert_eq!(plan.errors.len(), 1);
    // The failed record keeps no fingerprint, so the next plan tries it again.
    let again = spec.plan("c", &doc, Some(&plan.prints)).unwrap();
    assert_eq!(again.fields_runs, 1);
}

#[test]
fn changing_the_spec_re_runs_every_record() {
    let doc = chat(&[("m1", "anu", "hello", 1.0)]);
    let first = IndexSpec::load(CHAT).unwrap().plan("c:x", &doc, None).unwrap();
    let edited = CHAT.replace("body = m.text", "title = m.text");
    let plan = IndexSpec::load(&edited)
        .unwrap()
        .plan("c:x", &doc, Some(&first.prints))
        .unwrap();
    assert_eq!(plan.fields_runs, 1);
    assert_eq!(plan.upserts[0].1.title.as_deref(), Some("hello"));
}

// ── T9: `search.query` from app Lua ──────────────────────────────────────────

use crate::tests::{identity, noop_wake};
use crate::{LuaApp, LuaMsg, SearchHit};
use loro::LoroText;
use std::cell::RefCell;
use std::rc::Rc;

fn app(main: &str) -> LuaApp<LuaMsg> {
    let src = LoroDoc::new();
    let t = src
        .get_map("files")
        .insert_container("main.lua", LoroText::new())
        .unwrap();
    t.insert(0, main).unwrap();
    src.commit();
    LuaApp::open(src, Rc::new(|_| Ok(None)), noop_wake(), identity()).unwrap()
}

fn texts(app: &LuaApp<LuaMsg>) -> Vec<String> {
    fn walk(el: &runtime::ElInfo, out: &mut Vec<String>) {
        out.extend(el.text.clone());
        el.children.iter().for_each(|c| walk(c, out));
    }
    let mut out = Vec::new();
    walk(&app.view().info(), &mut out);
    out
}

const SHOWS_HITS: &str = r#"
return function()
	local hits = search.query("deploy", { limit = 5 })
	local h = hits[1]
	return ui.text({ h.doc .. "/" .. h.id .. ":" .. h.snippet .. ":" .. tostring(h.score > 0) })
end
"#;

fn one_hit() -> Vec<SearchHit> {
    vec![SearchHit {
        doc: "channel:general".into(),
        id: "m1".into(),
        score: 1.5,
        snippet: "the deploy is green".into(),
    }]
}

#[test]
fn search_query_reaches_the_host_and_returns_hits_as_tables() {
    let mut app = app(SHOWS_HITS);
    let asked = Rc::new(RefCell::new(Vec::new()));
    let log = asked.clone();
    app.set_search(Some(Rc::new(move |q: &str, limit: usize| {
        log.borrow_mut().push((q.to_string(), limit));
        Ok(one_hit())
    })));
    assert_eq!(texts(&app), ["channel:general/m1:the deploy is green:true"]);
    assert_eq!(asked.borrow()[0], ("deploy".to_string(), 5));
}

#[test]
fn search_without_a_host_index_says_so() {
    let app = app(SHOWS_HITS);
    let shown = texts(&app).join(" ");
    assert!(shown.contains("search is not available"), "{shown}");
}

#[test]
fn search_options_are_strict() {
    let mut app = app(r#"return function() search.query("x", { limt = 3 }) return ui.text({ "ran" }) end"#);
    app.set_search(Some(Rc::new(|_: &str, _: usize| Ok(Vec::new()))));
    let shown = texts(&app).join(" ");
    assert!(shown.contains("limt"), "{shown}");
}

#[test]
fn a_host_search_error_is_a_lua_error_naming_it() {
    let mut app = app(r#"return function() search.query("x") return ui.text({ "ran" }) end"#);
    app.set_search(Some(Rc::new(|_: &str, _: usize| Err("vault is locked".into()))));
    let shown = texts(&app).join(" ");
    assert!(shown.contains("vault is locked"), "{shown}");
}

#[test]
fn search_survives_a_reload() {
    let mut app = app(SHOWS_HITS);
    app.set_search(Some(Rc::new(|_: &str, _: usize| Ok(one_hit()))));
    app.reload().unwrap();
    assert_eq!(texts(&app), ["channel:general/m1:the deploy is green:true"]);
}
