use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::{Arc, atomic::AtomicU64},
};

use loro::{
    Container, EventTriggerKind, Index, LoroDoc, LoroMap, LoroMovableList, LoroStringValue,
    LoroText, LoroValue, Subscription, ValueOrContainer,
};
use mlua::{Error, FromLua, Function, IntoLua, Lua, Table, Value};

pub struct DocEntry {
    pub doc: LoroDoc,
    pub mirror: Table,
    pub version: Arc<AtomicU64>,
    /// Watermark for the Lua mirror. Advanced in `view()`.
    pub mirrored: u64,
    /// Watermark for the vault. Advanced in `flush()`, and only after the write lands.
    /// Two consumers means two watermarks — a single `dirty` flag would let whichever
    /// cleared it first starve the other.
    pub saved: u64,
    _sub: Subscription,
}
pub enum Seg {
    Name(String),
    Pos(usize),
}
/// Load a doc's stored snapshot by name. `Ok(None)` is "never saved" — the ordinary first
/// run. An `Err` is a *storage failure*, and it must stay distinguishable: collapsing the two
/// makes an unreadable vault look like an empty board, which the next flush then overwrites
/// with that emptiness.
pub type Resolve = Rc<dyn Fn(&str) -> Result<Option<Vec<u8>>, String>>;
pub type Docs = Rc<RefCell<HashMap<String, DocEntry>>>;
/// Ask the host for a frame. Called from Loro's subscriber, which is `Send + Sync`, so this is
/// too — it cannot touch the VM or the mirror, and does not need to: all it has to do is get the
/// event loop to run `view()` again, which repatches on its own.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

pub fn install(lua: &Lua, docs: Docs, resolve: Resolve, wake: Wake) -> mlua::Result<()> {
    let open = lua.create_function(move |lua, (_this, name): (Value, String)| {
        let hit = docs.borrow().get(&name).map(|e| e.mirror.clone());
        if let Some(mirror) = hit {
            return Ok(mirror);
        }
        let doc = LoroDoc::new();
        if let Some(bytes) = resolve(&name).map_err(Error::runtime)? {
            doc.import(&bytes).map_err(Error::external)?;
        }
        let mirror = lua.create_table()?;
        patch_into(lua, &mirror, &doc.get_deep_value())?;
        let mt = lua.create_table()?;
        mt.set("__index", methods(lua, &docs, &name)?)?;
        mirror.set_metatable(Some(mt));
        let version = Arc::new(AtomicU64::new(0));
        let v = version.clone();
        let w = wake.clone();
        let sub = doc.subscribe_root(Arc::new(move |ev| {
            v.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            // A local write already happened inside a frame the host asked for, and poking from
            // here would schedule a second one for every keystroke. An import did not: it is a
            // peer or the MCP bridge writing while the window sits idle, and without this the
            // change lands in the doc and stays invisible until the next mouse move.
            if ev.triggered_by == EventTriggerKind::Import {
                w();
            }
        }));
        docs.borrow_mut().insert(
            name,
            DocEntry {
                doc,
                mirror: mirror.clone(),
                version,
                mirrored: 0,
                saved: 0,
                _sub: sub,
            },
        );
        Ok(mirror)
    })?;
    let t = lua.create_table()?;
    t.set("open", open)?;
    install_kinds(lua, &t)?;
    lua.globals().set("doc", t)?;
    Ok(())
}

// ── container kinds ───────────────────────────────────────────────────────────

/// Registry slots holding one empty marker table per container kind. The tables are never read
/// — only their addresses are compared — so an empty table is the whole implementation.
const MAP_MT: &str = "osv.mt.map";
const LIST_MT: &str = "osv.mt.list";
const TEXT_MT: &str = "osv.mt.text";

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Map,
    List,
    Text,
}

