//! Render a bar chart with long category labels to PNG, to eyeball the slanted x-labels + clipping
//! without the live shell: `cargo run -p app_engine --example chart_shot -- /tmp/chart.png`

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use app_engine::data::{DataAccess, Handle, NamedOp, QueryState};
use table_core::{CellValue, ColKind, Column, Row, TableSpec};

struct FakeData {
    labels: Vec<&'static str>,
    values: Vec<f64>,
    gen: Cell<u64>,
}

impl FakeData {
    fn rows(&self) -> Vec<Row> {
        self.labels
            .iter()
            .zip(&self.values)
            .enumerate()
            .map(|(i, (l, v))| Row {
                id: format!("r{i}"),
                cells: HashMap::from([
                    ("country".to_string(), CellValue::Text((*l).into())),
                    ("total".to_string(), CellValue::Number(*v)),
                ]),
            })
            .collect()
    }
}

impl DataAccess for FakeData {
    fn use_source(&self, _alias: &str) {}
    fn sql(&self, _q: &str, _p: &[(String, CellValue)]) -> QueryState {
        QueryState::Ready(1)
    }
    fn op(&self, _h: Handle, _op: &NamedOp) -> QueryState {
        QueryState::Ready(1)
    }
    fn spec(&self, _h: Handle) -> TableSpec {
        let col = |k: &str, kind| Column {
            key: k.into(),
            label: k.into(),
            kind,
            width: None,
            locked: true,
            options: vec![],
            transitions: HashMap::new(),
        };
        TableSpec {
            columns: vec![col("country", ColKind::Text), col("total", ColKind::Number)],
            filter: vec![],
            order: None,
            row_height: None,
            row_heights: HashMap::new(),
        }
    }
    fn len(&self, _h: Handle) -> usize {
        self.labels.len()
    }
    fn window(&self, _h: Handle, offset: usize, count: usize) -> Vec<Row> {
        let all = self.rows();
        let end = (offset + count).min(all.len());
        all[offset.min(all.len())..end].to_vec()
    }
    fn value(&self, _h: Handle, col: &str) -> CellValue {
        self.rows().first().and_then(|r| r.cells.get(col).cloned()).unwrap_or(CellValue::Empty)
    }
    fn mutate(&self, _alias: &str, _op: table_core::RowOp) -> Result<String, String> {
        Ok("x".into())
    }
    fn version(&self) -> u64 {
        self.gen.get()
    }
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "/tmp/chart.png".into());

    let data = Rc::new(FakeData {
        labels: vec![
            "Somalia",
            "Yemen",
            "South Sudan",
            "Congo Democratic Republic",
            "Syria",
            "Central African Republic",
            "Afghanistan",
            "Sudan",
        ],
        values: vec![111.9, 108.9, 108.5, 107.2, 107.1, 105.7, 106.6, 106.2],
        gen: Cell::new(1),
    });

    let fonts = app_engine::FontBytes {
        regular: include_bytes!("../../sthalam/assets/fonts/NotoSans-Regular.ttf"),
        bold: include_bytes!("../../sthalam/assets/fonts/NotoSans-SemiBold.ttf"),
        mono: include_bytes!("../../sthalam/assets/fonts/JetBrainsMono-Regular.ttf"),
        fallback: &[include_bytes!("../../sthalam/assets/fonts/NotoSansMalayalam-Regular.ttf")],
    };

    let mut app = app_engine::EngineApp::script(
        r##"return function()
            return ui.col{ style = { padding = 16, gap = 12, background = "#0d1016", width = "100%", min_height = "100%" },
              ui.text{ "Most fragile (top 8)", style = { font = 16, color = "#cdd2dc" } },
              ui.chart{ data = data.sql("select *"), type = "bar", x = "country", y = "total",
                style = { height = 300, width = "100%" } },
            }
        end"##,
    )
    .with_data_access(data);

    let png = app
        .screenshot(820.0, 420.0, 2.0, egui::Color32::from_rgb(0x0A, 0x0B, 0x10), fonts)
        .expect("render");
    std::fs::write(&out, png).expect("write png");
    println!("wrote {out}");
}
