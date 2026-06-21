//! The host's concrete [`DataAccess`] for dashboard apps (World B). Imported `.table` sources stay
//! host-side as Loro docs in the vault; this resolves an app's `data.use(alias)` to the workspace's
//! `.table` item of that name, builds a Polars frame from it via `table_query`, runs the app's SQL,
//! and hands the engine only results (windowed rows + KPI scalars) — bulk data never enters Lua.
//!
//! Caching is generation-keyed: every source frame + query result is cached, and a write (this
//! app's own `mutate`, or an external `.table` write the shell forwards via [`TableData::invalidate`])
//! bumps the generation, dropping the caches so the next `view()` recomputes. The composite
//! generation is the engine's `version()` — folded into its scene key, so a source write triggers a
//! rebuild + recompute. Synchronous for v1 (always `Ready`); the async worker can drop in behind the
//! `QueryState::Pending` contract later.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use app_engine::data::{DataAccess, Handle, NamedOp, QueryState};
use loro::LoroDoc;
use table_core::RowOp;
use table_query::QueryResult;
use vault::{ItemKind, Vault};

/// The list container every `.table` keeps its rows in.
const ROWS: &str = "rows";

pub(crate) struct TableData {
    vault: Vault,
    ws_id: String,
    /// Bumped on every write (own or forwarded); the engine's `version()`.
    generation: Cell<u64>,
    /// alias → resolved `.table` item id (`None` = no such item). Re-resolved after invalidation.
    aliases: RefCell<HashMap<String, Option<String>>>,
    /// alias → its source frame, for the current generation (rebuilt lazily after a bump).
    sources: RefCell<HashMap<String, QueryResult>>,
    /// query/op cache-key → handle, stable within a generation (so a re-running `view()` reuses it).
    keys: RefCell<HashMap<String, Handle>>,
    /// handle → computed result.
    results: RefCell<HashMap<Handle, QueryResult>>,
    next_handle: Cell<Handle>,
}

impl TableData {
    pub(crate) fn new(vault: &Vault, ws_id: &str) -> Self {
        TableData {
            vault: vault.clone(),
            ws_id: ws_id.to_string(),
            generation: Cell::new(1),
            aliases: RefCell::default(),
            sources: RefCell::default(),
            keys: RefCell::default(),
            results: RefCell::default(),
            next_handle: Cell::new(1),
        }
    }

    /// A `.table` item `item_id` was written elsewhere. If it backs one of our sources (or we
    /// haven't resolved sources yet), drop the caches + bump the generation so the dashboard
    /// recomputes on the next frame.
    pub(crate) fn invalidate(&self, item_id: &str) {
        let relevant = {
            let a = self.aliases.borrow();
            a.is_empty() || a.values().any(|v| v.as_deref() == Some(item_id))
        };
        if relevant {
            self.bump();
        }
    }

    /// Drop every cache and advance the generation.
    fn bump(&self) {
        self.generation.set(self.generation.get() + 1);
        self.aliases.borrow_mut().clear();
        self.sources.borrow_mut().clear();
        self.keys.borrow_mut().clear();
        self.results.borrow_mut().clear();
    }

    /// Resolve `alias` to a `.table` item id by name in this workspace (cached). `None` if absent.
    fn item_id(&self, alias: &str) -> Option<String> {
        if let Some(hit) = self.aliases.borrow().get(alias) {
            return hit.clone();
        }
        let found = self
            .vault
            .items(&self.ws_id)
            .unwrap_or_default()
            .into_iter()
            .find(|i| i.kind == ItemKind::Table && i.name == alias)
            .map(|i| i.id);
        self.aliases.borrow_mut().insert(alias.to_string(), found.clone());
        found
    }

    /// The source frame for `alias` (cached for this generation). `None` if the item is missing or
    /// fails to load.
    fn source(&self, alias: &str) -> Option<QueryResult> {
        if let Some(r) = self.sources.borrow().get(alias) {
            return Some(r.clone());
        }
        let item_id = self.item_id(alias)?;
        let bytes = self.vault.get_state(&self.ws_id, &item_id).ok().flatten()?;
        let doc = LoroDoc::new();
        doc.import(&bytes).ok()?;
        let spec = table_core::read_schema(&doc)?;
        let r = table_query::frame(&doc, ROWS, &spec).ok()?;
        self.sources.borrow_mut().insert(alias.to_string(), r.clone());
        Some(r)
    }

