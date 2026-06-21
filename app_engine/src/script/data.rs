//! The `data` binding: World-B data access exposed to Lua. A declaration layer — it carries
//! opaque result *handles*, never rows. `data.use(alias)` registers an imported `.table` source;
//! `data.table(alias)` is a writable handle to it; `data.sql(query, params)` runs Polars SQL over
//! the registered sources (joins are SQL JOINs). Each returns a [`LuaQuery`] consumable by
//! `ui.table{ source = … }` / `ui.chart{ data = … }`, with `:value(col)` (a KPI scalar — the one
//! datum that crosses into Lua), `:pivot{…}`, and `:add/:set/:remove` (sources only). The bulk
//! data path (host cache → window → pixels) stays entirely in Rust behind [`DataAccess`].

use mlua::{Lua, Result as LuaResult, Table, UserData, UserDataMethods, Value};
use table_core::{CellValue, RowOp};

use crate::data::{DataAccessRef, NamedOp, QueryState};

/// Install the `data` global (a namespace table, so `data.use(...)` is a dot call like a module)
/// backed by `access`. The engine installs this *after* the app's setup chunk has run, so apps must
/// call `data.*` from inside their `view()` function (which runs per-frame), never at module top
/// level — registration is lazy + idempotent, so per-frame `data.use` is free.
pub(crate) fn install(lua: &Lua, access: DataAccessRef) -> LuaResult<()> {
    let t = lua.create_table()?;
    // data.use(alias) — register a source; returns a writable handle (reads = SELECT * FROM it).
    let a = access.clone();
    t.set(
        "use",
        lua.create_function(move |_, alias: String| Ok(open_source(&a, alias)))?,
    )?;
    // data.table(alias) — same as use; the explicit "this is a source I write to" spelling.
    let a = access.clone();
    t.set(
        "table",
        lua.create_function(move |_, alias: String| Ok(open_source(&a, alias)))?,
    )?;
    // data.sql(query, params?) — run Polars SQL over the registered sources; a read-only result.
    let a = access.clone();
    t.set(
        "sql",
        lua.create_function(move |_, (query, params): (String, Option<Table>)| {
            let params = parse_params(params)?;
            let state = a.sql(&query, &params);
            Ok(LuaQuery { access: a.clone(), source: None, state })
        })?,
    )?;
    lua.globals().set("data", t)
}

/// Register `alias` and resolve a read handle over all its rows; the alias is retained so writes
/// (`:add/:set/:remove`) address the source. The name is double-quoted (it may contain spaces),
/// with embedded quotes doubled per SQL.
fn open_source(access: &DataAccessRef, alias: String) -> LuaQuery {
    access.use_source(&alias);
    let quoted = alias.replace('"', "\"\"");
    let state = access.sql(&format!("SELECT * FROM \"{quoted}\""), &[]);
    LuaQuery { access: access.clone(), source: Some(alias), state }
}

/// A query result handle exposed to Lua. Holds the host data plane, the resolved result state, and
/// (for a source) the alias writes route to. Carries no rows.
struct LuaQuery {
    access: DataAccessRef,
    /// `Some(alias)` ⇒ a writable source; `None` ⇒ a derived (read-only) result.
    source: Option<String>,
    state: QueryState,
}

impl UserData for LuaQuery {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        // q:value(col) — the first row's value of `col` as a Lua scalar (a KPI). Nil while Pending.
        m.add_method("value", |lua, this, col: String| match this.state {
            QueryState::Ready(h) => cell_to_lua(lua, this.access.value(h, &col)),
            QueryState::Pending => Ok(Value::Nil),
        });
        // q:len() — result row count (0 while Pending).
        m.add_method("len", |_, this, ()| {
            Ok(match this.state {
                QueryState::Ready(h) => this.access.len(h),
                QueryState::Pending => 0,
            })
        });
        // q:pivot{ on, index, values } — a cross-tab over this result (a new derived result).
        m.add_method("pivot", |_, this, opts: Table| {
            let op = NamedOp::Pivot {
                on: req_str(&opts, "on")?,
                index: req_str(&opts, "index")?,
                values: req_str(&opts, "values")?,
            };
            let state = match this.state {
                QueryState::Ready(h) => this.access.op(h, &op),
                QueryState::Pending => QueryState::Pending,
            };
            Ok(LuaQuery { access: this.access.clone(), source: None, state })
        });
        // Writes (sources only) — append / edit-by-id / delete-by-id; route to DataAccess.mutate.
        m.add_method("add", |_, this, fields: Table| {
            this.write(RowOp::Add { fields: parse_fields(&fields)? })
        });
        m.add_method("set", |_, this, (row, fields): (String, Table)| {
            this.write(RowOp::Set { row, fields: parse_fields(&fields)? })
        });
        m.add_method("remove", |_, this, row: String| this.write(RowOp::Remove { row }));
    }
}

