//! Materialize a profiled frame into a `.table` Loro layer: the agent's column plan (source →
//! key/label/type) drives a typed coercion of every cell, and the result is written as a stored
//! schema + a `rows` list — exactly what the native `.table` view reads back. Value-cleaning
//! beyond type-coercion (splitting columns, ordinal→int) is the agent's job upstream via SQL; the
//! writer owns only re-typing + the one Excel-serial-date special case.

use anyhow::Result;
use loro::{ExportMode, LoroDoc, LoroMap};
use polars::prelude::Column as PlColumn;
use polars::prelude::*;
use rust_decimal::Decimal;
use table_core::{format_date, parse_date, row_id, write_schema, ColKind, Column, TableSpec};

/// One column of the agent's import plan: which frame column feeds it, its stored key/label, and
/// the target type the writer coerces to.
pub struct ColumnPlan {
    pub source: String,
    pub key: String,
    pub label: String,
    pub kind: ColKind,
}

/// Build a `.table` layer snapshot from `frame` under `plan`. Returns the Loro snapshot (stored
/// schema + typed `rows`) and the row count.
pub fn build_layer(frame: &DataFrame, plan: &[ColumnPlan]) -> Result<(Vec<u8>, usize)> {
    let spec = TableSpec {
        columns: plan
            .iter()
            .map(|p| Column {
                key: p.key.clone(),
                label: p.label.clone(),
                kind: p.kind,
                width: None,
                locked: false,
                options: Vec::new(),
                transitions: Default::default(),
            })
            .collect(),
        filter: Vec::new(),
        order: None,
        row_height: None,
        row_heights: Default::default(),
    };

    let doc = LoroDoc::new();
    write_schema(&doc, &spec)?;

    // Resolve each plan column to its frame column once.
    let cols: Vec<&PlColumn> =
        plan.iter().map(|p| frame.column(&p.source)).collect::<PolarsResult<_>>()?;

    let rows = doc.get_movable_list("rows");
    let n = frame.height();
    for i in 0..n {
        let m = rows.push_container(LoroMap::new())?;
        m.insert("id", row_id(&doc).as_str())?;
        for (p, col) in plan.iter().zip(&cols) {
            let av = col.get(i).unwrap_or(AnyValue::Null);
            insert_cell(&m, &p.key, &av, p.kind)?;
        }
    }
    doc.commit();
    Ok((doc.export(ExportMode::Snapshot)?, n))
}

/// Coerce one frame cell to its target type and store it in the row map (a `Null` source leaves the
/// cell empty). `Decimal`/`Date` get their canonical string forms; numbers stay numeric.
fn insert_cell(m: &LoroMap, key: &str, av: &AnyValue, kind: ColKind) -> Result<()> {
    match kind {
        ColKind::Text | ColKind::Select => {
            if let Some(s) = av_str(av) {
                m.insert(key, s.as_str())?;
            }
        }
        ColKind::Number => {
            if let Some(f) = av_f64(av) {
                if f.fract() == 0.0 && f.abs() < 9e15 {
                    m.insert(key, f as i64)?;
                } else {
                    m.insert(key, f)?;
                }
            }
        }
        ColKind::Decimal => {
            if let Some(s) = av_decimal(av) {
                m.insert(key, s.as_str())?;
            }
        }
        ColKind::Date => {
            if let Some(s) = av_date(av) {
                m.insert(key, s.as_str())?;
            }
        }
        ColKind::Check => {
            if let Some(b) = av_bool(av) {
                m.insert(key, b)?;
            }
        }
    }
    Ok(())
}

fn av_str(av: &AnyValue) -> Option<String> {
    match av {
        AnyValue::Null => None,
        AnyValue::String(s) => Some(s.to_string()),
        AnyValue::StringOwned(s) => Some(s.as_str().to_string()),
        AnyValue::Boolean(b) => Some(b.to_string()),
        _ => av_f64(av).map(fmt_num),
    }
}

fn av_f64(av: &AnyValue) -> Option<f64> {
    match av {
        AnyValue::Null => None,
        AnyValue::String(s) => s.trim().parse().ok(),
        AnyValue::StringOwned(s) => s.as_str().trim().parse().ok(),
        _ => av.try_extract::<f64>().ok(),
    }
}

fn av_bool(av: &AnyValue) -> Option<bool> {
    match av {
        AnyValue::Null => None,
        AnyValue::Boolean(b) => Some(*b),
        AnyValue::String(s) => parse_bool(s),
        AnyValue::StringOwned(s) => parse_bool(s.as_str()),
        _ => av_f64(av).map(|f| f != 0.0),
    }
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "y" | "1" => Some(true),
        "false" | "no" | "n" | "0" => Some(false),
        _ => None,
    }
}

/// An xlsx-stored float for money → exact decimal via shortest-round-trip; a string → parsed.
fn av_decimal(av: &AnyValue) -> Option<String> {
    match av {
        AnyValue::Null => None,
        AnyValue::String(s) => Decimal::from_str_exact(s.trim()).ok().map(|d| d.to_string()),
        AnyValue::StringOwned(s) => {
            Decimal::from_str_exact(s.as_str().trim()).ok().map(|d| d.to_string())
        }
        _ => av_f64(av).and_then(Decimal::from_f64_retain).map(|d| d.normalize().to_string()),
    }
}

/// A numeric source is an Excel serial date; a string source is parsed/normalized as `YYYY-MM-DD`.
fn av_date(av: &AnyValue) -> Option<String> {
    match av {
        AnyValue::Null => None,
        AnyValue::String(s) => parse_date(s).map(format_date),
        AnyValue::StringOwned(s) => parse_date(s.as_str()).map(format_date),
        _ => av_f64(av).map(|f| f as i64).and_then(excel_serial_date).map(format_date),
    }
}

/// Excel 1900 date system → `(y, m, d)`: serial days since 1899-12-30, shifted to days since the
/// Unix epoch, then civil-from-days.
fn excel_serial_date(serial: i64) -> Option<(i32, u32, u32)> {
    Some(civil_from_days(serial.checked_sub(25569)?))
}

/// Howard Hinnant's days-since-1970-01-01 → proleptic-Gregorian `(y, m, d)`.
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as i64;
    (y as i32 + if m <= 2 { 1 } else { 0 }, m as u32, d)
}

fn fmt_num(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 9e15 {
        format!("{}", f as i64)
    } else {
        format!("{f}")
    }
}
