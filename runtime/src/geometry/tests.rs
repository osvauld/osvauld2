use super::*;

fn viewport() -> Clip {
    Clip {
        rect: Rect::new(0.0, 0.0, 300.0, 200.0),
        to_screen: Affine::IDENTITY,
    }
}

#[test]
fn panning_an_offscreen_node_into_the_viewport_makes_it_visible() {
    let node = Rect::new(320.0, 20.0, 420.0, 120.0);
    let geometry = Geometry::resolve(node, Affine::translate((-250.0, 0.0)), [viewport()]);

    assert_eq!(
        geometry.screen_rect,
        as_screen_rect(Rect::new(70.0, 20.0, 170.0, 120.0))
    );
    assert_eq!(geometry.visible_rect, Some(geometry.screen_rect));
    assert!(geometry.contains(Point::new(120.0, 70.0)));
}

#[test]
fn viewport_clip_rejects_the_hidden_part_after_pan() {
    let node = Rect::new(320.0, 20.0, 520.0, 120.0);
    let geometry = Geometry::resolve(node, Affine::translate((-100.0, 0.0)), [viewport()]);

    assert_eq!(
        geometry.screen_rect,
        as_screen_rect(Rect::new(220.0, 20.0, 420.0, 120.0))
    );
    assert_eq!(
        geometry.visible_rect,
        Some(as_screen_rect(Rect::new(220.0, 20.0, 300.0, 120.0)))
    );
    assert!(geometry.contains(Point::new(250.0, 70.0)));
    assert!(!geometry.contains(Point::new(350.0, 70.0)));
}

#[test]
fn pointer_maps_back_through_pan_and_zoom() {
    let transform = Affine::translate((-100.0, 30.0)) * Affine::scale(2.0);
    let geometry = Geometry::resolve(Rect::new(100.0, 10.0, 200.0, 60.0), transform, [viewport()]);

    let content = geometry.content_point(ScreenPoint::new(200.0, 90.0));
    let node = geometry.node_point(ScreenPoint::new(200.0, 90.0));
    assert!((content.x - 150.0).abs() < 0.001);
    assert!((content.y - 30.0).abs() < 0.001);
    assert!((node.x - 50.0).abs() < 0.001);
    assert!((node.y - 20.0).abs() < 0.001);
}

#[test]
fn each_clip_is_resolved_from_its_own_space() {
    let moving_clip = Clip {
        rect: Rect::new(300.0, 0.0, 500.0, 150.0),
        to_screen: Affine::translate((-250.0, 0.0)),
    };
    let geometry = Geometry::resolve(
        Rect::new(320.0, 20.0, 420.0, 120.0),
        moving_clip.to_screen,
        [viewport(), moving_clip],
    );

    assert_eq!(
        geometry.visible_rect,
        Some(as_screen_rect(Rect::new(70.0, 20.0, 170.0, 120.0)))
    );
}
