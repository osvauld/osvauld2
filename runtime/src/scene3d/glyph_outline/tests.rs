use super::*;

const JETBRAINS_MONO: &[u8] = include_bytes!("../../../assets/JetBrainsMono-Regular.ttf");

#[test]
fn extracts_a_closed_outline_with_real_curves_for_o() {
    let outline = glyph_outline(JETBRAINS_MONO, 'O').expect("JetBrains Mono has 'O'");
    assert!(outline.units_per_em > 0);
    assert!(!outline.contours.is_empty());

    let has_curve = outline
        .contours
        .iter()
        .flat_map(|contour| &contour.segments)
        .any(|segment| matches!(segment, Segment::Quad { .. }));
    assert!(has_curve, "'O' is round; expected at least one quadratic segment");

    for contour in &outline.contours {
        let end = contour.segments.last().map(|segment| match segment {
            Segment::Line { end } | Segment::Quad { end, .. } => *end,
        });
        assert_eq!(end, Some(contour.start), "contour must close back to its start");
    }
}

#[test]
fn every_contour_closes_even_when_the_font_doesnt_draw_the_final_edge() {
    // Regression: 'A's counter (the hole above its crossbar) ends its last drawn segment away
    // from its own start — the font relies on close() to imply that final edge. A pen that
    // doesn't add it leaves the contour open, breaking winding-number parity at that height.
    for ch in "AOSVAULD3D".chars() {
        let outline = glyph_outline(JETBRAINS_MONO, ch).unwrap();
        for contour in &outline.contours {
            let end = contour.segments.last().map(|segment| match segment {
                Segment::Line { end } | Segment::Quad { end, .. } => *end,
            });
            assert_eq!(end, Some(contour.start), "{ch:?}'s contour must close back to its start");
        }
    }
}

#[test]
fn missing_glyph_returns_none() {
    // U+E000: private-use area, not mapped by JetBrains Mono.
    assert!(glyph_outline(JETBRAINS_MONO, '\u{E000}').is_none());
}
