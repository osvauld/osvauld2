//! `index.lua`: an app's description of what search sees (docs/design/search.md §4).
//!
//! The spec runs in its own VM — no `ui`, no `doc`, no `gfx`, no `os` — and only ever sees
//! frozen copies of doc values, so indexing can neither change the app nor break it. This module
//! decides *what* changed and what the records say ([`Plan`]); storing them is the caller's —
//! `app_host` stays vault- and tantivy-free.
//!
//! Change detection is by fingerprint, not by Loro event paths (§5): a list path is a position,
//! which a concurrent insert shifts, and a doc written while the app was closed fires nothing.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use loro::{LoroDoc, LoroValue};
use mlua::{Function, Lua, Table, Value};
use serde::{Deserialize, Serialize};

/// One hit as app Lua sees it: `{ doc, id, score, snippet }`.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchHit {
    pub doc: String,
    pub id: String,
    pub score: f32,
    pub snippet: String,
}

/// The host's query: text and limit in, hits out. The shell closes over the item's index.
pub type SearchFn = Rc<dyn Fn(&str, usize) -> Result<Vec<SearchHit>, String>>;
pub(crate) type SearchHook = Rc<RefCell<Option<SearchFn>>>;

const QUERY_OPTS: &[&str] = &["limit"];

/// `search.query(text, opts?)` in the app VM. Installed always: without a host index it is a
/// loud error, not a missing global — `attempt to index nil` would point at the app.
pub(crate) fn install_query(vm: &Lua, hook: SearchHook) -> mlua::Result<()> {
    let query = vm.create_function(move |lua, (q, opts): (String, Option<Table>)| {
        let mut limit = 20usize;
        if let Some(opts) = opts {
            for pair in opts.pairs::<String, Value>() {
                let (k, v) = pair?;
                match (k.as_str(), v) {
                    ("limit", Value::Integer(n)) if n > 0 => limit = n as usize,
                    ("limit", Value::Number(n)) if n >= 1.0 => limit = n as usize,
                    ("limit", _) => {
                        return Err(mlua::Error::runtime(
                            "search.query: `limit` must be a positive number",
                        ));
                    }
                    (other, _) => {
                        return Err(mlua::Error::runtime(format!(
                            "search.query: unknown option `{other}` (expected {})",
                            QUERY_OPTS.join(", ")
                        )));
                    }
                }
            }
        }
        let f = hook.borrow().clone().ok_or_else(|| {
            mlua::Error::runtime("search is not available: this host has no index for the app")
        })?;
        let hits = f(&q, limit.min(500))
            .map_err(|e| mlua::Error::runtime(format!("search.query: {e}")))?;
        let out = lua.create_table_with_capacity(hits.len(), 0)?;
        for (i, h) in hits.into_iter().enumerate() {
            let t = lua.create_table()?;
            t.set("doc", h.doc)?;
            t.set("id", h.id)?;
            t.set("score", h.score)?;
            t.set("snippet", h.snippet)?;
            out.raw_set(i + 1, t)?;
        }
        Ok(out)
    })?;
    let t = vm.create_table()?;
    t.set("query", query)?;
    vm.globals().set("search", t)
}

/// What `fields` returned for one record.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fields {
    pub title: Option<String>,
    pub body: Option<String>,
    pub facets: BTreeMap<String, String>,
    pub time: Option<f64>,
}

