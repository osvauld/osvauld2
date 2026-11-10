use std::sync::Arc;

use kurbo::PathEl;
use peniko::{Color, Fill};

use super::*;
use crate::frame::{Brush, Path};

fn square() -> Item {
    let path = Path::new(vec![
        PathEl::MoveTo((0.0, 0.0).into()),
        PathEl::LineTo((4.0, 0.0).into()),
        PathEl::LineTo((4.0, 4.0).into()),
        PathEl::ClosePath,
    ])
    .unwrap();
    let brush = Brush::solid(Color::from_rgba8(255, 0, 0, 255)).unwrap();
    Item::fill(Arc::new(path), Arc::new(brush), Fill::NonZero)
}

fn part(id: &str, parent: Option<&str>, pivot: (f64, f64), shapes: Vec<Item>) -> PartSpec {
    PartSpec {
        id: id.into(),
        parent: parent.map(Into::into),
        pivot: pivot.into(),
        shapes,
    }
}

#[test]
fn rejects_duplicate_empty_and_unknown_ids() {
    let dup = vec![
        part("a", None, (0.0, 0.0), vec![]),
        part("a", None, (0.0, 0.0), vec![]),
    ];
    assert_eq!(
        Drawing::new(10.0, 10.0, dup).unwrap_err(),
        DrawingError::DuplicateId("a".into())
    );
    let empty = vec![part("", None, (0.0, 0.0), vec![])];
    assert_eq!(
        Drawing::new(10.0, 10.0, empty).unwrap_err(),
        DrawingError::EmptyId
    );
    let orphan = vec![part("a", Some("ghost"), (0.0, 0.0), vec![])];
    assert_eq!(
        Drawing::new(10.0, 10.0, orphan).unwrap_err(),
        DrawingError::UnknownParent {
            part: "a".into(),
            parent: "ghost".into()
        }
    );
}

#[test]
fn rejects_parent_cycles_including_self() {
    let pair = vec![
        part("a", Some("b"), (0.0, 0.0), vec![]),
        part("b", Some("a"), (0.0, 0.0), vec![]),
    ];
    assert!(matches!(
        Drawing::new(10.0, 10.0, pair),
        Err(DrawingError::Cycle(_))
    ));
    let own = vec![part("a", Some("a"), (0.0, 0.0), vec![])];
    assert_eq!(
        Drawing::new(10.0, 10.0, own).unwrap_err(),
        DrawingError::Cycle("a".into())
    );
}

#[test]
fn rest_pose_emits_named_groups_in_list_order_and_skips_groups() {
    let drawing = Drawing::new(
        10.0,
        10.0,
        vec![
            part("arm", Some("body"), (0.0, 0.0), vec![square()]), // child drawn first: behind
            part("hinge", Some("body"), (0.0, 0.0), vec![]),
            part("body", None, (0.0, 0.0), vec![square(), square()]),
        ],
    )
    .unwrap();
    let frame = drawing.pose(&HashMap::new()).unwrap();
    let ids: Vec<_> = frame
        .items()
        .iter()
        .map(|item| item.id().map(|id| id.to_string()))
        .collect();
    assert_eq!(ids, [Some("arm".into()), Some("body".into())]);
}

#[test]
fn a_rotated_parent_carries_its_child_about_the_parent_pivot() {
    // The parent is listed after the child: the hierarchy must not depend on list order.
    let drawing = Drawing::new(
        100.0,
        100.0,
        vec![
            part("hand", Some("arm"), (20.0, 10.0), vec![square()]),
            part("arm", None, (10.0, 10.0), vec![square()]),
        ],
    )
    .unwrap();
    let overrides = HashMap::from([(
        "arm",
        Pose {
            rot: 90.0,
            ..Pose::default()
        },
    )]);
    let world = drawing.part_at("hand", &overrides).unwrap();
    let p = world * Point::new(20.0, 10.0); // the hand's pivot, 10 right of the arm's
    assert!(
        (p.x - 10.0).abs() < 1e-9 && (p.y - 20.0).abs() < 1e-9,
        "{p:?}"
    );
    assert_eq!(drawing.part_at("ghost", &overrides), None);
}

#[test]
fn a_pose_naming_an_unknown_part_is_an_error() {
    let drawing = Drawing::new(
        10.0,
        10.0,
        vec![part("a", None, (0.0, 0.0), vec![square()])],
    )
    .unwrap();
    let overrides = HashMap::from([("ghost", Pose::default())]);
    assert_eq!(
        drawing.pose(&overrides).unwrap_err(),
        DrawingError::UnknownPart("ghost".into())
    );
}

