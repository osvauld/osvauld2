//! The `doc` binding: the app's CRDT (a Loro [`LoroDoc`]) exposed to Lua. All Loro↔Lua glue lives
//! here.
//!
//! Primitives: `doc:list(name)` → a MovableList of maps; `doc:map(name)` / `list:get(i)` → a Map
//! (`m.key` / `m.key = v`); `doc:text(name)` → a LoroText.
//!
//! Handles are re-resolved by name/index on every access (never cached), so they can't go stale
//! after a future sync reset — the lesson from the old runtime. Mutations commit immediately, and
//! since the data is a CRDT an external writer (a peer or MCP) is the *same* operation the UI
//! makes.

use std::rc::Rc;

use loro::{
    Container, ExpandType, LoroDoc, LoroMap, LoroMovableList, LoroText, LoroValue, StyleConfig,
    StyleConfigMap, TextDelta, ValueOrContainer,
};
use mlua::{Lua, MetaMethod, Result as LuaResult, Table, UserData, UserDataMethods, Value};
use rich_text::{Marks, Run};

/// The inline marks the editor can carry. Boolean flags map to `bold`/`italic`/… ; `link` carries
/// its url. Mirrors `rich_text`'s known keys and `doc_editor`'s configured set.
const FLAG_MARKS: [&str; 4] = ["bold", "italic", "strike", "code"];

/// Install the `doc` global (the app's CRDT root) into `lua`, after registering the inline-mark
/// styles its editors can apply.
pub fn install(lua: &Lua, doc: Rc<LoroDoc>) -> LuaResult<()> {
    config_marks(&doc);
    lua.globals().set("doc", LuaDoc { doc })
}

/// Register the inline-mark styles an `ui.editor` can toggle. Loro errors when marking an
/// unconfigured key, and this config lives at runtime (not in the snapshot), so it must run on
/// every load. `None` = a mark covers exactly the chars it was applied to: typing at either edge
/// does *not* inherit it (so bolding a selection doesn't make everything you type afterwards bold).
/// Insertions strictly inside a marked run still inherit, which is correct. Continuing a mark while
/// typing at the edge needs a pending-format ("active mark") UI state we don't have yet — until
/// then `None` is the least surprising default for all flags and the link.
fn config_marks(doc: &LoroDoc) {
    let mut styles = StyleConfigMap::new();
    for key in FLAG_MARKS {
        styles.insert(key.into(), StyleConfig { expand: ExpandType::None });
    }
    styles.insert("link".into(), StyleConfig { expand: ExpandType::None });
    doc.config_text_style(styles);
}

/// Read the top-level text container `name` as styled [`Run`]s — its substring spans with their
/// inline marks, so an editor renders existing bold/italic/code/link instead of flat text.
pub(crate) fn text_runs(doc: &LoroDoc, name: &str) -> Vec<Run> {
    runs_from_delta(&doc.get_text(name))
}

/// Turn a LoroText's rich-text delta (already split into marked runs) into `rich_text::Run`s.
fn runs_from_delta(text: &LoroText) -> Vec<Run> {
    text.to_delta()
        .into_iter()
        .filter_map(|d| {
            // A text container's delta is all `Insert`s; ignore anything else defensively.
            let TextDelta::Insert { insert, attributes } = d else { return None };
            let attrs = attributes.unwrap_or_default();
            let mut marks = Marks::new();
            for key in FLAG_MARKS {
                if matches!(attrs.get(key), Some(LoroValue::Bool(true))) {
                    marks = marks.flag(key);
                }
            }
            if let Some(LoroValue::String(url)) = attrs.get("link") {
                marks = marks.with("link", url.to_string());
            }
            Some(Run { text: insert, marks, color: None })
        })
        .collect()
}

/// `doc` — opens named top-level containers.
#[derive(Clone)]
struct LuaDoc {
    doc: Rc<LoroDoc>,
}

impl UserData for LuaDoc {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("list", |_, this, name: String| Ok(LuaList { doc: this.doc.clone(), name }));
        m.add_method("map", |_, this, name: String| {
            Ok(LuaMap { doc: this.doc.clone(), at: MapAt::Root(name) })
        });
        m.add_method("text", |_, this, name: String| Ok(LuaText { doc: this.doc.clone(), name }));
    }
}

