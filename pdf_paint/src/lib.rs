//! Shared egui→printpdf emission: positioned galleys and vector primitives become page ops.
//! The caller owns layout — everything arrives absolutely positioned in its own source space —
//! and supplies the source→page transform ([`PageMap`]), the embedded faces ([`FontSet`]) and a
//! colour mapping (theme inversion, or [`over_white`] for verbatim). Text is emitted
//! glyph-by-glyph at its exact galley position (one absolute `Tm` per glyph) using the same
//! embedded TTFs, so wrapping and placement match the screen by construction.

use egui::{Align2, Color32, FontFamily, FontId, Galley, Pos2, Rect, Stroke, TextFormat};
pub use printpdf;
use printpdf::{
    Color, FontId as PdfFontId, Line, LinePoint, Op, PaintMode, ParsedFont, PdfDocument,
    PdfFontHandle, Point, Polygon, PolygonRing, Pt, Rgb, TextItem, TextMatrix, WindingOrder,
};

/// Shear applied to italic glyphs, matching egui's synthesised slant closely enough to read.
const ITALIC_SHEAR: f32 = 0.21;
/// Hairline floor — strokes thinner than this vanish in print.
const MIN_STROKE: f32 = 0.3;

/// Source space (px, y-down) → PDF page (pt, y-up). `(src_left, src_top)` maps to
/// `(offset_x, page_h − offset_y)`; lengths scale by `scale`.
pub struct PageMap {
    pub page_h: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub src_left: f32,
    pub src_top: f32,
    pub scale: f32,
}

impl PageMap {
    pub fn x(&self, cx: f32) -> f32 {
        self.offset_x + (cx - self.src_left) * self.scale
    }
    pub fn y(&self, cy: f32) -> f32 {
        self.page_h - self.offset_y - (cy - self.src_top) * self.scale
    }
    pub fn s(&self, len: f32) -> f32 {
        len * self.scale
    }
}

/// The TTF bytes to embed — the same faces the caller registered with egui.
pub struct FontBytes<'a> {
    pub regular: &'a [u8],
    pub bold: &'a [u8],
    pub mono: &'a [u8],
    /// Extra glyph-fallback faces (e.g. Indic). Shaping only — not embedded in PDFs yet.
    pub fallback: &'a [&'a [u8]],
}

/// The three embedded faces, and the rule mapping an egui family to one of them. Bold rides the
/// shared [`rich_text::BOLD_FAMILY`] named family.
pub struct FontSet {
    reg: PdfFontId,
    bold: PdfFontId,
    mono: PdfFontId,
}

impl FontSet {
    /// Parse and embed the three faces into `pdf`.
    pub fn load(pdf: &mut PdfDocument, bytes: FontBytes) -> FontSet {
        let mut warn = Vec::new();
        let mut add = |b: &[u8], what: &str| {
            pdf.add_font(&ParsedFont::from_bytes(b, 0, &mut warn).unwrap_or_else(|| panic!("{what} font")))
        };
        FontSet {
            reg: add(bytes.regular, "regular"),
            bold: add(bytes.bold, "bold"),
            mono: add(bytes.mono, "mono"),
        }
    }

    /// Returns the embedded font plus a small key (for change-detection in the glyph stream).
    fn pick(&self, family: &FontFamily) -> (&PdfFontId, u8) {
        match family {
            FontFamily::Monospace => (&self.mono, 2),
            FontFamily::Name(n) if n.as_ref() == rich_text::BOLD_FAMILY => (&self.bold, 1),
            _ => (&self.reg, 0),
        }
    }
}

/// Map a colour to print verbatim: composite over white at its alpha (PDF fills are opaque),
/// no theme inversion. Channels are premultiplied, so `out = ch + (1 − a)`.
pub fn over_white(c: Color32) -> Color {
    let a = c.a() as f32 / 255.0;
    let over = |ch: u8| (ch as f32 / 255.0 + (1.0 - a)).min(1.0);
    rgb(over(c.r()), over(c.g()), over(c.b()))
}

pub fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color::Rgb(Rgb { r, g, b, icc_profile: None })
}

fn lp(x: f32, y: f32) -> LinePoint {
    LinePoint { p: Point { x: Pt(x), y: Pt(y) }, bezier: false }
}

