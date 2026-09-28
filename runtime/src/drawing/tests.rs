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
    let world = drawing.world(0, &overrides, &mut [None, None]);
    let p = world * Point::new(20.0, 10.0); // the hand's pivot, 10 right of the arm's
    assert!(
        (p.x - 10.0).abs() < 1e-9 && (p.y - 20.0).abs() < 1e-9,
        "{p:?}"
    );
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
