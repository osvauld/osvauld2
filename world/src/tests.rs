use std::sync::Arc;

use runtime::drawing::{Drawing, PartSpec};
use runtime::frame::{Brush, Item, Path};
use runtime::vello::kurbo::{PathEl, Point};
use runtime::vello::peniko::{Color, Fill};

use super::*;

fn drawing() -> Arc<Drawing> {
    drawing_of("body")
}

/// A right triangle filling the box above its diagonal: on the row y = 4 it covers x 4..8, so a
/// hit there tells whether it is mirrored.
fn drawing_of(part: &str) -> Arc<Drawing> {
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
        id: part.into(),
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
        clip: None,
        controller: None,
        flip: false,
        attach: None,
        collider: None,
    }
}

/// Slides the drawing's `body` part 100 right over one second, then holds.
fn slide() -> Arc<Clip> {
    use crate::clip::{Easing, Key, Prop, Track};
    let key = |time, value| Key {
        time,
        value,
        easing: Easing::Linear,
    };
    let track = Track {
        part: "body".into(),
        prop: Prop::X,
        keys: vec![key(0.0, 0.0), key(1.0, 100.0)],
    };
    Arc::new(Clip::new(1.0, false, vec![track]).unwrap())
}

fn playing(id: &str, drawing: &Arc<Drawing>, clip: &Arc<Clip>) -> EntitySpec {
    EntitySpec {
        clip: Some(clip.clone()),
        ..spec(id, (0.0, 0.0), drawing)
    }
}