/// A filled/stroked rectangle as a 4-point polygon, in *page* coordinates. printpdf's
/// `Op::DrawRectangle` only sets a clip path and paints nothing (`re W n`), so rectangles must go
/// through `DrawPolygon`, which honours the paint mode.
pub fn rect_op(x: f32, y: f32, w: f32, h: f32, mode: PaintMode) -> Op {
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

fn paint_mode(fill: bool, stroke: bool) -> Option<PaintMode> {
    match (fill, stroke) {
        (true, true) => Some(PaintMode::FillStroke),
        (true, false) => Some(PaintMode::Fill),
        (false, true) => Some(PaintMode::Stroke),
        (false, false) => None,
    }
}

/// One emission pass into a page's op stream: primitives in source space go in, page ops come
/// out, transformed by `map` and coloured through `ink`.
pub struct Emit<'a> {
    pub ops: &'a mut Vec<Op>,
    pub map: &'a PageMap,
    pub fonts: &'a FontSet,
    pub ink: &'a dyn Fn(Color32) -> Color,
}

impl Emit<'_> {
    pub fn rect(&mut self, rect: Rect, fill: Option<Color32>, stroke: Option<Stroke>) {
        let Some(mode) = paint_mode(fill.is_some(), stroke.is_some()) else { return };
        if let Some(f) = fill {
            self.ops.push(Op::SetFillColor { col: (self.ink)(f) });
        }
        if let Some(s) = stroke {
            self.set_stroke(s);
        }
        // PDF rect origin is its bottom-left (the top edge in source space, mapped down).
        self.ops.push(rect_op(
            self.map.x(rect.min.x),
            self.map.y(rect.max.y),
            self.map.s(rect.width()),
            self.map.s(rect.height()),
            mode,
        ));
    }

    pub fn line(&mut self, from: Pos2, to: Pos2, stroke: Stroke) {
        self.line_seg(from.x, to.x, from.y, to.y, stroke.color, stroke.width);
    }

    pub fn circle(&mut self, center: Pos2, radius: f32, fill: Option<Color32>, stroke: Option<Stroke>) {
        let Some(mode) = paint_mode(fill.is_some(), stroke.is_some()) else { return };
        let n = 24;
        let points: Vec<LinePoint> = (0..n)
            .map(|i| {
                let a = std::f32::consts::TAU * i as f32 / n as f32;
                lp(self.map.x(center.x + radius * a.cos()), self.map.y(center.y + radius * a.sin()))
            })
            .collect();
        if let Some(f) = fill {
            self.ops.push(Op::SetFillColor { col: (self.ink)(f) });
        }
        if let Some(s) = stroke {
            self.set_stroke(s);
        }
        self.ops.push(Op::DrawPolygon {
            polygon: Polygon {
                rings: vec![PolygonRing { points }],
                mode,
                winding_order: WindingOrder::NonZero,
            },
        });
    }

    pub fn path(&mut self, points: &[Pos2], stroke: Stroke) {
        self.set_stroke(stroke);
        let pts: Vec<LinePoint> = points.iter().map(|p| lp(self.map.x(p.x), self.map.y(p.y))).collect();
        self.ops.push(Op::DrawLine { line: Line { points: pts, is_closed: false } });
    }

    /// Emit a galley's body text glyph-by-glyph: backgrounds first, then the glyphs at their
    /// exact positions, then strikethrough/underline over them. `fallback` replaces
    /// `Color32::PLACEHOLDER` runs (colour-neutral galleys recoloured at paint time).
    pub fn galley(&mut self, pos: Pos2, galley: &Galley, fallback: Color32) {
        let glyphs = collect_glyphs(pos, galley);
        if glyphs.is_empty() {
            return;
        }
        let resolve = |c: Color32| if c == Color32::PLACEHOLDER { fallback } else { c };

        // 1) Inline-code (and any) backgrounds, behind the text.
        for g in &glyphs {
            if g.fmt.background != Color32::TRANSPARENT {
                self.ops.push(Op::SetFillColor { col: (self.ink)(g.fmt.background) });
                self.ops.push(rect_op(
                    self.map.x(g.x),
                    self.map.y(g.baseline - g.ascent + g.line_h),
                    self.map.s(g.advance),
                    self.map.s(g.line_h),
                    PaintMode::Fill,
                ));
            }
        }

        // 2) The glyphs — one absolute `Tm` per glyph, so positions match the galley exactly.
        self.ops.push(Op::StartTextSection);
        let mut cur_font: Option<(u8, u32)> = None;
        let mut cur_col: Option<[u8; 4]> = None;
        for g in &glyphs {
            if g.chr.is_whitespace() {
                continue; // whitespace advances nothing here (every glyph is absolutely placed)
            }
            let (fid, key) = self.fonts.pick(&g.fmt.font_id.family);
            let size = g.fmt.font_id.size * self.map.scale;
            let fk = (key, size.to_bits());
            if cur_font != Some(fk) {
                self.ops.push(Op::SetFont { font: PdfFontHandle::External(fid.clone()), size: Pt(size) });
                cur_font = Some(fk);
            }
            let color = resolve(g.fmt.color);
            let ck = color.to_array();
            if cur_col != Some(ck) {
                self.ops.push(Op::SetFillColor { col: (self.ink)(color) });
                cur_col = Some(ck);
            }
            let (px, py) = (self.map.x(g.x), self.map.y(g.baseline));
            let matrix = if g.fmt.italics {
                TextMatrix::Raw([1.0, 0.0, ITALIC_SHEAR, 1.0, px, py])
            } else {
                TextMatrix::Translate(Pt(px), Pt(py))
            };
            self.ops.push(Op::SetTextMatrix { matrix });
            self.ops.push(Op::ShowText { items: vec![TextItem::Text(g.chr.to_string())] });
        }
        self.ops.push(Op::EndTextSection);

        // 3) Strikethrough / underline, over the glyphs (adjacent segments join into a rule).
        for g in &glyphs {
            if g.fmt.strikethrough.width > 0.0 {
                let y = g.baseline - g.ascent * 0.30;
                let c = resolve(g.fmt.strikethrough.color);
                self.line_seg(g.x, g.x + g.advance, y, y, c, g.fmt.strikethrough.width.max(1.0));
            }
            if g.fmt.underline.width > 0.0 {
                let y = g.baseline + g.ascent * 0.12;
                let c = resolve(g.fmt.underline.color);
                self.line_seg(g.x, g.x + g.advance, y, y, c, g.fmt.underline.width.max(1.0));
            }
        }
    }

    /// A one-off single-font label (list ordinal, code language tag). Width is approximated for
    /// right/centre alignment — exactness here doesn't matter the way body text does.
    pub fn marker(&mut self, pos: Pos2, text: &str, font: &FontId, color: Color32, align: Align2) {
        if text.is_empty() {
            return;
        }
        let (fid, _) = self.fonts.pick(&font.family);
        let per = if matches!(font.family, FontFamily::Monospace) { 0.6 } else { 0.5 };
        let w = text.chars().count() as f32 * font.size * per;
        let x_src = if matches!(align, Align2::RIGHT_TOP | Align2::RIGHT_CENTER | Align2::RIGHT_BOTTOM) {
            pos.x - w
        } else if matches!(align, Align2::CENTER_TOP | Align2::CENTER_CENTER | Align2::CENTER_BOTTOM) {
            pos.x - w * 0.5
        } else {
            pos.x
        };
        let baseline = pos.y + font.size * 0.8;
        self.ops.push(Op::SetFont {
            font: PdfFontHandle::External(fid.clone()),
            size: Pt(font.size * self.map.scale),
        });
        self.ops.push(Op::SetFillColor { col: (self.ink)(color) });
        self.ops.push(Op::StartTextSection);
        self.ops.push(Op::SetTextMatrix {
            matrix: TextMatrix::Translate(Pt(self.map.x(x_src)), Pt(self.map.y(baseline))),
        });
        self.ops.push(Op::ShowText { items: vec![TextItem::Text(text.to_string())] });
        self.ops.push(Op::EndTextSection);
    }

    fn set_stroke(&mut self, s: Stroke) {
        self.ops.push(Op::SetOutlineColor { col: (self.ink)(s.color) });
        self.ops.push(Op::SetOutlineThickness { pt: Pt(self.map.s(s.width).max(MIN_STROKE)) });
    }

    fn line_seg(&mut self, x0: f32, x1: f32, y0: f32, y1: f32, color: Color32, w: f32) {
        self.set_stroke(Stroke::new(w, color));
        self.ops.push(Op::DrawLine {
            line: Line {
                points: vec![lp(self.map.x(x0), self.map.y(y0)), lp(self.map.x(x1), self.map.y(y1))],
                is_closed: false,
            },
        });
    }
}

/// One glyph, lifted out of the galley with its absolute source position and resolved format.
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
