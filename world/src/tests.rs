use std::sync::Arc;

use runtime::drawing::{Drawing, PartSpec};
use runtime::frame::{Brush, Item, Path};
use runtime::vello::kurbo::{PathEl, Point};
use runtime::vello::peniko::{Color, Fill};

use super::*;

fn drawing() -> Arc<Drawing> {
    let path = Path::new(vec![
        PathEl::MoveTo((0.0, 0.0).into()),
        PathEl::LineTo((8.0, 0.0).into()),
        PathEl::LineTo((8.0, 8.0).into()),
        PathEl::ClosePath,
    ])
    .unwrap();
    let brush = Brush::solid(Color::from_rgba8(0, 0, 255, 255)).unwrap();
    let shape = Item::fill(Arc::new(path), Arc::new(brush), Fill::NonZero);
    let part = PartSpec {
        id: "body".into(),
        parent: None,
        pivot: Point::ZERO,
        shapes: vec![shape],
    };
    Arc::new(Drawing::new(8.0, 8.0, vec![part]).unwrap())
}

fn spec(id: &str, pos: (f64, f64), drawing: &Arc<Drawing>) -> EntitySpec {
    EntitySpec {
        id: id.into(),
        pos,
        drawing: drawing.clone(),
    }
}

fn ids(world: &World2d) -> Vec<String> {
    let frame = world.frame(100.0, 100.0).unwrap();
    frame
        .items()
        .iter()
        .map(|i| i.id().unwrap().to_string())
        .collect()
}

#[test]
fn reconcile_spawns_keeps_and_despawns_by_id_in_list_order() {
    let d = drawing();
    let mut world = World2d::default();
    world
        .reconcile(vec![
            spec("hero", (10.0, 20.0), &d),
            spec("chest", (50.0, 50.0), &d),
        ])
        .unwrap();
    assert_eq!(ids(&world), ["hero", "chest"]);

    world
        .reconcile(vec![
            spec("chest", (50.0, 50.0), &d),
            spec("hero", (10.0, 20.0), &d),
        ])
        .unwrap();
    assert_eq!(ids(&world), ["chest", "hero"]); // reordered, not respawned

    world
        .reconcile(vec![spec("hero", (10.0, 20.0), &d)])
        .unwrap();
    assert_eq!(ids(&world), ["hero"]);
    assert!(world.transform("chest").is_none());
    // Gone from the ECS, not just hidden. `entities().len()` counts allocated slots, not the living.
    let living = world.ecs.query::<&Name>().iter(&world.ecs).count();
    assert_eq!(living, 1);
}

#[test]
fn pos_is_read_only_at_spawn() {
    let d = drawing();
    let mut world = World2d::default();
    world
        .reconcile(vec![spec("hero", (10.0, 20.0), &d)])
        .unwrap();
    world
        .reconcile(vec![spec("hero", (99.0, 99.0), &d)])
        .unwrap();
    let t = world.transform("hero").unwrap();
    assert_eq!((t.x, t.y), (10.0, 20.0));
}

#[test]
fn a_new_drawing_handle_replaces_the_look() {
    let (a, b) = (drawing(), drawing());
    let mut world = World2d::default();
    world.reconcile(vec![spec("hero", (0.0, 0.0), &a)]).unwrap();
    let entity = world.by_id["hero"];
    world.reconcile(vec![spec("hero", (0.0, 0.0), &b)]).unwrap();
    assert_eq!(world.by_id["hero"], entity, "same entity, new look");
    assert!(Arc::ptr_eq(
        &world.ecs.get::<Look>(entity).unwrap().drawing,
        &b
    ));
}

#[test]
fn a_bad_description_leaves_the_last_good_world() {
    let d = drawing();
    let mut world = World2d::default();
    world.reconcile(vec![spec("hero", (0.0, 0.0), &d)]).unwrap();
    let dup = vec![spec("chest", (0.0, 0.0), &d), spec("chest", (1.0, 1.0), &d)];
    assert!(matches!(
        world.reconcile(dup),
        Err(WorldError::DuplicateId(_))
    ));
    assert_eq!(ids(&world), ["hero"]);
    assert!(matches!(
        world.reconcile(vec![spec("", (0.0, 0.0), &d)]),
        Err(WorldError::EmptyId)
    ));
}
