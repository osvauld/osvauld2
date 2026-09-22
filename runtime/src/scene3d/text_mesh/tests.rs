use super::super::glyph_mesh::glyph_mesh;
use super::super::glyph_outline::glyph_outline;
use super::*;

const JETBRAINS_MONO: &[u8] = include_bytes!("../../../assets/JetBrainsMono-Regular.ttf");

#[test]
fn every_glyphs_curves_are_concatenated() {
    let mesh = layout_text(JETBRAINS_MONO, "OI");
    let o = glyph_mesh(&glyph_outline(JETBRAINS_MONO, 'O').unwrap()).curves.len();
    let i = glyph_mesh(&glyph_outline(JETBRAINS_MONO, 'I').unwrap()).curves.len();
    assert_eq!(mesh.curves.len(), o + i);
}

#[test]
fn baseline_advances_each_glyph() {
    let one = layout_text(JETBRAINS_MONO, "I");
    let two = layout_text(JETBRAINS_MONO, "II");
    assert_eq!(two.curves.len(), one.curves.len() * 2);
    assert!(two.max[0] > one.max[0], "second glyph should extend the bounding box rightward");
}

#[test]
fn empty_string_has_finite_bounds() {
    let mesh = layout_text(JETBRAINS_MONO, "");
    assert!(mesh.curves.is_empty());
    assert!(mesh.min[0].is_finite() && mesh.min[1].is_finite());
    assert!(mesh.max[0].is_finite() && mesh.max[1].is_finite());
}

#[test]
fn unknown_glyph_advances_by_fallback_without_panicking() {
    // U+E000: private-use area, not mapped by JetBrains Mono — contributes no curves but must
    // still take up horizontal space so layout doesn't stall or panic.
    let with_gap = layout_text(JETBRAINS_MONO, "I\u{E000}I");
    let without_gap = layout_text(JETBRAINS_MONO, "II");
    assert_eq!(with_gap.curves.len(), without_gap.curves.len());
    assert!(with_gap.max[0] > without_gap.max[0]);
}
