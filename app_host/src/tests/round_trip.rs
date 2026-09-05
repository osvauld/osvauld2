//! Condition 3 in its strong form (docs/design/code-as-tree.md §9).
//!
//! `lua_tree`'s own suite checks that printed source is still *Lua* — it parses, it is idempotent,
//! it keeps its comments. None of that says it still means what it meant. A sabotage that sorted a
//! table's entries passed every one of those tests while destroying child order, which is what
//! sent this here: the only judge of meaning is the host that runs the program.
//!
//! So these run the real kanban twice — once from the source that ships, once from that source
//! printed through `lua_tree` — and compare the three things an app actually produces: the node
//! tree its `view()` builds, the elements `walk` turns that into, and the doc `model.lua` seeds.

use super::*;
use loro::LoroValue;

/// Every kanban file, through parse and print. A parse failure here is `lua_tree`'s to answer,
/// not this module's — its own tests cover the corpus, so a panic means the corpus moved.
///
/// `print_bare`, not `print`, and the reason is the finding below: source with `_nid` stamped into
/// it does not run at all, so it cannot be the thing a meaning test compares.
fn printed() -> Vec<(String, String)> {
    reprint(lua_tree::print_bare)
}

fn reprint(f: fn(&lua_tree::Block) -> String) -> Vec<(String, String)> {
    KANBAN
        .iter()
        .map(|(path, body)| {
            let tree = lua_tree::parse(body).unwrap_or_else(|e| panic!("{path}: {e:?}"));
            (path.to_string(), f(&tree))
        })
        .collect()
}

fn open(files: &[(String, String)]) -> LuaApp<LuaMsg> {
    let app = app_from(
        files.iter().map(|(p, b)| (p.as_str(), b.as_str())),
        Rc::new(|_| Ok(None)),
    );
    assert_eq!(app.error, None, "the printed source did not load");
    app
}

/// The node tree `view()` builds, as lines — `view()` first, because it is what repatches the
/// mirror the closure reads, and the closure is called directly because `LuaApp::view` swallows a
/// Lua error into a `text("View error: …")` element that would compare equal to nothing at all.
fn tree_of(app: &LuaApp<LuaMsg>) -> Vec<String> {
    let _ = app.view();
    let view_fn = app.view_fn.as_ref().expect("no view closure");
    let tree = match view_fn.call::<Table>(()) {
        Ok(t) => t,
        Err(e) => panic!("the view failed to build: {e}"),
    };
    let mut out = Vec::new();
    write_table(&tree, 0, &mut out);
    out
}

/// A canonical rendering of a `ui.*` node tree: named keys sorted, positional children in index
/// order, nested tables as indented blocks. Two keys are skipped, and the pair of them is the
/// argument for `_nid` in miniature.
///
/// `_nid` is what the printer *adds*, so comparing it would compare the two files rather than the
/// two programs. `line` is what the printer *breaks*: the prelude stamps it as `debug.info(2,"l")`
/// at construction, so reformatting moves it. The provenance the host already had cannot survive a
/// reprint; the whole point of the other one is that it can.
///
/// Functions render as `fn`, so this cannot tell two handlers apart — swapping two `on_click`
/// bodies would pass. It judges structure, not behaviour.
fn write_table(t: &Table, depth: usize, out: &mut Vec<String>) {
    // The view tree is a handful of levels deep. This only fires if a prop points back at an
    // ancestor, and a truncated failure beats a test that hangs.
    if depth > 64 {
        out.push(format!("{}…", "  ".repeat(depth)));
        return;
    }
    let (mut named, mut positional) = (Vec::new(), Vec::new());
    for pair in t.pairs::<Value, Value>() {
        let (k, v) = pair.expect("reading a node's keys");
        match k {
            Value::Integer(i) => positional.push((i, v)),
            // Luau has no integer subtype, so an array index arrives as a double.
            Value::Number(n) => positional.push((n as mlua::Integer, v)),
            Value::String(s) => {
                let k = s.to_string_lossy().to_string();
                if k != "line" && k != lua_tree::NID_KEY {
                    named.push((k, v));
                }
            }
            other => named.push((format!("<{}>", other.type_name()), v)),
        }
    }
    named.sort_by(|a, b| a.0.cmp(&b.0));
    positional.sort_by_key(|(i, _)| *i);

    for (k, v) in named {
        entry(&k, &v, depth, out);
    }
    for (i, v) in positional {
        entry(&format!("[{i}]"), &v, depth, out);
    }
}

fn entry(key: &str, v: &Value, depth: usize, out: &mut Vec<String>) {
    let pad = "  ".repeat(depth);
    match v {
        Value::Table(t) => {
            out.push(format!("{pad}{key}:"));
            write_table(t, depth + 1, out);
        }
        _ => out.push(format!("{pad}{key} = {}", scalar(v))),
    }
}

