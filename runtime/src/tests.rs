use super::*;
use crate::frame::{Brush, Frame, Item, Path};
use vello::kurbo::PathEl;
use vello::peniko::Fill;

fn square(size: f64) -> Arc<Path> {
    Arc::new(
        Path::new(vec![
            PathEl::MoveTo((0.0, 0.0).into()),
            PathEl::LineTo((size, 0.0).into()),
            PathEl::LineTo((size, size).into()),
            PathEl::LineTo((0.0, size).into()),
            PathEl::ClosePath,
        ])
        .unwrap(),
    )
}

/// A visual is painted at its element's *content* origin, so its own coordinates start inside the
/// padding. Pointer events are element-local, from the border box — the difference is the padding,
/// and forgetting it would shift every hit by it.
#[test]
fn a_visuals_coordinates_start_inside_the_padding() {
    let brush = Arc::new(Brush::solid(vello::peniko::Color::WHITE).unwrap());
    let frame = Arc::new(
        Frame::new(
            10.0,
            10.0,
            None,
            vec![Item::fill(square(10.0), brush, Fill::NonZero).with_id("face")],
        )
        .unwrap(),
    );
    let shapes = Shapes {
        frame,
        origin: (6.0, 4.0),
    };

    let hit = shapes.at(NodePoint::new(7.0, 5.0)).unwrap();
    assert_eq!(&*hit.id, "face");
    assert_eq!(hit.local, Point::new(1.0, 1.0));
    assert_eq!(shapes.at(NodePoint::new(3.0, 3.0)), None); // still in the padding
}

/// A drag reports the shape it *grabbed*, in that shape's coordinates, and keeps doing so after
/// the pointer has left it — which is what holding something means. The transform comes back with
/// the hit precisely so a later point can be put in the same space.
#[test]
fn a_grab_keeps_reporting_in_the_shape_it_took_hold_of() {
    // A quarter turn: the shape's +x is the frame's +y, so nothing about these two spaces is
    // interchangeable by accident.
    let turn = vello::kurbo::Affine::new([0.0, 1.0, -1.0, 0.0, 0.0, 0.0]);
    let brush = Arc::new(Brush::solid(vello::peniko::Color::WHITE).unwrap());
    let knob = Item::fill(square(10.0), brush, Fill::NonZero).with_id("knob");
    let frame = Frame::new(
        100.0,
        100.0,
        None,
        vec![Item::group(turn, vec![knob]).unwrap()],
    )
    .unwrap();
    let shapes = Some(Shapes {
        frame: Arc::new(frame),
        origin: (0.0, 0.0),
    });

    let grab = Grabbed::take(&shapes, NodePoint::new(-5.0, 5.0)).unwrap();
    assert_eq!(&*grab.id, "knob");

    let (id, at) = grab.at(NodePoint::new(-50.0, 5.0)); // dragged well outside the shape
    assert_eq!(&*id, "knob");
    assert_eq!(at, (5.0, 50.0));

    assert!(Grabbed::take(&shapes, NodePoint::new(50.0, 5.0)).is_none()); // pressed on nothing
}