/// `doc:text(name)` — a top-level LoroText. Read with `:get()`, seed/replace with `:set(s)`,
/// length via `:len()` / `#`. An `ui.editor{ id = name }` edits this same container.
#[derive(Clone)]
struct LuaText {
    doc: Rc<LoroDoc>,
    name: String,
}

impl LuaText {
    fn handle(&self) -> LoroText {
        self.doc.get_text(self.name.as_str())
    }
}

impl UserData for LuaText {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Len, |_, this, ()| Ok(this.handle().len_unicode()));
        m.add_method("len", |_, this, ()| Ok(this.handle().len_unicode()));
        m.add_method("get", |_, this, ()| Ok(this.handle().to_string()));
        // Replace the whole content — for seeding an initial value.
        m.add_method("set", |_, this, s: String| {
            let t = this.handle();
            let len = t.len_unicode();
            if len > 0 {
                t.delete(0, len).map_err(err)?;
            }
            t.insert(0, &s).map_err(err)?;
            this.doc.commit();
            Ok(())
        });
    }
}

/// If `value` is a `doc:text(name)` handle, its backing container name — so an
/// `ui.editor{ value = doc:text(name) }` binds to it even when its `id` differs (`None` otherwise).
pub(crate) fn text_name(value: &Value) -> Option<String> {
    match value {
        Value::UserData(ud) => ud.borrow::<LuaText>().ok().map(|t| t.name.clone()),
        _ => None,
    }
}

/// A [`text_edit::TextBuffer`] over a top-level LoroText. Lives here (the consumer), not in
/// `text_edit`, to keep loro out of the primitive. Char- (code-point-) indexed, matching
/// `len_unicode` and egui's `CCursor`. Edits set `dirty`; the owner [`commit`](Self::commit)s once.
pub(crate) struct LoroTextBuffer {
    text: LoroText,
    doc: Rc<LoroDoc>,
    pub dirty: bool,
}

impl LoroTextBuffer {
    /// Open the top-level text container `name` (created lazily by Loro if absent).
    pub fn open(doc: Rc<LoroDoc>, name: &str) -> Self {
        let text = doc.get_text(name);
        LoroTextBuffer { text, doc, dirty: false }
    }

    /// Commit a batch of edits to the doc, once, if anything actually changed.
    pub fn commit(&self) {
        if self.dirty {
            self.doc.commit();
        }
    }
}

impl text_edit::TextBuffer for LoroTextBuffer {
    fn char_len(&self) -> usize {
        self.text.len_unicode()
    }

    fn text(&self) -> String {
        self.text.to_string()
    }

    fn insert(&mut self, at: usize, s: &str) {
        if self.text.insert(at, s).is_ok() {
            self.dirty = true;
        }
    }

    fn delete(&mut self, at: usize, len: usize) {
        if self.text.delete(at, len).is_ok() {
            self.dirty = true;
        }
    }

    fn mark_covers(&self, a: usize, b: usize, key: &str) -> bool {
        if a >= b {
            return true;
        }
        let mut pos = 0usize;
        for run in runs_from_delta(&self.text) {
            let len = run.text.chars().count();
            let (run_start, run_end) = (pos, pos + len);
            pos = run_end;
            // If any part of this run inside [a, b) lacks the mark, the range isn't fully covered.
            if run_start.max(a) < run_end.min(b) && !run.marks.has(key) {
                return false;
            }
        }
        true
    }

    fn set_mark(&mut self, a: usize, b: usize, key: &str, on: bool) {
        if a >= b {
            return;
        }
        let res = if on { self.text.mark(a..b, key, true) } else { self.text.unmark(a..b, key) };
        if res.is_ok() {
            self.dirty = true;
        }
    }
}

/// `doc:list(name)` — a MovableList. `#list`, `list:get(i)`, `list:add{…}`, `list:remove(i)`,
/// `list:move(from, to)` (all 1-based, like Lua).
#[derive(Clone)]
struct LuaList {
    doc: Rc<LoroDoc>,
    name: String,
}

impl LuaList {
    fn handle(&self) -> LoroMovableList {
        self.doc.get_movable_list(self.name.as_str())
    }
}

