//! The table primitive's host-agnostic core: schema, row projection over a `doc:list`
//! (a Loro MovableList of row maps), the declarative query tier, and stable row identity.
//! Shared by every table host — `ui.table` in Lua apps, the MCP row tools, and (next) the
//! `.doc` table block — so they can't drift on row semantics. No egui, no mlua: rendering
//! and spec parsing stay with each host.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::str::FromStr;

use loro::{Container, LoroDoc, LoroList, LoroMap, LoroMovableList, LoroValue, ValueOrContainer};
use rust_decimal::Decimal;

/// A parsed table spec: columns plus the declarative query tier.
pub struct TableSpec {
    pub columns: Vec<Column>,
    /// Equality filters (`where = { status = "open" }`), ANDed.
    pub filter: Vec<(String, CellValue)>,
    /// `order_by = "due"` / `order_by = { "due", desc = true }`.
    pub order: Option<(String, bool)>,
    /// Fixed data-row height in px; `None` = content-sized.
    pub row_height: Option<f32>,
    /// Per-row height overrides by stable row id (user-resized rows), over `row_height`.
    pub row_heights: HashMap<String, f32>,
}

pub struct Column {
    pub key: String,
    pub label: String,
    pub kind: ColKind,
    /// Fixed width in px; `None` = share the leftover row width equally (flex).
    pub width: Option<f32>,
    /// `locked = true` keeps an otherwise-editable column read-only (ids, refs).
    pub locked: bool,
    /// A select column's legal values, in declaration order.
    pub options: Vec<String>,
    /// State-machine edges for a select column: current value → values it may move to.
    /// A value with no entry may move anywhere; an empty entry is terminal. UI guidance
    /// only — direct CRDT writers (MCP, peers) aren't blocked.
    pub transitions: HashMap<String, Vec<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColKind {
    Text,
    Number,
    Check,
    Select,
    /// A calendar date stored as `"YYYY-MM-DD"` text (sorts chronologically as a string).
    Date,
    /// Exact base-10 number (money, quantities that must not drift). Stored in Loro as a string;
    /// projected to [`CellValue::Decimal`] via the schema (a raw read sees only the string).
    Decimal,
}

impl ColKind {
    /// The stored-schema tag for this kind (inverse of [`ColKind::parse`]).
    pub fn as_str(self) -> &'static str {
        match self {
            ColKind::Text => "text",
            ColKind::Number => "number",
            ColKind::Check => "check",
            ColKind::Select => "select",
            ColKind::Date => "date",
            ColKind::Decimal => "decimal",
        }
    }

    /// Parse a stored-schema tag, defaulting unknown tags to `Text` (forward-compatible read).
    pub fn parse(s: &str) -> ColKind {
        match s {
            "number" => ColKind::Number,
            "check" => ColKind::Check,
            "select" => ColKind::Select,
            "date" => ColKind::Date,
            "decimal" => ColKind::Decimal,
            _ => ColKind::Text,
        }
    }
}

/// The values a select cell may move to from `current`: its `transitions` entry if one exists,
/// else every option.
pub fn legal_options<'a>(col: &'a Column, current: &str) -> Vec<&'a str> {
    match col.transitions.get(current) {
        Some(next) => next.iter().map(String::as_str).collect(),
        None => col.options.iter().map(String::as_str).collect(),
    }
}

/// Parse `"YYYY-MM-DD"` (one- or two-digit month/day accepted) as a real calendar date.
pub fn parse_date(s: &str) -> Option<(i32, u32, u32)> {
    let mut it = s.trim().splitn(3, '-');
    let y: i32 = it.next()?.parse().ok()?;
    let m: u32 = it.next()?.parse().ok()?;
    let d: u32 = it.next()?.parse().ok()?;
    (1..=12).contains(&m).then_some(())?;
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=days).contains(&d).then_some((y, m, d))
}

