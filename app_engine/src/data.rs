//! World-B data access: the seam between the engine and the host's query executor. Imported
//! `.table` sources stay host-side; the engine *declares* queries (Polars SQL + named ops) and
//! consumes only results — a window of result rows (host→`Node`, all Rust-side) and tiny KPI
//! scalars. Bulk rows never cross into Lua. No Polars here: that lives in the host's `table_query`
//! crate, behind this trait. See [[migration-dataflow-model]] / [[dashboard-apps-model]].

use std::rc::Rc;

use table_core::{CellValue, Row, RowOp, TableSpec};

/// Opaque id of a cached query result the host holds. Resolved back to a result via the trait.
pub type Handle = u64;

/// A query result is `Ready` (its handle) or still computing on a worker thread (`Pending`). A
/// `Pending→Ready` transition is a state change: the host repaints, the next `view()` re-resolves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryState {
    Ready(Handle),
    Pending,
}

/// A named transform SQL can't express. v1 ships only `Pivot` (cross-tab); rolling/explode/… get
/// added the same way. Each is implemented host-side via Polars.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NamedOp {
    /// `on`'s distinct values become columns, grouped by `index`, summing `values`.
    Pivot { on: String, index: String, values: String },
}

/// The host's World-B data plane, called by the `data` Lua binding. Cheap, cache-backed calls:
/// `sql`/`op` are idempotent for an unchanged `(sources, version, query)` (the host caches by
/// version vector), so a `view()` re-running them every frame just re-reads the cache.
pub trait DataAccess {
    /// Register a source `.table` under `alias` (lazy, idempotent); records the dep so the host
    /// watches its version vector and invalidates results on a write.
    fn use_source(&self, alias: &str);

    /// Run Polars SQL over the registered sources (each `FROM alias`); `:name` params bind from
    /// `params` as safe literals. Joins are SQL JOINs across registered sources.
    fn sql(&self, query: &str, params: &[(String, CellValue)]) -> QueryState;

    /// A named op over a prior result (v1: pivot). The output is another result.
    fn op(&self, handle: Handle, op: &NamedOp) -> QueryState;

    /// The result's schema — columns + types — for the grid / chart axes. All columns read-only
    /// (results are derived).
    fn spec(&self, handle: Handle) -> TableSpec;

    /// Total result row count (virtual-scroll extent / chart length).
    fn len(&self, handle: Handle) -> usize;

    /// A window of result rows `[offset, offset+count)` — the synchronous slice the grid renders.
    fn window(&self, handle: Handle, offset: usize, count: usize) -> Vec<Row>;

    /// A KPI scalar: the first row's value of column `col`. The only data that crosses into Lua.
    fn value(&self, handle: Handle, col: &str) -> CellValue;

    /// Apply a structured write to a source table (by `alias`), bumping its version. Returns the
    /// affected row id.
    fn mutate(&self, alias: &str, op: RowOp) -> Result<String, String>;

    /// A composite version stamp over every registered source — folded into the engine's scene key
    /// so an external write (a peer / MCP / the native grid) triggers a rebuild + recompute.
    fn version(&self) -> u64;
}

/// Shared handle to a host data plane (cloned into the engine and the Lua binding).
pub type DataAccessRef = Rc<dyn DataAccess>;