/// `doc.map{...}`, `doc.list{...}`, `doc.text("...")`.
///
/// Lua has one table type and Loro has six containers, so the shape of a Lua value cannot say
/// which one to create. Inference could cover map-vs-list (`raw_len() > 0`) but never `{}`, and
/// never text. Requiring the tag everywhere is the choice that stays the same when Tree and
/// Counter arrive — and it is additive, since bare-table inference could still be layered on
/// later as sugar without invalidating a single app already written.
///
/// The tag is a **metatable**, not a field. A field would be visible to the app's own `pairs()`
/// and would round-trip back through `patch_into` as if it were real document data.
fn install_kinds(lua: &Lua, doc: &Table) -> mlua::Result<()> {
    for key in [MAP_MT, LIST_MT, TEXT_MT] {
        let mt = lua.create_table()?;
        lua.set_named_registry_value(key, mt)?;
    }
    doc.set("map", tagger(lua, MAP_MT)?)?;
    doc.set("list", tagger(lua, LIST_MT)?)?;
    // Strings share one global metatable in Lua and cannot carry their own, so text is the one
    // kind that has to be boxed. The string lands at [1].
    doc.set(
        "text",
        lua.create_function(move |lua, s: mlua::String| {
            let t = lua.create_table()?;
            t.raw_set(1, s)?;
            t.set_metatable(Some(lua.named_registry_value::<Table>(TEXT_MT)?));
            Ok(t)
        })?,
    )?;
    Ok(())
}

fn tagger(lua: &Lua, key: &'static str) -> mlua::Result<Function> {
    lua.create_function(move |lua, t: Table| {
        t.set_metatable(Some(lua.named_registry_value::<Table>(key)?));
        Ok(t)
    })
}

/// Which container a tagged table asked for. Identity of the metatable is the tag — comparing
/// pointers rather than contents means an app cannot forge one by building a lookalike table.
fn kind_of(lua: &Lua, t: &Table) -> mlua::Result<Kind> {
    let untagged = || {
        Error::runtime(
            "a plain table cannot be written — say doc.map{...}, doc.list{...} or doc.text(\"...\")",
        )
    };
    let mt = t.metatable().ok_or_else(untagged)?;
    for (kind, key) in [
        (Kind::Map, MAP_MT),
        (Kind::List, LIST_MT),
        (Kind::Text, TEXT_MT),
    ] {
        if lua.named_registry_value::<Table>(key)?.to_pointer() == mt.to_pointer() {
            return Ok(kind);
        }
    }
    Err(untagged())
}

pub fn patch_into(lua: &Lua, dst: &Table, src: &LoroValue) -> mlua::Result<()> {
    match src {
        LoroValue::Map(entries) => {
            for (k, v) in entries.iter() {
                match v {
                    LoroValue::Map(_) | LoroValue::List(_) => {
                        patch_into(lua, &child(lua, dst, k.as_str())?, v)?;
                    }
                    _ => {
                        dst.set(k.as_str(), scalar(lua, v)?)?;
                    }
                };
            }

            let stale: Vec<mlua::String> = dst
                .pairs::<mlua::String, Value>()
                .filter_map(|p| p.ok())
                .filter(|(k, _)| k.to_str().is_ok_and(|s| !entries.contains_key(&*s)))
                .map(|(k, _)| k)
                .collect();
            for k in stale {
                dst.set(k, Value::Nil)?;
            }
        }
        LoroValue::List(items) => {
            let old = dst.raw_len();
            for (i, item) in items.iter().enumerate() {
                match item {
                    LoroValue::Map(_) | LoroValue::List(_) => {
                        patch_into(lua, &child(lua, dst, i + 1)?, item)?;
                    }
                    _ => dst.set(i + 1, scalar(lua, item)?)?,
                }
            }
            for i in (items.len() + 1..=old).rev() {
                dst.set(i, Value::Nil)?;
            }
        }

        _ => return Err(Error::runtime("patch into: Expected a map or a list")),
    };

    Ok(())
}
fn child<K: IntoLua + Clone>(lua: &Lua, dst: &Table, k: K) -> mlua::Result<Table> {
    Ok(match dst.get::<Value>(k.clone())? {
        Value::Table(t) => t,
        _ => {
            let t = lua.create_table()?;
            dst.set(k, &t)?;
            t
        }
    })
}