fn close(p: Point, x: f64, y: f64) -> bool {
    (p.x - x).abs() < 1e-9 && (p.y - y).abs() < 1e-9
}

// U-C1
#[test]
fn scale_x_and_scale_y_act_along_the_parts_own_axes_after_its_rotation() {
    let drawing = Drawing::new(100.0, 100.0, vec![part("arm", None, (10.0, 10.0), vec![])]).unwrap();
    let at = |pose: Pose| drawing.part_at("arm", &HashMap::from([("arm", pose)])).unwrap();
    // Rotated a quarter turn, the part's own x points down the page.
    let turned = at(Pose { rot: 90.0, scale_x: 2.0, ..Pose::default() });
    assert!(close(turned * Point::new(11.0, 10.0), 10.0, 12.0), "{:?}", turned * Point::new(11.0, 10.0));
    assert!(close(turned * Point::new(10.0, 11.0), 9.0, 10.0), "its own y is not stretched");
    assert!(close(turned * Point::new(10.0, 10.0), 10.0, 10.0), "about the pivot");
    // `scale` scales both and multiplies them.
    let both = at(Pose { scale: 2.0, scale_x: 3.0, scale_y: 0.5, ..Pose::default() });
    assert!(close(both * Point::new(11.0, 11.0), 16.0, 11.0), "{:?}", both * Point::new(11.0, 11.0));
}

#[test]
fn old_poses_mean_what_they_meant() {
    assert_eq!((Pose::default().scale_x, Pose::default().scale_y), (1.0, 1.0));
    let drawing = Drawing::new(100.0, 100.0, vec![part("arm", None, (10.0, 20.0), vec![])]).unwrap();
    let pose = Pose { x: 3.0, y: -4.0, rot: 33.0, scale: 1.7, ..Pose::default() };
    let pivot = kurbo::Vec2::new(10.0, 20.0);
    let before = Affine::translate((3.0, -4.0))
        * Affine::translate(pivot)
        * Affine::rotate(33f64.to_radians())
        * Affine::scale(1.7)
        * Affine::translate(-pivot);
    let now = drawing.part_at("arm", &HashMap::from([("arm", pose)])).unwrap();
    assert_eq!(now.as_coeffs().map(f64::to_bits), before.as_coeffs().map(f64::to_bits));
}

#[test]
fn a_child_of_an_unequally_scaled_parent_shears() {
    let drawing = Drawing::new(
        100.0,
        100.0,
        vec![
            part("hand", Some("arm"), (0.0, 0.0), vec![]),
            part("arm", None, (0.0, 0.0), vec![]),
        ],
    )
    .unwrap();
    let poses = HashMap::from([
        ("arm", Pose { scale_x: 2.0, ..Pose::default() }),
        ("hand", Pose { rot: 45.0, ..Pose::default() }),
    ]);
    let hand = drawing.part_at("hand", &poses).unwrap();
    let arm = drawing.part_at("arm", &poses).unwrap();
    let local = Affine::rotate(45f64.to_radians());
    assert_eq!(hand.as_coeffs(), (arm * local).as_coeffs(), "a plain affine product");
    let [a, b, c, d, _, _] = hand.as_coeffs();
    assert!((a * c + b * d).abs() > 1.0, "its axes are no longer square: {:?}", hand.as_coeffs());
}

// U-C4 (the drawing half): hiding a part hides its own shapes, not its children.
#[test]
fn a_hidden_part_paints_nothing_and_is_never_hit_but_its_children_still_follow_it() {
    let drawing = Drawing::new(
        100.0,
        100.0,
        vec![
            part("arm", None, (0.0, 0.0), vec![square()]),
            part("hand", Some("arm"), (0.0, 0.0), vec![square()]),
        ],
    )
    .unwrap();
    let poses = HashMap::from([
        ("arm", Pose { x: 50.0, show: false, ..Pose::default() }),
        ("hand", Pose { y: 10.0, ..Pose::default() }),
    ]);
    let frame = drawing.pose(&poses).unwrap();
    let ids: Vec<_> = frame.items().iter().map(|i| i.id().unwrap().to_string()).collect();
    assert_eq!(ids, ["hand"], "only the child is emitted");
    assert!(frame.hit(Point::new(53.0, 1.0)).is_none(), "nothing where the hidden arm is");
    let hand = frame.hit(Point::new(53.0, 11.0)).expect("the hand followed the hidden arm");
    assert_eq!(hand.id.to_string(), "hand");
    assert!(Pose::default().show, "parts are shown by default");
    let at = drawing.part_at("arm", &poses).unwrap();
    assert!(close(at * Point::ZERO, 50.0, 0.0), "a hidden part still has a place");
}
