//! PDF export — the second consumer of the [`Scene`](crate::scene). It walks the same display
//! list the editor drew rather than re-typesetting: text is emitted glyph-by-glyph at its exact
//! galley position (one absolute `Tm` per glyph) using the same embedded TTFs, so wrapping and
//! glyph placement match the screen. egui's fake-italic becomes a matrix shear; decorations are
//! re-emitted as vector primitives. The page is a light/print theme (white page, dark ink): the
//! dark-theme colours are mapped by hue-preserving luminance inversion, composited over white.
//! Fonts are supplied by the caller (the consumer owns fonts); layout uses the caller's egui
//! `Context`, so export must run on the UI thread.

use egui::{Align2, Color32, FontFamily, FontId, Galley, Pos2, TextFormat, Ui};
use printpdf::{
    Color, FontId as PdfFontId, Line, LinePoint, Mm, Op, PaintMode, ParsedFont, PdfDocument,
    PdfFontHandle, PdfPage, PdfSaveOptions, Point, Polygon, PolygonRing, Pt, Rgb, TextItem,
    TextMatrix, WindingOrder,
};

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
/// Shear applied to italic glyphs, matching egui's synthesised slant closely enough to read.
const ITALIC_SHEAR: f32 = 0.21;
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
    let mut fwarn = Vec::new(); // font-parse warnings (distinct type from save warnings)
    let reg = pdf.add_font(&ParsedFont::from_bytes(fonts.regular, 0, &mut fwarn).expect("regular font"));
    let bold = pdf.add_font(&ParsedFont::from_bytes(fonts.bold, 0, &mut fwarn).expect("bold font"));
    let mono = pdf.add_font(&ParsedFont::from_bytes(fonts.mono, 0, &mut fwarn).expect("mono font"));
    let fonts = FontSet { reg, bold, mono };

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
            let tr = Tr { start_y: sy };
            for prim in &b.prims {
                emit(&mut ops, &tr, &fonts, prim);
            }
        }
        pages.push(PdfPage::new(Mm(PAGE_W_MM), Mm(PAGE_H_MM), ops));
    }

    let mut warn = Vec::new();
    pdf.with_pages(pages).save(&PdfSaveOptions::default(), &mut warn)
}

// ── Coordinate transform: editor column space (px, y-down) → PDF page (pt, y-up) ─────
struct Tr {
    /// Column y at the top of this block's page (so the page content starts at the top margin).
    start_y: f32,
}
impl Tr {
    fn x(&self, cx: f32) -> f32 {
        MARGIN_X + (cx - CONTENT_LEFT0) * SCALE
    }
    fn y(&self, cy: f32) -> f32 {
        PAGE_H - MARGIN_TOP - (cy - self.start_y) * SCALE
    }
    fn s(&self, len: f32) -> f32 {
        len * SCALE
    }
}