/// A date as canonical zero-padded `"YYYY-MM-DD"`.
pub fn format_date((y, m, d): (i32, u32, u32)) -> String {
    format!("{y:04}-{m:02}-{d:02}")
}

/// Parse text as an exact decimal, returning its canonical string form for Loro storage (a
/// `Decimal` cell is stored as a string). `None` when the text isn't a valid decimal.
pub fn parse_decimal(s: &str) -> Option<String> {
    Decimal::from_str(s.trim()).ok().map(|d| d.to_string())
}

/// One row projected out of the CRDT: its stable id and scalar cells by field key.
#[derive(Clone)]
pub struct Row {
    pub id: String,
    pub cells: HashMap<String, CellValue>,
}

/// A structured write against a `.table` row list, addressed by stable id (never display index).
/// The one write currency every host shares: the native grid's in-cell edit, a dashboard app's
/// `:add/:set/:remove`, and the MCP row tools all reduce to these. Applied host-side by
/// [`apply_row_op`] — bulk data never crosses into Lua to mutate.
#[derive(Clone, Debug)]
pub enum RowOp {
    /// Append a row; a supplied `id` (in `fields`) wins, else one is stamped. Returns the id.
    Add { fields: HashMap<String, CellValue> },
    /// Set the given fields on the row with stable id `row` (a `CellValue::Empty` deletes a field).
    Set { row: String, fields: HashMap<String, CellValue> },
    /// Delete the row with stable id `row`.
    Remove { row: String },
}

/// A scalar cell (the value subset a `doc:list` row map holds).
#[derive(Clone, Debug, PartialEq)]
pub enum CellValue {
    Text(String),
    Number(f64),
    /// Exact base-10 value (a `Decimal` column). Stored in Loro as a string, projected here via
    /// the schema; see [`coerce`].
    Decimal(Decimal),
    Bool(bool),
    Empty,
}

impl CellValue {
    pub fn display(&self) -> String {
        match self {
            CellValue::Text(s) => s.clone(),
            // A whole number prints without the trailing `.0` Lua/Loro float round-trips add.
            CellValue::Number(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", *n as i64),
            CellValue::Number(n) => format!("{n}"),
            CellValue::Decimal(d) => d.to_string(),
            CellValue::Bool(b) => b.to_string(),
            CellValue::Empty => String::new(),
        }
    }
}

/// A table row id unique across peers: the doc's (random, per-session) peer id + a process
/// counter. Stamped on every new row — Lua `list:add` and the MCP bridge use the same scheme.
pub fn row_id(doc: &LoroDoc) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    format!("{:x}-{:x}", doc.peer_id(), SEQ.fetch_add(1, Ordering::Relaxed))
}

/// The index of the row with stable id `row` in a list of row maps (`#index` is the fallback
/// id shown for rows born without one). Every edit addresses "first row with this id".
pub fn find_row(list: &LoroMovableList, row: &str) -> Option<usize> {
    for i in 0..list.len() {
        let Some(ValueOrContainer::Container(Container::Map(m))) = list.get(i) else {
            continue;
        };
        let matched = match m.get("id") {
            Some(ValueOrContainer::Value(LoroValue::String(s))) => *s == row,
            _ => row == format!("#{i}"),
        };
        if matched {
            return Some(i);
        }
    }
    None
}

