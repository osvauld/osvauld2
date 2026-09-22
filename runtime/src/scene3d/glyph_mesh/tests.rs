use super::super::glyph_outline::glyph_outline;
use super::*;

const JETBRAINS_MONO: &[u8] = include_bytes!("../../../assets/JetBrainsMono-Regular.ttf");

#[test]
fn every_segment_becomes_one_curve() {
    let outline = glyph_outline(JETBRAINS_MONO, 'O').unwrap();
    let segment_count: usize = outline.contours.iter().map(|c| c.segments.len()).sum();
    let mesh = glyph_mesh(&outline);
    assert_eq!(mesh.curves.len(), segment_count);
}

#[test]
fn curvature_survives_normalization() {
    let outline = glyph_outline(JETBRAINS_MONO, 'O').unwrap();
    let mesh = glyph_mesh(&outline);
    let has_real_curve = mesh.curves.iter().any(|c| {
        let mid = [(c.p0[0] + c.p1[0]) / 2.0, (c.p0[1] + c.p1[1]) / 2.0];
        (c.control[0] - mid[0]).abs() > 1e-4 || (c.control[1] - mid[1]).abs() > 1e-4
    });
    assert!(has_real_curve, "'O' should keep at least one non-degenerate curve");
}

#[test]
fn bounds_are_sane_and_normalized_to_em_space() {
    let outline = glyph_outline(JETBRAINS_MONO, 'O').unwrap();
    let mesh = glyph_mesh(&outline);
    assert!(mesh.min[0] < mesh.max[0]);
    assert!(mesh.min[1] < mesh.max[1]);
    // em-space: a glyph's footprint should be a small multiple of one em, not thousands of units.
    assert!(mesh.max[0] - mesh.min[0] < 2.0);
    assert!(mesh.max[1] - mesh.min[1] < 2.0);
}