/// Which entity is at `x` along the 8px-tall row the test drawing sits on.
fn at(world: &World2d, x: f64) -> Option<String> {
    let hit = world.frame(200.0, 100.0).unwrap().hit(Point::new(x, 4.0))?;
    Some(hit.id.to_string())
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
        &world.ecs.get::<Appearance>(entity).unwrap().drawing,
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

#[test]
fn a_clip_plays_on_the_world_clock_from_when_it_appears() {
    let (d, clip) = (drawing(), slide());
    let mut world = World2d::default();
    world.tick(10.0, 0.0);
    world.reconcile(vec![playing("hero", &d, &clip)]).unwrap();
    assert_eq!(
        at(&world, 4.0).as_deref(),
        Some("hero"),
        "starts at its first key"
    );
    world.tick(10.5, 0.0);
    world.reconcile(vec![playing("hero", &d, &clip)]).unwrap();
    assert_eq!(
        at(&world, 4.0),
        None,
        "the same handle keeps playing, not restarting"
    );
    assert_eq!(at(&world, 54.0).as_deref(), Some("hero"));
}

#[test]
fn a_new_clip_handle_restarts_and_no_clip_returns_to_rest() {
    let d = drawing();
    let mut world = World2d::default();
    world
        .reconcile(vec![playing("hero", &d, &slide())])
        .unwrap();
    world.tick(0.5, 0.0);
    world
        .reconcile(vec![playing("hero", &d, &slide())])
        .unwrap();
    assert_eq!(at(&world, 4.0).as_deref(), Some("hero"), "restarted at 0.5");
    world.tick(2.0, 0.0);
    world.reconcile(vec![spec("hero", (0.0, 0.0), &d)]).unwrap();
    assert_eq!(at(&world, 4.0).as_deref(), Some("hero"), "back at rest");
}

#[test]
fn a_clip_naming_a_part_its_drawing_lacks_is_refused() {
    use crate::clip::{Easing, Key, Prop, Track};
    let key = Key {
        time: 0.0,
        value: 1.0,
        easing: Easing::Linear,
    };
    let track = Track {
        part: "wing".into(),
        prop: Prop::Rot,
        keys: vec![key],
    };
    let wing = Arc::new(Clip::new(1.0, true, vec![track]).unwrap());
    let d = drawing();
    let mut world = World2d::default();
    world.reconcile(vec![spec("hero", (0.0, 0.0), &d)]).unwrap();
    assert!(matches!(
        world.reconcile(vec![playing("hero", &d, &wing)]),
        Err(WorldError::Drawing(DrawingError::UnknownPart(_)))
    ));
    assert_eq!(
        at(&world, 4.0).as_deref(),
        Some("hero"),
        "last good world kept"
    );
}

fn wasd(speed: f64) -> Controller {
    let axis = |neg: &str, pos: &str| {
        Some(Axis {
            neg: neg.into(),
            pos: pos.into(),
        })
    };
    Controller {
        speed,
        axis_x: axis("KeyA", "KeyD"),
        axis_y: axis("KeyW", "KeyS"),
    }
}

fn controlled(d: &Arc<Drawing>, controller: Controller) -> EntitySpec {
    EntitySpec {
        controller: Some(controller),
        ..spec("hero", (0.0, 0.0), d)
    }
}

#[test]
fn a_controller_moves_along_held_axes_and_a_diagonal_is_no_faster() {
    let d = drawing();
    let mut world = World2d::default();
    world
        .reconcile(vec![controlled(&d, wasd(100.0))])
        .unwrap();
    assert!(world.needs_ticks());
    world.tick(0.0, 0.5);
    assert_eq!(world.transform("hero").unwrap().x, 0.0, "no key held");

    world.key("KeyD", true);
    world.tick(0.5, 0.5);
    assert_eq!(world.transform("hero").unwrap().x, 50.0);
    world.key("KeyA", true);
    world.tick(1.0, 0.5);
    assert_eq!(
        world.transform("hero").unwrap().x,
        50.0,
        "opposite keys cancel"
    );

    world.key("KeyA", false);
    world.key("KeyS", true);
    world.tick(1.5, 0.5);
    let t = world.transform("hero").unwrap();
    let step = ((t.x - 50.0).powi(2) + t.y.powi(2)).sqrt();
    assert!((step - 50.0).abs() < 1e-9, "diagonal stepped {step}");

    world.release_all();
    world.tick(2.0, 0.5);
    assert_eq!(
        world.transform("hero").unwrap(),
        t,
        "focus lost, nothing held"
    );
}

#[test]
fn a_bad_controller_is_refused() {
    let d = drawing();
    let mut world = World2d::default();
    for speed in [f64::NAN, -1.0] {
        assert!(matches!(
            world.reconcile(vec![controlled(&d, wasd(speed))]),
            Err(WorldError::Speed(_))
        ));
    }
    assert!(!world.needs_ticks(), "nothing was accepted");
}

#[test]
fn flip_mirrors_the_drawing_within_its_box() {
    let d = drawing();
    let mut world = World2d::default();
    world.reconcile(vec![spec("hero", (0.0, 0.0), &d)]).unwrap();
    assert_eq!(
        (at(&world, 6.0).is_some(), at(&world, 2.0).is_some()),
        (true, false)
    );
    let flipped = EntitySpec {
        flip: true,
        ..spec("hero", (0.0, 0.0), &d)
    };
    world.reconcile(vec![flipped]).unwrap();
    assert_eq!(
        (at(&world, 6.0).is_some(), at(&world, 2.0).is_some()),
        (false, true)
    );
}

#[test]
fn feet_order_draws_the_lower_entity_in_front_and_ties_keep_list_order() {
    let d = drawing();
    let mut world = World2d::default();
    world
        .reconcile(vec![
            spec("low", (0.0, 5.0), &d),
            spec("high", (20.0, 0.0), &d),
            spec("level", (40.0, 0.0), &d),
        ])
        .unwrap();
    assert_eq!(ids(&world), ["low", "high", "level"]);
    world.set_order(Order::Feet);
    assert_eq!(ids(&world), ["high", "level", "low"]);
}

#[test]
fn a_move_event_marks_each_change_of_held_direction_not_each_frame() {
    let d = drawing();
    let mut world = World2d::default();
    world
        .reconcile(vec![controlled(&d, wasd(100.0))])
        .unwrap();
    let moved = |dx, dy| WorldEvent::Move {
        id: "hero".into(),
        dx,
        dy,
    };
    world.tick(0.0, 0.1);
    assert_eq!(world.drain_events(), [], "standing still at spawn is not news");

    world.key("KeyD", true);
    world.tick(0.1, 0.1);
    world.tick(0.2, 0.1);
    assert_eq!(world.drain_events(), [moved(1, 0)], "the start, not every frame");
    world.key("KeyW", true);
    world.tick(0.3, 0.1);
    world.key("KeyD", false);
    world.key("KeyW", false);
    world.tick(0.4, 0.1);
    assert_eq!(
        world.drain_events(),
        [moved(1, -1), moved(0, 0)],
        "a turn, then the stop"
    );
}

#[test]
fn an_action_fires_on_a_fresh_press_only() {
    let mut world = World2d::default();
    assert!(!world.wants_keys());
    world.set_actions(vec![("jump".into(), "Space".into())]);
    assert!(world.wants_keys(), "actions alone take keys");
    world.key("Space", true);
    world.key("Space", true); // a repeat while held
    world.key("KeyQ", true);
    let jump = || WorldEvent::Action("jump".into());
    assert_eq!(world.drain_events(), [jump()]);
    world.key("Space", false);
    world.key("Space", true);
    assert_eq!(world.drain_events(), [jump()], "pressed again");
}

#[test]
fn a_once_clip_reports_its_end_once_per_play() {
    let (d, once) = (drawing(), slide()); // one second, then holds
    let mut world = World2d::default();
    world.reconcile(vec![playing("hero", &d, &once)]).unwrap();
    world.tick(0.5, 0.1);
    assert_eq!(world.drain_events(), [], "still playing");
    world.tick(1.0, 0.1);
    world.tick(1.5, 0.1);
    let ended = || WorldEvent::ClipEnd("hero".into());
    assert_eq!(world.drain_events(), [ended()], "once, then it holds quietly");

    world.reconcile(vec![playing("hero", &d, &once)]).unwrap();
    world.tick(2.0, 0.1);
    assert_eq!(world.drain_events(), [], "the same handle is the same play");
    world.reconcile(vec![playing("hero", &d, &slide())]).unwrap();
    world.tick(3.0, 0.1);
    assert_eq!(world.drain_events(), [ended()], "a new handle is a new play");
}

fn carried(on: &str, part: &str, d: &Arc<Drawing>) -> EntitySpec {
    EntitySpec {
        attach: Some(Attach {
            to: on.into(),
            part: part.into(),
            at: (2.0, -5.0),
        }),
        ..spec("chest", (300.0, 300.0), d)
    }
}

#[test]
fn a_carried_entity_rides_its_carriers_part_and_stays_where_dropped() {
    let (d, glide) = (drawing(), slide());
    let hero = EntitySpec {
        clip: Some(glide.clone()), // moves `body` 100 right over a second
        ..controlled(&d, wasd(100.0))
    };
    let mut world = World2d::default();
    world.set_order(Order::Feet);
    world
        .reconcile(vec![carried("hero", "body", &d), hero])
        .unwrap();
    let at = |world: &World2d| {
        let t = world.transform("chest").unwrap();
        (t.x, t.y)
    };
    assert_eq!(at(&world), (2.0, -5.0), "placed at once, not at its pos");
    assert_eq!(
        ids(&world),
        ["hero", "chest"],
        "in front of its carrier, though its own feet are higher"
    );

    world.key("KeyD", true);
    world.tick(0.5, 0.5);
    assert_eq!(at(&world), (102.0, -5.0), "the hero walked 50, the body slid 50");

    let hero = EntitySpec {
        clip: Some(glide),
        ..controlled(&d, wasd(100.0))
    };
    world
        .reconcile(vec![spec("chest", (0.0, 0.0), &d), hero])
        .unwrap();
    world.tick(1.0, 0.5);
    assert_eq!(at(&world), (102.0, -5.0), "dropped where it was carried");
}

#[test]
fn a_flipped_carrier_mirrors_the_carried_box_not_just_its_point() {
    let d = drawing(); // 8 wide
    let hero = EntitySpec {
        flip: true,
        ..spec("hero", (0.0, 0.0), &d)
    };
    let mut world = World2d::default();
    world
        .reconcile(vec![hero, carried("hero", "body", &d)])
        .unwrap();
    // `at` x = 2 mirrors to 6, which is now the chest's right edge: 6 - 8.
    assert_eq!(world.transform("chest").unwrap().x, -2.0);
}

#[test]
fn a_bad_attach_is_refused() {
    let d = drawing();
    let mut world = World2d::default();
    let hero = || spec("hero", (0.0, 0.0), &d);
    let cases = [
        (vec![carried("ghost", "body", &d)], "which the world does not describe"),
        (vec![carried("chest", "body", &d)], "attached to itself"),
        (vec![carried("hero", "wing", &d), hero()], "\"hero\" has no part \"wing\""),
        (
            vec![carried("hero", "body", &d), carried("chest", "body", &d)],
            "duplicate",
        ),
    ];
    for (specs, wanted) in cases {
        let err = world.reconcile(specs).unwrap_err().to_string();
        assert!(err.contains(wanted), "wanted {wanted:?}, got {err}");
    }
    let mut rider = carried("hero", "body", &d);
    rider.id = "rider".into();
    let chain = vec![hero(), carried("rider", "body", &d), rider];
    let err = world.reconcile(chain).unwrap_err().to_string();
    assert!(err.contains("which is itself attached"), "{err}");
}

fn solid(id: &str, pos: (f64, f64), shape: Shape, at: (f64, f64), d: &Arc<Drawing>) -> EntitySpec {
    EntitySpec {
        collider: Some(Collider { shape, at }),
        ..spec(id, pos, d)
    }
}

#[test]
fn a_collider_becomes_a_body_placed_by_its_centre() {
    let d = drawing();
    let mut world = World2d::default();
    let wall = solid("wall", (10.0, 20.0), Shape::Rect(100.0, 12.0), (0.0, 0.0), &d);
    let hero = solid("hero", (0.0, 0.0), Shape::Circle(3.0), (4.0, 8.0), &d);
    world.reconcile(vec![wall, hero, spec("ghost", (0.0, 0.0), &d)]).unwrap();
    // A rect's `at` is its corner, a circle's its centre; an entity without a collider has none.
    assert_eq!(world.physics.centres(), [(4.0, 8.0), (60.0, 26.0)]);
}

#[test]
fn a_body_is_kept_while_its_collider_is_and_goes_with_it() {
    let d = drawing();
    let mut world = World2d::default();
    let wall = || solid("wall", (0.0, 0.0), Shape::Rect(4.0, 4.0), (0.0, 0.0), &d);
    let body = |w: &World2d| w.ecs.get::<Solid>(w.by_id["wall"]).map(|s| s.body);
    world.reconcile(vec![wall()]).unwrap();
    let first = body(&world);
    world.reconcile(vec![wall()]).unwrap();
    assert_eq!(body(&world), first, "the same collider keeps its body");
    let wider = solid("wall", (0.0, 0.0), Shape::Rect(8.0, 4.0), (0.0, 0.0), &d);
    world.reconcile(vec![wider]).unwrap();
    assert_eq!(world.physics.centres(), [(4.0, 2.0)], "a changed collider replaces its body");
    world.reconcile(vec![spec("wall", (0.0, 0.0), &d)]).unwrap();
    assert_eq!(world.physics.centres(), [], "dropping the collider drops the body");
    world.reconcile(vec![wall()]).unwrap();
    world.reconcile(vec![]).unwrap();
    assert_eq!(world.physics.centres(), [], "despawning drops the body");
}

#[test]
fn a_bad_collider_is_refused() {
    let d = drawing();
    let mut world = World2d::default();
    let cases = [
        (Shape::Circle(0.0), (0.0, 0.0), "above zero"),
        (Shape::Rect(4.0, f64::NAN), (0.0, 0.0), "above zero"),
        (Shape::Circle(2.0), (f64::INFINITY, 0.0), "at must be finite"),
    ];
    for (shape, at, wanted) in cases {
        let err = world
            .reconcile(vec![solid("wall", (0.0, 0.0), shape, at, &d)])
            .unwrap_err()
            .to_string();
        assert!(err.contains(wanted), "wanted {wanted:?}, got {err}");
    }
    assert_eq!(world.physics.centres(), [], "nothing was applied");
}