/// Project every row of the named MovableList into [`Row`]s, in list order. A row without a
/// stamped `id` falls back to its display index (read-only safe; edits address by real id).
pub fn read_rows(doc: &LoroDoc, list: &str) -> Vec<Row> {
    let list = doc.get_movable_list(list);
    let mut rows = Vec::with_capacity(list.len());
    for i in 0..list.len() {
        let Some(ValueOrContainer::Container(Container::Map(map))) = list.get(i) else { continue };
        let LoroValue::Map(fields) = map.get_value() else { continue };
        let mut cells = HashMap::new();
        for (k, v) in fields.iter() {
            let cv = match v {
                LoroValue::String(s) => CellValue::Text(s.to_string()),
                LoroValue::I64(n) => CellValue::Number(*n as f64),
                LoroValue::Double(d) => CellValue::Number(*d),
                LoroValue::Bool(b) => CellValue::Bool(*b),
                _ => continue,
            };
            cells.insert(k.clone(), cv);
        }
        let id = match cells.get("id") {
            Some(CellValue::Text(s)) => s.clone(),
            _ => format!("#{i}"),
        };
        rows.push(Row { id, cells });
    }
    rows
}

/// Project rows of `list` with the schema applied, so `Decimal` columns come back as
/// [`CellValue::Decimal`] (a raw [`read_rows`] sees only the stored string). The schema-aware read
/// every typed host (direct `.table` grid, importing app) uses.
pub fn typed_rows(doc: &LoroDoc, list: &str, spec: &TableSpec) -> Vec<Row> {
    let mut rows = read_rows(doc, list);
    coerce(&mut rows, spec);
    rows
}

/// Re-type a raw projection against the schema: `Decimal` columns parse their stored string (or a
/// stray number) into an exact [`Decimal`]; an unparseable cell becomes `Empty`. Other kinds are
/// left as projected. Idempotent.
pub fn coerce(rows: &mut [Row], spec: &TableSpec) {
    for col in spec.columns.iter().filter(|c| c.kind == ColKind::Decimal) {
        for row in rows.iter_mut() {
            if let Some(cell) = row.cells.get_mut(&col.key) {
                *cell = match cell {
                    CellValue::Decimal(_) | CellValue::Empty => continue,
                    CellValue::Text(s) => {
                        Decimal::from_str(s.trim()).map_or(CellValue::Empty, CellValue::Decimal)
                    }
                    CellValue::Number(n) => {
                        Decimal::try_from(*n).map_or(CellValue::Empty, CellValue::Decimal)
                    }
                    CellValue::Bool(_) => CellValue::Empty,
                };
            }
        }
    }
}

/// The stored-schema container's root map (`schema`), holding a `columns` list. Lives in the
/// `.table` layer's own Loro doc alongside the row list — the *stored* provenance, the counterpart
/// of an app's declared (Lua) schema. Written once by migration, edited later via grid/MCP.
const SCHEMA_MAP: &str = "schema";
const SCHEMA_COLS: &str = "columns";

/// Persist `spec.columns` into the doc's stored-schema container, replacing any existing columns.
/// View config (filter/order/heights) is NOT stored here — it is per-view, not part of the data's
/// schema. `ref`/`compute`/`transitions` are deferred to the relations phase.
pub fn write_schema(doc: &LoroDoc, spec: &TableSpec) -> Result<(), loro::LoroError> {
    let schema = doc.get_map(SCHEMA_MAP);
    let cols = match schema.get(SCHEMA_COLS) {
        Some(ValueOrContainer::Container(Container::List(list))) => list,
        _ => schema.insert_container(SCHEMA_COLS, LoroList::new())?,
    };
    let n = cols.len();
    if n > 0 {
        cols.delete(0, n)?;
    }
    for c in &spec.columns {
        let m = cols.push_container(LoroMap::new())?;
        m.insert("key", c.key.as_str())?;
        m.insert("label", c.label.as_str())?;
        m.insert("type", c.kind.as_str())?;
        if let Some(w) = c.width {
            m.insert("width", w as f64)?;
        }
        if c.locked {
            m.insert("locked", true)?;
        }
        if !c.options.is_empty() {
            let opts = m.insert_container("options", LoroList::new())?;
            for o in &c.options {
                opts.push(o.as_str())?;
            }
        }
    }
    Ok(())
}

