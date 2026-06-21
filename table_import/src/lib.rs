//! Excel→osvauld migration staging. The agent drives this through MCP: `open` loads an .xlsx into
//! Polars frames under a handle; `head`/`sql` let it profile the data; later phases design a
//! `table_core` schema and materialize typed rows into a Loro `.table` layer. The staging frames
//! are ephemeral host-side state (keyed by handle), never the canonical store. See
//! [[migration-dataflow-model]].

mod read;
mod write;

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use polars::prelude::*;
use polars::sql::SQLContext;

use read::NamedFrame;
pub use write::ColumnPlan;

/// A loaded workbook held in staging, addressed by handle.
struct Book {
    sheets: Vec<NamedFrame>,
}

impl Book {
    fn sheet(&self, name: Option<&str>) -> Result<&NamedFrame> {
        match name {
            Some(n) => self.sheets.iter().find(|s| s.name == n).ok_or_else(|| anyhow!("no sheet {n}")),
            None => self.sheets.first().ok_or_else(|| anyhow!("workbook has no sheets")),
        }
    }
}

/// One sheet's shape, returned from [`Staging::open`] so the agent knows what it loaded.
pub struct SheetInfo {
    pub name: String,
    pub rows: usize,
    pub cols: usize,
    /// Column names in order, with the dtype Polars inferred (the agent's starting point, not the
    /// final schema).
    pub columns: Vec<(String, String)>,
}

/// What `open` reports back: the staging handle plus a per-sheet summary.
pub struct Opened {
    pub handle: String,
    pub sheets: Vec<SheetInfo>,
}

/// The staging registry: every open workbook keyed by an opaque handle. Host-owned (the live Vault
/// process), so the same instance that profiles can later write to Loro.
#[derive(Default)]
pub struct Staging {
    books: HashMap<String, Book>,
    seq: u64,
}

impl Staging {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load an .xlsx into staging and report its sheets. The handle addresses it for later calls.
    pub fn open(&mut self, path: &str) -> Result<Opened> {
        let sheets = read::open_workbook(path)?;
        let summary = sheets.iter().map(sheet_info).collect();
        let handle = format!("imp-{:x}", self.seq);
        self.seq += 1;
        self.books.insert(handle.clone(), Book { sheets });
        Ok(Opened { handle, sheets: summary })
    }

    /// Drop a staged workbook (frames are ephemeral — released once profiling is done).
    pub fn close(&mut self, handle: &str) -> bool {
        self.books.remove(handle).is_some()
    }

    /// The first `n` rows of a sheet, rendered as a text table for the agent to eyeball.
    pub fn head(&self, handle: &str, sheet: Option<&str>, n: usize) -> Result<String> {
        let frame = &self.book(handle)?.sheet(sheet)?.frame;
        Ok(format!("{}", frame.head(Some(n))))
    }

    /// Run SQL over a sheet (registered as table `data`) via Polars' SQL engine — the profiling
    /// escape hatch (DISTINCT, GROUP BY, COUNT, aggregates) the agent uses to understand the data.
    pub fn sql(&self, handle: &str, sheet: Option<&str>, query: &str) -> Result<String> {
        let frame = self.book(handle)?.sheet(sheet)?.frame.clone();
        let mut ctx = SQLContext::new();
        ctx.register("data", frame.lazy());
        let out = ctx.execute(query)?.collect()?;
        Ok(format!("{out}"))
    }

    /// Materialize a staged sheet into a `.table` Loro layer under the agent's column `plan`
    /// (source → key/label/type, typed-coerced). Returns the Loro snapshot to persist as the new
    /// item's state, plus the row count.
    pub fn to_layer(
        &self,
        handle: &str,
        sheet: Option<&str>,
        plan: &[ColumnPlan],
    ) -> Result<(Vec<u8>, usize)> {
        let frame = &self.book(handle)?.sheet(sheet)?.frame;
        write::build_layer(frame, plan)
    }

    fn book(&self, handle: &str) -> Result<&Book> {
        self.books.get(handle).ok_or_else(|| anyhow!("no staged workbook {handle}"))
    }
}

fn sheet_info(s: &NamedFrame) -> SheetInfo {
    let columns = s
        .frame
        .columns()
        .iter()
        .map(|c| (c.name().as_str().to_string(), c.dtype().to_string()))
        .collect();
    SheetInfo { name: s.name.clone(), rows: s.frame.height(), cols: s.frame.width(), columns }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        format!("{}/../{name}", env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn materializes_a_typed_table_layer() {
        use table_core::{read_schema, typed_rows, CellValue, ColKind};

        let mut st = Staging::new();
        let opened = st.open(&fixture("Financial Sample.xlsx")).unwrap();
        let plan = vec![
            ColumnPlan { source: "Segment".into(), key: "segment".into(), label: "Segment".into(), kind: ColKind::Text },
            ColumnPlan { source: "Sales".into(), key: "sales".into(), label: "Sales".into(), kind: ColKind::Decimal },
            ColumnPlan { source: "Date".into(), key: "date".into(), label: "Date".into(), kind: ColKind::Date },
        ];
        let (snapshot, n) = st.to_layer(&opened.handle, None, &plan).unwrap();
        assert!(n >= 700);

        // Read the snapshot back exactly as the .table view would.
        let doc = loro::LoroDoc::new();
        doc.import(&snapshot).unwrap();
        let spec = read_schema(&doc).expect("stored schema");
        assert_eq!(spec.columns[1].kind, ColKind::Decimal);
        let rows = typed_rows(&doc, "rows", &spec);
        assert_eq!(rows.len(), n);
        // Money came back as an exact Decimal, the Excel serial as a real date.
        assert!(matches!(rows[0].cells.get("sales"), Some(CellValue::Decimal(_))));
        match rows[0].cells.get("date") {
            Some(CellValue::Text(d)) => assert!(d.starts_with("201"), "date = {d}"),
            other => panic!("date not a YYYY-MM-DD string: {other:?}"),
        }
    }

    #[test]
    fn opens_financial_sample_and_profiles() {
        let mut st = Staging::new();
        let opened = st.open(&fixture("Financial Sample.xlsx")).unwrap();
        let sheet = &opened.sheets[0];
        assert!(sheet.rows >= 700, "got {} rows", sheet.rows);
        assert!(sheet.cols >= 10, "got {} cols", sheet.cols);
        // Header row became column names, not data.
        let names: Vec<_> = sheet.columns.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"Segment"), "columns: {names:?}");

        // The profiling path works end to end.
        let grouped = st
            .sql(&opened.handle, None, "SELECT Segment, COUNT(*) AS n FROM data GROUP BY Segment")
            .unwrap();
        assert!(grouped.contains("Segment"), "{grouped}");
    }
}
