//! Render a native `.table` view (no Lua) to PNG: build a doc with a stored schema + rows, then
//! screenshot the grid `EngineApp::table` renders from it.
//! `cargo run -p app_engine --example table_shot -- /tmp/table.png`

use loro::LoroDoc;
use table_core::{row_id, write_schema, ColKind, Column, TableSpec};

fn col(key: &str, label: &str, kind: ColKind, width: Option<f32>) -> Column {
    Column { key: key.into(), label: label.into(), kind, width, locked: false, options: vec![], transitions: Default::default() }
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "/tmp/table.png".into());

    // A .table layer: stored schema + a `rows` list, exactly what import_to_layer will write.
    let doc = LoroDoc::new();
    let spec = TableSpec {
        columns: vec![
            col("ref", "Ref", ColKind::Text, Some(70.0)),
            col("segment", "Segment", ColKind::Text, None),
            col("product", "Product", ColKind::Text, None),
            col("units", "Units", ColKind::Number, Some(90.0)),
            col("sales", "Sales", ColKind::Decimal, Some(120.0)),
            col("profit", "Profit", ColKind::Decimal, Some(120.0)),
        ],
        filter: vec![],
        order: Some(("sales".into(), true)),
        row_height: None,
        row_heights: Default::default(),
    };
    write_schema(&doc, &spec).unwrap();
    let rows = doc.get_movable_list("rows");
    let data = [
        ("R-001", "Government", "Carretera", 1618, "32370.00", "16425.00"),
        ("R-002", "Midmarket", "Montana", 2470, "23987.50", "-614.63"),
        ("R-003", "Enterprise", "Paseo", 921, "96390.00", "11388.25"),
        ("R-004", "Channel Partners", "Velo", 1513, "1979.06", "1316.80"),
    ];
    for (r, seg, prod, units, sales, profit) in data {
        let m = rows.push_container(loro::LoroMap::new()).unwrap();
        m.insert("id", row_id(&doc).as_str()).unwrap();
        m.insert("ref", r).unwrap();
        m.insert("segment", seg).unwrap();
        m.insert("product", prod).unwrap();
        m.insert("units", units as i64).unwrap();
        m.insert("sales", sales).unwrap(); // Decimal stored as string
        m.insert("profit", profit).unwrap();
    }
    doc.commit();
    let snapshot = doc.export(loro::ExportMode::Snapshot).unwrap();

    let fonts = app_engine::FontBytes {
        regular: include_bytes!("../../sthalam/assets/fonts/NotoSans-Regular.ttf"),
        bold: include_bytes!("../../sthalam/assets/fonts/NotoSans-SemiBold.ttf"),
        mono: include_bytes!("../../sthalam/assets/fonts/JetBrainsMono-Regular.ttf"),
        fallback: &[include_bytes!("../../sthalam/assets/fonts/NotoSansMalayalam-Regular.ttf")],
    };
    let mut app = app_engine::EngineApp::table(Some(&snapshot), egui::Color32::from_gray(220), 14.0);
    let png = app
        .screenshot(760.0, 260.0, 2.0, egui::Color32::from_rgb(0x0A, 0x0B, 0x10), fonts)
        .expect("render");
    std::fs::write(&out, png).expect("write png");
    println!("wrote {out}");
}
