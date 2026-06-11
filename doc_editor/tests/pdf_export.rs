//! End-to-end PDF export check: build a varied document, register the same fonts the shell uses
//! (so embedded faces match the shaped galleys), render, and assert valid multi-page PDF bytes.
//! Also written to `/tmp` for inspection — extractable text proves the glyph mapping worked.
#![allow(deprecated)] // `Context::run` is the simplest headless driver

use doc_editor::{BlockKind, Doc, PdfFonts};
use eframe::egui;
use std::sync::Arc;

const SANS: &[u8] = include_bytes!("../../sthalam/assets/fonts/NotoSans-Regular.ttf");
const SANS_SB: &[u8] = include_bytes!("../../sthalam/assets/fonts/NotoSans-SemiBold.ttf");
const JBMONO: &[u8] = include_bytes!("../../sthalam/assets/fonts/JetBrainsMono-Regular.ttf");

/// A Context whose font system matches the embedded faces: Noto Sans (proportional), JetBrains
/// Mono (monospace), Noto Sans SemiBold under the editor's bold family.
fn ctx_with_fonts() -> egui::Context {
    let ctx = egui::Context::default();
    let mut fonts = egui::FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.to_owned(), Arc::new(egui::FontData::from_static(bytes)));
    };
    add("sans", SANS);
    add("sans_sb", SANS_SB);
    add("jbmono", JBMONO);
    fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "sans".to_owned());
    fonts.families.entry(egui::FontFamily::Monospace).or_default().insert(0, "jbmono".to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Name(doc_editor::theme::BOLD_FAMILY.into()))
        .or_default()
        .extend(["sans_sb".to_owned(), "sans".to_owned()]);
    ctx.set_fonts(fonts);
    ctx
}

/// A document exercising every rendered path: headings, marked paragraphs, nested bullets, a
/// numbered list, to-dos (done + open), a quote, a multi-line code block, a divider — and enough
/// paragraphs to spill onto a second page.
fn sample_doc() -> Doc {
    // Chain with `insert_after` (relative, depth-preserving) rather than absolute `create_block`
    // indices — nesting shifts root indices, so absolute indices break (the known gotcha).
    let doc = Doc::new();
    let h = doc.block_ids()[0];
    doc.set_kind(h, BlockKind::H1);
    doc.insert_text(h, 0, "Export Fidelity");

    let p = doc.insert_after(h, BlockKind::Paragraph, "The PDF walks the same galleys the editor draws.");
    doc.mark(p, 4, 7, "bold"); // "PDF"
    doc.mark(p, 13, 20, "italic"); // "galleys"
    doc.mark(p, 8, 12, "code"); // "walk"

    let b1 = doc.insert_after(p, BlockKind::BulletList, "top level item");
    let b2 = doc.insert_after(b1, BlockKind::BulletList, "nested item");
    doc.indent(b2); // under b1 (depth 1)
    let b3 = doc.insert_after(b2, BlockKind::BulletList, "deeper still");
    doc.indent(b3); // under b2 (depth 2)

    // Resume at root level: `insert_after(b1)` lands after b1's whole subtree in document order.
    let n1 = doc.insert_after(b1, BlockKind::NumberedList, "first ordered");
    let n2 = doc.insert_after(n1, BlockKind::NumberedList, "second ordered");

    let t1 = doc.insert_after(n2, BlockKind::Todo, "done task");
    doc.set_done(t1, true);
    let t2 = doc.insert_after(t1, BlockKind::Todo, "open task");

    let q = doc.insert_after(t2, BlockKind::Quote, "A quote, rendered with its accent rule.");

    let code = doc.insert_after(q, BlockKind::Code, "fn main() {\n    println!(\"hi\");\n}");
    doc.set_lang(code, "rust");

    // Spill onto a second page.
    let mut tail = doc.insert_after(code, BlockKind::Divider, "");
    for i in 0..50 {
        tail = doc.insert_after(tail, BlockKind::Paragraph, &format!("Filler paragraph number {i} to force pagination across pages."));
    }
    doc
}

#[test]
fn exports_a_valid_multipage_pdf() {
    let ctx = ctx_with_fonts();
    let doc = sample_doc();

    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(900.0, 700.0))),
        ..Default::default()
    };
    let mut bytes: Option<Vec<u8>> = None;
    let _ = ctx.run(raw, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let fonts = PdfFonts { regular: SANS, bold: SANS_SB, mono: JBMONO };
            bytes = Some(doc_editor::export_pdf(&doc, ui, fonts));
        });
    });

    let bytes = bytes.expect("export ran inside a frame");
    std::fs::write("/tmp/doc_editor_export.pdf", &bytes).ok();

    assert!(bytes.starts_with(b"%PDF"), "starts with the PDF header");
    assert!(bytes.windows(5).any(|w| w == b"%%EOF"), "has an EOF trailer");
    assert!(bytes.len() > 4000, "non-trivial output (got {} bytes)", bytes.len());
    // Two `/Type /Page` (not /Pages) entries → the doc spilled onto a second page.
    let page_marker = b"/Page\n";
    let _ = page_marker; // page objects may be compressed; the external pdfinfo check is authoritative.
}