fn scalar(lua: &Lua, val: &LoroValue) -> mlua::Result<Value> {
    let result = match val {
        LoroValue::Null => Value::Nil,
        LoroValue::Bool(b) => Value::Boolean(*b),
        LoroValue::Double(d) => Value::Number(*d),
        LoroValue::I64(i) => Value::Number(*i as f64),
        LoroValue::String(s) => Value::String(lua.create_string(s.as_str())?),
        LoroValue::Binary(_) => return Err(Error::runtime("binary not supported")),
        LoroValue::List(_) | LoroValue::Map(_) | LoroValue::Container(_) => {
            return Err(Error::runtime("container reached scalar()"));
        }
    };
    Ok(result)
}
/// A Lua scalar as a `LoroValue`. Tables never reach here — they become *containers*, which is
/// a different operation entirely (see [`write`]).
///
/// `nil` is rejected rather than stored as `LoroValue::Null`. Two reasons: a Null inside a list
/// punches a hole in the Lua mirror that `raw_len` then reads wrong, and `set(path, x)` where
/// `x` turned out nil is the commonest Lua accident there is — an error catches it, a silent
/// Null does not. Removing a key is `:delete`, which says so.
fn from_lua(v: &Value) -> mlua::Result<LoroValue> {
    let result = match v {
        Value::Boolean(b) => LoroValue::Bool(*b),
        // Luau has no integer subtype, so both arms are doubles and round-trip identically
        // through `scalar()`. Above 2^53 that is lossy, which is a Luau property, not ours.
        Value::Number(d) => LoroValue::Double(*d),
        Value::Integer(i) => LoroValue::Double(*i as f64),
        Value::String(s) => LoroValue::String(LoroStringValue::from(s.to_string_lossy())),
        Value::Nil => {
            return Err(Error::runtime(
                "cannot write nil — use :delete to remove a key",
            ));
        }
        other => {
            return Err(Error::runtime(format!(
                "cannot write a {}",
                other.type_name()
            )));
        }
    };
    Ok(result)
}

// ── the write path ────────────────────────────────────────────────────────────

/// Write `v` at `at` inside `parent` — the *what* to [`resolve_path`]'s *where*.
///
/// A tagged table becomes a **new container**, replacing whatever the slot held. Replace, not
/// merge: `insert_container` mints a fresh `ContainerID`, so any concurrent edit inside the old
/// subtree is discarded wholesale. That is what `:set` reads as, and merging would need a
/// delete-semantics story that does not exist yet — but it is the sharp edge of this function,
/// and "I set one field and lost a colleague's edit to a sibling" is the bug it makes.
///
/// Not atomic: `LoroDoc` exposes `commit` but no abort, so ops applied before an error stay in
/// the pending transaction and are sealed by the next successful commit. A write that fails
/// half way leaves the doc half written.
pub(crate) fn write(lua: &Lua, parent: &Container, at: &Index, v: &Value) -> mlua::Result<()> {
    match v {
        Value::Table(t) => {
            let c = place(parent, at, kind_of(lua, t)?)?;
            fill(lua, &c, t)
        }
        _ => put(parent, at, from_lua(v)?),
    }
}

/// A one-segment path names a **root**, and roots need their own path here: nothing owns them,
/// so they cannot be expressed as `resolve_path`'s (container, index) pair. Loro addresses them
/// by name *and type* — `get_map` / `get_movable_list` / `get_text` are all get-or-create.
///
/// That forces one asymmetry with nested `:set`. A nested container is replaced by minting a new
/// `ContainerID`; a root cannot be deleted and re-minted, so `:set` on a root **clears and
/// refills** the container that is already there. The observable result is the same — old
/// contents gone — but the container's identity survives.
///
/// Module scope runs on every open, so seed a root behind the ordinary Lua guard:
///
/// ```lua
/// if not b.cards then b:set({"cards"}, doc.list{}) end
/// ```
///
/// Without it, reopening the app would clear the board it just loaded.
fn write_root(lua: &Lua, doc: &LoroDoc, name: &str, v: &Value) -> mlua::Result<()> {
    let ext = Error::external;
    let Value::Table(t) = v else {
        return Err(Error::runtime(
            "a root is always a container — doc.map{...}, doc.list{...} or doc.text(\"...\")",
        ));
    };
    let c = match kind_of(lua, t)? {
        Kind::Map => {
            let m = doc.get_map(name);
            m.clear().map_err(ext)?;
            Container::Map(m)
        }
        Kind::List => {
            let l = doc.get_movable_list(name);
            l.clear().map_err(ext)?;
            Container::MovableList(l)
        }
        Kind::Text => {
            let x = doc.get_text(name);
            x.delete(0, x.len_unicode()).map_err(ext)?;
            Container::Text(x)
        }
    };
    fill(lua, &c, t)
}

