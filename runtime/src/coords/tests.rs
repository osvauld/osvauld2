use super::*;

fn camera() -> CameraMap {
    CameraMap::new(
        ScreenPoint::new(100.0, 50.0),
        ViewportVector::new(-20.0, 10.0),
        0.5,
    )
}

#[test]
fn camera_round_trips_between_content_and_screen() {
    let content = ContentPoint::new(200.0, 80.0);
    let screen = camera().content_to_screen(content);

    assert_eq!(screen, ScreenPoint::new(180.0, 100.0));
    assert_eq!(camera().screen_to_content(screen), content);
}

#[test]
fn vectors_ignore_the_cameras_translation() {
    let content = camera().screen_vector_to_content(ScreenVector::new(10.0, -5.0));

    assert_eq!(content, ContentVector::new(20.0, -10.0));
}

#[test]
fn typed_camera_converts_to_kurbo_at_the_render_boundary() {
    let affine = camera().to_affine();
    let screen = affine * kurbo::Point::new(200.0, 80.0);

    assert_eq!(screen, kurbo::Point::new(180.0, 100.0));
}