/// One doc's worth of index work. `prints` replaces the previous fingerprints once the caller
/// has applied the upserts and deletes.
#[derive(Debug, Default)]
pub struct Plan {
    pub upserts: Vec<(String, Fields)>,
    pub deletes: Vec<String>,
    pub prints: Vec<u8>,
    /// How many times `fields` ran — what "incremental" is measured by.
    pub fields_runs: usize,
    /// Per-record failures. A failed record keeps no fingerprint, so the next plan retries it;
    /// what the index already holds for it stays until then.
    pub errors: Vec<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct Prints {
    rest: u64,
    records: BTreeMap<String, u64>,
}

pub struct IndexSpec {
    /// Folded into the `rest` fingerprint: an edited spec makes every record stale.
    src_hash: u64,
    vm: Lua,
    budget: Arc<crate::Budget>,
    doc: String,
    each: Vec<String>,
    key: Option<Function>,
    fields: Function,
    recent: bool,
}

const SPEC_KEYS: &[&str] = &["doc", "each", "key", "fields", "rank"];
const FIELD_KEYS: &[&str] = &["title", "body", "facet", "time"];

impl IndexSpec {
    pub fn load(source: &str) -> Result<Self, String> {
        let err = |m: String| format!("index.lua: {m}");
        let (vm, budget) = crate::test_vm().map_err(|e| err(e.to_string()))?;
        let spec = match vm
            .load(source)
            .set_name("index.lua")
            .eval::<Value>()
            .map_err(|e| err(e.to_string()))?
        {
            Value::Table(t) => t,
            _ => return Err(err("must return a table".into())),
        };
        for pair in spec.pairs::<Value, Value>() {
            let (k, _) = pair.map_err(|e| err(e.to_string()))?;
            let name = k.to_string().unwrap_or_default();
            if !SPEC_KEYS.contains(&name.as_str()) {
                return Err(err(format!(
                    "unknown key `{name}` (expected {})",
                    SPEC_KEYS.join(", ")
                )));
            }
        }
        let doc: String = match spec.get::<Value>("doc") {
            Ok(Value::String(s)) => s.to_str().map_err(|e| err(e.to_string()))?.to_string(),
            _ => return Err(err("`doc` must be a doc name or a `prefix*` pattern".into())),
        };
        let each: Vec<String> = match spec.get::<Value>("each") {
            Ok(Value::Table(t)) => t
                .sequence_values::<String>()
                .collect::<mlua::Result<_>>()
                .map_err(|e| err(format!("`each` must be a list of names: {e}")))?,
            _ => return Err(err("`each` must be the path to the collection, e.g. { \"cards\" }".into())),
        };
        if each.is_empty() {
            return Err(err("`each` must name at least one path segment".into()));
        }
        let fields = match spec.get::<Value>("fields") {
            Ok(Value::Function(f)) => f,
            _ => return Err(err("`fields` must be a function(id, record, doc)".into())),
        };
        let key = match spec.get::<Value>("key") {
            Ok(Value::Function(f)) => Some(f),
            Ok(Value::Nil) => None,
            _ => return Err(err("`key` must be a function(record)".into())),
        };
        let recent = match spec.get::<Value>("rank") {
            Ok(Value::Nil) => false,
            Ok(Value::String(s)) if s.to_str().is_ok_and(|s| s == "relevance") => false,
            Ok(Value::String(s)) if s.to_str().is_ok_and(|s| s == "recent") => true,
            _ => return Err(err("`rank` must be \"relevance\" or \"recent\"".into())),
        };
        let mut h = DefaultHasher::new();
        source.hash(&mut h);
        Ok(Self {
            src_hash: h.finish(),
            vm,
            budget,
            doc,
            each,
            key,
            fields,
            recent,
        })
    }

    /// `"board"` covers exactly `board`; `"channel:*"` covers `channel:<anything>`.
    pub fn covers(&self, doc_name: &str) -> bool {
        match self.doc.strip_suffix('*') {
            Some(prefix) => doc_name.len() > prefix.len() && doc_name.starts_with(prefix),
            None => doc_name == self.doc,
        }
    }

    pub fn recent(&self) -> bool {
        self.recent
    }

    /// Compare `doc` against the fingerprints of the last plan (`None` the first time) and say
    /// what to upsert and delete. Errors only for a spec that cannot apply to this doc at all.
    pub fn plan(&self, doc_name: &str, doc: &LoroDoc, prev: Option<&[u8]>) -> Result<Plan, String> {
        let root = doc.get_deep_value();
        let prev: Prints = prev
            .and_then(|b| serde_json::from_slice(b).ok())
            .unwrap_or_default();
        let mut plan = Plan::default();
        let mut next = Prints {
            rest: hash_except(&root, &self.each) ^ self.src_hash,
            records: BTreeMap::new(),
        };
        let full = next.rest != prev.rest;

        let records = self.records(doc_name, &root, &prev, &mut plan.errors)?;
        let mut failed = HashSet::new();
        let mut doc_table = None;
        for (id, value, h) in records {
            if !full && prev.records.get(&id) == Some(&h) {
                next.records.insert(id, h);
                continue;
            }
            if doc_table.is_none() {
                doc_table = Some(frozen(&self.vm, &root).map_err(|e| e.to_string())?);
            }
            plan.fields_runs += 1;
            match self.run_fields(&id, value, doc_table.as_ref().unwrap()) {
                Ok(Some(f)) => {
                    plan.upserts.push((id.clone(), f));
                    next.records.insert(id, h);
                }
                Ok(None) => {
                    if prev.records.contains_key(&id) {
                        plan.deletes.push(id.clone());
                    }
                    next.records.insert(id, h);
                }
                Err(e) => {
                    plan.errors.push(format!("{doc_name}/{id}: {e}"));
                    failed.insert(id);
                }
            }
        }
        plan.deletes.extend(
            prev.records
                .keys()
                .filter(|id| !next.records.contains_key(*id) && !failed.contains(*id))
                .cloned(),
        );
        plan.prints = serde_json::to_vec(&next).map_err(|e| e.to_string())?;
        Ok(plan)
    }