/// Read the stored schema back into a [`TableSpec`] (columns only; query fields default empty).
/// `None` when the doc carries no schema container — e.g. an app-owned list with a declared schema.
pub fn read_schema(doc: &LoroDoc) -> Option<TableSpec> {
    let schema = doc.get_map(SCHEMA_MAP);
    let Some(ValueOrContainer::Container(Container::List(list))) = schema.get(SCHEMA_COLS) else {
        return None;
    };
    let mut columns = Vec::with_capacity(list.len());
    for i in 0..list.len() {
        let Some(ValueOrContainer::Container(Container::Map(m))) = list.get(i) else { continue };
        let Some(key) = map_str(&m, "key") else { continue };
        let label = map_str(&m, "label").unwrap_or_else(|| key.clone());
        let kind = ColKind::parse(&map_str(&m, "type").unwrap_or_default());
        columns.push(Column {
            key,
            label,
            kind,
            width: map_f64(&m, "width").map(|w| w as f32),
            locked: map_bool(&m, "locked").unwrap_or(false),
            options: map_str_list(&m, "options"),
            transitions: HashMap::new(),
        });
    }
    Some(TableSpec {
        columns,
        filter: Vec::new(),
        order: None,
        row_height: None,
        row_heights: HashMap::new(),
    })
}

fn map_str(m: &LoroMap, key: &str) -> Option<String> {
    match m.get(key) {
        Some(ValueOrContainer::Value(LoroValue::String(s))) => Some(s.to_string()),
        _ => None,
    }
}

fn map_f64(m: &LoroMap, key: &str) -> Option<f64> {
    match m.get(key) {
        Some(ValueOrContainer::Value(LoroValue::Double(d))) => Some(d),
        Some(ValueOrContainer::Value(LoroValue::I64(n))) => Some(n as f64),
        _ => None,
    }
}

fn map_bool(m: &LoroMap, key: &str) -> Option<bool> {
    match m.get(key) {
        Some(ValueOrContainer::Value(LoroValue::Bool(b))) => Some(b),
        _ => None,
    }
}

fn map_str_list(m: &LoroMap, key: &str) -> Vec<String> {
    let Some(ValueOrContainer::Container(Container::List(list))) = m.get(key) else {
        return Vec::new();
    };
    (0..list.len())
        .filter_map(|i| match list.get(i) {
            Some(ValueOrContainer::Value(LoroValue::String(s))) => Some(s.to_string()),
            _ => None,
        })
        .collect()
}

/// Apply one structured [`RowOp`] to the named row list, committing once. The single host-side
/// write path: same id stamping and find-by-id as Lua `list:add` and the MCP row tools, so they
/// can't drift. Returns the affected row's id (`Add` returns the new/supplied id). A `Set`/`Remove`
/// for a missing id, or a duplicate supplied `Add` id, is an error.
pub fn apply_row_op(doc: &LoroDoc, list: &str, op: RowOp) -> Result<String, String> {
    let l = doc.get_movable_list(list);
    let id = match op {
        RowOp::Add { fields } => {
            // A supplied id must be a unique string — every edit addresses "first row with this
            // id", so a collision silently edits the wrong row.
            if let Some(CellValue::Text(id)) = fields.get("id") {
                if id.starts_with('#') {
                    return Err("row ids may not start with '#' (reserved for index addressing)".into());
                }
                if find_row(&l, id).is_some() {
                    return Err(format!("duplicate row id '{id}' in list '{list}'"));
                }
            }
            let map = l.push_container(LoroMap::new()).map_err(|e| e.to_string())?;
            for (k, v) in &fields {
                set_cell(&map, k, v).map_err(|e| e.to_string())?;
            }
            // Stable id stamped at birth; an app-supplied id wins.
            match map.get("id") {
                Some(ValueOrContainer::Value(LoroValue::String(s))) => s.to_string(),
                _ => {
                    let id = row_id(doc);
                    map.insert("id", id.as_str()).map_err(|e| e.to_string())?;
                    id
                }
            }
        }
        RowOp::Set { row, fields } => {
            let i = find_row(&l, &row).ok_or_else(|| format!("no row '{row}' in list '{list}'"))?;
            let Some(ValueOrContainer::Container(Container::Map(map))) = l.get(i) else {
                return Err(format!("row '{row}' is not a map"));
            };
            for (k, v) in &fields {
                set_cell(&map, k, v).map_err(|e| e.to_string())?;
            }
            row
        }
        RowOp::Remove { row } => {
            let i = find_row(&l, &row).ok_or_else(|| format!("no row '{row}' in list '{list}'"))?;
            l.delete(i, 1).map_err(|e| e.to_string())?;
            row
        }
    };
    doc.commit();
    Ok(id)
}