/// The three embedded faces, and the rule mapping an egui family to one of them.
struct FontSet {
    reg: PdfFontId,
    bold: PdfFontId,
    mono: PdfFontId,
}
impl FontSet {
    /// Returns the embedded font plus a small key (for change-detection in the glyph stream).
    fn pick(&self, family: &FontFamily) -> (&PdfFontId, u8) {
        match family {
            FontFamily::Monospace => (&self.mono, 2),
            FontFamily::Name(n) if n.as_ref() == theme::BOLD_FAMILY => (&self.bold, 1),
            _ => (&self.reg, 0),
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

fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color::Rgb(Rgb { r, g, b, icc_profile: None })
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

fn lp(x: f32, y: f32) -> LinePoint {
    LinePoint { p: Point { x: Pt(x), y: Pt(y) }, bezier: false }
}

/// A filled/stroked rectangle as a 4-point polygon. printpdf's `Op::DrawRectangle` only sets a
/// clip path and paints nothing (`re W n`), so rectangles must go through `DrawPolygon`, which
/// honours the paint mode.
fn rect_op(x: f32, y: f32, w: f32, h: f32, mode: PaintMode) -> Op {
    Op::DrawPolygon {
        polygon: Polygon {
            rings: vec![PolygonRing {
                points: vec![lp(x, y), lp(x + w, y), lp(x + w, y + h), lp(x, y + h)],
            }],
            mode,
            winding_order: WindingOrder::NonZero,
        },
    }
}

/// Emit one scene primitive into the page's op stream.
fn emit(ops: &mut Vec<Op>, tr: &Tr, fonts: &FontSet, prim: &Prim) {
    match prim {
        Prim::Rect { rect, fill, stroke } => {
            let mode = match (fill.is_some(), stroke.is_some()) {
                (true, true) => PaintMode::FillStroke,
                (true, false) => PaintMode::Fill,
                (false, true) => PaintMode::Stroke,
                (false, false) => return,
            };
            if let Some(f) = fill {
                ops.push(Op::SetFillColor { col: ink(*f) });
            }
            if let Some(s) = stroke {
                ops.push(Op::SetOutlineColor { col: ink(s.color) });
                ops.push(Op::SetOutlineThickness { pt: Pt(tr.s(s.width).max(0.3)) });
            }
            // PDF rect origin is its bottom-left (the top edge in column space, mapped down).
            ops.push(rect_op(tr.x(rect.min.x), tr.y(rect.max.y), tr.s(rect.width()), tr.s(rect.height()), mode));
        }
        Prim::Line { from, to, stroke } => {
            line_seg(ops, tr, from.x, to.x, from.y, to.y, stroke.color, stroke.width);
        }
        Prim::Circle { center, radius, fill, stroke } => {
            let n = 24;
            let points: Vec<LinePoint> = (0..n)
                .map(|i| {
                    let a = std::f32::consts::TAU * i as f32 / n as f32;
                    lp(tr.x(center.x + radius * a.cos()), tr.y(center.y + radius * a.sin()))
                })
                .collect();
            let mode = match (fill.is_some(), stroke.is_some()) {
                (true, true) => PaintMode::FillStroke,
                (true, false) => PaintMode::Fill,
                (false, true) => PaintMode::Stroke,
                (false, false) => return,
            };
            if let Some(f) = fill {
                ops.push(Op::SetFillColor { col: ink(*f) });
            }
            if let Some(s) = stroke {
                ops.push(Op::SetOutlineColor { col: ink(s.color) });
                ops.push(Op::SetOutlineThickness { pt: Pt(tr.s(s.width).max(0.3)) });
            }
            ops.push(Op::DrawPolygon {
                polygon: Polygon {
                    rings: vec![PolygonRing { points }],
                    mode,
                    winding_order: WindingOrder::NonZero,
                },
            });
        }
        Prim::Path { points, stroke } => {
            ops.push(Op::SetOutlineColor { col: ink(stroke.color) });
            ops.push(Op::SetOutlineThickness { pt: Pt(tr.s(stroke.width).max(0.3)) });
            let pts: Vec<LinePoint> = points.iter().map(|p| lp(tr.x(p.x), tr.y(p.y))).collect();
            ops.push(Op::DrawLine { line: Line { points: pts, is_closed: false } });
        }
        Prim::Galley { pos, galley } => draw_galley(ops, tr, fonts, *pos, galley),
        Prim::Marker { pos, text, font, color, align } => {
            draw_marker(ops, tr, fonts, *pos, text, font, *color, *align)
        }
    }
}

fn line_seg(ops: &mut Vec<Op>, tr: &Tr, x0: f32, x1: f32, y0: f32, y1: f32, color: Color32, w: f32) {
    ops.push(Op::SetOutlineColor { col: ink(color) });
    ops.push(Op::SetOutlineThickness { pt: Pt(tr.s(w).max(0.3)) });
    ops.push(Op::DrawLine {
        line: Line { points: vec![lp(tr.x(x0), tr.y(y0)), lp(tr.x(x1), tr.y(y1))], is_closed: false },
    });
}

/// One glyph, lifted out of the galley with its absolute column position and resolved format.
struct PlacedGlyph {
    chr: char,
    x: f32,
    baseline: f32,
    advance: f32,
    ascent: f32,
    line_h: f32,
    fmt: TextFormat,
}

/// Walk a galley's rows/glyphs in document order, reconstructing each glyph's byte offset (and
/// thus its `LayoutSection` format) — `Glyph::section_index` is private, so we re-derive it.
fn collect_glyphs(pos: Pos2, galley: &Galley) -> Vec<PlacedGlyph> {
    let sections = &galley.job.sections;
    let mut out = Vec::new();
    let mut byte = 0usize;
    for row in &galley.rows {
        for g in &row.glyphs {
            let fmt = sections
                .iter()
                .find(|s| s.byte_range.contains(&byte))
                .map(|s| s.format.clone())
                .unwrap_or_default();
            out.push(PlacedGlyph {
                chr: g.chr,
                x: pos.x + row.pos.x + g.pos.x,
                baseline: pos.y + row.pos.y + g.pos.y,
                advance: g.advance_width,
                ascent: g.font_ascent,
                line_h: g.line_height,
                fmt,
            });
            byte += g.chr.len_utf8();
        }
        if row.ends_with_newline {
            byte += 1; // the implicit '\n' carries no glyph
        }
    }
    out
}

/// Emit a block's body text glyph-by-glyph: backgrounds first, then the glyphs at their exact
/// positions, then strikethrough/underline over them.
fn draw_galley(ops: &mut Vec<Op>, tr: &Tr, fonts: &FontSet, pos: Pos2, galley: &Galley) {
    let glyphs = collect_glyphs(pos, galley);
    if glyphs.is_empty() {
        return;
    }

    // 1) Inline-code (and any) backgrounds, behind the text.
    for g in &glyphs {
        if g.fmt.background != Color32::TRANSPARENT {
            ops.push(Op::SetFillColor { col: ink(g.fmt.background) });
            ops.push(rect_op(
                tr.x(g.x),
                tr.y(g.baseline - g.ascent + g.line_h),
                tr.s(g.advance),
                tr.s(g.line_h),
                PaintMode::Fill,
            ));
        }
    }

    // 2) The glyphs — one absolute `Tm` per glyph, so positions match the galley exactly.
    ops.push(Op::StartTextSection);
    let mut cur_font: Option<(u8, u32)> = None;
    let mut cur_col: Option<[u8; 4]> = None;
    for g in &glyphs {
        if g.chr.is_whitespace() {
            continue; // whitespace advances nothing here (every glyph is absolutely placed)
        }
        let (fid, key) = fonts.pick(&g.fmt.font_id.family);
        let size = g.fmt.font_id.size * SCALE;
        let fk = (key, size.to_bits());
        if cur_font != Some(fk) {
            ops.push(Op::SetFont { font: PdfFontHandle::External(fid.clone()), size: Pt(size) });
            cur_font = Some(fk);
        }
        let ck = g.fmt.color.to_array();
        if cur_col != Some(ck) {
            ops.push(Op::SetFillColor { col: ink(g.fmt.color) });
            cur_col = Some(ck);
        }
        let (px, py) = (tr.x(g.x), tr.y(g.baseline));
        let matrix = if g.fmt.italics {
            TextMatrix::Raw([1.0, 0.0, ITALIC_SHEAR, 1.0, px, py])
        } else {
            TextMatrix::Translate(Pt(px), Pt(py))
        };
        ops.push(Op::SetTextMatrix { matrix });
        ops.push(Op::ShowText { items: vec![TextItem::Text(g.chr.to_string())] });
    }
    ops.push(Op::EndTextSection);

    // 3) Strikethrough / underline, over the glyphs (adjacent segments join into a continuous rule).
    for g in &glyphs {
        if g.fmt.strikethrough.width > 0.0 {
            let y = g.baseline - g.ascent * 0.30;
            line_seg(ops, tr, g.x, g.x + g.advance, y, y, g.fmt.strikethrough.color, g.fmt.strikethrough.width.max(1.0));
        }
        if g.fmt.underline.width > 0.0 {
            let y = g.baseline + g.ascent * 0.12;
            line_seg(ops, tr, g.x, g.x + g.advance, y, y, g.fmt.underline.color, g.fmt.underline.width.max(1.0));
        }
    }
}

/// A one-off single-font label (list ordinal, code language tag). Width is approximated for
/// right/centre alignment — exactness here doesn't matter the way body text does.
fn draw_marker(ops: &mut Vec<Op>, tr: &Tr, fonts: &FontSet, pos: Pos2, text: &str, font: &FontId, color: Color32, align: Align2) {
    if text.is_empty() {
        return;
    }
    let (fid, _) = fonts.pick(&font.family);
    let per = if matches!(font.family, FontFamily::Monospace) { 0.6 } else { 0.5 };
    let w = text.chars().count() as f32 * font.size * per;
    let x_col = if matches!(align, Align2::RIGHT_TOP | Align2::RIGHT_CENTER | Align2::RIGHT_BOTTOM) {
        pos.x - w
    } else if matches!(align, Align2::CENTER_TOP | Align2::CENTER_CENTER | Align2::CENTER_BOTTOM) {
        pos.x - w * 0.5
    } else {
        pos.x
    };
    let baseline = pos.y + font.size * 0.8;
    ops.push(Op::SetFont { font: PdfFontHandle::External(fid.clone()), size: Pt(font.size * SCALE) });
    ops.push(Op::SetFillColor { col: ink(color) });
    ops.push(Op::StartTextSection);
    ops.push(Op::SetTextMatrix { matrix: TextMatrix::Translate(Pt(tr.x(x_col)), Pt(tr.y(baseline))) });
    ops.push(Op::ShowText { items: vec![TextItem::Text(text.to_string())] });
    ops.push(Op::EndTextSection);
}