impl LuaQuery {
    /// Apply a write to the backing source (error if this handle is a derived result).
    fn write(&self, op: RowOp) -> LuaResult<String> {
        let alias = self
            .source
            .as_deref()
            .ok_or_else(|| mlua::Error::RuntimeError("cannot write to a derived query result".into()))?;
        self.access.mutate(alias, op).map_err(mlua::Error::RuntimeError)
    }
}

/// A query handle's host plane + resolved state, cloned out for the view walk to render (`ui.table`
/// `source` / `ui.chart` `data`).
#[derive(Clone)]
pub(crate) struct QueryRef {
    pub access: DataAccessRef,
    pub state: QueryState,
    /// The source alias, when this is a writable source (`data.table`/`data.use`) — used as the
    /// stable scroll-region id for the rendered grid.
    pub source: Option<String>,
}

/// Extract a [`QueryRef`] from a Lua value if it's a `data` query handle (`None` otherwise).
pub(crate) fn query_ref(value: &Value) -> Option<QueryRef> {
    match value {
        Value::UserData(ud) => ud.borrow::<LuaQuery>().ok().map(|q| QueryRef {
            access: q.access.clone(),
            state: q.state,
            source: q.source.clone(),
        }),
        _ => None,
    }
}

// --- value conversion -------------------------------------------------------

/// A Lua params table (`{ status = "open", min = 10 }`) → bound SQL params. Non-scalar values are
/// skipped.
fn parse_params(params: Option<Table>) -> LuaResult<Vec<(String, CellValue)>> {
    let mut out = Vec::new();
    if let Some(t) = params {
        for pair in t.pairs::<String, Value>() {
            let (k, v) = pair?;
            if let Some(cv) = scalar(&v) {
                out.push((k, cv));
            }
        }
    }
    Ok(out)
}

/// A Lua table of scalar fields (`{ segment = "smb", sales = 100 }`) → cells for a [`RowOp`].
fn parse_fields(t: &Table) -> LuaResult<std::collections::HashMap<String, CellValue>> {
    let mut out = std::collections::HashMap::new();
    for pair in t.pairs::<String, Value>() {
        let (k, v) = pair?;
        if let Some(cv) = scalar(&v) {
            out.insert(k, cv);
        }
    }
    Ok(out)
}

fn scalar(v: &Value) -> Option<CellValue> {
    match v {
        Value::Boolean(b) => Some(CellValue::Bool(*b)),
        Value::Integer(i) => Some(CellValue::Number(*i as f64)),
        Value::Number(n) => Some(CellValue::Number(*n)),
        Value::String(s) => Some(CellValue::Text(s.to_str().ok()?.to_string())),
        _ => None,
    }
}

fn cell_to_lua(lua: &Lua, v: CellValue) -> LuaResult<Value> {
    Ok(match v {
        CellValue::Text(s) => Value::String(lua.create_string(s)?),
        CellValue::Number(n) => Value::Number(n),
        // Exact decimals cross as a float for display/formatting (KPI scalars only).
        CellValue::Decimal(d) => Value::Number(d.to_string().parse().unwrap_or(0.0)),
        CellValue::Bool(b) => Value::Boolean(b),
        CellValue::Empty => Value::Nil,
    })
}

fn req_str(t: &Table, key: &str) -> LuaResult<String> {
    match t.get::<Value>(key)? {
        Value::String(s) => Ok(s.to_str()?.to_string()),
        _ => Err(mlua::Error::RuntimeError(format!("pivot needs a string '{key}'"))),
    }
}