/// Write one [`CellValue`] into a row map (`Empty` deletes the field). `Decimal` is stored as its
/// canonical string, matching the schema-aware read in [`coerce`].
fn set_cell(map: &LoroMap, key: &str, v: &CellValue) -> Result<(), loro::LoroError> {
    match v {
        CellValue::Empty => map.delete(key),
        CellValue::Text(s) => map.insert(key, s.as_str()),
        CellValue::Number(n) => map.insert(key, *n),
        CellValue::Bool(b) => map.insert(key, *b),
        CellValue::Decimal(d) => map.insert(key, d.to_string().as_str()),
    }
}

/// Run the declarative query: filter, then a stable sort. Never writes back — display order is
/// derived, the list order stays canonical.
pub fn apply(spec: &TableSpec, mut rows: Vec<Row>) -> Vec<Row> {
    if !spec.filter.is_empty() {
        rows.retain(|r| {
            spec.filter
                .iter()
                .all(|(k, want)| r.cells.get(k).is_some_and(|have| cell_eq(have, want)))
        });
    }
    if let Some((key, desc)) = &spec.order {
        rows.sort_by(|a, b| {
            cmp_cells(
                a.cells.get(key).unwrap_or(&CellValue::Empty),
                b.cells.get(key).unwrap_or(&CellValue::Empty),
                *desc,
            )
        });
    }
    rows
}

fn cell_eq(a: &CellValue, b: &CellValue) -> bool {
    match (a, b) {
        // Integers and doubles compare as one number type.
        (CellValue::Number(x), CellValue::Number(y)) => x == y,
        (CellValue::Decimal(x), CellValue::Decimal(y)) => x == y,
        _ => a == b,
    }
}

