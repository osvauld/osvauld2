//! Host-side query executor for World B (imported tables → dashboards). Turns a `.table` Loro
//! layer into a Polars frame, runs Polars SQL over one or more named sources (joins are just SQL
//! JOINs), and projects results back to `table_core` rows the native grid/chart render. Stateless:
//! the host owns the `vv`-keyed cache + handle registry (it holds the Vault and the version
//! vectors). Polars lives here, OFF app_engine's render path.

use anyhow::Result;
use loro::LoroDoc;
use polars::prelude::*;
use polars::sql::SQLContext;
use table_core::{typed_rows, CellValue, ColKind, Row, TableSpec};

/// An opaque computed result — a Polars frame plus the projection back to `table_core` types. Hosts
/// hold and cache these without depending on Polars: `schema`/`len`/`rows`/`value` are the whole
/// surface the engine consumes (a window of rows + KPI scalars). Built by [`frame`] / [`sql`] /
/// [`pivot`]. Cheap to clone (Polars columns are `Arc`-backed).
#[derive(Clone)]
pub struct QueryResult {
    df: DataFrame,
}

impl QueryResult {
    /// An empty result (no columns/rows) — the host's fallback when a query errors, so the grid
    /// renders empty instead of stalling.
    pub fn empty() -> Self {
        QueryResult { df: DataFrame::empty() }
    }

    /// The result schema (columns + types), all read-only — results are derived.
    pub fn schema(&self) -> TableSpec {
        result_spec(&self.df)
    }

