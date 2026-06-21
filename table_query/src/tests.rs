//! Executor tests: Loro layer → frame, SQL (computed + GROUP BY, parameterized filter, JOIN
//! across registered sources), pivot, and the result projection back to `table_core` rows.

use super::*;
use loro::LoroMap;
use table_core::{row_id, write_schema, Column};

fn build(cols: &[(&str, ColKind)], rows: &[&[(&str, CellValue)]]) -> (LoroDoc, TableSpec) {
    let spec = TableSpec {
        columns: cols
            .iter()
            .map(|(k, kind)| Column {
                key: k.to_string(),
                label: k.to_string(),
                kind: *kind,
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
    write_schema(&doc, &spec).unwrap();
    let list = doc.get_movable_list("rows");
    for r in rows {
        let m = list.push_container(LoroMap::new()).unwrap();
        m.insert("id", row_id(&doc).as_str()).unwrap();
        for (k, v) in *r {
            match v {
                CellValue::Text(s) => m.insert(*k, s.as_str()).unwrap(),
                CellValue::Number(n) => m.insert(*k, *n).unwrap(),
                CellValue::Bool(b) => m.insert(*k, *b).unwrap(),
                CellValue::Decimal(d) => m.insert(*k, d.to_string().as_str()).unwrap(),
                CellValue::Empty => {}
            }
        }
    }
    doc.commit();
    (doc, spec)
}

fn t(s: &str) -> CellValue {
    CellValue::Text(s.into())
}
fn n(x: f64) -> CellValue {
    CellValue::Number(x)
}

fn orders() -> (LoroDoc, TableSpec) {
    build(
        &[("segment", ColKind::Text), ("sales", ColKind::Number), ("status", ColKind::Text)],
        &[
            &[("segment", t("smb")), ("sales", n(100.0)), ("status", t("paid"))],
            &[("segment", t("smb")), ("sales", n(50.0)), ("status", t("open"))],
            &[("segment", t("ent")), ("sales", n(400.0)), ("status", t("paid"))],
            &[("segment", t("ent")), ("sales", n(600.0)), ("status", t("paid"))],
        ],
    )
}

#[test]
fn frame_shapes_typed_columns() {
    let (doc, spec) = orders();
    let r = frame(&doc, "rows", &spec).unwrap();
    assert_eq!(r.len(), 4);
    let cols = r.schema();
    assert_eq!(cols.columns.iter().find(|c| c.key == "sales").unwrap().kind, ColKind::Number);
    assert_eq!(cols.columns.iter().find(|c| c.key == "segment").unwrap().kind, ColKind::Text);
}

#[test]
fn sql_computed_and_group_by() {
    let (doc, spec) = orders();
    let r = frame(&doc, "rows", &spec).unwrap();
    let out = sql(
        vec![("orders".into(), r)],
        "SELECT segment, SUM(sales) AS total, COUNT(*) AS n \
         FROM orders WHERE status = :st GROUP BY segment ORDER BY total DESC",
        &[("st".into(), t("paid"))],
    )
    .unwrap();
    let rows = out.rows(0, 10);
    assert_eq!(rows.len(), 2);
    // ent paid = 400+600 = 1000 (first, desc); smb paid = 100.
    assert_eq!(rows[0].cells["segment"], t("ent"));
    assert_eq!(rows[0].cells["total"], n(1000.0));
    assert_eq!(rows[1].cells["total"], n(100.0));
    assert_eq!(out.value("total"), n(1000.0));
}

#[test]
fn sql_join_two_sources() {
    let (od, os) = build(
        &[("cust", ColKind::Text), ("sales", ColKind::Number)],
        &[&[("cust", t("c1")), ("sales", n(10.0))], &[("cust", t("c2")), ("sales", n(20.0))]],
    );
    let (cd, cs) = build(
        &[("cid", ColKind::Text), ("region", ColKind::Text)],
        &[&[("cid", t("c1")), ("region", t("west"))], &[("cid", t("c2")), ("region", t("west"))]],
    );
    let of = frame(&od, "rows", &os).unwrap();
    let cf = frame(&cd, "rows", &cs).unwrap();
    let out = sql(
        vec![("orders".into(), of), ("customers".into(), cf)],
        "SELECT cu.region, SUM(o.sales) AS total \
         FROM orders o JOIN customers cu ON o.cust = cu.cid GROUP BY cu.region",
        &[],
    )
    .unwrap();
    let rows = out.rows(0, 10);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].cells["region"], t("west"));
    assert_eq!(rows[0].cells["total"], n(30.0));
}

#[test]
fn pivot_cross_tab() {
    let (doc, spec) = orders();
    let r = frame(&doc, "rows", &spec).unwrap();
    let cross = pivot(&r, "segment", "status", "sales").unwrap();
    // index column `status` (paid, open) + one column per segment (ent, smb).
    let names = cross.column_names();
    assert!(names.contains(&"status".to_string()));
    assert!(names.contains(&"ent".to_string()));
    assert!(names.contains(&"smb".to_string()));
    // ent total across statuses = 1000 (all paid → paid row), smb = 150.
    assert!(cross.schema().columns.iter().any(|c| c.key == "ent"));
}
