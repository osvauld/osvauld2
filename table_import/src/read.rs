//! calamine (.xlsx) → Polars projection. We do NOT impose a schema here: every column is built
//! from its raw cells with `strict = false`, so Polars picks a per-column supertype (mixed int/
//! float → Float64, anything truly mixed → String). Type design is a later, deliberate step the
//! agent makes after profiling — loading must not pre-decide it.

use anyhow::{Context, Result};
use calamine::{open_workbook_auto, Data, Range, Reader};
use polars::prelude::*;

/// One sheet read into a frame, keyed by its workbook name.
pub struct NamedFrame {
    pub name: String,
    pub frame: DataFrame,
}

/// Read every worksheet of a workbook into a Polars frame (first row = header).
pub fn open_workbook(path: &str) -> Result<Vec<NamedFrame>> {
    let mut wb = open_workbook_auto(path).with_context(|| format!("open {path}"))?;
    let names = wb.sheet_names().to_owned();
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let range = wb
            .worksheet_range(&name)
            .with_context(|| format!("read sheet {name}"))?;
        let frame = range_to_frame(&range).with_context(|| format!("frame sheet {name}"))?;
        out.push(NamedFrame { name, frame });
    }
    Ok(out)
}

/// First row → column names (blank header → `col{i}`); remaining rows → columns of [`AnyValue`]s,
/// each inferred to a supertype.
fn range_to_frame(range: &Range<Data>) -> Result<DataFrame> {
    let width = range.width();
    let mut rows = range.rows();
    let Some(header) = rows.next() else {
        return Ok(DataFrame::empty());
    };
    let names: Vec<String> = (0..width)
        .map(|i| match header.get(i) {
            Some(Data::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
            Some(c) if !matches!(c, Data::Empty) => cell_to_string(c),
            _ => format!("col{i}"),
        })
        .collect();

    let mut cols: Vec<Vec<AnyValue<'static>>> = vec![Vec::new(); width];
    for row in rows {
        for (c, slot) in cols.iter_mut().enumerate() {
            slot.push(data_to_any(row.get(c).unwrap_or(&Data::Empty)));
        }
    }

    let columns: Vec<Column> = names
        .iter()
        .zip(cols)
        .map(|(name, vals)| {
            Series::from_any_values(name.as_str().into(), &vals, false)
                .map(Column::from)
                .with_context(|| format!("column {name}"))
        })
        .collect::<Result<_>>()?;
    Ok(DataFrame::new_infer_height(columns)?)
}

/// A calamine cell → a Polars value. Dates become their serial number (Float64) for now; precise
/// date typing is part of the agent's later schema design, not load.
fn data_to_any(d: &Data) -> AnyValue<'static> {
    match d {
        Data::Int(i) => AnyValue::Int64(*i),
        Data::Float(f) => AnyValue::Float64(*f),
        Data::Bool(b) => AnyValue::Boolean(*b),
        Data::String(s) => AnyValue::StringOwned(s.as_str().into()),
        Data::DateTimeIso(s) | Data::DurationIso(s) => AnyValue::StringOwned(s.as_str().into()),
        Data::DateTime(dt) => AnyValue::Float64(dt.as_f64()),
        Data::Empty | Data::Error(_) => AnyValue::Null,
    }
}

fn cell_to_string(d: &Data) -> String {
    match d {
        Data::String(s) => s.clone(),
        other => other.to_string(),
    }
}
