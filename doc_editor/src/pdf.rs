//! PDF export — the second consumer of the [`Scene`](crate::scene). It walks the same display
//! list the editor drew rather than re-typesetting (see [`pdf_paint`] for the glyph-exact
//! emission). The page is a light/print theme (white page, dark ink): the dark-theme colours are
//! mapped by hue-preserving luminance inversion, composited over white. Fonts are supplied by the
//! caller (the consumer owns fonts); layout uses the caller's egui `Context`, so export must run
//! on the UI thread.

use egui::{Color32, Ui};
use pdf_paint::printpdf::{Color, Mm, Op, PaintMode, PdfDocument, PdfPage, PdfSaveOptions};
use pdf_paint::{rect_op, rgb, Emit, FontBytes, FontSet, PageMap};

use crate::model::Doc;
use crate::scene::Prim;
use crate::theme;

// ── Page + mapping constants (A4 portrait) ──────────────────────────────────────────
const PT_PER_MM: f32 = 72.0 / 25.4;
const PAGE_W_MM: f32 = 210.0;
const PAGE_H_MM: f32 = 297.0;
const PAGE_W: f32 = PAGE_W_MM * PT_PER_MM; // ≈ 595.28 pt
const PAGE_H: f32 = PAGE_H_MM * PT_PER_MM; // ≈ 841.89 pt
const MARGIN_X: f32 = 39.0; // pt — left edge of the depth-0 content column
const MARGIN_TOP: f32 = 54.0;
const MARGIN_BOTTOM: f32 = 54.0;
/// Editor logical-px → PDF pt. 16px body → ~10.9pt; 760px measure → ~517pt (fits A4 margins).
const SCALE: f32 = 0.68;
/// Wide enough that `theme::MAX_CONTENT` (not the window) is the binding wrap width.
const EXPORT_WIDTH: f32 = 2000.0;
/// Depth-0 content-column left in editor space; mapped to the page's left margin.
const CONTENT_LEFT0: f32 = theme::OUTER_LEFT + theme::GUTTER + theme::SPINE + theme::CONTENT_PAD;

/// The TTF bytes to embed — the same faces the caller registered with egui (body, bold/heading,
/// mono). Borrowed so the caller keeps ownership of its bundled assets.
pub struct PdfFonts<'a> {
    pub regular: &'a [u8],
    pub bold: &'a [u8],
    pub mono: &'a [u8],
}

/// Render `doc` to PDF bytes, laid out exactly as the editor renders it. `ui` provides the egui
/// font system for shaping (nothing is painted to it).
pub fn export_pdf(doc: &Doc, ui: &Ui, fonts: PdfFonts) -> Vec<u8> {
    let scene = crate::editor::scene_for_export(doc, ui, EXPORT_WIDTH);

    let mut pdf = PdfDocument::new("Document");
    let fonts = FontSet::load(
        &mut pdf,
        FontBytes { regular: fonts.regular, bold: fonts.bold, mono: fonts.mono, fallback: &[] },
    );

    // Paginate by block: a block never splits across a page; one that would overflow starts a
    // fresh one. Each block remembers its page index and that page's top (column y).
    let page_h_px = (PAGE_H - MARGIN_TOP - MARGIN_BOTTOM) / SCALE;
    let mut page_of = Vec::with_capacity(scene.blocks.len());
    let mut page = 0usize;
    let mut start_y = scene.blocks.first().map_or(0.0, |b| b.row_top);
    for b in &scene.blocks {
        if b.row_top > start_y && (b.row_bottom - start_y) > page_h_px {
            page += 1;
            start_y = b.row_top;
        }
        page_of.push((page, start_y));
    }
    let n_pages = page + 1;

    let mut pages = Vec::with_capacity(n_pages);
    for pi in 0..n_pages {
        let mut ops = Vec::new();
        // The white page surface (light/print theme — dark ink on white).
        ops.push(Op::SetFillColor { col: rgb(1.0, 1.0, 1.0) });
        ops.push(rect_op(0.0, 0.0, PAGE_W, PAGE_H, PaintMode::Fill));
        for (b, &(bp, sy)) in scene.blocks.iter().zip(&page_of) {
            if bp != pi {
                continue;
            }
            let map = PageMap {
                page_h: PAGE_H,
                offset_x: MARGIN_X,
                offset_y: MARGIN_TOP,
                src_left: CONTENT_LEFT0,
                src_top: sy,
                scale: SCALE,
            };
            let mut emit = Emit { ops: &mut ops, map: &map, fonts: &fonts, ink: &ink };
            for prim in &b.prims {
                emit_prim(&mut emit, prim);
            }
        }
        pages.push(PdfPage::new(Mm(PAGE_W_MM), Mm(PAGE_H_MM), ops));
    }

    let mut warn = Vec::new();
    pdf.with_pages(pages).save(&PdfSaveOptions::default(), &mut warn)
}

/// Emit one scene primitive into the page's op stream.
fn emit_prim(e: &mut Emit, prim: &Prim) {
    match prim {
        Prim::Rect { rect, fill, stroke } => e.rect(*rect, *fill, *stroke),
        Prim::Line { from, to, stroke } => e.line(*from, *to, *stroke),
        Prim::Circle { center, radius, fill, stroke } => e.circle(*center, *radius, *fill, *stroke),
        Prim::Path { points, stroke } => e.path(points, *stroke),
        Prim::Galley { pos, galley } => e.galley(*pos, galley, theme::FG_1),
        Prim::Marker { pos, text, font, color, align } => {
            e.marker(*pos, text, font, *color, *align)
        }
    }
}

/// Map an editor dark-theme colour to its light/print equivalent: hue-preserving luminance
/// inversion, composited over white (PDF fills are opaque, so low-alpha hairlines flatten here).
fn ink(c: Color32) -> Color {
    let a = c.a() as f32 / 255.0;
    if a <= 0.0 {
        return rgb(1.0, 1.0, 1.0);
    }
    // Un-premultiply to straight channels, invert lightness (keeping hue + saturation).
    let sr = (c.r() as f32 / 255.0 / a).min(1.0);
    let sg = (c.g() as f32 / 255.0 / a).min(1.0);
    let sb = (c.b() as f32 / 255.0 / a).min(1.0);
    let (h, s, l) = rgb_to_hsl(sr, sg, sb);
    let (ir, ig, ib) = hsl_to_rgb(h, s, 1.0 - l);
    let over = |ch: f32| ch * a + (1.0 - a); // composite over white at the original alpha
    rgb(over(ir), over(ig), over(ib))
}

fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-6 {
        return (0.0, 0.0, l); // grey: hue/sat irrelevant
    }
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s < 1e-6 {
        return (l, l, l);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let comp = |mut t: f32| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    (comp(h + 1.0 / 3.0), comp(h), comp(h - 1.0 / 3.0))
}