    /// The collection's `(id, value, fingerprint)`s. A list's ids come from `key`, which is only
    /// called for a value not seen before: the same value had the same key last time.
    fn records<'v>(
        &self,
        doc_name: &str,
        root: &'v LoroValue,
        prev: &Prints,
        errors: &mut Vec<String>,
    ) -> Result<Vec<(String, &'v LoroValue, u64)>, String> {
        let Some(coll) = self.each.iter().try_fold(root, |v, seg| match v {
            LoroValue::Map(m) => m.get(seg),
            _ => None,
        }) else {
            return Ok(Vec::new());
        };
        match coll {
            LoroValue::Map(m) => {
                let mut out: Vec<_> = m.iter().map(|(k, v)| (k.clone(), v, fingerprint(v))).collect();
                out.sort_by(|a, b| a.0.cmp(&b.0));
                Ok(out)
            }
            LoroValue::List(items) => {
                let Some(key) = &self.key else {
                    return Err(format!(
                        "index.lua: `{}` is a list, so the spec needs `key = function(rec) return rec.id end` — a position is not an id",
                        self.each.join(".")
                    ));
                };
                let seen: HashMap<u64, &String> = prev.records.iter().map(|(id, h)| (*h, id)).collect();
                let mut out = Vec::with_capacity(items.len());
                for (i, v) in items.iter().enumerate() {
                    let h = fingerprint(v);
                    if let Some(id) = seen.get(&h) {
                        out.push(((*id).clone(), v, h));
                        continue;
                    }
                    match self.call_key(key, v) {
                        Ok(id) => out.push((id, v, h)),
                        Err(e) => errors.push(format!("{doc_name}/#{}: {e}", i + 1)),
                    }
                }
                Ok(out)
            }
            _ => Ok(Vec::new()),
        }
    }

    fn call_key(&self, key: &Function, v: &LoroValue) -> Result<String, String> {
        let _armed = self.budget.arm();
        let rec = frozen(&self.vm, v).map_err(|e| e.to_string())?;
        match key.call::<Value>(rec).map_err(|e| e.to_string())? {
            Value::String(s) => Ok(s.to_str().map_err(|e| e.to_string())?.to_string()),
            Value::Integer(n) => Ok(n.to_string()),
            Value::Number(n) if n.fract() == 0.0 => Ok((n as i64).to_string()),
            other => Err(format!("key returned {}, not a string id", other.type_name())),
        }
    }

    fn run_fields(&self, id: &str, v: &LoroValue, doc: &Value) -> Result<Option<Fields>, String> {
        let _armed = self.budget.arm();
        let rec = frozen(&self.vm, v).map_err(|e| e.to_string())?;
        let t = match self
            .fields
            .call::<Value>((id, rec, doc.clone()))
            .map_err(|e| e.to_string())?
        {
            Value::Nil => return Ok(None),
            Value::Table(t) => t,
            other => return Err(format!("fields returned {}, not a table or nil", other.type_name())),
        };
        to_fields(&t).map(Some)
    }
}