/// Empty cells sort last regardless of direction; mixed types group by kind.
fn cmp_cells(a: &CellValue, b: &CellValue, desc: bool) -> Ordering {
    use CellValue::*;
    match (a, b) {
        (Empty, Empty) => Ordering::Equal,
        (Empty, _) => Ordering::Greater,
        (_, Empty) => Ordering::Less,
        _ => {
            let rank = |c: &CellValue| match c {
                Number(_) => 0,
                Decimal(_) => 1,
                Text(_) => 2,
                Bool(_) => 3,
                Empty => 4,
            };
            let ord = match (a, b) {
                (Number(x), Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
                (Decimal(x), Decimal(y)) => x.cmp(y),
                (Text(x), Text(y)) => x.cmp(y),
                (Bool(x), Bool(y)) => x.cmp(y),
                _ => rank(a).cmp(&rank(b)),
            };
            if desc {
                ord.reverse()
            } else {
                ord
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, cells: &[(&str, CellValue)]) -> Row {
        Row {
            id: id.into(),
            cells: cells.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
        }
    }

    fn spec(filter: Vec<(String, CellValue)>, order: Option<(String, bool)>) -> TableSpec {
        TableSpec { columns: vec![], filter, order, row_height: None, row_heights: HashMap::new() }
    }

    #[test]
    fn query_filters_then_sorts_with_empty_last() {
        let rows = vec![
            row("a", &[("st", CellValue::Text("open".into())), ("n", CellValue::Number(2.0))]),
            row("b", &[("st", CellValue::Text("done".into())), ("n", CellValue::Number(1.0))]),
            row("c", &[("st", CellValue::Text("open".into()))]),
            row("d", &[("st", CellValue::Text("open".into())), ("n", CellValue::Number(1.0))]),
        ];
        let s = spec(
            vec![("st".into(), CellValue::Text("open".into()))],
            Some(("n".into(), false)),
        );
        let out: Vec<_> = apply(&s, rows).into_iter().map(|r| r.id).collect();
        assert_eq!(out, ["d", "a", "c"]); // "c" has no n ⇒ sorts last even ascending
    }

    #[test]
    fn integer_filters_match_double_cells() {
        let rows = vec![row("a", &[("n", CellValue::Number(3.0))])];
        let s = spec(vec![("n".into(), CellValue::Number(3.0))], None);
        assert_eq!(apply(&s, rows).len(), 1);
    }

    #[test]
    fn whole_numbers_display_without_decimal() {
        assert_eq!(CellValue::Number(42.0).display(), "42");
        assert_eq!(CellValue::Number(1.5).display(), "1.5");
    }

    #[test]
    fn transitions_gate_select_options() {
        let col = Column {
            key: "st".into(),
            label: "st".into(),
            kind: ColKind::Select,
            width: None,
            locked: false,
            options: vec!["open".into(), "doing".into(), "done".into()],
            transitions: [("open".to_string(), vec!["doing".to_string()]), ("done".to_string(), vec![])]
                .into_iter()
                .collect(),
        };
        assert_eq!(legal_options(&col, "open"), ["doing"]);
        assert_eq!(legal_options(&col, "done"), [] as [&str; 0]); // terminal
        assert_eq!(legal_options(&col, ""), ["open", "doing", "done"]); // no entry ⇒ all
    }

    #[test]
    fn dates_parse_validate_and_normalize() {
        assert_eq!(parse_date("2026-6-1").map(format_date).as_deref(), Some("2026-06-01"));
        assert_eq!(parse_date("2024-02-29"), Some((2024, 2, 29))); // leap day
        assert_eq!(parse_date("2026-02-29"), None);
        assert_eq!(parse_date("2026-13-01"), None);
        assert_eq!(parse_date("soon"), None);
    }

    #[test]
    fn schema_round_trips_through_loro() {
        let doc = LoroDoc::new();
        let spec = TableSpec {
            columns: vec![
                Column { key: "id".into(), label: "Ref".into(), kind: ColKind::Text, width: Some(110.0), locked: true, options: vec![], transitions: HashMap::new() },
                Column { key: "total".into(), label: "Total".into(), kind: ColKind::Decimal, width: None, locked: false, options: vec![], transitions: HashMap::new() },
                Column { key: "status".into(), label: "Status".into(), kind: ColKind::Select, width: None, locked: false, options: vec!["open".into(), "done".into()], transitions: HashMap::new() },
            ],
            filter: vec![],
            order: None,
            row_height: None,
            row_heights: HashMap::new(),
        };
        write_schema(&doc, &spec).unwrap();
        doc.commit();

        let back = read_schema(&doc).expect("schema present");
        assert_eq!(back.columns.len(), 3);
        assert_eq!(back.columns[0].key, "id");
        assert_eq!(back.columns[0].width, Some(110.0));
        assert!(back.columns[0].locked);
        assert_eq!(back.columns[1].kind, ColKind::Decimal);
        assert_eq!(back.columns[2].options, ["open", "done"]);

        // Rewriting replaces, not appends.
        write_schema(&doc, &spec).unwrap();
        assert_eq!(read_schema(&doc).unwrap().columns.len(), 3);
        // A doc with no schema container reads as None.
        assert!(read_schema(&LoroDoc::new()).is_none());
    }

    #[test]
    fn decimal_cells_coerce_from_stored_string_and_sort_exactly() {
        let spec = TableSpec {
            columns: vec![Column { key: "amt".into(), label: "Amt".into(), kind: ColKind::Decimal, width: None, locked: false, options: vec![], transitions: HashMap::new() }],
            filter: vec![],
            order: Some(("amt".into(), false)),
            row_height: None,
            row_heights: HashMap::new(),
        };
        let mut rows = vec![
            row("a", &[("amt", CellValue::Text("19.99".into()))]),
            row("b", &[("amt", CellValue::Text("19.90".into()))]),
            row("c", &[("amt", CellValue::Number(100.0))]),
            row("d", &[("amt", CellValue::Text("oops".into()))]),
        ];
        coerce(&mut rows, &spec);
        assert_eq!(rows[0].cells["amt"], CellValue::Decimal(Decimal::from_str("19.99").unwrap()));
        assert_eq!(rows[2].cells["amt"], CellValue::Decimal(Decimal::from(100)));
        assert_eq!(rows[3].cells["amt"], CellValue::Empty); // unparseable → Empty
        // Exact base-10 ordering; "19.99" displays without float wobble.
        let out: Vec<_> = apply(&spec, rows).into_iter().map(|r| r.id).collect();
        assert_eq!(out, ["b", "a", "c", "d"]); // 19.90 < 19.99 < 100, empty last
        assert_eq!(CellValue::Decimal(Decimal::from_str("19.90").unwrap()).display(), "19.90");
    }

    #[test]
    fn rows_round_trip_with_stamped_ids_and_index_fallback() {
        let doc = LoroDoc::new();
        let l = doc.get_movable_list("t");
        let m = l.push_container(loro::LoroMap::new()).unwrap();
        m.insert("id", row_id(&doc).as_str()).unwrap();
        m.insert("title", "first").unwrap();
        let m2 = l.push_container(loro::LoroMap::new()).unwrap();
        m2.insert("title", "unstamped").unwrap();
        doc.commit();

        let rows = read_rows(&doc, "t");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].id, "#1");
        assert_eq!(find_row(&l, &rows[0].id), Some(0));
        assert_eq!(find_row(&l, "#1"), Some(1));
        assert_eq!(find_row(&l, "missing"), None);
    }

    #[test]
    fn row_ops_add_set_remove_by_stable_id() {
        let doc = LoroDoc::new();
        let cells = |pairs: &[(&str, CellValue)]| -> HashMap<String, CellValue> {
            pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
        };

        // Add stamps an id; a second Add with the same supplied id is rejected.
        let id = apply_row_op(&doc, "t", RowOp::Add { fields: cells(&[("title", CellValue::Text("a".into())), ("done", CellValue::Bool(false))]) }).unwrap();
        assert_eq!(read_rows(&doc, "t").len(), 1);
        assert!(apply_row_op(&doc, "t", RowOp::Add { fields: cells(&[("id", CellValue::Text(id.clone()))]) }).is_err());

        // Set edits by id; Empty deletes the field.
        apply_row_op(&doc, "t", RowOp::Set { row: id.clone(), fields: cells(&[("done", CellValue::Bool(true)), ("title", CellValue::Empty)]) }).unwrap();
        let l = doc.get_movable_list("t");
        let i = find_row(&l, &id).unwrap();
        let rows = read_rows(&doc, "t");
        assert_eq!(rows[i].cells.get("done"), Some(&CellValue::Bool(true)));
        assert!(!rows[i].cells.contains_key("title"));

        // Set/Remove on a missing id errors; Remove drops the row.
        assert!(apply_row_op(&doc, "t", RowOp::Set { row: "nope".into(), fields: cells(&[]) }).is_err());
        apply_row_op(&doc, "t", RowOp::Remove { row: id }).unwrap();
        assert_eq!(read_rows(&doc, "t").len(), 0);
    }
}