/// Create an empty container of `kind` at `at`, replacing whatever was there.
fn place(parent: &Container, at: &Index, kind: Kind) -> mlua::Result<Container> {
    let ext = Error::external;
    Ok(match (parent, at) {
        (Container::Map(m), Index::Key(k)) => match kind {
            Kind::Map => Container::Map(m.insert_container(k, LoroMap::new()).map_err(ext)?),
            Kind::List => {
                Container::MovableList(m.insert_container(k, LoroMovableList::new()).map_err(ext)?)
            }
            Kind::Text => Container::Text(m.insert_container(k, LoroText::new()).map_err(ext)?),
        },
        // `set_container`, not `insert_container`: resolve_path only ever hands back a position
        // that already exists on a list, so this replaces an element rather than growing the
        // list. Growing it is `:insert`, which is a different operation with a different name.
        (Container::MovableList(l), Index::Seq(i)) => match kind {
            Kind::Map => Container::Map(l.set_container(*i, LoroMap::new()).map_err(ext)?),
            Kind::List => {
                Container::MovableList(l.set_container(*i, LoroMovableList::new()).map_err(ext)?)
            }
            Kind::Text => Container::Text(l.set_container(*i, LoroText::new()).map_err(ext)?),
        },
        _ => return Err(mismatch(parent, at)),
    })
}

/// Store a scalar at `at`.
fn put(parent: &Container, at: &Index, v: LoroValue) -> mlua::Result<()> {
    match (parent, at) {
        (Container::Map(m), Index::Key(k)) => m.insert(k, v).map_err(Error::external),
        (Container::MovableList(l), Index::Seq(i)) => l.set(*i, v).map_err(Error::external),
        _ => Err(mismatch(parent, at)),
    }
}

/// Fill a container just created by [`place`] from the Lua table that asked for it.
fn fill(lua: &Lua, c: &Container, t: &Table) -> mlua::Result<()> {
    match c {
        Container::Map(_) => {
            for pair in t.pairs::<Value, Value>() {
                let (k, v) = pair?;
                let Value::String(k) = k else {
                    return Err(Error::runtime(format!(
                        "doc.map keys must be strings, got {}",
                        k.type_name()
                    )));
                };
                write(lua, c, &Index::Key((&*k.to_str()?).into()), &v)?;
            }
        }
        Container::MovableList(l) => {
            // Only the array part is written, so a stray named field would vanish silently.
            // Say so instead — it means the author wanted doc.map.
            for pair in t.pairs::<Value, Value>() {
                let (k, _) = pair?;
                if !matches!(k, Value::Integer(_) | Value::Number(_)) {
                    return Err(Error::runtime(format!(
                        "doc.list takes positional entries only, got the key {}",
                        show_value(&k)
                    )));
                }
            }
            for i in 1..=t.raw_len() {
                append(lua, l, &t.get::<Value>(i)?)?;
            }
        }
        Container::Text(txt) => {
            let s: mlua::String = t.raw_get(1)?;
            txt.insert(0, &s.to_string_lossy())
                .map_err(Error::external)?;
        }
        other => return Err(Error::runtime(format!("cannot fill a {}", kind(other)))),
    }
    Ok(())
}

/// Push onto the end of a list. Filling a list grows it, so this is the one place that appends
/// rather than replacing.
fn append(lua: &Lua, l: &LoroMovableList, v: &Value) -> mlua::Result<()> {
    let ext = Error::external;
    match v {
        Value::Table(t) => {
            let c = match kind_of(lua, t)? {
                Kind::Map => Container::Map(l.push_container(LoroMap::new()).map_err(ext)?),
                Kind::List => {
                    Container::MovableList(l.push_container(LoroMovableList::new()).map_err(ext)?)
                }
                Kind::Text => Container::Text(l.push_container(LoroText::new()).map_err(ext)?),
            };
            fill(lua, &c, t)
        }
        _ => l.push(from_lua(v)?).map_err(ext),
    }
}

