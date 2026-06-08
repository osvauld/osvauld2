//! The backend-neutral display list. Layout composes the document's content into a [`Scene`] of
//! primitives in column-relative coordinates; two backends consume it — the egui painter (the
//! screen, [`Scene::paint`]) and the PDF writer (`crate::pdf`) — so paper matches glass by
//! construction. Only content lives here; screen-only chrome (caret, selection, hover, gutter,
//! spine, type-tags) stays in `paint`. Coordinates are relative to the column's top-left (the
//! `Placed` space); each backend supplies its own offset.

use std::sync::Arc;

use egui::{
    Align2, Color32, CornerRadius, FontId, Galley, Painter, Pos2, Rect, Shape, Stroke, StrokeKind,
    Vec2,
};

use crate::theme;

/// One frame's content as an ordered list of per-block primitive groups. The grouping carries
/// each block's vertical extent so the PDF backend can paginate by block (the screen renderer
/// ignores it and draws straight through).
pub struct Scene {
    pub blocks: Vec<BlockPrims>,
}

/// The content primitives for a single block, plus its row extent (for pagination).
pub struct BlockPrims {
    pub row_top: f32,
    pub row_bottom: f32,
    pub prims: Vec<Prim>,
}

/// A single drawable. The set is deliberately small: text (a shaped galley or a one-off marker)
/// plus the vector shapes decorations need; math and code highlighting reduce to these too.
pub enum Prim {
    /// Filled and/or stroked rectangle — code-block fill, checkbox, deep-bullet square.
    Rect { rect: Rect, fill: Option<Color32>, stroke: Option<Stroke> },
    /// A straight segment — divider rule, quote rule.
    Line { from: Pos2, to: Pos2, stroke: Stroke },
    /// Filled and/or stroked circle — bullets (a dot at the top level, a ring one in).
    Circle { center: Pos2, radius: f32, fill: Option<Color32>, stroke: Option<Stroke> },
    /// An open polyline — the to-do checkmark.
    Path { points: Vec<Pos2>, stroke: Stroke },
    /// A block's shaped body text. The galley holds exact glyph positions + per-section format,
    /// so the PDF walks the same layout the screen drew — identical wrapping, no re-typeset.
    Galley { pos: Pos2, galley: Arc<Galley> },
    /// A one-off run of single-font text — list ordinals, the code language tag.
    Marker { pos: Pos2, text: String, font: FontId, color: Color32, align: Align2 },
}

impl Scene {
    /// Render the whole content list to an egui painter, translated by `offset` (the column's
    /// screen origin). Drawn in order, so a block's fill precedes its text.
    pub fn paint(&self, painter: &Painter, offset: Vec2) {
        for block in &self.blocks {
            for prim in &block.prims {
                prim.paint(painter, offset);
            }
        }
    }
}

impl Prim {
    fn paint(&self, painter: &Painter, off: Vec2) {
        match self {
            Prim::Rect { rect, fill, stroke } => {
                let r = rect.translate(off);
                if let Some(f) = fill {
                    painter.rect_filled(r, CornerRadius::same(0), *f);
                }
                if let Some(s) = stroke {
                    painter.rect_stroke(r, CornerRadius::same(0), *s, StrokeKind::Inside);
                }
            }
            Prim::Line { from, to, stroke } => {
                painter.line_segment([*from + off, *to + off], *stroke);
            }
            Prim::Circle { center, radius, fill, stroke } => {
                if let Some(f) = fill {
                    painter.circle_filled(*center + off, *radius, *f);
                }
                if let Some(s) = stroke {
                    painter.circle_stroke(*center + off, *radius, *s);
                }
            }
            Prim::Path { points, stroke } => {
                let pts = points.iter().map(|p| *p + off).collect();
                painter.add(Shape::line(pts, *stroke));
            }
            Prim::Galley { pos, galley } => {
                painter.galley(*pos + off, galley.clone(), theme::FG_1);
            }
            Prim::Marker { pos, text, font, color, align } => {
                painter.text(*pos + off, *align, text, font.clone(), *color);
            }
        }
    }
}
