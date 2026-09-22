//! Lays out a string as one flat list of em-normalized curves, advancing the baseline with the
//! font's real per-glyph advance widths. This is what slice 4's GPU side uploads per text object,
//! replacing the single hardcoded glyph from the checkpoint. Still unconsumed by rendering.
#![allow(dead_code)]

use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};

use super::glyph_mesh::{Curve, glyph_mesh};
use super::glyph_outline::glyph_outline;

/// Fallback advance (in em) for a character the font can't shape — keeps layout moving instead of
/// stacking the next glyph on top of it.
const FALLBACK_ADVANCE: f32 = 0.6;

/// A string's curves, already positioned along the baseline in em-space, plus the bounding box
/// they occupy.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextMesh {
    pub curves: Vec<Curve>,
    pub min: [f32; 2],
    pub max: [f32; 2],
}

pub(crate) fn layout_text(font_bytes: &[u8], text: &str) -> TextMesh {
    let font = FontRef::new(font_bytes).ok();
    let units_per_em = font
        .as_ref()
        .map(|f| f.metrics(Size::unscaled(), LocationRef::default()).units_per_em)
        .filter(|&upm| upm > 0)
        .unwrap_or(1000) as f32;

    let mut curves = Vec::new();
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    let bound = |point: [f32; 2], min: &mut [f32; 2], max: &mut [f32; 2]| {
        *min = [min[0].min(point[0]), min[1].min(point[1])];
        *max = [max[0].max(point[0]), max[1].max(point[1])];
    };

    let mut cursor = 0.0f32;
    for ch in text.chars() {
        if let Some(outline) = glyph_outline(font_bytes, ch) {
            for curve in glyph_mesh(&outline).curves {
                let shifted = Curve {
                    p0: [curve.p0[0] + cursor, curve.p0[1]],
                    control: [curve.control[0] + cursor, curve.control[1]],
                    p1: [curve.p1[0] + cursor, curve.p1[1]],
                };
                bound(shifted.p0, &mut min, &mut max);
                bound(shifted.control, &mut min, &mut max);
                bound(shifted.p1, &mut min, &mut max);
                curves.push(shifted);
            }
        }
        let advance = font
            .as_ref()
            .and_then(|f| {
                let glyph_id = f.charmap().map(ch)?;
                f.glyph_metrics(Size::unscaled(), LocationRef::default())
                    .advance_width(glyph_id)
            })
            .map(|units| units / units_per_em)
            .unwrap_or(FALLBACK_ADVANCE);
        cursor += advance;
    }
    if curves.is_empty() {
        min = [0.0; 2];
        max = [0.0; 2];
    }
    TextMesh { curves, min, max }
}

#[cfg(test)]
mod tests;