fn mismatch(parent: &Container, at: &Index) -> Error {
    Error::runtime(format!(
        "cannot write {} into a {} container",
        match at {
            Index::Key(k) => format!("the key `{k}`"),
            Index::Seq(n) => format!("position {n}"),
            Index::Node(n) => format!("node {n}"),
        },
        kind(parent)
    ))
}

fn show_value(v: &Value) -> String {
    match v {
        Value::String(s) => format!("`{}`", s.to_string_lossy()),
        other => other.type_name().to_string(),
    }
}

fn methods(lua: &Lua, docs: &Docs, name: &str) -> mlua::Result<Table> {
    let methods = lua.create_table()?;
    // One clone per method: each `create_function` closure owns its captures for the life of
    // the VM, and they all need the same two.
    let (docs2, name2) = (docs.clone(), name.to_string());
    let (docs3, name3) = (docs.clone(), name.to_string());
    let (docs4, name4) = (docs.clone(), name.to_string());
    let (docs, name) = (docs.clone(), name.to_string());
    let set = lua.create_function(move |lua, (_this, path, val): (Value, Table, Value)| {
        let doc = handle(&docs, &name)?;
        match segments(&path)?.as_slice() {
            [Seg::Name(root)] => write_root(lua, &doc, root, &val)?,
            [Seg::Pos(_)] => return Err(Error::runtime("a root is named, not positional")),
            segs => {
                let (parent, at) = resolve_path(&doc, segs)?;
                write(lua, &parent, &at, &val)?;
            }
        }
        doc.commit();
        Ok(())
    })?;
    methods.set("set", set)?;

    let (docs, name) = (docs2, name2);
    // `:insert(path, v)` appends; `:insert(path, pos, v)` inserts at `pos`. Unlike every other
    // method, the path names the **list itself** rather than a slot inside it — you insert
    // *into* a list, but set, delete and move act *at* an element.
    //
    // The append form is not a convenience. The mirror is only repatched in `view()`, so a
    // handler that has just written cannot read its own change back: `#b.cards` is a frame
    // behind, and `b:insert({"cards", #b.cards + 1}, ...)` would silently target the wrong
    // slot after the first card. Appending has to be something Lua can say without counting.
    let insert = lua.create_function(
        move |lua, (_this, path, a, b): (Value, Table, Value, Value)| {
            let doc = handle(&docs, &name)?;
            let (pos, val) = match b {
                Value::Nil => (None, a),
                val => (Some(a), val),
            };
            let Container::MovableList(l) = container_for(&doc, &segments(&path)?)? else {
                return Err(Error::runtime(
                    "insert needs a list — to add a key to a map, use :set",
                ));
            };
            let i = match pos {
                None => l.len(),
                Some(p) => {
                    let p = usize::from_lua(p, lua)?;
                    let i = p
                        .checked_sub(1)
                        .ok_or_else(|| Error::runtime("list positions start at 1"))?;
                    if i > l.len() {
                        return Err(Error::runtime(format!(
                            "cannot insert at {p} — the list holds {} elements",
                            l.len()
                        )));
                    }
                    i
                }
            };
            insert_at(lua, &l, i, &val)?;
            doc.commit();
            Ok(())
        },
    )?;
    methods.set("insert", insert)?;

    let (docs, name) = (docs3, name3);
    let delete = lua.create_function(move |_, (_this, path): (Value, Table)| {
        let doc = handle(&docs, &name)?;
        let (parent, at) = resolve_path(&doc, &segments(&path)?)?;
        match (&parent, &at) {
            (Container::Map(m), Index::Key(k)) => m.delete(k).map_err(Error::external)?,
            (Container::MovableList(l), Index::Seq(i)) => {
                l.delete(*i, 1).map_err(Error::external)?
            }
            _ => return Err(mismatch(&parent, &at)),
        }
        doc.commit();
        Ok(())
    })?;
    methods.set("delete", delete)?;

    let (docs, name) = (docs4, name4);
    let mov = lua.create_function(move |_, (_this, path, to): (Value, Table, usize)| {
        let doc = handle(&docs, &name)?;
        let (parent, at) = resolve_path(&doc, &segments(&path)?)?;
        let (Container::MovableList(l), Index::Seq(from)) = (&parent, &at) else {
            return Err(Error::runtime("move needs an element of a list"));
        };
        let to = to
            .checked_sub(1)
            .ok_or_else(|| Error::runtime("list positions start at 1"))?;
        if to >= l.len() {
            return Err(Error::runtime(format!(
                "cannot move to {} — the list holds {} elements",
                to + 1,
                l.len()
            )));
        }
        // The operation the whole MovableList choice exists for: `pos` is its own LWW register,
        // so a concurrent move and a concurrent field edit both survive. Delete-then-insert
        // would duplicate the card instead.
        l.mov(*from, to).map_err(Error::external)?;
        doc.commit();
        Ok(())
    })?;
    methods.set("move", mov)?;

    Ok(methods)
}

