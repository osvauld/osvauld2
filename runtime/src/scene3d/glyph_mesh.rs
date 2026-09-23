//! Turns a `glyph_outline::GlyphOutline` into GPU-ready curves: one em-normalized quadratic per
//! segment (a line becomes a degenerate quadratic, control = its own midpoint), so the eventual
//! shader only ever evaluates one curve shape. Still unconsumed — the GPU side lands next.
#![allow(dead_code)]

use super::glyph_outline::{GlyphOutline, Segment};

/// A quadratic in em-space (font-unit coordinates divided by `units_per_em`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Curve {
    pub p0: [f32; 2],
    pub control: [f32; 2],
    pub p1: [f32; 2],
}

/// One glyph ready for the GPU: its curves, and the em-space box they occupy.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GlyphMesh {
    pub curves: Vec<Curve>,
    pub min: [f32; 2],
    pub max: [f32; 2],
}

pub(crate) fn glyph_mesh(outline: &GlyphOutline) -> GlyphMesh {
    let scale = 1.0 / outline.units_per_em as f32;
    let mut curves = Vec::new();
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    let bound = |point: [f32; 2], min: &mut [f32; 2], max: &mut [f32; 2]| {
        *min = [min[0].min(point[0]), min[1].min(point[1])];
        *max = [max[0].max(point[0]), max[1].max(point[1])];
    };
    for contour in &outline.contours {
        let mut current = [contour.start[0] * scale, contour.start[1] * scale];
        bound(current, &mut min, &mut max);
        for segment in &contour.segments {
            let (control, end) = match *segment {
                Segment::Line { end } => {
                    let end = [end[0] * scale, end[1] * scale];
                    let mid = [(current[0] + end[0]) / 2.0, (current[1] + end[1]) / 2.0];
                    (mid, end)
                }
                Segment::Quad { control, end } => (
                    [control[0] * scale, control[1] * scale],
                    [end[0] * scale, end[1] * scale],
                ),
            };
            bound(control, &mut min, &mut max);
            bound(end, &mut min, &mut max);
            curves.push(Curve {
                p0: current,
                control,
                p1: end,
            });
            current = end;
        }
    }
    GlyphMesh { curves, min, max }
}

#[cfg(test)]
mod tests;
