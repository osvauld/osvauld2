//! Glyph outlines as font-unit curves, pulled straight from the font via `skrifa` — the same
//! outline source Vello's own glyph rendering uses. Exploration for painting text directly on a
//! 3D face from the cube shader instead of baking to a flat atlas; nothing consumes this yet.
#![allow(dead_code)]

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};

/// One glyph's outline in font-unit space (unscaled — divide by `units_per_em` to normalize).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GlyphOutline {
    pub units_per_em: u16,
    pub contours: Vec<Contour>,
}

/// A closed loop: `start`, then each segment's end point in turn. The last segment must end
/// back at `start`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Contour {
    pub start: [f32; 2],
    pub segments: Vec<Segment>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Segment {
    Line { end: [f32; 2] },
    Quad { control: [f32; 2], end: [f32; 2] },
}

/// Extracts `ch`'s outline from a font's bytes, or `None` if the font lacks the glyph or the
/// data doesn't parse.
pub(crate) fn glyph_outline(font_bytes: &[u8], ch: char) -> Option<GlyphOutline> {
    let font = FontRef::new(font_bytes).ok()?;
    let units_per_em = font
        .metrics(Size::unscaled(), LocationRef::default())
        .units_per_em;
    let glyph_id: GlyphId = font.charmap().map(ch)?;
    let outline = font.outline_glyphs().get(glyph_id)?;
    let mut pen = ContourPen::default();
    outline
        .draw(
            DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
            &mut pen,
        )
        .ok()?;
    pen.close_current();
    Some(GlyphOutline {
        units_per_em,
        contours: pen.contours,
    })
}

#[derive(Default)]
struct ContourPen {
    contours: Vec<Contour>,
    current: Option<Contour>,
}

impl ContourPen {
    fn close_current(&mut self) {
        if let Some(mut contour) = self.current.take() {
            // A contour's last drawn point doesn't always land back on `start` — e.g. an outline
            // ending on a straight join relies on close() itself implying that final edge. Without
            // adding it, the contour is left open, which breaks winding-number parity (missing
            // exactly one crossing) for any scanline through the gap.
            let end = contour
                .segments
                .last()
                .map(|segment| match segment {
                    Segment::Line { end } | Segment::Quad { end, .. } => *end,
                })
                .unwrap_or(contour.start);
            if end != contour.start {
                contour.segments.push(Segment::Line { end: contour.start });
            }
            self.contours.push(contour);
        }
    }
}

impl OutlinePen for ContourPen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close_current();
        self.current = Some(Contour {
            start: [x, y],
            segments: Vec::new(),
        });
    }

    fn line_to(&mut self, x: f32, y: f32) {
        if let Some(contour) = &mut self.current {
            contour.segments.push(Segment::Line { end: [x, y] });
        }
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        if let Some(contour) = &mut self.current {
            contour.segments.push(Segment::Quad {
                control: [cx0, cy0],
                end: [x, y],
            });
        }
    }

    fn curve_to(&mut self, _cx0: f32, _cy0: f32, _cx1: f32, _cy1: f32, x: f32, y: f32) {
        // Our bundled fonts are TrueType (quadratic-only glyf outlines); a cubic segment here
        // would mean a CFF font, out of scope for this probe. Flatten rather than panic.
        if let Some(contour) = &mut self.current {
            contour.segments.push(Segment::Line { end: [x, y] });
        }
    }

    fn close(&mut self) {
        self.close_current();
    }
}

#[cfg(test)]
mod tests;
