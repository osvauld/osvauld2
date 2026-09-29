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
        facing: Facings::default(),
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

fn wasd(speed: f64, moving: Option<Arc<Clip>>) -> Controller {
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
        moving,
    }
}

fn controlled(d: &Arc<Drawing>, controller: Controller) -> EntitySpec {
    EntitySpec {
        controller: Some(controller),
        ..spec("hero", (0.0, 0.0), d)
    }
}

fn now_playing(world: &World2d) -> (Arc<Clip>, f64) {
    let a = world.ecs.get::<Animator>(world.by_id["hero"]).unwrap();
    (a.clip.clone(), a.started)
}

#[test]
fn a_controller_moves_along_held_axes_and_a_diagonal_is_no_faster() {
    let d = drawing();
    let mut world = World2d::default();
    world
        .reconcile(vec![controlled(&d, wasd(100.0, None))])
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
fn the_moving_clip_plays_only_while_moving_and_survives_reconcile() {
    let (d, walk, idle) = (drawing(), slide(), slide());
    let describe = |world: &mut World2d| {
        let hero = EntitySpec {
            clip: Some(idle.clone()),
            ..controlled(&d, wasd(10.0, Some(walk.clone())))
        };
        world.reconcile(vec![hero]).unwrap();
    };
    let mut world = World2d::default();
    describe(&mut world);
    assert!(Arc::ptr_eq(&now_playing(&world).0, &idle));

    world.key("KeyD", true);
    world.tick(1.0, 0.1);
    assert!(Arc::ptr_eq(&now_playing(&world).0, &walk));
    describe(&mut world);
    world.tick(2.0, 0.1);
    let (clip, started) = now_playing(&world);
    assert!(
        Arc::ptr_eq(&clip, &walk),
        "a reconcile does not stop the walk"
    );
    assert_eq!(started, 1.0, "nor restart it");

    world.key("KeyD", false);
    world.tick(3.0, 0.1);
    let (clip, started) = now_playing(&world);
    assert!(Arc::ptr_eq(&clip, &idle));
    assert_eq!(started, 3.0);
}

#[test]
fn a_bad_controller_is_refused() {
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
    let flap = Arc::new(Clip::new(1.0, true, vec![track]).unwrap());
    let d = drawing();
    let mut world = World2d::default();
    for speed in [f64::NAN, -1.0] {
        assert!(matches!(
            world.reconcile(vec![controlled(&d, wasd(speed, None))]),
            Err(WorldError::Speed(_))
        ));
    }
    assert!(matches!(
        world.reconcile(vec![controlled(&d, wasd(1.0, Some(flap)))]),
        Err(WorldError::Drawing(DrawingError::UnknownPart(_)))
    ));
    assert!(!world.needs_ticks(), "nothing was accepted");
}

fn view(drawing: &Arc<Drawing>, moving: Option<&Arc<Clip>>) -> Option<facing::View> {
    Some(facing::View {
        drawing: Some(drawing.clone()),
        clip: None,
        moving: moving.cloned(),
    })
}

/// Holds `code` for one zero-length tick — it turns without moving — then lets go.
fn turn(world: &mut World2d, code: &str) {
    world.key(code, true);
    world.tick(0.0, 0.0);
    world.key(code, false);
}

#[test]
fn left_mirrors_the_side_view_and_stopping_keeps_the_facing() {
    let d = drawing();
    let hero = EntitySpec {
        facing: Facings {
            side: view(&d, None),
            ..Facings::default()
        },
        ..controlled(&d, wasd(10.0, None))
    };
    let mut world = World2d::default();
    world.reconcile(vec![hero]).unwrap();
    assert_eq!(
        (at(&world, 6.0).is_some(), at(&world, 2.0).is_some()),
        (true, false)
    );

    turn(&mut world, "KeyA");
    world.tick(0.0, 0.0);
    assert_eq!(
        world.facing("hero"),
        Some(facing::Dir::Left),
        "kept after stopping"
    );
    assert_eq!(
        (at(&world, 6.0).is_some(), at(&world, 2.0).is_some()),
        (false, true)
    );
    turn(&mut world, "KeyD");
    assert_eq!(
        (at(&world, 6.0).is_some(), at(&world, 2.0).is_some()),
        (true, false)
    );
}

#[test]
fn a_facing_brings_its_own_drawing_and_moving_clip() {
    let (front, back, walk, walk_up) = (drawing(), drawing(), slide(), slide());
    let hero = EntitySpec {
        facing: Facings {
            up: view(&back, Some(&walk_up)),
            ..Facings::default()
        },
        ..controlled(&front, wasd(10.0, Some(walk.clone())))
    };
    let mut world = World2d::default();
    world.reconcile(vec![hero]).unwrap();
    let showing = |world: &World2d| {
        let e = world.ecs.entity(world.by_id["hero"]);
        let look = e.get::<Appearance>().unwrap();
        look.shown(*e.get::<facing::Dir>().unwrap(), e.get::<Controller>())
            .drawing
            .clone()
    };
    world.key("KeyW", true);
    world.tick(1.0, 0.1);
    assert!(Arc::ptr_eq(&showing(&world), &back));
    assert!(Arc::ptr_eq(&now_playing(&world).0, &walk_up));
    world.key("KeyW", false);
    world.key("KeyS", true);
    world.tick(2.0, 0.1);
    assert!(Arc::ptr_eq(&showing(&world), &front));
    assert!(Arc::ptr_eq(&now_playing(&world).0, &walk));
}

#[test]
fn every_facing_is_checked_against_the_clips_it_plays() {
    let (front, back) = (drawing(), drawing_of("torso"));
    let hero = EntitySpec {
        clip: Some(slide()), // moves `body`, which the back view lacks
        facing: Facings {
            up: view(&back, None),
            ..Facings::default()
        },
        ..spec("hero", (0.0, 0.0), &front)
    };
    let mut world = World2d::default();
    assert!(matches!(
        world.reconcile(vec![hero]),
        Err(WorldError::Drawing(DrawingError::UnknownPart(_)))
    ));
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
        .reconcile(vec![controlled(&d, wasd(100.0, None))])
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
