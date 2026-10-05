use super::*;

fn wanted(keys: &[&str]) -> Vec<WindowFrame> {
    keys.iter()
        .map(|k| WindowFrame {
            key: k.to_string(),
            title: k.to_string(),
            frame: TileFrame::default(),
        })
        .collect()
}

#[test]
fn offscreen_a_window_is_a_size_told_once_and_gone_when_unnamed() {
    let mut w = Windows::default();
    let told = w.sync(wanted(&["a"]), None, Some((900.0, 700.0)));
    assert!(matches!(&told[..], [(k, WindowIn::Resized((900.0, 700.0)))] if k == "a"), "{told:?}");
    assert!(w.sync(wanted(&["a"]), None, Some((900.0, 700.0))).is_empty(), "told once");
    let told = w.sync(wanted(&["a", "b"]), None, Some((900.0, 700.0)));
    assert!(matches!(&told[..], [(k, _)] if k == "b"), "{told:?}");
    w.sync(wanted(&["b"]), None, Some((900.0, 700.0)));
    assert_eq!(w.keys().collect::<Vec<_>>(), ["b"]);
}

#[test]
fn windowed_with_no_event_loop_a_window_waits() {
    let mut w = Windows::default();
    assert!(w.sync(wanted(&["a"]), None, None).is_empty());
    assert_eq!(w.keys().count(), 0);
}