/// Insert `v` at `i`, growing the list. [`write`] replaces; this is the one that shifts.
fn insert_at(lua: &Lua, l: &LoroMovableList, i: usize, v: &Value) -> mlua::Result<()> {
    let ext = Error::external;
    match v {
        Value::Table(t) => {
            let c = match kind_of(lua, t)? {
                Kind::Map => Container::Map(l.insert_container(i, LoroMap::new()).map_err(ext)?),
                Kind::List => Container::MovableList(
                    l.insert_container(i, LoroMovableList::new()).map_err(ext)?,
                ),
                Kind::Text => Container::Text(l.insert_container(i, LoroText::new()).map_err(ext)?),
            };
            fill(lua, &c, t)
        }
        _ => l.insert(i, from_lua(v)?).map_err(ext),
    }
}

/// The doc handle, with the `docs` borrow released before it is returned.
///
/// `LoroDoc::clone` is a reference clone (loro/src/lib.rs:114), so this costs an refcount bump
/// and buys the guarantee that no `RefCell` borrow is alive across `commit()` — which fires
/// subscribers synchronously, and will soon poke the event loop from inside one.
fn handle(docs: &Docs, name: &str) -> mlua::Result<LoroDoc> {
    docs.borrow()
        .get(name)
        .map(|e| e.doc.clone())
        .ok_or_else(|| Error::runtime("doc closed"))
}

/// `{"cards", card_id, "title"}` → three segments. Lua's array part only, so a named field in a
/// path is a typo rather than something to guess at.
fn segments(path: &Table) -> mlua::Result<Vec<Seg>> {
    let n = path.raw_len();
    if n == 0 {
        return Err(Error::runtime(
            r#"path must be a list of segments, e.g. {"meta", "title"}"#,
        ));
    }
    (1..=n).map(|i| seg(&path.raw_get::<Value>(i)?)).collect()
}

/// Walk `path[..len-1]` to the container that *owns* the last segment, and hand back that
/// container plus the unresolved final segment. The caller writes through the pair —
/// resolving every segment would yield a `LoroValue`, which cannot be written to.
///
/// Leaving the last segment unresolved is also what lets `:set` create a key that does not
/// exist yet. Everything *before* it must already exist: there is deliberately no
/// auto-vivification, because a typo would silently invent a subtree and the type of an
/// invented intermediate (map or list?) is unguessable.
pub(crate) fn resolve_path(doc: &LoroDoc, path: &[Seg]) -> mlua::Result<(Container, Index)> {
    let Some((last, parents)) = path.split_last() else {
        return Err(Error::runtime("empty path"));
    };
    let Some((root, rest)) = parents.split_first() else {
        return Err(Error::runtime(
            r#"path needs a container and a key inside it, e.g. {"meta", "title"}"#,
        ));
    };
    // Loro's roots are named containers, so the first segment is always a key — never a
    // position, and never a value.
    let Seg::Name(root) = root else {
        return Err(Error::runtime("the first path segment must be a name"));
    };

    let mut idx = vec![Index::Key(root.as_str().into())];
    let mut current = container_at(doc, &idx)?;
    for seg in rest {
        idx.push(index_in(&current, seg)?);
        current = container_at(doc, &idx)?;
    }
    let last = index_in(&current, last)?;
    Ok((current, last))
}

/// Walk *every* segment, ending on the container the path names — where [`resolve_path`] stops
/// one short and hands back the slot. `:insert` needs this because it acts on the list itself.
fn container_for(doc: &LoroDoc, path: &[Seg]) -> mlua::Result<Container> {
    let Some((Seg::Name(root), rest)) = path.split_first() else {
        return Err(Error::runtime("the first path segment must be a name"));
    };
    let mut idx = vec![Index::Key(root.as_str().into())];
    let mut current = container_at(doc, &idx)?;
    for seg in rest {
        idx.push(index_in(&current, seg)?);
        current = container_at(doc, &idx)?;
    }
    Ok(current)
}