fn to_fields(t: &Table) -> Result<Fields, String> {
    let mut f = Fields::default();
    for pair in t.pairs::<Value, Value>() {
        let (k, v) = pair.map_err(|e| e.to_string())?;
        let name = k.to_string().unwrap_or_default();
        let text = |v: &Value| match v {
            Value::String(s) => s.to_str().map(|s| s.to_string()).map_err(|e| e.to_string()),
            Value::Integer(n) => Ok(n.to_string()),
            Value::Number(n) => Ok(n.to_string()),
            Value::Boolean(b) => Ok(b.to_string()),
            other => Err(format!("`{name}` must be a string, got {}", other.type_name())),
        };
        match (name.as_str(), &v) {
            (_, Value::Nil) => {}
            ("title", v) => f.title = Some(text(v)?),
            ("body", v) => f.body = Some(text(v)?),
            ("time", Value::Number(n)) => f.time = Some(*n),
            ("time", Value::Integer(n)) => f.time = Some(*n as f64),
            ("time", other) => return Err(format!("`time` must be a number, got {}", other.type_name())),
            ("facet", Value::Table(facets)) => {
                for pair in facets.pairs::<String, Value>() {
                    let (fk, fv) = pair.map_err(|e| e.to_string())?;
                    if !matches!(fv, Value::Nil) {
                        f.facets.insert(fk, text(&fv)?);
                    }
                }
            }
            ("facet", other) => return Err(format!("`facet` must be a table, got {}", other.type_name())),
            _ => {
                return Err(format!(
                    "fields returned unknown key `{name}` (expected {})",
                    FIELD_KEYS.join(", ")
                ));
            }
        }
    }
    Ok(f)
}

/// A doc value as a frozen Lua table — `fields` reads it, and a write is a loud error rather
/// than a silent change to a copy nobody else sees.
fn frozen(lua: &Lua, v: &LoroValue) -> mlua::Result<Value> {
    Ok(match v {
        LoroValue::Null | LoroValue::Container(_) | LoroValue::Binary(_) => Value::Nil,
        LoroValue::Bool(b) => Value::Boolean(*b),
        LoroValue::Double(n) => Value::Number(*n),
        LoroValue::I64(n) => Value::Number(*n as f64),
        LoroValue::String(s) => Value::String(lua.create_string(s.as_str())?),
        LoroValue::List(items) => {
            let t = lua.create_table_with_capacity(items.len(), 0)?;
            for (i, item) in items.iter().enumerate() {
                t.raw_set(i + 1, frozen(lua, item)?)?;
            }
            t.set_readonly(true);
            Value::Table(t)
        }
        LoroValue::Map(m) => {
            let t = lua.create_table_with_capacity(0, m.len())?;
            for (k, item) in m.iter() {
                t.raw_set(k.as_str(), frozen(lua, item)?)?;
            }
            t.set_readonly(true);
            Value::Table(t)
        }
    })
}

/// Stable within a build; a different std hasher after an upgrade only costs one re-index.
fn fingerprint(v: &LoroValue) -> u64 {
    let mut h = DefaultHasher::new();
    hash_value(v, &mut h);
    h.finish()
}

/// Everything in the doc except the collection at `path` — the part a join reads.
fn hash_except(root: &LoroValue, path: &[String]) -> u64 {
    fn walk(v: &LoroValue, path: &[String], h: &mut DefaultHasher) {
        match (v, path) {
            (LoroValue::Map(m), [seg, rest @ ..]) => {
                5u8.hash(h);
                let mut keys: Vec<_> = m.keys().collect();
                keys.sort();
                for k in keys {
                    k.hash(h);
                    if k == seg {
                        if !rest.is_empty() {
                            walk(&m[k], rest, h);
                        }
                    } else {
                        hash_value(&m[k], h);
                    }
                }
            }
            _ => hash_value(v, h),
        }
    }
    let mut h = DefaultHasher::new();
    walk(root, path, &mut h);
    h.finish()
}

fn hash_value(v: &LoroValue, h: &mut DefaultHasher) {
    match v {
        LoroValue::Null => 0u8.hash(h),
        LoroValue::Bool(b) => (1u8, b).hash(h),
        LoroValue::Double(n) => (2u8, n.to_bits()).hash(h),
        LoroValue::I64(n) => (3u8, n).hash(h),
        LoroValue::String(s) => (4u8, s.as_str()).hash(h),
        LoroValue::Map(m) => {
            5u8.hash(h);
            let mut keys: Vec<_> = m.keys().collect();
            keys.sort();
            for k in keys {
                k.hash(h);
                hash_value(&m[k], h);
            }
        }
        LoroValue::List(items) => {
            (6u8, items.len()).hash(h);
            for item in items.iter() {
                hash_value(item, h);
            }
        }
        LoroValue::Binary(b) => (7u8, &b[..]).hash(h),
        LoroValue::Container(id) => (8u8, id.to_string()).hash(h),
    }
}