fn scalar(v: &Value) -> String {
    match v {
        Value::Nil => "nil".into(),
        Value::Boolean(b) => b.to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("{:?}", s.to_string_lossy()),
        Value::Function(_) => "fn".into(),
        other => format!("<{}>", other.type_name()),
    }
}

/// `assert_eq!` on two thousand-line renderings names the failure and hides it. This points at
/// the one line that diverged, with what came before it.
fn diff(a: &[String], b: &[String]) -> Option<String> {
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        if x != y {
            let context: String = a[i.saturating_sub(4)..i]
                .iter()
                .map(|l| format!("      {l}\n"))
                .collect();
            return Some(format!(
                "line {i}:\n{context}  original: {x}\n   printed: {y}"
            ));
        }
    }
    (a.len() != b.len()).then(|| {
        let longer = if a.len() > b.len() { a } else { b };
        format!(
            "same prefix, different length: original {} lines, printed {} — first extra line: {}",
            a.len(),
            b.len(),
            longer[a.len().min(b.len())]
        )
    })
}

// ---------------------------------------------------------------- the three outputs

/// The finding this module was written to look for, in the place it actually turned up.
///
/// §4 assumed an id could ride into the VM as an ordinary field. It cannot, because the printer
/// stamps `_nid` into *every table constructor* and only some tables are elements. `doc.list({ … })`
/// takes positional entries only — a deliberate guard, so that a named field cannot vanish into a
/// list silently — and `model.lua` seeds the board through it at module scope. So the app does not
/// merely render wrong: it fails to load, before `view()` is ever reached and well before
/// `props::apply` gets its own chance to reject the key.
///
/// The narrow reading is that two host functions need teaching. The wider one is that ids in text
/// are a claim about every table in the file, which is more than the design meant to claim — and
/// that is §12's argument for identity living on the Loro node instead (level 3), where the text
/// carries none of it. This test pins the defect: it fails the day the answer is chosen, which is
/// the point.
#[test]
fn stamping_ids_into_source_breaks_the_data_path() {
    let with_ids = reprint(lua_tree::print);
    let app = app_from(
        with_ids.iter().map(|(p, b)| (p.as_str(), b.as_str())),
        Rc::new(|_| Ok(None)),
    );
    let e = app.error.as_deref().unwrap_or("");
    assert!(
        e.contains("doc.list takes positional entries only")
            && e.contains(lua_tree::NID_KEY)
            && e.contains("model.lua"),
        "expected the seed to reject `_nid`, got: {e:?}"
    );
}

/// The narrowest of the three that follow, and the one that fails first if the schema drops
/// something the host needs: printed source has to reach `El` at all before its shape can matter.
#[test]
fn the_printed_kanban_walks_into_elements() {
    let app = open(&printed());
    // Repatches the mirror the closure reads; its own result is not what is under test here.
    let _ = app.view();
    let tree = app
        .view_fn
        .as_ref()
        .expect("no view closure")
        .call::<Table>(())
        .expect("the printed view failed to build");

    let mut handlers: Vec<Function> = Vec::new();
    let mut ctx = Ctx::new(&mut handlers, identity());
    if let Err(e) = walk(tree, &mut ctx) {
        panic!("walk rejected the printed tree: {e}");
    }
}

/// The one the sorting sabotage was invisible to. Print is allowed to move whitespace and add
/// `_nid`; it is not allowed to change what `view()` returns.
#[test]
fn the_printed_kanban_renders_the_same_tree() {
    let original = kanban_app(Rc::new(|_| Ok(None)));
    assert_eq!(original.error, None, "the shipped source did not load");
    let reprinted = open(&printed());

    let (a, b) = (tree_of(&original), tree_of(&reprinted));
    assert!(
        a.len() > 50,
        "suspiciously small view tree: {} lines",
        a.len()
    );
    if let Some(d) = diff(&a, &b) {
        panic!("the printed program renders a different tree.\n{d}");
    }
}

/// The view is not an app's only output. `model.lua` seeds the board at module scope, and
/// `doc.map({ id = "c-todo", name = "Todo" })` is a table like any other — so whatever the printer
/// stamps into it is written to the CRDT and travels to every peer that opens the board.
///
/// That makes this the test that says where `_nid` may and may not go: it belongs to the source,
/// and the data is not the source.
#[test]
fn the_printed_kanban_seeds_the_same_board() {
    let a = seeded(&mut kanban_app(Rc::new(|_| Ok(None))));
    let b = seeded(&mut open(&printed()));
    assert_eq!(a, b, "the printed program seeds a different board");
}

fn seeded(app: &mut LuaApp<LuaMsg>) -> LoroValue {
    let puts = Puts::default();
    app.flush(puts.recorder()).unwrap();
    let (name, bytes) = puts
        .take()
        .pop()
        .expect("seeding should have made the doc dirty");
    assert_eq!(name, "board");
    let doc = LoroDoc::new();
    doc.import(&bytes).unwrap();
    doc.get_deep_value()
}