/// The container at `idx`. "Missing" and "there, but a scalar" are different mistakes, so they
/// get different messages — both name the path, since the caller only passed a Lua table.
fn container_at(doc: &LoroDoc, idx: &[Index]) -> mlua::Result<Container> {
    match doc.get_by_path(idx) {
        Some(ValueOrContainer::Container(c)) => Ok(c),
        Some(ValueOrContainer::Value(_)) => Err(Error::runtime(format!(
            "`{}` is a value, not a container",
            show(idx)
        ))),
        None => Err(Error::runtime(format!("`{}` does not exist", show(idx)))),
    }
}

/// One segment, interpreted against the container it is being looked up *in*. The same string
/// is a key on a map and an element's `id` on a list.
fn index_in(parent: &Container, seg: &Seg) -> mlua::Result<Index> {
    match (parent, seg) {
        (Container::Map(_), Seg::Name(k)) => Ok(Index::Key(k.as_str().into())),
        (Container::MovableList(l), Seg::Name(id)) => scan_for_id(l, id)
            .map(Index::Seq)
            .ok_or_else(|| Error::runtime(format!("no element with id `{id}`"))),
        // Lua counts from 1, Loro from 0. Getting this wrong edits the neighbouring element and
        // reports nothing, so the conversion lives here and nowhere else.
        (Container::MovableList(_), Seg::Pos(p)) => p
            .checked_sub(1)
            .map(Index::Seq)
            .ok_or_else(|| Error::runtime("list positions start at 1")),
        (Container::Map(_), Seg::Pos(_)) => Err(Error::runtime(
            "a map is addressed by name, not by position",
        )),
        // Writes only ever create MovableList, so a plain List can only have arrived by import.
        // Treating it as movable would quietly drop the per-element `pos` register.
        (Container::List(_), _) => Err(Error::runtime(
            "plain lists cannot be written through — writes create a MovableList",
        )),
        (c, _) => Err(Error::runtime(format!(
            "cannot address into a {} container",
            kind(c)
        ))),
    }
}

fn show(idx: &[Index]) -> String {
    idx.iter()
        .map(|i| match i {
            Index::Key(k) => k.to_string(),
            Index::Seq(n) => n.to_string(),
            Index::Node(n) => n.to_string(),
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn kind(c: &Container) -> &'static str {
    match c {
        Container::Map(_) => "map",
        Container::List(_) => "list",
        Container::MovableList(_) => "movable list",
        Container::Text(_) => "text",
        Container::Tree(_) => "tree",
        _ => "unsupported",
    }
}

/// One Lua path element. A string is a map key *or* a list element's id; a number is a raw
/// list position.
fn seg(v: &Value) -> mlua::Result<Seg> {
    match v {
        Value::String(s) => Ok(Seg::Name(s.to_str()?.to_owned())),
        Value::Integer(i) => Ok(Seg::Pos(*i as usize)),
        Value::Number(n) => Ok(Seg::Pos(*n as usize)),
        _ => Err(Error::runtime("path segment must be a string or a number")),
    }
}

/// Translate an element's `id` field into its current position.
///
/// The one thing Loro cannot do for us: `Index::Seq` is a position, and Loro has no idea the
/// elements carry an `id`, because that is app convention rather than CRDT structure. Ids are
/// stable; positions shift whenever anything is inserted or removed ahead of them, including
/// by a peer between the click and the handler.
///
/// Returns a **0-based Loro index**. `Seg::Pos` holds a 1-based Lua one — never mix them.
/// Elements that cannot carry an id (scalars, maps without one) are skipped, not rejected.
pub(crate) fn scan_for_id(list: &LoroMovableList, id: &str) -> Option<usize> {
    for i in 0..list.len() {
        let Some(ValueOrContainer::Container(Container::Map(m))) = list.get(i) else {
            continue;
        };
        let Some(ValueOrContainer::Value(LoroValue::String(s))) = m.get("id") else {
            continue;
        };
        if s.as_str() == id {
            return Some(i);
        }
    }
    None
}
