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
        sensor: None,
        loose: None,
        group: None,
        blocks: None,
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
fn a_clip_track_for_a_part_the_drawing_lacks_is_skipped_and_noted_once() {
    use crate::clip::{Easing, Key, Prop, Track};
    let key = |time, value| Key {
        time,
        value,
        easing: Easing::Linear,
    };
    let track = |part: &str, prop, keys| Track {
        part: part.into(),
        prop,
        keys,
    };
    // `body` slides 100 right over a second; `wing` is not in the drawing.
    let tracks = vec![
        track("body", Prop::X, vec![key(0.0, 0.0), key(1.0, 100.0)]),
        track("wing", Prop::Rot, vec![key(0.0, 1.0)]),
    ];
    let both = Arc::new(Clip::new(1.0, false, tracks).unwrap());
    let d = drawing();
    let mut world = World2d::default();
    world.reconcile(vec![playing("hero", &d, &both)]).unwrap();
    world.tick(0.5, 0.5);
    assert_eq!(at(&world, 56.0).as_deref(), Some("hero"), "the body's track still plays");
    let wanted = "entity \"hero\": its clip moves \"wing\", which its drawing lacks — skipped";
    assert_eq!(world.drain_notes(), [wanted]);
    world.reconcile(vec![playing("hero", &d, &both)]).unwrap();
    assert_eq!(world.drain_notes(), Vec::<String>::new(), "said once, not every frame");

    // The part comes back (a drawing with a `wing`), then goes again: that is news again.
    let winged = drawing_of("wing");
    world.reconcile(vec![playing("hero", &winged, &both)]).unwrap();
    world.reconcile(vec![playing("hero", &d, &both)]).unwrap();
    assert_eq!(world.drain_notes().len(), 2, "body is missing from the winged one, then wing");
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
            pivot: (0.0, 0.0),
            turn: false,
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

/// A hat whose plug (1, 2) mounts on the hero's `body` at (8, 0).
fn hat(turn: bool, d: &Arc<Drawing>) -> EntitySpec {
    EntitySpec {
        attach: Some(Attach {
            to: "hero".into(),
            part: "body".into(),
            at: (8.0, 0.0),
            pivot: (1.0, 2.0),
            turn,
        }),
        ..spec("hat", (50.0, 50.0), d)
    }
}

/// Over a second the hero's `body` turns 90° and doubles in size about its pivot, (0, 0).
fn nod() -> Arc<Clip> {
    use crate::clip::{Easing, Key, Prop, Track};
    let track = |prop, to| Track {
        part: "body".into(),
        prop,
        keys: [(0.0, if prop == Prop::Scale { 1.0 } else { 0.0 }), (1.0, to)]
            .map(|(time, value)| Key { time, value, easing: Easing::Linear })
            .into(),
    };
    Arc::new(Clip::new(1.0, false, vec![track(Prop::Rot, 90.0), track(Prop::Scale, 2.0)]).unwrap())
}

/// Where the carried entity's own point `p` is drawn.
fn drawn(world: &World2d, id: &str, p: (f64, f64)) -> (f64, f64) {
    let q = world.place(world.by_id[id]) * Point::new(p.0, p.1);
    ((q.x * 1e6).round() / 1e6, (q.y * 1e6).round() / 1e6)
}

#[test]
fn an_upright_mount_puts_its_pivot_on_the_point() {
    let d = drawing(); // 8 wide
    let mut world = World2d::default();
    world.reconcile(vec![spec("hero", (0.0, 0.0), &d), hat(false, &d)]).unwrap();
    assert_eq!(drawn(&world, "hat", (1.0, 2.0)), (8.0, 0.0));

    // Flipped, both mirror: `at` x 8 becomes 0, and the hat's own mirror keeps its plug on it.
    let hero = EntitySpec { flip: true, ..spec("hero", (0.0, 0.0), &d) };
    let hat = EntitySpec { flip: true, ..hat(false, &d) };
    world.reconcile(vec![hero, hat]).unwrap();
    assert_eq!(drawn(&world, "hat", (1.0, 2.0)), (0.0, 0.0), "the mirrored plug");
    assert_eq!(drawn(&world, "hat", (1.0, 3.0)), (0.0, 1.0), "still upright");
}

#[test]
fn a_turned_mount_takes_on_its_parts_rotation_scale_and_mirror_about_its_pivot() {
    let d = drawing();
    let mut world = World2d::default();
    let nod = nod(); // one handle, so flipping the hero does not restart the clip
    let hero = |flip| EntitySpec { flip, ..playing("hero", &d, &nod) };
    world.reconcile(vec![hero(false), hat(true, &d)]).unwrap();
    world.tick(1.0, 0.0);
    // `at` (8, 0) doubled and turned 90° is (0, 16); the plug stays on it.
    assert_eq!(drawn(&world, "hat", (1.0, 2.0)), (0.0, 16.0), "on the socket");
    // One unit down the hat is two units left: turned and doubled with the part.
    assert_eq!(drawn(&world, "hat", (1.0, 3.0)), (-2.0, 16.0), "turned and scaled");
    // Its box corner, (-1, -2) from the plug, doubled and turned, is what a drop starts from.
    assert_eq!(drawn(&world, "hat", (0.0, 0.0)), (4.0, 14.0));
    let t = world.transform("hat").unwrap();
    assert!((t.x - 4.0).abs() < 1e-9 && (t.y - 14.0).abs() < 1e-9, "{t:?}");

    // A flipped carrier mirrors the turned hat too, with no `flip` of its own.
    world.reconcile(vec![hero(true), hat(true, &d)]).unwrap();
    world.tick(1.0, 0.0);
    assert_eq!(drawn(&world, "hat", (1.0, 2.0)), (8.0, 16.0), "on the mirrored socket");
    assert_eq!(drawn(&world, "hat", (1.0, 3.0)), (10.0, 16.0), "mirrored: down is now right");
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
    let doubly = EntitySpec { flip: true, ..hat(true, &d) };
    let err = world.reconcile(vec![hero(), doubly]).unwrap_err().to_string();
    assert!(err.contains("which mirrors it too: drop flip"), "{err}");
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

/// A hero whose feet are a circle of radius 4 at (4, 4), and a wall whose left face is x = 20.
fn walled() -> World2d {
    let d = drawing();
    let hero = EntitySpec {
        controller: Some(wasd(100.0)),
        ..solid("hero", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let wall = solid("wall", (20.0, -50.0), Shape::Rect(10.0, 400.0), (0.0, 0.0), &d);
    let mut world = World2d::default();
    world.reconcile(vec![hero, wall]).unwrap();
    world
}

#[test]
fn a_solid_hero_stops_at_a_wall() {
    let mut world = walled();
    world.key("KeyD", true);
    for i in 1..=20 {
        world.tick(i as f64 * 0.1, 0.1);
    }
    // The circle's right edge (x + 8) rests against the wall, a hair short of touching.
    let x = world.transform("hero").unwrap().x;
    assert!((11.8..=12.0).contains(&x), "stopped at {x}");
}

/// Rapier's controller knows an "up"; top-down, no direction may be special.
#[test]
fn walls_stop_the_hero_the_same_way_in_every_direction() {
    let d = drawing();
    // A room 100 across, inside walls 10 thick; the hero's circle (radius 4) starts in the middle.
    let wall = |id: &str, pos, size: (f64, f64)| {
        solid(id, pos, Shape::Rect(size.0, size.1), (0.0, 0.0), &d)
    };
    for (key, axis, wanted) in [("KeyD", 0, 96.0), ("KeyA", 0, 4.0), ("KeyS", 1, 96.0), ("KeyW", 1, 4.0)] {
        let hero = EntitySpec {
            controller: Some(wasd(100.0)),
            ..solid("hero", (50.0, 50.0), Shape::Circle(4.0), (0.0, 0.0), &d)
        };
        let mut world = World2d::default();
        world
            .reconcile(vec![
                hero,
                wall("n", (-10.0, -10.0), (120.0, 10.0)),
                wall("s", (-10.0, 100.0), (120.0, 10.0)),
                wall("w", (-10.0, 0.0), (10.0, 100.0)),
                wall("e", (100.0, 0.0), (10.0, 100.0)),
            ])
            .unwrap();
        world.key(key, true);
        for i in 1..=20 {
            world.tick(i as f64 * 0.1, 0.1);
        }
        let t = world.transform("hero").unwrap();
        let at = [t.x, t.y][axis];
        assert!((at - wanted).abs() < 0.2, "{key}: stopped at {at}, wanted {wanted}");
    }
}

#[test]
fn a_carried_solid_is_off_the_floor_until_put_down() {
    let d = drawing();
    let hero = || EntitySpec {
        controller: Some(wasd(100.0)),
        ..solid("hero", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let chest = |carried: bool| EntitySpec {
        attach: carried.then(|| Attach {
            to: "hero".into(),
            part: "body".into(),
            at: (10.0, 0.0),
            pivot: (0.0, 0.0),
            turn: false,
        }),
        ..solid("chest", (50.0, 0.0), Shape::Rect(8.0, 8.0), (0.0, 0.0), &d)
    };
    let mut world = World2d::default();
    world.reconcile(vec![hero(), chest(false)]).unwrap();
    assert_eq!(world.physics.centres(), [(4.0, 4.0), (54.0, 4.0)], "on the floor, solid");

    world.reconcile(vec![hero(), chest(true)]).unwrap();
    assert_eq!(world.physics.centres(), [(4.0, 4.0)], "carried, it has no body");
    world.key("KeyD", true);
    for i in 1..=3 {
        world.tick(i as f64 * 0.1, 0.1);
    }
    world.key("KeyD", false);
    let x = world.transform("hero").unwrap().x;
    assert!(x > 29.0, "the hero carried it to {x}");

    world.reconcile(vec![hero(), chest(false)]).unwrap();
    let (cx, _) = world.physics.centres()[1];
    assert_eq!(cx as f64, x + 10.0 + 4.0, "put down, solid again where it was let go");
}

/// A hero walking right at 100 with a solid 8×8 box held 20 ahead of it, let go after `walk`
/// ticks; then `after` ticks with no key held. `wall_x` adds a wall whose left face is there.
/// Says the box's x each tick after the drop, and whether it is still sliding.
fn drop_box(walk: usize, wall_x: Option<f64>, after: usize) -> (Vec<f64>, bool) {
    let d = drawing();
    let hero = || EntitySpec {
        controller: Some(wasd(100.0)),
        ..solid("hero", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let held = |carried: bool| EntitySpec {
        attach: carried.then(|| Attach {
            to: "hero".into(),
            part: "body".into(),
            at: (20.0, 0.0),
            pivot: (0.0, 0.0),
            turn: false,
        }),
        ..solid("box", (0.0, 0.0), Shape::Rect(8.0, 8.0), (0.0, 0.0), &d)
    };
    let wall = || wall_x.map(|x| solid("wall", (x, -50.0), Shape::Rect(10.0, 100.0), (0.0, 0.0), &d));
    let mut world = World2d::default();
    world.reconcile([hero(), held(true)].into_iter().chain(wall()).collect()).unwrap();
    let mut clock = 0.0;
    let mut tick = |world: &mut World2d| {
        clock += 0.05;
        world.tick(clock, 0.05);
    };
    world.key("KeyD", walk > 0);
    for _ in 0..walk {
        tick(&mut world);
    }
    world.reconcile([hero(), held(false)].into_iter().chain(wall()).collect()).unwrap();
    world.key("KeyD", false);
    let xs = (0..after)
        .map(|_| {
            tick(&mut world);
            world.transform("box").unwrap().x
        })
        .collect();
    (xs, world.needs_ticks() && world.ecs.get::<Solid>(world.by_id["box"]).unwrap().sliding)
}

#[test]
fn a_let_go_body_keeps_its_carriers_momentum_then_settles() {
    let (xs, sliding) = drop_box(4, None, 40);
    // Let go at x 40 (the hero walked 20), moving at 100: it slides on, slowing to a stop.
    let (first, last) = (xs[0], *xs.last().unwrap());
    assert!(first > 40.0 && first < xs[3], "it keeps moving: {xs:?}");
    assert!((50.0..70.0).contains(&last), "slid to {last}");
    assert!(xs.windows(2).all(|w| w[1] >= w[0]), "forward only, slowing: {xs:?}");
    assert!(!sliding, "come to rest, it is fixed again");
}

#[test]
fn a_let_go_body_bounces_off_a_wall() {
    // Let go at x 40 heading right, with a wall at 52 — the box's right edge is 4 short of it.
    let (xs, _) = drop_box(4, Some(52.0), 40);
    let furthest = xs.iter().cloned().fold(f64::MIN, f64::max);
    assert!(furthest <= 44.1, "stopped by the wall, not through it: {furthest}");
    assert!(*xs.last().unwrap() < furthest - 1.0, "and came back off it: {xs:?}");
}

#[test]
fn a_body_let_go_inside_a_wall_is_pushed_out() {
    // Standing still, let go at x 20 to 28, with a wall from 25: three units deep in it.
    let (xs, sliding) = drop_box(0, Some(25.0), 60);
    let last = *xs.last().unwrap();
    // Out but for Rapier's resting overlap: settling waits until it is under twice that, 0.2.
    assert!(last + 8.0 <= 25.2, "pushed out to {last}: {xs:?}");
    assert!(!sliding, "and at rest");
}

fn zone(id: &str, pos: (f64, f64), collider: Option<Shape>, d: &Arc<Drawing>) -> EntitySpec {
    EntitySpec {
        collider: collider.map(|shape| Collider { shape, at: (0.0, 0.0) }),
        sensor: Some(Collider {
            shape: Shape::Circle(10.0),
            at: (4.0, 4.0),
        }),
        ..spec(id, pos, d)
    }
}

fn walker(d: &Arc<Drawing>) -> EntitySpec {
    EntitySpec {
        controller: Some(wasd(100.0)),
        ..solid("hero", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), d)
    }
}

/// Only the sensor moments, as `(entered, zone, who)`.
fn sensed(world: &mut World2d) -> Vec<(bool, String, String)> {
    let events = world.drain_events().into_iter();
    let sensed = events.filter_map(|e| match e {
        WorldEvent::Enter { id, who } => Some((true, id, who)),
        WorldEvent::Exit { id, who } => Some((false, id, who)),
        _ => None,
    });
    sensed.collect()
}

#[test]
fn a_sensor_reports_who_comes_in_and_goes_out_once_each() {
    let d = drawing();
    let mut world = World2d::default();
    // The zone's circle spans x 34..54; the hero's feet are a circle of radius 4 walking right.
    world.reconcile(vec![walker(&d), zone("chest", (40.0, 0.0), None, &d)]).unwrap();
    world.key("KeyD", true);
    let mut log = Vec::new();
    for i in 1..=10 {
        world.tick(i as f64 * 0.1, 0.1);
        log.extend(sensed(&mut world).into_iter().map(|e| (i, e)));
    }
    let (chest, hero) = ("chest".to_string(), "hero".to_string());
    assert_eq!(
        log,
        [(5, (true, chest.clone(), hero.clone())), (8, (false, chest, hero))],
        "in as its feet reach the zone, out as they leave it, two ticks after each: a walker's \
         step lands in Rapier's next one, and Rapier senses before it moves"
    );
}

#[test]
fn walls_in_a_sensor_are_not_news() {
    let d = drawing();
    let mut world = World2d::default();
    let wall = solid("wall", (40.0, 0.0), Shape::Rect(8.0, 8.0), (0.0, 0.0), &d);
    world.reconcile(vec![wall, zone("chest", (40.0, 0.0), None, &d)]).unwrap();
    for i in 1..=3 {
        world.tick(i as f64 * 0.1, 0.1);
    }
    assert_eq!(sensed(&mut world), [], "fixed in fixed: nothing moved, nothing to say");
}

#[test]
fn carrying_takes_a_sensor_away_and_dropping_brings_it_back() {
    let d = drawing();
    let chest = |carried: bool| EntitySpec {
        attach: carried.then(|| Attach {
            to: "hero".into(),
            part: "body".into(),
            at: (0.0, 0.0),
            pivot: (0.0, 0.0),
            turn: false,
        }),
        ..zone("chest", (0.0, 0.0), Some(Shape::Rect(2.0, 2.0)), &d)
    };
    let mut world = World2d::default();
    // The hero's feet (x 10..18) overlap the zone (x -6..14), clear of the chest's own box.
    let hero = || EntitySpec {
        pos: (10.0, 0.0),
        ..walker(&d)
    };
    world.reconcile(vec![hero(), chest(false)]).unwrap();
    let mut tick = {
        let mut clock = 0.0;
        move |world: &mut World2d| {
            clock += 0.1;
            world.tick(clock, 0.1);
            sensed(world)
        }
    };
    let (c, h) = ("chest".to_string(), "hero".to_string());
    assert_eq!(tick(&mut world), [(true, c.clone(), h.clone())], "standing beside it");
    world.reconcile(vec![hero(), chest(true)]).unwrap();
    assert_eq!(tick(&mut world), [(false, c.clone(), h.clone())], "picked up: no zone");
    world.reconcile(vec![hero(), chest(false)]).unwrap();
    assert_eq!(tick(&mut world), [(true, c, h)], "put down beside it again");
}

#[test]
fn a_solid_hero_slides_along_a_wall() {
    let mut world = walled();
    world.key("KeyD", true);
    world.key("KeyS", true);
    for i in 1..=10 {
        world.tick(i as f64 * 0.1, 0.1);
    }
    // Blocked across, it keeps the diagonal's downward share: about 70 a second.
    let t = world.transform("hero").unwrap();
    assert!((11.8..=12.0).contains(&t.x), "stopped at x {}", t.x);
    assert!(t.y > 60.0, "slid only to y {}", t.y);
}

#[test]
fn inspection_says_where_each_entity_is_how_it_moves_and_what_it_is_in() {
    let d = drawing();
    let glide = slide();
    let pad = || EntitySpec {
        clip: Some(glide.clone()),
        ..zone("pad", (0.0, 0.0), None, &d)
    };
    let held = |carried: bool| EntitySpec {
        attach: carried.then(|| Attach {
            to: "hero".into(),
            part: "body".into(),
            at: (20.0, 0.0),
            pivot: (0.0, 0.0),
            turn: false,
        }),
        ..solid("box", (0.0, 0.0), Shape::Rect(8.0, 8.0), (0.0, 0.0), &d)
    };
    let mut world = World2d::default();
    world.reconcile(vec![walker(&d), pad(), held(true)]).unwrap();
    world.key("KeyD", true);
    world.tick(0.05, 0.05);
    world.tick(0.1, 0.05);
    let seen = world.inspect();
    let [hero, pad_seen, carried] = &seen[..] else { panic!("{seen:?}") };
    assert_eq!((hero.id.as_str(), hero.body, hero.pos), ("hero", "moved", (10.0, 0.0)));
    assert_eq!(hero.velocity, (100.0, 0.0));
    assert_eq!(hero.zones, ["pad"], "its feet are inside the pad's zone");
    assert_eq!(pad_seen.body, "fixed");
    let clip = pad_seen.clip.as_ref().unwrap();
    assert_eq!((clip.length, clip.looped), (1.0, false));
    assert!((clip.time - 0.1).abs() < 1e-9, "{clip:?}");
    assert_eq!(carried.body, "none", "carried, it has no body");
    assert_eq!(carried.attached, Some(("hero".into(), "body".into())));

    world.reconcile(vec![walker(&d), pad(), held(false)]).unwrap();
    world.tick(0.15, 0.05);
    let thrown = &world.inspect()[2];
    assert_eq!((thrown.body, thrown.attached.clone()), ("thrown", None));
    assert!(thrown.velocity.0 > 50.0, "it left with the hero's speed: {thrown:?}");
}

#[test]
fn a_body_let_go_across_a_wall_lands_on_its_carriers_side_and_settles() {
    // Held 20 ahead, the box spans x 20..28; a thin wall at 21..23 runs through it, nearer its
    // left edge. Pushed out the short way, it would land beyond the wall — out of the room.
    let d = drawing();
    let hero = || EntitySpec {
        controller: Some(wasd(100.0)),
        ..solid("hero", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let held = |carried: bool| EntitySpec {
        attach: carried.then(|| Attach {
            to: "hero".into(),
            part: "body".into(),
            at: (20.0, 0.0),
            pivot: (0.0, 0.0),
            turn: false,
        }),
        ..solid("box", (0.0, 0.0), Shape::Rect(8.0, 8.0), (0.0, 0.0), &d)
    };
    let wall = || solid("wall", (21.0, -50.0), Shape::Rect(2.0, 100.0), (0.0, 0.0), &d);
    let mut world = World2d::default();
    world.reconcile(vec![hero(), held(true), wall()]).unwrap();
    world.tick(0.05, 0.05);
    world.reconcile(vec![hero(), held(false), wall()]).unwrap();
    let x = world.transform("box").unwrap().x;
    assert!(x + 8.0 <= 21.0 && x > 8.0, "against the wall, the hero's side, at once: {x}");
    for i in 1..=40 {
        world.tick(0.05 + 0.05 * i as f64, 0.05);
    }
    let seen = &world.inspect()[1];
    assert_eq!(seen.body, "fixed", "and settled: {seen:?}");
}

/// Carries a loose box 20 ahead at 100/s for four ticks and lets it go, with a wall at `wall_x`
/// if any; says its x and x-velocity each tick after, from the inspection.
fn fling(material: Material, wall_x: Option<f64>, after: usize) -> (Vec<(f64, f64)>, World2d) {
    let d = drawing();
    let hero = || EntitySpec {
        controller: Some(wasd(100.0)),
        ..solid("hero", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let held = |carried: bool| EntitySpec {
        attach: carried.then(|| Attach {
            to: "hero".into(),
            part: "body".into(),
            at: (20.0, 0.0),
            pivot: (0.0, 0.0),
            turn: false,
        }),
        loose: Some(material),
        ..solid("box", (0.0, 0.0), Shape::Rect(8.0, 8.0), (0.0, 0.0), &d)
    };
    let wall = || wall_x.map(|x| solid("wall", (x, -50.0), Shape::Rect(10.0, 100.0), (0.0, 0.0), &d));
    let mut world = World2d::default();
    world.reconcile([hero(), held(true)].into_iter().chain(wall()).collect()).unwrap();
    world.key("KeyD", true);
    let mut clock = 0.0;
    for _ in 0..4 {
        clock += 0.05;
        world.tick(clock, 0.05);
    }
    world.reconcile([hero(), held(false)].into_iter().chain(wall()).collect()).unwrap();
    world.key("KeyD", false);
    let trace = (0..after)
        .map(|_| {
            clock += 0.05;
            world.tick(clock, 0.05);
            let b = &world.inspect()[1];
            (b.pos.0, b.velocity.0)
        })
        .collect();
    (trace, world)
}

#[test]
fn a_springy_thing_keeps_more_speed_off_a_wall_than_a_dead_one() {
    let rebound = |bounce| {
        let (trace, _) = fling(Material { bounce, friction: 0.5, grip: None }, Some(52.0), 20);
        trace.iter().map(|&(_, vx)| vx).fold(f64::MAX, f64::min)
    };
    // Let go at 100/s, 4 short of the wall: it hits almost at once, then comes back.
    let (ball, crate_) = (rebound(0.9), rebound(0.1));
    assert!(ball < -70.0, "a ball comes back fast: {ball}");
    assert!(crate_ > -20.0, "a crate barely comes back: {crate_}");
}

#[test]
fn friction_decides_how_far_a_loose_thing_slides() {
    let slid = |friction| {
        let (trace, _) = fling(Material { bounce: 0.5, friction, grip: None }, None, 60);
        trace.last().unwrap().0
    };
    let (ball, crate_) = (slid(0.5), slid(8.0));
    // Let go at x 40: at 8 per second lost it stops within about 20; at 0.5 it rolls on and on.
    assert!(crate_ < 60.0, "the crate stopped soon: {crate_}");
    assert!(ball > crate_ + 100.0, "the ball rolled far further: {ball} vs {crate_}");
}

#[test]
fn a_loose_thing_rests_asleep_and_stays_loose() {
    let (_, world) = fling(Material::default(), None, 100);
    let b = &world.inspect()[1];
    assert_eq!((b.body, b.velocity), ("loose", (0.0, 0.0)), "not fixed: {b:?}");
    let s = world.ecs.get::<Solid>(world.by_id["box"]).unwrap();
    assert!(!s.sliding && world.physics.asleep(s.body), "asleep, so nothing to tick for it");

    // Spawned at rest, it starts asleep: a world of only loose things at rest does not tick.
    let d = drawing();
    let crate_ = EntitySpec {
        loose: Some(Material::default()),
        ..solid("crate", (0.0, 0.0), Shape::Rect(8.0, 8.0), (0.0, 0.0), &d)
    };
    let mut world = World2d::default();
    world.reconcile(vec![crate_]).unwrap();
    assert!(!world.needs_ticks());
    assert_eq!(world.inspect()[0].body, "loose");
}

#[test]
fn a_bad_loose_is_refused() {
    let d = drawing();
    let crate_ = |loose| EntitySpec {
        loose: Some(loose),
        ..solid("crate", (0.0, 0.0), Shape::Rect(8.0, 8.0), (0.0, 0.0), &d)
    };
    let fine = Material::default();
    let shapeless = EntitySpec { collider: None, ..crate_(fine) };
    let driven = EntitySpec { controller: Some(wasd(10.0)), ..crate_(fine) };
    let cases = [
        (shapeless, "loose needs a collider"),
        (driven, "loose with a controller"),
        (crate_(Material { bounce: 1.5, ..fine }), "bounce must be from 0 to 1, got 1.5"),
        (crate_(Material { friction: f64::NAN, ..fine }), "friction must be 0 or more"),
    ];
    let mut world = World2d::default();
    for (spec, wanted) in cases {
        let err = world.reconcile(vec![spec]).unwrap_err().to_string();
        assert!(err.contains(wanted), "wanted {wanted:?}, got {err}");
    }
}

#[test]
fn a_walker_pushes_a_loose_thing_and_is_stopped_by_a_fixed_one() {
    let d = drawing();
    let walk = |loose| {
        let hero = EntitySpec {
            controller: Some(wasd(100.0)),
            ..solid("hero", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
        };
        let thing = EntitySpec {
            loose,
            ..solid("box", (20.0, 0.0), Shape::Rect(8.0, 8.0), (0.0, 0.0), &d)
        };
        let mut world = World2d::default();
        world.reconcile(vec![hero, thing]).unwrap();
        world.key("KeyD", true);
        let mut clock = 0.0;
        for _ in 0..10 {
            clock += 0.05;
            world.tick(clock, 0.05);
        }
        let seen = world.inspect();
        (seen[0].pos.0, seen[1].pos.0, seen[1].body)
    };
    // Half a second at 100/s: 50 units if nothing were in the way. The hero pushes the crate
    // along, against it the whole way (its edge is at 8).
    let (hero_x, box_x, body) = walk(Some(Material::default()));
    assert_eq!(body, "loose");
    assert!(box_x > 30.0, "the crate was pushed along: {box_x}");
    assert!(hero_x > 25.0 && box_x - hero_x < 10.0, "the hero kept walking, behind it: {hero_x}");
    let (hero_x, box_x, _) = walk(None);
    assert_eq!(box_x, 20.0, "a fixed box does not move");
    assert!(hero_x < 13.0, "the hero stopped at it: {hero_x}");
}

#[test]
fn a_line_stops_only_the_group_it_blocks() {
    let d = drawing();
    // A walker from x 0 heading right for half a second at 100/s, a line across x 30, and maybe a
    // loose box at x 12 in its path; says where the walker and the box ended.
    let run = |group: Option<&str>, with_box: bool| {
        let hero = EntitySpec {
            controller: Some(wasd(100.0)),
            group: group.map(String::from),
            ..solid("hero", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
        };
        let line = EntitySpec {
            blocks: Some(vec!["paddle".into()]),
            ..solid("line", (30.0, -50.0), Shape::Rect(2.0, 100.0), (0.0, 0.0), &d)
        };
        let thing = EntitySpec {
            loose: Some(Material { bounce: 0.0, friction: 0.0, grip: None }),
            ..solid("box", (12.0, 0.0), Shape::Rect(8.0, 8.0), (0.0, 0.0), &d)
        };
        let mut world = World2d::default();
        world.reconcile([hero, line].into_iter().chain(with_box.then_some(thing)).collect()).unwrap();
        world.key("KeyD", true);
        let mut clock = 0.0;
        for _ in 0..10 {
            clock += 0.05;
            world.tick(clock, 0.05);
        }
        let seen = world.inspect();
        (seen[0].pos.0, seen.get(2).map(|b| b.pos.0))
    };
    let (paddle, _) = run(Some("paddle"), false);
    assert!(paddle < 22.0 && paddle > 20.0, "a paddle stops at the line: {paddle}");
    let (other, _) = run(None, false);
    assert!(other > 40.0, "anything else walks through it: {other}");
    let (_, pushed) = run(Some("paddle"), true);
    assert!(pushed.unwrap() > 32.0, "a puck it pushes crosses it: {pushed:?}");
}

#[test]
fn a_fast_loose_thing_does_not_pass_through_a_thin_wall() {
    let d = drawing();
    // Carried at 4000/s and let go 100 short of a wall 2 thick: one step would jump it clean over.
    let hero = || EntitySpec {
        controller: Some(wasd(4000.0)),
        ..solid("hero", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let held = |carried: bool| EntitySpec {
        attach: carried.then(|| Attach {
            to: "hero".into(),
            part: "body".into(),
            at: (20.0, 0.0),
            pivot: (0.0, 0.0),
            turn: false,
        }),
        loose: Some(Material { bounce: 0.0, friction: 0.0, grip: None }),
        ..solid("puck", (0.0, 0.0), Shape::Rect(8.0, 8.0), (0.0, 0.0), &d)
    };
    let wall = || solid("wall", (320.0, -50.0), Shape::Rect(2.0, 100.0), (0.0, 0.0), &d);
    let mut world = World2d::default();
    world.reconcile(vec![hero(), held(true), wall()]).unwrap();
    world.key("KeyD", true);
    world.tick(0.05, 0.05);
    world.key("KeyD", false);
    world.reconcile(vec![hero(), held(false), wall()]).unwrap();
    for i in 2..12 {
        world.tick(i as f64 * 0.05, 0.05);
        }
    let x = world.inspect()[1].pos.0;
    assert!(x + 8.0 <= 320.5, "stopped at the wall, not past it: {x}");
}

#[test]
fn groups_are_checked() {
    let d = drawing();
    let line = |group: Option<&str>, blocks: Option<Vec<String>>, collider: bool| EntitySpec {
        group: group.map(String::from),
        blocks,
        collider: collider.then(|| Collider { shape: Shape::Rect(2.0, 2.0), at: (0.0, 0.0) }),
        ..solid("line", (0.0, 0.0), Shape::Rect(2.0, 2.0), (0.0, 0.0), &d)
    };
    let mut world = World2d::default();
    let err = |world: &mut World2d, spec| world.reconcile(vec![spec]).unwrap_err().to_string();
    assert!(err(&mut world, line(Some("paddle"), None, false)).contains("need a collider"));
    assert!(err(&mut world, line(None, Some(vec![]), true)).contains("blocks is empty"));
    let many = (0..32).map(|i| format!("g{i}")).collect();
    assert!(err(&mut world, line(None, Some(many), true)).contains("\"g31\" would be the 32nd"));
}

#[test]
fn a_loose_thing_put_down_inside_a_walker_is_pushed_out_not_left_asleep_in_it() {
    let d = drawing();
    let hero = EntitySpec {
        controller: Some(wasd(100.0)),
        ..solid("hero", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let mut world = World2d::default();
    world.reconcile(vec![hero]).unwrap();
    world.tick(0.05, 0.05);
    // The hero's circle is x 0 to 8; the puck spawns at x 5, three deep in it.
    let hero = EntitySpec {
        controller: Some(wasd(100.0)),
        ..solid("hero", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let puck = EntitySpec {
        loose: Some(Material { bounce: 0.5, friction: 6.0, grip: None }),
        ..solid("puck", (5.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    world.reconcile(vec![hero, puck]).unwrap();
    for i in 2..40 {
        world.tick(i as f64 * 0.05, 0.05);
    }
    let x = world.inspect()[1].pos.0;
    assert!(x >= 7.8, "pushed clear of the hero: {x}");
}

#[test]
fn set_moves_an_entity_and_sets_a_loose_things_velocity() {
    let d = drawing();
    let specs = || {
        let wall = solid("wall", (100.0, -50.0), Shape::Rect(4.0, 100.0), (2.0, 50.0), &d);
        let puck = EntitySpec {
            loose: Some(Material { bounce: 0.5, friction: 0.0, grip: None }),
            ..solid("puck", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
        };
        vec![wall, puck]
    };
    let mut world = World2d::default();
    world.reconcile(specs()).unwrap();
    world.tick(0.0, 0.0);
    // Asleep at rest: set wakes it, and it goes on until the wall stops it.
    let to = Set { pos: Some((50.0, 0.0)), velocity: Some((200.0, 0.0)), spin: None };
    world.set("puck", to).unwrap();
    assert_eq!(world.inspect()[1].pos, (50.0, 0.0), "moved at once, before a tick");
    assert!(world.needs_ticks(), "a woken puck needs the clock");
    for i in 1..=10 {
        world.tick(i as f64 * 0.02, 0.02);
    }
    let x = world.inspect()[1].pos.0;
    assert!(x > 80.0 && x < 100.0, "moved on, stopped short of the wall: {x}");
    // A description never moves an entity that exists: the puck stays where set put it.
    world.reconcile(specs()).unwrap();
    assert_eq!(world.inspect()[1].pos.0, x);
    // The wall is fixed: it can be put somewhere, not given a velocity.
    world.set("wall", Set { pos: Some((200.0, -50.0)), velocity: None, spin: None }).unwrap();
    assert_eq!(world.inspect()[0].pos, (200.0, -50.0));
}

#[test]
fn set_is_checked() {
    let d = drawing();
    let hero = EntitySpec {
        controller: Some(wasd(100.0)),
        ..solid("hero", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let mut world = World2d::default();
    world.reconcile(vec![hero]).unwrap();
    let velocity = |v| Set { pos: None, velocity: Some(v), spin: None };
    let cases = [
        ("ghost", velocity((1.0, 0.0)), "there is no such entity"),
        ("hero", velocity((1.0, 0.0)), "velocity needs a loose thing"),
        ("hero", Set { pos: Some((f64::NAN, 0.0)), velocity: None, spin: None }, "must be finite"),
    ];
    for (id, to, wanted) in cases {
        let err = world.set(id, to).unwrap_err().to_string();
        assert!(err.contains(wanted), "wanted {wanted:?}, got {err}");
    }
}

#[test]
fn a_timer_fires_once_its_seconds_have_passed_from_the_next_frame() {
    let mut world = World2d::default();
    world.tick(5.0, 0.1);
    // Set while idle: the world's clock is its last frame's, so counting starts at the next one.
    world.after("faceoff", 1.5).unwrap();
    world.after("blink", 0.5).unwrap();
    assert!(world.needs_ticks(), "a timer keeps the world ticking");
    world.tick(60.0, 0.1);
    assert_eq!(world.timers()[0], TimerInspection { name: "blink".into(), left: 0.5 });
    world.tick(60.5, 0.1);
    assert_eq!(world.drain_events(), [WorldEvent::Timer("blink".into())]);
    // The same name again starts it over; cancel drops it.
    world.after("faceoff", 1.0).unwrap();
    world.tick(61.6, 0.1);
    assert!(world.drain_events().is_empty(), "the first faceoff, due at 61.5, is gone");
    world.tick(62.5, 0.1);
    assert!(world.drain_events().is_empty(), "the new one counts from 61.6");
    world.tick(62.6, 0.1);
    assert_eq!(world.drain_events(), [WorldEvent::Timer("faceoff".into())]);
    world.after("never", 0.1).unwrap();
    world.cancel("never");
    world.tick(70.0, 0.1);
    assert!(world.drain_events().is_empty() && !world.needs_ticks());
    let err = world.after("bad", -1.0).unwrap_err().to_string();
    assert!(err.contains("finite number, zero or more"), "{err}");
}

#[test]
fn a_thing_taken_out_of_a_zone_on_a_frame_with_no_time_leaves_it() {
    let d = drawing();
    let goal = || EntitySpec {
        sensor: Some(Collider { shape: Shape::Rect(40.0, 40.0), at: (0.0, 0.0) }),
        ..spec("goal", (0.0, 0.0), &d)
    };
    let puck = |live: bool| EntitySpec {
        loose: live.then_some(Material::default()),
        collider: live.then_some(Collider { shape: Shape::Circle(4.0), at: (4.0, 4.0) }),
        ..spec("puck", (10.0, 10.0), &d)
    };
    let mut world = World2d::default();
    world.reconcile(vec![goal(), puck(true)]).unwrap();
    world.tick(0.05, 0.05);
    assert_eq!(world.drain_events(), [WorldEvent::Enter { id: "goal".into(), who: "puck".into() }]);
    // Its body goes; the next frame has no time in it, so Rapier does not step.
    world.reconcile(vec![goal(), puck(false)]).unwrap();
    world.tick(0.05, 0.0);
    assert_eq!(world.drain_events(), [WorldEvent::Exit { id: "goal".into(), who: "puck".into() }]);
}

#[test]
fn a_loose_thing_reports_a_hit_once_at_the_speed_it_met_at() {
    let d = drawing();
    let wall = solid("wall", (100.0, -50.0), Shape::Rect(4.0, 100.0), (2.0, 50.0), &d);
    let puck = EntitySpec {
        loose: Some(Material { bounce: 0.0, friction: 0.0, grip: None }),
        ..solid("puck", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let mut world = World2d::default();
    world.reconcile(vec![wall, puck]).unwrap();
    world.tick(0.0, 0.0);
    world.set("puck", Set { pos: None, velocity: Some((300.0, 0.0)), spin: None }).unwrap();
    let mut hits = vec![];
    for i in 1..=60 {
        world.tick(i as f64 / 60.0, 1.0 / 60.0);
        hits.extend(world.drain_events());
    }
    // One hit, though it then rests against the wall for most of a second.
    let [WorldEvent::Hit { id, who, speed }] = hits.as_slice() else { panic!("{hits:?}") };
    assert_eq!((id.as_str(), who.as_str()), ("puck", "wall"));
    assert!((speed - 300.0).abs() < 1.0, "met at its own speed: {speed}");
}

#[test]
fn zero_time_observations_preserve_controller_velocity_for_the_next_contact() {
    let d = drawing();
    let hero = EntitySpec {
        controller: Some(wasd(200.0)),
        ..solid("hero", (0.0, 0.0), Shape::Circle(8.0), (8.0, 8.0), &d)
    };
    let puck = EntitySpec {
        loose: Some(Material::default()),
        ..solid("puck", (40.0, 4.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let mut world = World2d::default();
    world.reconcile(vec![hero, puck]).unwrap();
    world.key("KeyD", true);
    let mut hits = vec![];
    for i in 1..=20 {
        let elapsed = i as f64 / 60.0;
        world.tick(elapsed, 1.0 / 60.0);
        hits.extend(world.drain_events().into_iter().filter(|e| matches!(e, WorldEvent::Hit { .. })));
        let before = world.inspect()[0].velocity;
        world.tick(elapsed, 0.0);
        assert_eq!(world.inspect()[0].velocity, before);
        hits.extend(world.drain_events().into_iter().filter(|e| matches!(e, WorldEvent::Hit { .. })));
    }
    let [WorldEvent::Hit { id, who, speed }] = hits.as_slice() else { panic!("{hits:?}") };
    assert_eq!((id.as_str(), who.as_str()), ("puck", "hero"));
    assert!(*speed > 100.0);
}

#[test]
fn a_walker_running_into_a_loose_thing_is_a_hit_for_the_loose_thing() {
    let d = drawing();
    let hero = EntitySpec {
        controller: Some(wasd(200.0)),
        ..solid("hero", (0.0, 0.0), Shape::Circle(8.0), (8.0, 8.0), &d)
    };
    let puck = EntitySpec {
        loose: Some(Material::default()),
        ..solid("puck", (40.0, 4.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let mut world = World2d::default();
    world.reconcile(vec![hero, puck]).unwrap();
    world.key("KeyD", true);
    let mut hits = vec![];
    for i in 1..=20 {
        world.tick(i as f64 / 60.0, 1.0 / 60.0);
        hits.extend(world.drain_events().into_iter().filter(|e| matches!(e, WorldEvent::Hit { .. })));
    }
    let [WorldEvent::Hit { id, who, speed }] = hits.as_slice() else { panic!("{hits:?}") };
    assert_eq!((id.as_str(), who.as_str()), ("puck", "hero"));
    assert!(*speed > 100.0, "met at about the walker's speed: {speed}");
}

#[test]
fn a_walker_strikes_a_loose_thing_it_runs_into_rather_than_shoving_it() {
    let d = drawing();
    let paddle = EntitySpec {
        controller: Some(wasd(400.0)),
        ..solid("paddle", (0.0, 0.0), Shape::Circle(8.0), (8.0, 8.0), &d)
    };
    let puck = EntitySpec {
        loose: Some(Material { bounce: 1.0, friction: 0.0, grip: None }),
        ..solid("puck", (40.0, 4.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let mut world = World2d::default();
    world.reconcile(vec![paddle, puck]).unwrap();
    world.key("KeyD", true);
    let mut fastest: f64 = 0.0;
    for i in 1..=20 {
        world.tick(i as f64 / 60.0, 1.0 / 60.0);
        fastest = fastest.max(world.inspect()[1].velocity.0);
    }
    // Struck by something far heavier at 400, it leaves at up to twice that; shoved, at 400.
    assert!(fastest > 600.0, "the puck was struck, not shoved: {fastest}");
}

/// A puck 8 across at x 0..8, above a floor at y 40, set going at `velocity` and `spin`; where it
/// was, its turn and its spin each tick for `ticks` ticks.
fn spun(
    grip: Option<f64>,
    friction: f64,
    velocity: (f64, f64),
    spin: Option<f64>,
    ticks: usize,
) -> Vec<((f64, f64), f64, f64)> {
    let d = drawing();
    let floor = solid("floor", (-500.0, 40.0), Shape::Rect(1000.0, 4.0), (0.0, 0.0), &d);
    let puck = EntitySpec {
        loose: Some(Material { bounce: 0.5, friction, grip }),
        ..solid("puck", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let mut world = World2d::default();
    world.reconcile(vec![floor, puck]).unwrap();
    world.tick(0.0, 0.0);
    world.set("puck", Set { pos: None, velocity: Some(velocity), spin }).unwrap();
    (1..=ticks)
        .map(|i| {
            world.tick(i as f64 / 60.0, 1.0 / 60.0);
            let p = &world.inspect()[1];
            (p.pos, p.rot, p.spin)
        })
        .collect()
}

#[test]
fn a_thing_with_grip_struck_glancing_turns_and_one_without_never_does() {
    // Down and to the right, onto the floor at a slant.
    let gripped = spun(Some(0.8), 0.0, (300.0, 300.0), None, 30);
    let (_, rot, spin) = *gripped.last().unwrap();
    assert!(spin.abs() > 100.0 && rot != 0.0, "the floor caught it and set it turning: {spin} {rot}");
    let slick = spun(None, 0.0, (300.0, 300.0), None, 30);
    assert!(slick.iter().all(|&(_, rot, spin)| rot == 0.0 && spin == 0.0), "no grip, no turn");
}

#[test]
fn a_thing_spinning_in_place_stays_put_and_its_spin_runs_down_with_friction() {
    // Turning about its centre: the drawing box's corner does not wander.
    let free = spun(Some(0.5), 0.0, (0.0, 0.0), Some(360.0), 30);
    assert!(free.iter().all(|&(pos, ..)| pos == (0.0, 0.0)), "it spun in place: {:?}", free.last());
    let (_, rot, spin) = *free.last().unwrap();
    assert!((spin - 360.0).abs() < 1.0, "it keeps its spin with no friction: {spin}");
    assert!((rot - 180.0).abs() < 2.0, "half a turn in half a second: {rot}");
    let slowed = spun(Some(0.5), 4.0, (0.0, 0.0), Some(360.0), 180);
    assert!(slowed.last().unwrap().2.abs() < 5.0, "friction ran it down: {:?}", slowed.last());
}

#[test]
fn spin_and_grip_are_checked() {
    let d = drawing();
    let bad = EntitySpec {
        loose: Some(Material { grip: Some(-1.0), ..Material::default() }),
        ..solid("puck", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let err = World2d::default().reconcile(vec![bad]).unwrap_err().to_string();
    assert!(err.contains("grip must be 0 or more"), "{err}");
    let slick = EntitySpec {
        loose: Some(Material::default()),
        ..solid("puck", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let mut world = World2d::default();
    world.reconcile(vec![slick]).unwrap();
    let spin = |s| Set { spin: Some(s), ..Set::default() };
    let err = world.set("puck", spin(90.0)).unwrap_err().to_string();
    assert!(err.contains("spin needs a loose thing with grip"), "{err}");
    let err = world.set("puck", spin(f64::INFINITY)).unwrap_err().to_string();
    assert!(err.contains("must be finite"), "{err}");
}
