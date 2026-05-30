//! Exercises the frame loop natively — no wasm needed, since `Runtime::frame`
//! works on byte slices. (The raw pointer ABI helpers in `lib.rs` truncate to
//! `u32` and so are wasm32-only; they're validated end-to-end when the host
//! loads a real module in the next slice.)

use super::*;

#[derive(Default)]
struct Counter {
    n: i64,
}

fn draw(ui: &mut egui::Ui, state: &mut Counter) {
    // Constant text → a stable glyph set, so the font atlas stops changing after
    // frame 1. (Varying text would append new glyphs and keep producing texture
    // deltas — a property of egui, not of our Context.)
    ui.label("steady label");
    state.n += 1; // bump each frame so we can prove state persists
}

fn input() -> Vec<u8> {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 120.0))),
        ..Default::default()
    };
    app_abi::encode_input(&raw).expect("encode input")
}

#[test]
fn frame_draws_a_decodable_surface() {
    let mut rt = Runtime::new(Counter::default(), draw);
    let out = rt.frame(&input());
    let surface = app_abi::decode_surface(&out).expect("host can decode the surface");
    assert!(!surface.primitives.is_empty(), "the frame drew geometry");
    assert!(!surface.textures_delta.set.is_empty(), "first frame uploads the font atlas");
}

#[test]
fn context_and_state_persist_across_frames() {
    let mut rt = Runtime::new(Counter::default(), draw);

    let first = app_abi::decode_surface(&rt.frame(&input())).unwrap();
    let second = app_abi::decode_surface(&rt.frame(&input())).unwrap();

    // The font atlas is uploaded only once: an empty texture delta on the second
    // frame proves the *same* egui Context was reused, not built anew each call.
    assert!(!first.textures_delta.set.is_empty(), "atlas uploaded on frame 1");
    assert!(second.textures_delta.set.is_empty(), "atlas NOT re-uploaded on frame 2 — Context persisted");

    // And `state.n` kept counting (3 after two frames), proving state persists too.
    assert_eq!(rt.state.n, 2, "state mutated across frames is retained");
}

#[test]
fn bad_input_does_not_panic() {
    let mut rt = Runtime::new(Counter::default(), draw);
    // Garbage bytes decode-fail and fall back to default input; still draws.
    let out = rt.frame(&[0xff, 0x00, 0x42]);
    assert!(app_abi::decode_surface(&out).is_ok(), "a malformed input still yields a surface");
}
