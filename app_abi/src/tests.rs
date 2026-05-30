//! Proves the load-bearing assumption of the whole wasm move: egui's `RawInput`
//! and a tessellated `Surface` survive a postcard round-trip losslessly. If this
//! holds, the boundary can be bytes instead of a shared Rust value.

use super::*;

/// Run one real egui frame and hand back the surface it drew — a button and a
/// label, so the result carries both geometry *and* a font-atlas texture upload.
fn sample_surface() -> Surface {
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 120.0))),
        ..Default::default()
    };
    let output = ctx.run_ui(input, |ui| {
        ui.label("count: 41");
        let _ = ui.button("+");
    });
    let repaint_after = output
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .map_or(std::time::Duration::MAX, |v| v.repaint_delay);
    let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    Surface::from_tessellated(primitives, output.textures_delta, output.pixels_per_point, repaint_after)
}

#[test]
fn surface_round_trips_losslessly() {
    let surface = sample_surface();
    assert!(!surface.primitives.is_empty(), "the frame drew geometry");
    assert!(!surface.textures_delta.set.is_empty(), "first frame uploads the font atlas");

    let bytes = encode_surface(&surface).expect("encode");
    let decoded = decode_surface(&bytes).expect("decode");

    assert_eq!(decoded.primitives.len(), surface.primitives.len(), "primitive count preserved");
    assert_eq!(decoded.pixels_per_point, surface.pixels_per_point, "ppp preserved");
    assert_eq!(decoded.repaint_after, surface.repaint_after, "repaint signal preserved");
    assert_eq!(decoded.textures_delta.set.len(), surface.textures_delta.set.len(), "texture uploads preserved");
    // The rebuilt renderer input matches too — meshes survive the trip intact.
    assert_eq!(decoded.to_clipped_primitives().len(), surface.primitives.len(), "rebuilds renderer primitives");

    // Re-encoding the decoded value must yield identical bytes: the strongest
    // lossless check, and it needs no `PartialEq` on the egui types.
    let reencoded = encode_surface(&decoded).expect("re-encode");
    assert_eq!(bytes, reencoded, "decode∘encode is a fixed point — round-trip is lossless");
}

#[test]
fn input_round_trips_losslessly() {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(260.0, 200.0))),
        focused: true,
        events: vec![
            egui::Event::PointerMoved(egui::pos2(12.0, 34.0)),
            egui::Event::PointerButton {
                pos: egui::pos2(12.0, 34.0),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            },
        ],
        ..Default::default()
    };

    let bytes = encode_input(&input).expect("encode");
    let decoded = decode_input(&bytes).expect("decode");

    assert_eq!(decoded.events.len(), input.events.len(), "events preserved");
    let reencoded = encode_input(&decoded).expect("re-encode");
    assert_eq!(bytes, reencoded, "decode∘encode is a fixed point — round-trip is lossless");
}