impl UserData for LuaList {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Len, |_, this, ()| Ok(this.handle().len()));
        m.add_method("len", |_, this, ()| Ok(this.handle().len()));

        m.add_method("get", |_, this, i: usize| {
            let list = this.handle();
            if i >= 1 && i <= list.len() {
                Ok(Some(LuaMap {
                    doc: this.doc.clone(),
                    at: MapAt::InList { list: this.name.clone(), index: i - 1 },
                }))
            } else {
                Ok(None)
            }
        });

        m.add_method("add", |_, this, fields: Table| {
            let map = this.handle().push_container(LoroMap::new()).map_err(err)?;
            fill_map(&map, fields)?;
            this.doc.commit();
            Ok(())
        });

        m.add_method("remove", |_, this, i: usize| {
            let list = this.handle();
            if i >= 1 && i <= list.len() {
                list.delete(i - 1, 1).map_err(err)?;
                this.doc.commit();
            }
            Ok(())
        });

        m.add_method("move", |_, this, (from, to): (usize, usize)| {
            let list = this.handle();
            if from >= 1 && to >= 1 && from <= list.len() && to <= list.len() {
                list.mov(from - 1, to - 1).map_err(err)?;
                this.doc.commit();
            }
            Ok(())
        });
    }
}

/// How to re-resolve a map each access (never cached → never stale).
#[derive(Clone)]
enum MapAt {
    Root(String),
    InList { list: String, index: usize },
}

/// A map: fields via `m.key` (read) and `m.key = v` (write; `nil` deletes).
#[derive(Clone)]
struct LuaMap {
    doc: Rc<LoroDoc>,
    at: MapAt,
}

impl LuaMap {
    fn handle(&self) -> Option<LoroMap> {
        match &self.at {
            MapAt::Root(name) => Some(self.doc.get_map(name.as_str())),
            MapAt::InList { list, index } => match self.doc.get_movable_list(list.as_str()).get(*index) {
                Some(ValueOrContainer::Container(Container::Map(map))) => Some(map),
                _ => None,
            },
        }
    }
}

impl UserData for LuaMap {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        // `m.key` → scalar value (nested containers read as nil for now).
        m.add_meta_method(MetaMethod::Index, |lua, this, key: String| match this.handle() {
            Some(map) => value_to_lua(lua, map.get(&key)),
            None => Ok(Value::Nil),
        });
        // `m.key = v` → set, or delete when `v` is nil.
        m.add_meta_method(MetaMethod::NewIndex, |_, this, (key, value): (String, Value)| {
            if let Some(map) = this.handle() {
                set_field(&map, &key, value)?;
                this.doc.commit();
            }
            Ok(())
        });
    }
}

// --- value conversion -------------------------------------------------------

fn err(e: loro::LoroError) -> mlua::Error {
    mlua::Error::RuntimeError(e.to_string())
}

/// Populate a fresh map from a Lua table of scalar fields.
fn fill_map(map: &LoroMap, fields: Table) -> LuaResult<()> {
    for pair in fields.pairs::<String, Value>() {
        let (key, value) = pair?;
        set_field(map, &key, value)?;
    }
    Ok(())
}

/// Set (or delete, on `Nil`) one scalar field. Non-scalar values are ignored for now.
fn set_field(map: &LoroMap, key: &str, value: Value) -> LuaResult<()> {
    let result = match value {
        Value::Nil => map.delete(key),
        Value::Boolean(b) => map.insert(key, b),
        Value::Integer(i) => map.insert(key, i),
        Value::Number(n) => map.insert(key, n),
        Value::String(s) => map.insert(key, s.to_str()?.to_owned()),
        _ => return Ok(()),
    };
    result.map_err(err)
}

/// Convert a Loro value to Lua (scalars; nested containers become nil for now).
fn value_to_lua(lua: &Lua, value: Option<ValueOrContainer>) -> LuaResult<Value> {
    let Some(ValueOrContainer::Value(lv)) = value else {
        return Ok(Value::Nil);
    };
    Ok(match lv {
        LoroValue::Null => Value::Nil,
        LoroValue::Bool(b) => Value::Boolean(b),
        LoroValue::I64(n) => Value::Integer(n),
        LoroValue::Double(d) => Value::Number(d),
        LoroValue::String(s) => Value::String(lua.create_string(s.to_string())?),
        _ => Value::Nil,
    })
}