    /// Store a result under a fresh handle and return it as `Ready`.
    fn ready(&self, key: String, result: QueryResult) -> QueryState {
        let h = self.next_handle.get();
        self.next_handle.set(h + 1);
        self.results.borrow_mut().insert(h, result);
        self.keys.borrow_mut().insert(key, h);
        QueryState::Ready(h)
    }
}

impl DataAccess for TableData {
    fn use_source(&self, alias: &str) {
        // Lazy: resolve + cache the alias→item mapping (item_id does the insert) so it's registered
        // for joins; the frame itself loads on first query. (Don't hold an `aliases` borrow across
        // `item_id` — it borrows `aliases` too.)
        let _ = self.item_id(alias);
    }

    fn sql(&self, query: &str, params: &[(String, table_core::CellValue)]) -> QueryState {
        let gen = self.generation.get();
        let key = format!("{gen}|sql|{query}|{params:?}");
        if let Some(&h) = self.keys.borrow().get(&key) {
            return QueryState::Ready(h);
        }
        // Register every known source frame, so any aliased table in the SQL (incl. joins) resolves.
        let aliases: Vec<String> = self.aliases.borrow().keys().cloned().collect();
        let mut sources = Vec::new();
        for a in aliases {
            if let Some(frame) = self.source(&a) {
                sources.push((a, frame));
            }
        }
        let result = table_query::sql(sources, query, params).unwrap_or_else(|e| {
            eprintln!("sthalam: data.sql failed: {e}");
            QueryResult::empty()
        });
        self.ready(key, result)
    }

    fn op(&self, handle: Handle, op: &NamedOp) -> QueryState {
        let gen = self.generation.get();
        let key = format!("{gen}|op|{handle}|{op:?}");
        if let Some(&h) = self.keys.borrow().get(&key) {
            return QueryState::Ready(h);
        }
        let Some(src) = self.results.borrow().get(&handle).cloned() else {
            return self.ready(key, QueryResult::empty());
        };
        let result = match op {
            NamedOp::Pivot { on, index, values } => {
                table_query::pivot(&src, on, index, values).unwrap_or_else(|e| {
                    eprintln!("sthalam: pivot failed: {e}");
                    QueryResult::empty()
                })
            }
        };
        self.ready(key, result)
    }

    fn spec(&self, handle: Handle) -> table_core::TableSpec {
        self.results
            .borrow()
            .get(&handle)
            .map(|r| r.schema())
            .unwrap_or_else(|| table_core::TableSpec {
                columns: Vec::new(),
                filter: Vec::new(),
                order: None,
                row_height: None,
                row_heights: HashMap::new(),
            })
    }

    fn len(&self, handle: Handle) -> usize {
        self.results.borrow().get(&handle).map_or(0, |r| r.len())
    }

    fn window(&self, handle: Handle, offset: usize, count: usize) -> Vec<table_core::Row> {
        self.results.borrow().get(&handle).map(|r| r.rows(offset, count)).unwrap_or_default()
    }

    fn value(&self, handle: Handle, col: &str) -> table_core::CellValue {
        self.results
            .borrow()
            .get(&handle)
            .map(|r| r.value(col))
            .unwrap_or(table_core::CellValue::Empty)
    }

    fn mutate(&self, alias: &str, op: RowOp) -> Result<String, String> {
        let item_id = self.item_id(alias).ok_or_else(|| format!("no .table named '{alias}'"))?;
        let doc = LoroDoc::new();
        if let Some(bytes) = self.vault.get_state(&self.ws_id, &item_id).ok().flatten() {
            doc.import(&bytes).map_err(|e| e.to_string())?;
        }
        let id = table_core::apply_row_op(&doc, ROWS, op)?;
        let snapshot = doc.export(loro::ExportMode::Snapshot).map_err(|e| e.to_string())?;
        self.vault.put_state(&self.ws_id, &item_id, &snapshot).map_err(|e| e.to_string())?;
        self.bump(); // the dashboard recomputes against its own write
        Ok(id)
    }

    fn version(&self) -> u64 {
        self.generation.get()
    }
}
