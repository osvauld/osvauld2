//! The first demo app, rendered headlessly. `round_trip` judges the kanban's meaning under a
//! reprint; this one proves the smaller claim the demo apps exist for: the app loads, its view
//! builds a tree, and the composer's loop runs — type, add, restart, delete — against the real
//! doc binding, with no window involved.
//!
//! Closures are pulled out of the node tree and called directly, the same exception
//! `round_trip` makes for `view_fn`: what is under test is the app, not the host's dispatch.

use super::*;
use loro::LoroText;
use mlua::Table;

const SCRATCH: [(&str, &str); 2] = [
    (
        "main.lua",
        include_str!("../../../demo_apps/scratch/main.lua"),
    ),
    (
        "manifest.osv",
        include_str!("../../../demo_apps/scratch/manifest.osv"),
    ),
];

fn scratch_app(resolve: Resolve) -> LuaApp<LuaMsg> {
    let src = LoroDoc::new();
    let files = src.get_map("files");
    for (path, body) in SCRATCH {
        let t = files.insert_container(path, LoroText::new()).unwrap();
        t.insert(0, body).unwrap();
    }
    src.commit();
    LuaApp::open(src, resolve, noop_wake(), identity()).unwrap()
}

/// Every tagged table in a node tree, depth-first, splices included.
fn visit(t: &Table, f: &mut impl FnMut(&Table)) {
    if t.contains_key("tag").unwrap_or(false) {
        f(t);
    }
    for i in 1..=t.raw_len() {
        if let Ok(Value::Table(c)) = t.get::<Value>(i) {
            visit(&c, f);
        }
    }
}

/// The labels the tree would put on screen: every `ui.text` child plus bare string children.
fn texts_of(tree: &Table) -> Vec<String> {
    let mut out = Vec::new();
    visit(tree, &mut |t| {
        let tag: String = t.get("tag").unwrap();
        if tag == "text" {
            if let Ok(s) = t.get::<mlua::String>(1) {
                out.push(s.to_string_lossy().to_owned());
            }
        }
    });
    out
}

/// The view tree, built the way `view()` builds it — mirror repatched first.
fn tree_of(app: &LuaApp<LuaMsg>) -> Table {
    let _ = app.view();
    app.view_fn
        .as_ref()
        .expect("no view closure")
        .call(())
        .unwrap()
}

fn notes_len(app: &LuaApp<LuaMsg>) -> usize {
    let docs = app.docs.borrow();
    docs.get("scratch")
        .unwrap()
        .mirror
        .get::<Table>("notes")
        .unwrap()
        .raw_len()
}

#[test]
fn scratch_loads_renders_and_round_trips() {
    let mut app = scratch_app(Rc::new(|_| Ok(None)));
    assert_eq!(app.error, None, "the app did not load");

    // It renders: the seeds are on screen, and the count badge agrees with the mirror.
    let tree = tree_of(&app);
    let texts = texts_of(&tree);
    assert!(texts.iter().any(|t| t == "write a small app in lua"));
    assert!(
        texts
            .iter()
            .any(|t| t == "render it, type into it, restart it")
    );
    assert!(
        texts.iter().any(|t| t == "2"),
        "the badge shows the seed count"
    );
    assert_eq!(notes_len(&app), 2);

    // Type into the composer. `on_input` writes ui.state, which `add` reads back — the draft
    // never touches the doc, so the frame-behind rule cannot bite it.
    let input = find_input(&tree);
    input
        .get::<mlua::Function>("on_input")
        .unwrap()
        .call::<()>("hello scratch")
        .unwrap();
    let add = find_button(&tree, "add");
    add.get::<mlua::Function>("on_click")
        .unwrap()
        .call::<()>(())
        .unwrap();

    let tree = tree_of(&app);
    let texts = texts_of(&tree);
    assert!(
        texts.iter().any(|t| t == "hello scratch"),
        "the new note renders: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t == "3"),
        "the badge counts the new note"
    );
    assert_eq!(notes_len(&app), 3);

    // Restart: flush to a snapshot, reopen with a resolver that serves it. The board the new
    // VM sees is the board this one left.
    let mut saved = None;
    app.flush(|_name, bytes| {
        saved = Some(bytes.to_vec());
        Ok(())
    })
    .unwrap();
    let bytes = saved.expect("the flush wrote a doc");
    let app = scratch_app(Rc::new(move |_| Ok(Some(bytes.clone()))));
    let tree = tree_of(&app);
    assert!(
        texts_of(&tree).iter().any(|t| t == "hello scratch"),
        "the note survived the restart"
    );

    // And delete it, by the button inside its own row.
    let row = find_row(&tree, "hello scratch");
    let delete = find_button(&row, "x");
    delete
        .get::<mlua::Function>("on_click")
        .unwrap()
        .call::<()>(())
        .unwrap();
    let tree = tree_of(&app);
    assert!(
        !texts_of(&tree).iter().any(|t| t == "hello scratch"),
        "the note is gone from the view"
    );
    assert_eq!(notes_len(&app), 2);
}

fn find_input(tree: &Table) -> Table {
    let mut hit = None;
    visit(tree, &mut |t| {
        if (t.get::<String>("tag").as_deref().ok()) == Some("input")
            && (t.get::<String>("id").as_deref().ok()) == Some("draft")
        {
            hit = Some(t.clone());
        }
    });
    hit.expect("the draft input is in the tree")
}

/// The button whose first label is `label` — buttons here carry exactly one `ui.text` child.
fn find_button(tree: &Table, label: &str) -> Table {
    let mut hit = None;
    visit(tree, &mut |t| {
        if (t.get::<String>("tag").as_deref().ok()) == Some("button")
            && texts_of(t).iter().any(|s| s == label)
        {
            hit = Some(t.clone());
        }
    });
    hit.unwrap_or_else(|| panic!("no button labelled {label:?}"))
}

fn find_row(tree: &Table, contains: &str) -> Table {
    let mut hit = None;
    visit(tree, &mut |t| {
        if (t.get::<String>("tag").as_deref().ok()) == Some("row")
            && texts_of(t).iter().any(|s| s == contains)
        {
            hit = Some(t.clone());
        }
    });
    hit.unwrap_or_else(|| panic!("no row containing {contains:?}"))
}
