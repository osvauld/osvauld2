//! The shaping cache (`ui-reuse.md` C1–C4): same question, one shaping; any difference, a miss;
//! unused for a frame, gone; and never a different picture.
use super::*;

const PARA: &str = "the quick brown fox jumps over the lazy dog and keeps on running";

fn runs(len: usize) -> Vec<Run> {
    let ink = Color::from_rgba8(10, 20, 30, 255);
    vec![Run::new(0..4, 16.0, ink).bold(), Run::new(4..len, 16.0, ink)]
}

fn drawn(e: &mut TextEngine, rich: bool, width: Option<f32>) -> Scene {
    let mut scene = Scene::new();
    let at = Affine::translate((3.0, 4.0));
    match rich {
        true => e.draw_rich(&mut scene, PARA, &runs(PARA.len()), at, width, 1.0),
        false => e.draw(&mut scene, PARA, UI_FAMILY, 16.0, at, Color::from_rgba8(1, 2, 3, 255), width),
    }
    scene
}

/// C1: a label is shaped once however many times a frame asks about it — measure, its two
/// content widths, then paint.
#[test]
fn the_same_question_shapes_once() {
    let mut e = TextEngine::new();
    let first = e.measure(PARA, UI_FAMILY, 16.0, Some(200.0));
    assert_eq!(e.shaped(), 1);
    assert_eq!(e.measure(PARA, UI_FAMILY, 16.0, Some(200.0)), first);
    drawn_plain(&mut e, Some(200.0));
    assert_eq!(e.shaped(), 1, "painting what was measured shaped it again");

    e.measure(PARA, UI_FAMILY, 16.0, None);
    e.content_widths(PARA, UI_FAMILY, 16.0);
    assert_eq!(e.shaped(), 2, "the content widths share the unwrapped measure");

    let mut rich = TextEngine::new();
    let first = rich.measure_rich(PARA, &runs(PARA.len()), Some(200.0));
    assert_eq!(rich.measure_rich(PARA, &runs(PARA.len()), Some(200.0)), first);
    drawn(&mut rich, true, Some(200.0));
    assert_eq!(rich.shaped(), 1);
}

fn drawn_plain(e: &mut TextEngine, width: Option<f32>) {
    drawn(e, false, width);
}

/// C2: every input that shapes a string is part of the key.
#[test]
fn any_difference_is_a_miss() {
    let mut e = TextEngine::new();
    e.measure(PARA, UI_FAMILY, 16.0, Some(200.0));
    let misses = [
        ("string", e.measure("another string", UI_FAMILY, 16.0, Some(200.0))),
        ("size", e.measure(PARA, UI_FAMILY, 17.0, Some(200.0))),
        ("family", e.measure(PARA, MONO_FAMILY, 16.0, Some(200.0))),
        ("width", e.measure(PARA, UI_FAMILY, 16.0, Some(201.0))),
        ("no width", e.measure(PARA, UI_FAMILY, 16.0, None)),
    ];
    assert_eq!(e.shaped(), 1 + misses.len(), "{misses:?}");

    let base = runs(PARA.len());
    e.measure_rich(PARA, &base, None);
    let before = e.shaped();
    let mut changed = [base.clone(), base.clone(), base.clone(), base.clone(), base.clone()];
    changed[0][0].range = 0..5;
    changed[0][1].range = 5..PARA.len();
    changed[1][0].weight = 400.0;
    changed[2][1].italic = true;
    changed[3][1].size = 18.0;
    changed[4][1].color = Color::from_rgba8(200, 0, 0, 255);
    for runs in &changed {
        e.measure_rich(PARA, runs, None);
    }
    assert_eq!(e.shaped(), before + changed.len(), "a run difference hit the cache");
}

/// C3: what a frame did not use is gone a frame later; what every frame uses stays.
#[test]
fn unused_for_a_frame_is_dropped() {
    let mut e = TextEngine::new();
    e.measure(PARA, UI_FAMILY, 16.0, None);
    for _ in 0..5 {
        e.sweep();
        e.measure(PARA, UI_FAMILY, 16.0, None);
    }
    assert_eq!(e.shaped(), 1, "a label used every frame was shaped again");

    e.sweep();
    e.sweep();
    e.measure(PARA, UI_FAMILY, 16.0, None);
    assert_eq!(e.shaped(), 2, "an entry survived two frames unused");
}

/// C4: a cached layout paints exactly what a freshly shaped one does.
#[test]
fn a_cached_layout_paints_the_same_picture() {
    for rich in [false, true] {
        for width in [None, Some(120.0)] {
            let fresh = drawn(&mut TextEngine::new(), rich, width);
            let mut e = TextEngine::new();
            e.measure(PARA, UI_FAMILY, 16.0, width);
            e.measure_rich(PARA, &runs(PARA.len()), width);
            let (first, again) = (drawn(&mut e, rich, width), drawn(&mut e, rich, width));
            for scene in [&first, &again] {
                let (a, b) = (scene.encoding(), fresh.encoding());
                assert_eq!(a.draw_data, b.draw_data, "rich {rich}, width {width:?}");
                assert_eq!(a.path_data, b.path_data, "rich {rich}, width {width:?}");
                assert_eq!(a.n_paths, b.n_paths);
            }
        }
    }
}
