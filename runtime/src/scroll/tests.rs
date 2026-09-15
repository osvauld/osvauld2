use super::*;

#[test]
fn thumb_follows_pan_and_zoom_and_keeps_local_drag_gain() {
    let local = Rect::new(100.0, 20.0, 300.0, 420.0);
    let transform = Affine::translate((-30.0, 10.0)) * Affine::scale(0.5);
    let thumb = axis_thumb(local, &Id::from("cards"), Axis::Y, 400.0, 800.0, 200.0).unwrap();
    let local_gain = thumb.gain;
    let screen = thumb.to_screen(transform, Rect::new(0.0, 0.0, 500.0, 500.0));

    assert_eq!(screen.rect, Rect::new(115.0, 70.0, 119.0, 170.0));
    assert!((screen.gain - local_gain * 2.0).abs() < 0.001);
}

#[test]
fn hidden_thumb_area_cannot_be_grabbed() {
    let local = Rect::new(100.0, 20.0, 300.0, 420.0);
    let thumb = axis_thumb(local, &Id::from("cards"), Axis::Y, 400.0, 800.0, 0.0)
        .unwrap()
        .to_screen(Affine::IDENTITY, Rect::new(0.0, 100.0, 500.0, 500.0));

    assert_eq!(thumb.hit_rect.y0, 100.0);
    assert!(!thumb.hit_rect.contains((thumb.rect.x0, 50.0)));
}
