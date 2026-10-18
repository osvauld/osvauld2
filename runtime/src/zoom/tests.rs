use super::*;

#[test]
fn zoom_keeps_the_content_point_under_the_pointer() {
    let rect = Rect::new(100.0, 50.0, 900.0, 650.0);
    let pointer = Point::new(420.0, 260.0);
    let mut zoom = Zoom {
        scale: 1.0,
        pan: (-40.0, 25.0),
        ..Zoom::default()
    };
    let before = CameraMap::new(
        ScreenPoint::new(rect.x0, rect.y0),
        ViewportVector::new(zoom.pan.0 as f64, zoom.pan.1 as f64),
        zoom.scale as f64,
    )
    .screen_to_content(ScreenPoint::new(pointer.x, pointer.y));

    zoom.at(rect, (1600.0, 1200.0), pointer, 2.0);

    let after = CameraMap::new(
        ScreenPoint::new(rect.x0, rect.y0),
        ViewportVector::new(zoom.pan.0 as f64, zoom.pan.1 as f64),
        zoom.scale as f64,
    )
    .screen_to_content(ScreenPoint::new(pointer.x, pointer.y));
    assert!((after.x - before.x).abs() < 0.0001);
    assert!((after.y - before.y).abs() < 0.0001);
}

/// Zooming stops at the element's range: the default stops at 3×, a wider one goes on.
#[test]
fn zoom_stops_at_its_range() {
    let rect = Rect::new(0.0, 0.0, 800.0, 600.0);
    let pointer = Point::new(400.0, 300.0);
    let zoom_in = |range| {
        let mut zoom = Zoom { range, ..Zoom::default() };
        for _ in 0..100 {
            zoom.at(rect, (800.0, 600.0), pointer, 1.0);
        }
        zoom.scale
    };
    assert_eq!(zoom_in(DEFAULT_RANGE), 3.0);
    assert_eq!(zoom_in((0.4, 40.0)), 40.0);
    assert_eq!(zoom_in((5.0, 2.0)), 2.0, "a reversed range stops at its max, without a panic");
}