    /// Total result row count (virtual-scroll extent).
    pub fn len(&self) -> usize {
        self.df.height()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// A window of result rows `[offset, offset+count)`, projected to `table_core` rows.
    pub fn rows(&self, offset: usize, count: usize) -> Vec<Row> {
        result_rows(&self.df, offset, count)
    }

    /// The first row's value of `col` — a KPI scalar.
    pub fn value(&self, col: &str) -> CellValue {
        result_value(&self.df, col)
    }

    /// Result column names (debug / introspection).
    pub fn column_names(&self) -> Vec<String> {
        self.df.get_column_names().iter().map(|s| s.to_string()).collect()
    }

    /// A pretty-printed table of the result (for the MCP `table_sql` tool's text reply).
    pub fn display(&self) -> String {
        format!("{}", self.df)
    }
}

/// Build a result from a `.table` layer's typed rows. Number/Decimal → Float64, text / select /
/// date → String, check → Boolean, plus the stable `id` as a String column (so a join can address
/// rows). This is the source currency: register results in [`sql`] to query/join them.
pub fn frame(doc: &LoroDoc, list: &str, spec: &TableSpec) -> Result<QueryResult> {
    let rows = typed_rows(doc, list, spec);
    Ok(QueryResult { df: frame_from_rows(spec, &rows)? })
}

fn frame_from_rows(spec: &TableSpec, rows: &[Row]) -> Result<DataFrame> {
    let mut cols: Vec<Column> = Vec::with_capacity(spec.columns.len() + 1);
    let ids: Vec<Option<String>> = rows.iter().map(|r| Some(r.id.clone())).collect();
    cols.push(Series::new("id".into(), ids).into_column());
    for c in &spec.columns {
        let key = c.key.as_str();
        let series = match c.kind {
            ColKind::Text | ColKind::Select | ColKind::Date => {
                let v: Vec<Option<String>> =
                    rows.iter().map(|r| cell_str(r.cells.get(&c.key))).collect();
                Series::new(key.into(), v)
            }
            ColKind::Number | ColKind::Decimal => {
                let v: Vec<Option<f64>> =
                    rows.iter().map(|r| cell_f64(r.cells.get(&c.key))).collect();
                Series::new(key.into(), v)
            }
            ColKind::Check => {
                let v: Vec<Option<bool>> =
                    rows.iter().map(|r| cell_bool(r.cells.get(&c.key))).collect();
                Series::new(key.into(), v)
            }
        };
        cols.push(series.into_column());
    }
    Ok(DataFrame::new_infer_height(cols)?)
}

/// Run a Polars SQL `query` over `sources` (each registered under its alias — `FROM alias`).
/// `:name` parameters are substituted as safe SQL literals (no string-splicing by the caller).
/// Joins fall out of SQL JOIN across the registered tables.
pub fn sql(
    sources: Vec<(String, QueryResult)>,
    query: &str,
    params: &[(String, CellValue)],
) -> Result<QueryResult> {
    let mut ctx = SQLContext::new();
    for (name, src) in sources {
        ctx.register(&name, src.df.lazy());
    }
    let bound = bind_params(query, params);
    Ok(QueryResult { df: ctx.execute(&bound)?.collect()? })
}

/// A cross-tab: `on`'s distinct values become columns, grouped by `index`, summing `values`. The
/// one named op SQL can't express in v1; more (rolling/explode) get added the same way. (0.54
/// dropped the eager `pivot` fn, so this is a group-by with one filtered-sum column per `on` value.)
pub fn pivot(src: &QueryResult, on: &str, index: &str, values: &str) -> Result<QueryResult> {
    let df = &src.df;
    let on_col = df.column(on)?;
    let mut distinct = std::collections::BTreeSet::new();
    for i in 0..on_col.len() {
        let av = on_col.get(i)?;
        if !av.is_null() {
            distinct.insert(any_to_cell(&av).display());
        }
    }
    let aggs: Vec<Expr> = distinct
        .iter()
        .map(|v| col(values).filter(col(on).eq(lit(v.clone()))).sum().alias(v.as_str()))
        .collect();
    Ok(QueryResult { df: df.clone().lazy().group_by([col(index)]).agg(aggs).collect()? })
}

/// Derive a `table_core` schema from a result frame's column dtypes (all read-only — results are
/// derived).
fn result_spec(df: &DataFrame) -> TableSpec {
    let columns = df
        .columns()
        .iter()
        .map(|c| table_core::Column {
            key: c.name().to_string(),
            label: c.name().to_string(),
            kind: dtype_to_kind(c.dtype()),
            width: None,
            locked: true,
            options: Vec::new(),
            transitions: Default::default(),
        })
        .collect();
    TableSpec {
        columns,
        filter: Vec::new(),
        order: None,
        row_height: None,
        row_heights: Default::default(),
    }
}

/// A window of result rows `[offset, offset+count)` projected to `table_core` [`Row`]s — the
/// synchronous slice the grid renders (no recompute on scroll). Synthetic ids (`r{i}`) for derived
/// rows that carry no stored `id`.
fn result_rows(df: &DataFrame, offset: usize, count: usize) -> Vec<Row> {
    let total = df.height();
    let end = (offset + count).min(total);
    let cols = df.columns();
    let mut rows = Vec::with_capacity(end.saturating_sub(offset));
    for i in offset..end {
        let mut cells = std::collections::HashMap::new();
        for c in cols {
            let av = c.get(i).unwrap_or(AnyValue::Null);
            cells.insert(c.name().to_string(), any_to_cell(&av));
        }
        let id = match cells.get("id") {
            Some(CellValue::Text(s)) => s.clone(),
            _ => format!("r{i}"),
        };
        rows.push(Row { id, cells });
    }
    rows
}

/// The first row's value of `col` — a KPI scalar (the only data that crosses into Lua).
fn result_value(df: &DataFrame, col: &str) -> CellValue {
    df.column(col)
        .ok()
        .and_then(|c| c.get(0).ok())
        .map(|av| any_to_cell(&av))
        .unwrap_or(CellValue::Empty)
}

// --- conversions ------------------------------------------------------------

fn cell_str(v: Option<&CellValue>) -> Option<String> {
    match v {
        None | Some(CellValue::Empty) => None,
        Some(c) => Some(c.display()),
    }
}

fn cell_f64(v: Option<&CellValue>) -> Option<f64> {
    use rust_decimal::prelude::ToPrimitive;
    match v {
        Some(CellValue::Number(n)) => Some(*n),
        Some(CellValue::Decimal(d)) => d.to_f64(),
        Some(CellValue::Bool(b)) => Some(if *b { 1.0 } else { 0.0 }),
        Some(CellValue::Text(s)) => s.trim().parse().ok(),
        _ => None,
    }
}

fn cell_bool(v: Option<&CellValue>) -> Option<bool> {
    match v {
        Some(CellValue::Bool(b)) => Some(*b),
        Some(CellValue::Number(n)) => Some(*n != 0.0),
        _ => None,
    }
}

fn any_to_cell(av: &AnyValue) -> CellValue {
    match av {
        AnyValue::Null => CellValue::Empty,
        AnyValue::Boolean(b) => CellValue::Bool(*b),
        AnyValue::String(s) => CellValue::Text(s.to_string()),
        AnyValue::StringOwned(s) => CellValue::Text(s.to_string()),
        _ => match av.try_extract::<f64>() {
            Ok(f) => CellValue::Number(f),
            Err(_) => CellValue::Text(av.to_string()),
        },
    }
}

fn dtype_to_kind(dt: &DataType) -> ColKind {
    match dt {
        DataType::Boolean => ColKind::Check,
        DataType::String => ColKind::Text,
        DataType::Int8
        | DataType::Int16
        | DataType::Int32
        | DataType::Int64
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32
        | DataType::UInt64
        | DataType::Float32
        | DataType::Float64 => ColKind::Number,
        _ => ColKind::Text,
    }
}

// --- parameter binding ------------------------------------------------------

fn bind_params(query: &str, params: &[(String, CellValue)]) -> String {
    let mut out = query.to_string();
    for (name, val) in params {
        out = replace_token(&out, &format!(":{name}"), &literal(val));
    }
    out
}

/// Replace `:name` only at token boundaries (so `:status` never clobbers `:status_x`).
fn replace_token(s: &str, token: &str, repl: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with(token) {
            let after = s[i + token.len()..].chars().next();
            if !matches!(after, Some(c) if c.is_alphanumeric() || c == '_') {
                result.push_str(repl);
                i += token.len();
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        result.push(ch);
        i += ch.len_utf8();
    }
    result
}

fn literal(v: &CellValue) -> String {
    match v {
        CellValue::Text(s) => format!("'{}'", s.replace('\'', "''")),
        CellValue::Number(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", *n as i64),
        CellValue::Number(n) => format!("{n}"),
        CellValue::Decimal(d) => d.to_string(),
        CellValue::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        CellValue::Empty => "NULL".to_string(),
    }
}

#[cfg(test)]
mod tests;
