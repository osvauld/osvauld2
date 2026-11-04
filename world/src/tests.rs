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

/// `dt` of frame time as 1/60 s frames ending at `elapsed`. Many tests tick in long frames,
/// which the stall cap (`MAX_STEPS`) would cut short.
fn frames(world: &mut World2d, elapsed: f64, dt: f64) {
    let n = (dt * 60.0 - 1e-9).ceil().max(1.0) as usize;
    for i in 1..=n {
        world.tick(elapsed - dt + dt * i as f64 / n as f64, dt / n as f64);
    }
}

/// Equal but for rounding: steps add up in f64 here and f32 in Rapier.
fn near((ax, ay): (f64, f64), (bx, by): (f64, f64)) -> bool {
    (ax - bx).abs() < 1e-4 && (ay - by).abs() < 1e-4
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
    frames(&mut world, 0.0, 0.5);
    assert_eq!(world.transform("hero").unwrap().x, 0.0, "no key held");

    world.key("KeyD", true);
    frames(&mut world, 0.5, 0.5);
    let t = world.transform("hero").unwrap();
    assert!(near((t.x, t.y), (50.0, 0.0)), "{t:?}");
    world.key("KeyA", true);
    frames(&mut world, 1.0, 0.5);
    assert_eq!(world.transform("hero").unwrap(), t, "opposite keys cancel");

    world.key("KeyA", false);
    world.key("KeyS", true);
    frames(&mut world, 1.5, 0.5);
    let t = world.transform("hero").unwrap();
    let step = ((t.x - 50.0).powi(2) + t.y.powi(2)).sqrt();
    assert!((step - 50.0).abs() < 1e-9, "diagonal stepped {step}");

    world.release_all();
    frames(&mut world, 2.0, 0.5);
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
    frames(&mut world, 0.5, 0.5);
    assert!(near(at(&world), (102.0, -5.0)), "the hero walked 50, the body slid 50");

    let hero = EntitySpec {
        clip: Some(glide),
        ..controlled(&d, wasd(100.0))
    };
    world
        .reconcile(vec![spec("chest", (0.0, 0.0), &d), hero])
        .unwrap();
    frames(&mut world, 1.0, 0.5);
    assert!(near(at(&world), (102.0, -5.0)), "dropped where it was carried");
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
        frames(&mut world, i as f64 * 0.1, 0.1);
    }
    world.key("KeyD", false);
    let x = world.transform("hero").unwrap().x;
    assert!(x > 29.0, "the hero carried it to {x}");

    world.reconcile(vec![hero(), chest(false)]).unwrap();
    let (cx, _) = world.physics.centres()[1];
    assert!(near((cx as f64, 0.0), (x + 10.0 + 4.0, 0.0)), "put down, solid again where it was let go");
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
        frames(&mut world, i as f64 * 0.1, 0.1);
        log.extend(sensed(&mut world).into_iter().map(|e| (i, e)));
    }
    let (chest, hero) = ("chest".to_string(), "hero".to_string());
    assert_eq!(
        log,
        [(3, (true, chest.clone(), hero.clone())), (6, (false, chest, hero))],
        "in as its feet reach the zone (0.26 s), out as they leave it (0.58 s), two steps after \
         each: a walker's step lands in Rapier's next one, and Rapier senses before it moves"
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
        frames(&mut world, i as f64 * 0.1, 0.1);
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
    assert_eq!((hero.id.as_str(), hero.body), ("hero", "moved"));
    assert!(near(hero.pos, (10.0, 0.0)), "{hero:?}");
    assert!(near(hero.velocity, (100.0, 0.0)), "{hero:?}");
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
        let (trace, _) = fling(Material { bounce, friction: 0.5, grip: None, density: 1.0, mass: None, center: None }, Some(52.0), 20);
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
        let (trace, _) = fling(Material { bounce: 0.5, friction, grip: None, density: 1.0, mass: None, center: None }, None, 60);
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
            loose: Some(Material { bounce: 0.0, friction: 0.0, grip: None, density: 1.0, mass: None, center: None }),
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
        loose: Some(Material { bounce: 0.0, friction: 0.0, grip: None, density: 1.0, mass: None, center: None }),
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
        loose: Some(Material { bounce: 0.5, friction: 6.0, grip: None, density: 1.0, mass: None, center: None }),
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
            loose: Some(Material { bounce: 0.5, friction: 0.0, grip: None, density: 1.0, mass: None, center: None }),
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
        loose: Some(Material { bounce: 0.0, friction: 0.0, grip: None, density: 1.0, mass: None, center: None }),
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
        loose: Some(Material { bounce: 1.0, friction: 0.0, grip: None, density: 1.0, mass: None, center: None }),
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
        loose: Some(Material { bounce: 0.5, friction, grip, density: 1.0, mass: None, center: None }),
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

/// Two walls across a corridor at x 50 and x 100, a zone between them at x 70, a hero at the
/// start; stepped once, so Rapier has them all where it answers from.
fn corridor() -> World2d {
    let d = drawing();
    let wall = |id, x| solid(id, (x, -50.0), Shape::Rect(4.0, 100.0), (0.0, 0.0), &d);
    let hero = solid("hero", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d);
    let mut world = World2d::default();
    world.reconcile(vec![hero, wall("near", 50.0), zone("lamp", (66.0, 0.0), None, &d), wall("far", 100.0)]).unwrap();
    world.tick(1.0 / 60.0, 1.0 / 60.0);
    world
}

#[test]
fn a_ray_answers_the_first_solid_thing_on_it_and_where_it_met_it() {
    let world = corridor();
    // From the hero's centre, skipping the hero; the zone does not stop a ray.
    let hit = world.ray((4.0, 4.0), (200.0, 4.0), Some("hero")).unwrap().unwrap();
    assert_eq!(hit.id, "near");
    assert!((hit.at.0 - 50.0).abs() < 1e-3 && (hit.at.1 - 4.0).abs() < 1e-3, "{hit:?}");
    assert!((hit.normal.0 + 1.0).abs() < 1e-3 && hit.normal.1.abs() < 1e-3, "{hit:?}");
    assert!((hit.dist - 46.0).abs() < 1e-3, "{hit:?}");
    // Not skipped, the hero it starts inside is the first thing on it.
    assert_eq!(world.ray((4.0, 4.0), (200.0, 4.0), None).unwrap().unwrap().id, "hero");
    // Short of the wall, or along the corridor, nothing.
    assert_eq!(world.ray((10.0, 4.0), (40.0, 4.0), None).unwrap(), None);
    assert_eq!(world.ray((10.0, 4.0), (10.0, 200.0), None).unwrap(), None);
}

#[test]
fn at_answers_everything_whose_collider_or_zone_covers_a_point() {
    let world = corridor();
    assert_eq!(world.at((70.0, 4.0)).unwrap(), ["lamp"]);
    assert_eq!(world.at((52.0, 0.0)).unwrap(), ["near"]);
    assert_eq!(world.at((4.0, 4.0)).unwrap(), ["hero"]);
    assert!(world.at((30.0, 30.0)).unwrap().is_empty());
}

#[test]
fn questions_are_checked() {
    let world = corridor();
    let err = |r: Result<_, WorldError>| r.map(|_: Option<RayHit>| ()).unwrap_err().to_string();
    assert!(err(world.ray((0.0, 0.0), (f64::NAN, 0.0), None)).contains("must be finite"));
    assert!(err(world.ray((1.0, 1.0), (1.0, 1.0), None)).contains("from and to are the same point"));
    assert!(err(world.ray((0.0, 0.0), (9.0, 0.0), Some("ghost"))).contains("skip: there is no such entity"));
    assert!(world.at((f64::INFINITY, 0.0)).unwrap_err().to_string().contains("must be finite"));
}

#[test]
fn the_world_counts_its_steps_and_a_frame_with_no_time_is_not_one() {
    let mut world = corridor();
    assert_eq!(world.steps(), 2, "a 60 fps frame is two steps");
    world.tick(1.0 / 60.0, 0.0);
    assert_eq!(world.steps(), 2);
    world.tick(2.0 / 60.0, 1.0 / 60.0);
    assert_eq!(world.steps(), 4);
}

/// 500 crates in a 25×20 grid, stepped once so Rapier answers from them.
fn crates() -> World2d {
    let d = drawing();
    let crates = (0..500).map(|i| {
        let (x, y) = ((i % 25) as f64 * 40.0, (i / 25) as f64 * 40.0);
        solid(&format!("crate{i}"), (x, y), Shape::Rect(8.0, 8.0), (0.0, 0.0), &d)
    });
    let mut world = World2d::default();
    world.reconcile(crates.collect()).unwrap();
    world.tick(1.0 / 60.0, 1.0 / 60.0);
    world
}

/// A thousand slanted rays across the crates: how many hit.
fn thousand_rays(world: &World2d) -> usize {
    (0..1000)
        .filter(|i| {
            let y = (i % 800) as f64 + 0.5;
            world.ray((-10.0, y), (1100.0, y + 37.0), None).unwrap().is_some()
        })
        .count()
}

#[test]
fn slanted_rays_across_a_grid_of_crates_mostly_meet_one() {
    let hits = thousand_rays(&crates());
    assert!(hits > 100, "most rays meet a crate: {hits}");
}

/// A walker held right and a puck thrown at a wall, run for a second and a half-step at `fps`:
/// where each ended up and how many steps it took.
fn run_at(fps: f64) -> ((f64, f64), (f64, f64), u64) {
    let d = drawing();
    let wall = solid("wall", (300.0, -100.0), Shape::Rect(4.0, 200.0), (0.0, 0.0), &d);
    let puck = EntitySpec {
        loose: Some(Material { bounce: 0.9, friction: 0.1, grip: Some(0.3), density: 1.0, mass: None, center: None }),
        ..solid("puck", (0.0, 40.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let hero = EntitySpec {
        controller: Some(wasd(100.0)),
        ..solid("hero", (0.0, -40.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let mut world = World2d::default();
    world.reconcile(vec![wall, puck, hero]).unwrap();
    world.tick(0.0, 0.0);
    world.set("puck", Set { pos: None, velocity: Some((500.0, 30.0)), spin: None }).unwrap();
    world.key("KeyD", true);
    let frames = ((1.0 + 0.5 / 120.0) * fps).floor() as usize;
    for i in 1..=frames {
        world.tick(i as f64 / fps, 1.0 / fps);
    }
    let at = |id| world.inspect().into_iter().find(|e| e.id == id).unwrap().pos;
    (at("hero"), at("puck"), world.steps())
}

#[test]
fn the_same_second_ends_the_same_at_any_frame_rate() {
    let sixty = run_at(60.0);
    assert_eq!(sixty.2, 120, "a step is 1/120 s");
    assert!((sixty.0 .0 - 100.0).abs() < 1e-3, "a second at 100 per second: {sixty:?}");
    assert!(sixty.1 .0 < 290.0, "the puck came back off the wall: {sixty:?}");
    for fps in [30.0, 144.0, 120.0, 240.0] {
        assert_eq!(run_at(fps), sixty, "at {fps} fps");
    }
}

#[test]
fn a_stall_runs_at_most_eight_steps_and_drops_the_rest() {
    let mut world = World2d::default();
    world.reconcile(vec![controlled(&drawing(), wasd(120.0))]).unwrap();
    world.key("KeyD", true);
    world.tick(1.0, 1.0);
    assert_eq!(world.steps(), 8);
    assert!((world.transform("hero").unwrap().x - 8.0).abs() < 1e-9, "eight steps of one");
    assert!((world.dropped() - (1.0 - 8.0 / 120.0)).abs() < 1e-9, "{}", world.dropped());
}

#[test]
fn time_short_of_a_step_carries_to_the_next_frame() {
    let mut world = World2d::default();
    world.reconcile(vec![controlled(&drawing(), wasd(120.0))]).unwrap();
    world.key("KeyD", true);
    world.tick(1.0 / 240.0, 1.0 / 240.0);
    assert_eq!((world.steps(), world.transform("hero").unwrap().x), (0, 0.0), "half a step: none yet");
    world.tick(2.0 / 240.0, 1.0 / 240.0);
    assert_eq!(world.steps(), 1, "the halves make one");
    assert!((world.transform("hero").unwrap().x - 1.0).abs() < 1e-9);
    assert_eq!(world.dropped(), 0.0);
}

/// `grass + 1000` small things on a 4000×4000 map: `grass` drawings alone (grass, coins), 800
/// fixed solids (rocks), 200 loose balls inside four walls.
fn crowd(d: &Arc<Drawing>, grass: usize) -> Vec<EntitySpec> {
    let d = d.clone();
    let mut specs = vec![];
    let wall = |id: &str, pos, w, h| solid(id, pos, Shape::Rect(w, h), (0.0, 0.0), &d);
    specs.push(wall("north", (0.0, -10.0), 4000.0, 10.0));
    specs.push(wall("south", (0.0, 4000.0), 4000.0, 10.0));
    specs.push(wall("west", (-10.0, 0.0), 10.0, 4000.0));
    specs.push(wall("east", (4000.0, 0.0), 10.0, 4000.0));
    let across = (grass as f64).sqrt().ceil().max(1.0) as usize;
    for i in 0..grass {
        let step = 4000.0 / across as f64;
        let at = ((i % across) as f64 * step + 3.0, (i / across) as f64 * step + 3.0);
        specs.push(spec(&format!("grass{i}"), at, &d));
    }
    for i in 0..800 {
        let at = ((i % 40) as f64 * 100.0 + 50.0, (i / 40) as f64 * 200.0 + 20.0);
        specs.push(solid(&format!("rock{i}"), at, Shape::Rect(8.0, 8.0), (0.0, 0.0), &d));
    }
    for i in 0..200 {
        let at = ((i % 20) as f64 * 200.0 + 100.0, (i / 20) as f64 * 400.0 + 120.0);
        specs.push(EntitySpec {
            loose: Some(Material { bounce: 0.9, friction: 0.05, grip: None, density: 1.0, mass: None, center: None }),
            ..solid(&format!("ball{i}"), at, Shape::Circle(4.0), (4.0, 4.0), &d)
        });
    }
    specs
}

/// Times `f` over `runs` runs after one to warm up, and prints `BENCH <name> <µs>` for
/// `scripts/bench.py`. Benchmarks are `#[ignore]`d `bench_*` tests; run them through the script,
/// which builds release (debug is 10–30× off and says so).
fn bench(name: &str, runs: u32, mut f: impl FnMut()) {
    f();
    let started = std::time::Instant::now();
    for _ in 0..runs {
        f();
    }
    let us = started.elapsed().as_secs_f64() * 1e6 / f64::from(runs);
    let debug = if cfg!(debug_assertions) { " debug" } else { "" };
    println!("BENCH {name} {us:.1}{debug}");
}

/// `n` coins in a row, for groups.
fn coins(n: usize, d: &Arc<Drawing>) -> Vec<EntitySpec> {
    (0..n).map(|i| spec(&format!("coin{i}"), (i as f64 * 10.0, 50.0), d)).collect()
}

#[test]
fn a_kept_group_stays_as_it_was_while_the_rest_is_described_afresh() {
    let d = drawing();
    let mut world = World2d::default();
    world.reconcile_parts(vec![
        Part::Specs(vec![controlled(&d, wasd(100.0))]),
        Part::Group("coins".into(), Some(coins(3, &d))),
        Part::Specs(vec![spec("tree", (0.0, 90.0), &d)]),
    ]).unwrap();
    assert_eq!(ids(&world), ["hero", "coin0", "coin1", "coin2", "tree"], "parts in order");
    world.key("KeyD", true);
    frames(&mut world, 0.1, 0.1);
    // The coins are kept; the hero and the tree are described again, the tree moved on.
    world.reconcile_parts(vec![
        Part::Specs(vec![controlled(&d, wasd(100.0))]),
        Part::Group("coins".into(), None),
    ]).unwrap();
    assert_eq!(ids(&world), ["hero", "coin0", "coin1", "coin2"], "the tree went; the coins stayed");
    assert!(world.transform("hero").unwrap().x > 9.0, "the hero kept walking");
    assert!(world.needs_ticks(), "a kept group does not hide the hero's controller");
    // Described afresh without a coin: that coin goes.
    world.reconcile_parts(vec![
        Part::Specs(vec![controlled(&d, wasd(100.0))]),
        Part::Group("coins".into(), Some(coins(2, &d))),
    ]).unwrap();
    assert_eq!(ids(&world), ["hero", "coin0", "coin1"]);
    // A group not described at all goes, with all its entities.
    world.reconcile_parts(vec![Part::Specs(vec![controlled(&d, wasd(100.0))])]).unwrap();
    assert_eq!(ids(&world), ["hero"]);
}

#[test]
fn an_entity_moving_between_parts_is_the_same_entity() {
    let d = drawing();
    let mut world = World2d::default();
    let clip = slide();
    let chest = || EntitySpec { clip: Some(clip.clone()), ..spec("chest", (0.0, 0.0), &d) };
    world.reconcile_parts(vec![Part::Group("loot".into(), Some(vec![chest()]))]).unwrap();
    world.tick(0.5, 0.0);
    world.reconcile_parts(vec![Part::Specs(vec![chest()]), Part::Group("loot".into(), Some(vec![]))]).unwrap();
    world.tick(0.5, 0.0);
    assert_eq!(at(&world, 56.0).as_deref(), Some("chest"), "its clip kept playing from where it was");
}

#[test]
fn parts_are_checked_together() {
    let d = drawing();
    let mut world = World2d::default();
    world.reconcile_parts(vec![Part::Group("coins".into(), Some(coins(2, &d)))]).unwrap();
    let err = |w: &mut World2d, parts| w.reconcile_parts(parts).unwrap_err().to_string();
    // An id in a kept group cannot also be described elsewhere.
    let dup = err(&mut world, vec![Part::Specs(vec![spec("coin1", (0.0, 0.0), &d)]), Part::Group("coins".into(), None)]);
    assert!(dup.contains("duplicate entity id \"coin1\""), "{dup}");
    // Kept, it must have been described before; and a group appears once.
    let unknown = err(&mut world, vec![Part::Group("gems".into(), None)]);
    assert!(unknown.contains("group \"gems\" is kept but was never described"), "{unknown}");
    let twice = err(&mut world, vec![Part::Group("coins".into(), None), Part::Group("coins".into(), None)]);
    assert!(twice.contains("two groups share the id \"coins\""), "{twice}");
    // A bad entity in a fresh group leaves the whole world as it was.
    let bad = EntitySpec { id: String::new(), ..spec("x", (0.0, 0.0), &d) };
    assert!(world.reconcile_parts(vec![Part::Group("coins".into(), Some(vec![bad]))]).is_err());
    assert_eq!(ids(&world), ["coin0", "coin1"]);
}

#[test]
fn an_attach_reaches_across_parts_and_a_kept_rider_needs_its_carrier() {
    let d = drawing();
    let mut world = World2d::default();
    let hat = || carried("hero", "body", &d);
    let hero = || spec("hero", (0.0, 0.0), &d);
    // The hat rides in a group; its carrier is described outside it.
    world.reconcile_parts(vec![Part::Specs(vec![hero()]), Part::Group("gear".into(), Some(vec![hat()]))]).unwrap();
    world.reconcile_parts(vec![Part::Specs(vec![hero()]), Part::Group("gear".into(), None)]).unwrap();
    // A fresh rider on a kept carrier.
    world.reconcile_parts(vec![Part::Group("people".into(), Some(vec![hero()])), Part::Specs(vec![hat()])]).unwrap();
    world.reconcile_parts(vec![Part::Group("people".into(), None), Part::Specs(vec![hat()])]).unwrap();
    // The carrier gone while its rider's group is kept: refused, the world unchanged.
    world.reconcile_parts(vec![Part::Specs(vec![hero()]), Part::Group("gear".into(), Some(vec![hat()]))]).unwrap();
    let err = world.reconcile_parts(vec![Part::Group("gear".into(), None)]).unwrap_err().to_string();
    assert!(err.contains("rides \"hero\", which the world no longer describes"), "{err}");
    assert_eq!(ids(&world), ["hero", "chest"]);
}


/// A crowd of `grass + 1000`, spawned, its balls thrown, stepped once.
fn crowded(d: &Arc<Drawing>, grass: usize) -> World2d {
    let mut world = World2d::default();
    world.reconcile(crowd(d, grass)).unwrap();
    world.tick(0.0, 0.0);
    for i in 0..200 {
        let v = ((i * 37 % 400) as f64 - 200.0, (i * 91 % 400) as f64 - 200.0);
        world.set(&format!("ball{i}"), Set { pos: None, velocity: Some(v), spin: None }).unwrap();
    }
    world.tick(1.0 / 60.0, 1.0 / 60.0);
    world
}

#[test]
#[ignore = "benchmark: scripts/bench.py"]
fn bench_world_steps() {
    let mut world = crowded(&drawing(), 4000);
    let mut t = 1.0 / 60.0;
    bench("world_steps_5000", 120, || {
        t += 1.0 / 60.0;
        world.tick(t, 1.0 / 60.0);
        world.drain_events();
    });
}

#[test]
#[ignore = "benchmark: scripts/bench.py"]
fn bench_world_reconcile() {
    let d = drawing();
    let mut world = crowded(&d, 4000);
    // Specs built outside the timing: this is the diff alone.
    let mut described: Vec<_> = (0..21).map(|_| crowd(&d, 4000)).collect();
    bench("world_reconcile_fresh_5000", 20, || world.reconcile(described.pop().unwrap()).unwrap());
    let mut world = World2d::default();
    let hero = || controlled(&d, wasd(100.0));
    world.reconcile_parts(vec![Part::Specs(vec![hero()]), Part::Group("all".into(), Some(crowd(&d, 4000)))]).unwrap();
    bench("world_reconcile_kept_5000", 200, || {
        world.reconcile_parts(vec![Part::Specs(vec![hero()]), Part::Group("all".into(), None)]).unwrap();
    });
}

/// Every entity drawn: the box is the whole map, so nothing is culled.
#[test]
#[ignore = "benchmark: scripts/bench.py"]
fn bench_world_frame() {
    let d = drawing();
    for grass in [4000, 19000] {
        let world = crowded(&d, grass);
        let name = format!("world_frame_{}", grass + 1000);
        bench(&name, 20, || {
            world.frame(4010.0, 4010.0).unwrap();
        });
    }
}

#[test]
#[ignore = "benchmark: scripts/bench.py"]
fn bench_world_inspect() {
    let world = crowded(&drawing(), 4000);
    bench("world_inspect_5000", 20, || {
        world.inspect();
    });
}

#[test]
#[ignore = "benchmark: scripts/bench.py"]
fn bench_world_rays() {
    let world = crates();
    bench("world_rays_1000", 20, || {
        thousand_rays(&world);
    });
}

const VIEW: (f64, f64) = (1280.0, 720.0);

fn follow(id: &str) -> CameraSpec {
    CameraSpec {
        follow: Some(id.into()),
        ..CameraSpec::default()
    }
}

/// A walking hero at `(x, y)` on a map many screens wide, seen through `camera`.
fn on_map(x: f64, y: f64, camera: CameraSpec) -> World2d {
    let d = drawing();
    let mut world = World2d::default();
    let hero = EntitySpec {
        pos: (x, y),
        ..controlled(&d, wasd(100.0))
    };
    world.reconcile(vec![hero]).unwrap();
    world.describe_camera(VIEW, Some(camera)).unwrap();
    world
}

fn camera_at(world: &World2d) -> (f64, f64) {
    world.camera().unwrap().at
}

/// Which entity is on screen at `(x, 360)`, the view's middle row.
fn on_screen(world: &World2d, x: f64) -> Option<String> {
    let hit = world.frame(VIEW.0, VIEW.1).unwrap().hit(Point::new(x, 360.0))?;
    Some(hit.id.to_string())
}

/// C1.
#[test]
fn a_following_camera_centres_its_target_from_the_first_frame() {
    let mut world = on_map(2000.0, 1500.0, follow("hero"));
    assert_eq!(camera_at(&world), (2004.0, 1504.0), "the box's centre, not eased in from 0, 0");
    // The test drawing covers x 4..8 on its row y = 4; its box's top-left is drawn 4 up and 4
    // left of the view's centre.
    assert_eq!(on_screen(&world, 642.0).as_deref(), Some("hero"));
    assert_eq!(on_screen(&world, 637.0), None);
    world.key("KeyD", true);
    frames(&mut world, 1.0, 1.0);
    let t = world.transform("hero").unwrap();
    assert!(t.x > 2090.0);
    assert!(near(camera_at(&world), (t.x + 4.0, t.y + 4.0)), "without ease, exactly on it");
    assert_eq!(on_screen(&world, 642.0).as_deref(), Some("hero"), "still in the middle");
}

/// C2.
#[test]
fn an_eased_camera_closes_the_same_share_of_the_gap_each_step() {
    let eased = CameraSpec {
        ease: Some(8.0),
        ..follow("hero")
    };
    let mut world = on_map(1000.0, 500.0, eased.clone());
    let there = Set {
        pos: Some((1200.0, 500.0)),
        ..Set::default()
    };
    world.set("hero", there).unwrap();
    world.tick(STEP, STEP);
    let share = 1.0 - (-8.0 * STEP).exp();
    assert!(near(camera_at(&world), (1004.0 + 200.0 * share, 504.0)), "{:?}", camera_at(&world));

    let a_second_at = |fps: f64| {
        let mut world = on_map(1000.0, 500.0, eased.clone());
        world.key("KeyD", true);
        for i in 1..=fps as usize {
            world.tick(i as f64 / fps, 1.0 / fps);
        }
        camera_at(&world)
    };
    let at60 = a_second_at(60.0);
    assert!(at60.0 > 1004.0 && at60.0 < 1104.0, "behind the walking hero: {at60:?}");
    for fps in [30.0, 144.0, 240.0] {
        let at = a_second_at(fps);
        assert!(near(at, at60), "{fps} fps: {at:?}, at 60: {at60:?}");
    }
}

#[test]
fn an_easing_camera_asks_for_ticks_only_until_it_arrives() {
    let d = drawing();
    let mut world = World2d::default();
    world.reconcile(vec![spec("chest", (1000.0, 500.0), &d)]).unwrap();
    let eased = CameraSpec {
        ease: Some(8.0),
        ..follow("chest")
    };
    world.describe_camera(VIEW, Some(eased)).unwrap();
    assert!(!world.needs_ticks(), "already there");
    let there = Set {
        pos: Some((1500.0, 500.0)),
        ..Set::default()
    };
    world.set("chest", there).unwrap();
    assert!(world.needs_ticks(), "behind");
    frames(&mut world, 3.0, 3.0);
    assert_eq!(camera_at(&world), (1504.0, 504.0), "arrived, exactly");
    assert!(!world.needs_ticks());
}

/// C3.
#[test]
fn bounds_stop_the_camera_at_the_maps_edge() {
    let map = CameraSpec {
        bounds: Some((0.0, 0.0, 4000.0, 3000.0)),
        ..follow("hero")
    };
    let world = on_map(10.0, 2990.0, map.clone());
    assert_eq!(camera_at(&world), (640.0, 2640.0), "the view's corner on the map's");
    let world = on_map(3990.0, 10.0, map);
    assert_eq!(camera_at(&world), (3360.0, 360.0));
    let small = CameraSpec {
        bounds: Some((100.0, 100.0, 800.0, 400.0)),
        ..follow("hero")
    };
    let world = on_map(10.0, 10.0, small);
    assert_eq!(camera_at(&world), (500.0, 300.0), "a map smaller than the view is centred");
}

/// C4.
#[test]
fn a_lost_target_holds_the_camera_until_it_comes_back() {
    let d = drawing();
    let boss = |x| spec("boss", (x, 1000.0), &d);
    let mut world = World2d::default();
    world.reconcile(vec![boss(2000.0)]).unwrap();
    let eased = CameraSpec {
        ease: Some(8.0),
        ..follow("boss")
    };
    world.describe_camera(VIEW, Some(eased)).unwrap();
    world.reconcile(vec![]).unwrap();
    frames(&mut world, 1.0, 1.0);
    let c = world.camera().unwrap();
    assert_eq!((c.at, c.follow.as_deref(), c.lost), ((2004.0, 1004.0), Some("boss"), true));
    world.reconcile(vec![boss(3000.0)]).unwrap();
    frames(&mut world, 2.0, 1.0);
    let c = world.camera().unwrap();
    assert!(!c.lost);
    assert!(c.at.0 > 2900.0 && c.at.0 <= 3004.0, "eased back to it: {:?}", c.at);

    let mut world = World2d::default();
    world.describe_camera(VIEW, Some(follow("ghost"))).unwrap();
    let c = world.camera().unwrap();
    assert_eq!((c.at, c.lost), ((640.0, 360.0), true), "lost from the start: shows 0, 0 on");
}

#[test]
fn set_camera_holds_until_the_description_changes() {
    let d = drawing();
    let mut world = World2d::default();
    let both = vec![spec("hero", (1000.0, 500.0), &d), spec("boss", (3000.0, 900.0), &d)];
    world.reconcile(both).unwrap();
    world.describe_camera(VIEW, Some(follow("hero"))).unwrap();
    world.set_camera(Look::At((2000.0, 1000.0))).unwrap();
    let c = world.camera().unwrap();
    assert_eq!((c.at, c.follow), ((2000.0, 1000.0), None), "looking here clears follow");
    world.describe_camera(VIEW, Some(follow("hero"))).unwrap();
    frames(&mut world, 1.0, 1.0);
    assert_eq!(camera_at(&world), (2000.0, 1000.0), "the same description does not undo it");
    world.set_camera(Look::Follow("boss".into())).unwrap();
    frames(&mut world, 2.0, 1.0);
    assert_eq!(camera_at(&world), (3004.0, 904.0));
    let changed = CameraSpec {
        ease: Some(4.0),
        ..follow("hero")
    };
    world.describe_camera(VIEW, Some(changed)).unwrap();
    assert_eq!(world.camera().unwrap().follow.as_deref(), Some("hero"), "a change applies");
    world.describe_camera(VIEW, None).unwrap();
    assert!(world.camera().is_none(), "described away");
}

#[test]
fn a_bad_camera_is_refused_and_changes_nothing() {
    let mut world = World2d::default();
    let bad = [
        CameraSpec {
            at: Some((0.0, 0.0)),
            ..follow("hero")
        },
        CameraSpec {
            ease: Some(0.0),
            ..follow("hero")
        },
        CameraSpec {
            ease: Some(f64::NAN),
            ..CameraSpec::default()
        },
        CameraSpec {
            bounds: Some((0.0, 0.0, -1.0, 10.0)),
            ..CameraSpec::default()
        },
        CameraSpec {
            at: Some((f64::INFINITY, 0.0)),
            ..CameraSpec::default()
        },
    ];
    for spec in bad {
        assert!(world.describe_camera(VIEW, Some(spec.clone())).is_err(), "{spec:?}");
    }
    assert!(world.camera().is_none());
    assert!(world.set_camera(Look::At((0.0, 0.0))).is_err(), "no camera to point");
    world.describe_camera(VIEW, Some(CameraSpec::default())).unwrap();
    assert!(world.set_camera(Look::At((f64::NAN, 0.0))).is_err());
}

/// Part of C7.
#[test]
fn screen_and_world_points_convert_through_the_camera() {
    let world = on_map(2000.0, 1500.0, follow("hero"));
    assert_eq!(world.to_world((640.0, 360.0)), (2004.0, 1504.0));
    assert_eq!(world.to_screen(world.to_world((10.0, 20.0))), (10.0, 20.0));
    assert_eq!(World2d::default().to_world((10.0, 20.0)), (10.0, 20.0), "no camera: the same");
}

/// A 100 × 100 grid of the 8 × 8 test drawing, 40 apart: a 4000 × 4000 map, `t{col + 100 row}`.
fn grid_map(at: (f64, f64)) -> World2d {
    let d = drawing();
    let mut world = World2d::default();
    let grid = (0..10_000)
        .map(|i| spec(&format!("t{i}"), ((i % 100) as f64 * 40.0, (i / 100) as f64 * 40.0), &d))
        .collect();
    world.reconcile(grid).unwrap();
    let look = CameraSpec {
        at: Some(at),
        ..CameraSpec::default()
    };
    world.describe_camera(VIEW, Some(look)).unwrap();
    world
}

/// C6.
#[test]
fn a_camera_draws_only_what_meets_the_view() {
    // The view is x 1360..2640, y 1640..2360. A box x..x+8 grown by 4 each side meets it for
    // x in (1348, 2644) — columns 34..=66 — and y in (1628, 2364) — rows 41..=59.
    let world = grid_map((2000.0, 2000.0));
    let frame = world.frame(VIEW.0, VIEW.1).unwrap();
    assert_eq!(frame.stats().hittable, 33 * 19, "of 10000");
    assert_eq!(world.camera().unwrap().drawn, 33 * 19);

    // Moved 4 right, column 34 (x 1360..1368) straddles the left edge: its visible half draws.
    let world = grid_map((2004.0, 2000.0));
    let hit = world.frame(VIEW.0, VIEW.1).unwrap().hit(Point::new(2.0, 364.0));
    assert_eq!(hit.map(|h| h.id.to_string()).as_deref(), Some("t5034"));
}

#[test]
fn a_world_without_a_camera_draws_only_what_meets_its_box() {
    let d = drawing();
    let mut world = World2d::default();
    world.reconcile(vec![spec("in", (10.0, 10.0), &d), spec("out", (500.0, 10.0), &d)]).unwrap();
    assert_eq!(world.frame(200.0, 100.0).unwrap().stats().hittable, 1);
}

/// W-L2: one physics step of a driven cart on a 2000-point, 200 000-unit hilly line.
#[test]
#[ignore = "benchmark: scripts/bench.py"]
fn bench_cart_on_long_line() {
    let cart = Cart { stiffness: 5000.0, damping: 300.0, travel: (15.0, 20.0), drop: 0.0, grip: 1.0 };
    // Gentle hills, every point within 30 of the flat ground's y 40 the cart starts on.
    let hills: Vec<_> = (0..2000)
        .map(|i| {
            let x = i as f64 * 100.0;
            (x, 30.0 * (x / 700.0).sin() * (x / 3100.0).cos())
        })
        .collect();
    let mut world = cart.world_on((-1000.0, 40.0), &hills);
    let go = Motor { speed: 1440.0, torque: 2e5 };
    cart.drive(&mut world, Some(go), Some(go));
    let mut clock = 0.0;
    bench("cart_on_long_line_step", 1200, || {
        clock += STEP;
        world.tick(clock, STEP);
    });
}

/// B7: the frame of the 20000-entity crowd through a 1280 × 720 camera — about 6% of the map.
#[test]
#[ignore = "benchmark: scripts/bench.py"]
fn bench_world_frame_camera() {
    let mut world = crowded(&drawing(), 19000);
    let look = CameraSpec {
        at: Some((2000.0, 2000.0)),
        ..CameraSpec::default()
    };
    world.describe_camera(VIEW, Some(look)).unwrap();
    bench("world_frame_20000_camera", 20, || {
        world.frame(VIEW.0, VIEW.1).unwrap();
    });
}

// ── gravity ────────────────────────────────────────────────────────────────────────────────────

/// A loose ball (radius 4, box 8) and a fixed floor whose top is at `floor_y`.
fn ball_over_floor(floor_y: f64, ball: Material) -> (Vec<EntitySpec>, Arc<Drawing>) {
    let d = drawing();
    let ball = EntitySpec {
        loose: Some(ball),
        ..solid("ball", (50.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let floor = solid("floor", (0.0, floor_y), Shape::Rect(400.0, 20.0), (0.0, 0.0), &d);
    (vec![ball, floor], d)
}

const DEAD: Material = Material { bounce: 0.0, friction: 0.0, grip: None, density: 1.0, mass: None, center: None };

/// Ticks `secs` of 1/60 s frames from `*clock`.
fn run(world: &mut World2d, clock: &mut f64, secs: f64) {
    for _ in 0..(secs * 60.0).round() as usize {
        *clock += 1.0 / 60.0;
        world.tick(*clock, 1.0 / 60.0);
    }
}

fn ball_of(world: &World2d) -> EntityInspection {
    world.inspect().into_iter().find(|e| e.id == "ball").expect("a ball")
}

#[test]
fn gravity_pulls_a_loose_thing_down_and_it_rests_on_the_floor() {
    let (specs, _) = ball_over_floor(500.0, DEAD);
    let mut world = World2d::default();
    world.set_gravity((0.0, 900.0)).unwrap();
    world.reconcile(specs).unwrap();
    let mut clock = 0.0;
    run(&mut world, &mut clock, 0.5);
    // Free fall: about ½·g·t² = 112.5 in half a second.
    let fallen = ball_of(&world).pos.1;
    assert!((fallen - 112.5).abs() < 15.0, "fell {fallen}");
    run(&mut world, &mut clock, 3.0);
    let b = ball_of(&world);
    assert!((b.pos.1 + 8.0 - 500.0).abs() < 2.0, "rests on the floor: {b:?}");
    assert_eq!((b.body, b.velocity), ("loose", (0.0, 0.0)), "asleep and still: {b:?}");
}

#[test]
fn no_gravity_set_keeps_the_old_top_down_behaviour() {
    let (specs, _) = ball_over_floor(500.0, DEAD);
    let mut world = World2d::default();
    world.reconcile(specs).unwrap();
    assert_eq!(world.gravity(), (0.0, 0.0));
    world.set("ball", Set { velocity: Some((100.0, 0.0)), ..Set::default() }).unwrap();
    let mut clock = 0.0;
    run(&mut world, &mut clock, 1.0);
    let b = ball_of(&world);
    assert!((b.pos.0 - 150.0).abs() < 3.0 && b.pos.1.abs() < 0.5, "glided straight: {b:?}");
}

#[test]
fn a_thing_spawned_in_a_gravity_world_starts_awake_and_falls() {
    let (specs, _) = ball_over_floor(500.0, DEAD);
    let mut world = World2d::default();
    world.set_gravity((0.0, 900.0)).unwrap();
    world.reconcile(specs).unwrap();
    assert!(world.needs_ticks(), "a falling world needs the clock");
    let mut clock = 0.0;
    run(&mut world, &mut clock, 0.3);
    assert!(ball_of(&world).pos.1 > 20.0, "it fell: {:?}", ball_of(&world));
}

#[test]
fn changing_gravity_wakes_the_sleepers() {
    let (specs, _) = ball_over_floor(500.0, DEAD);
    let mut world = World2d::default();
    world.set_gravity((0.0, 900.0)).unwrap();
    world.reconcile(specs).unwrap();
    let mut clock = 0.0;
    run(&mut world, &mut clock, 3.0);
    let rested = ball_of(&world).pos.1;
    assert!(!world.needs_ticks(), "at rest, no clock");
    world.set_gravity((0.0, -900.0)).unwrap();
    assert!(world.needs_ticks(), "a flip wakes the world: the frame clock has to start");
    run(&mut world, &mut clock, 0.5);
    assert!(ball_of(&world).pos.1 < rested - 20.0, "it rose: {rested} -> {:?}", ball_of(&world));
}

#[test]
fn a_bad_gravity_is_refused_and_the_last_good_one_kept() {
    let mut world = World2d::default();
    world.set_gravity((0.0, 900.0)).unwrap();
    for bad in [(f64::NAN, 0.0), (0.0, f64::INFINITY), (0.0, 1.0e9)] {
        let err = world.set_gravity(bad).unwrap_err().to_string();
        assert!(err.contains("gravity"), "{err}");
        assert_eq!(world.gravity(), (0.0, 900.0));
    }
}

// ── density ────────────────────────────────────────────────────────────────────────────────────

#[test]
fn a_denser_thing_flings_a_lighter_one_further_than_it_slows() {
    let d = drawing();
    let ball = |id: &str, x: f64, density: f64| EntitySpec {
        loose: Some(Material { bounce: 1.0, friction: 0.0, grip: None, density, mass: None, center: None }),
        ..solid(id, (x, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let mut world = World2d::default();
    world.reconcile(vec![ball("heavy", 0.0, 10.0), ball("light", 30.0, 1.0)]).unwrap();
    world.set("heavy", Set { velocity: Some((200.0, 0.0)), ..Set::default() }).unwrap();
    let mut clock = 0.0;
    run(&mut world, &mut clock, 0.5);
    let vx = |id: &str| world.inspect().into_iter().find(|e| e.id == id).unwrap().velocity.0;
    // An elastic head-on meeting of 10 and 1: the light one leaves at 20/11 of the heavy one's
    // speed, the heavy one at 9/11 of it.
    assert!(vx("light") > 300.0, "flung: {}", vx("light"));
    assert!(vx("heavy") > 130.0, "hardly slowed: {}", vx("heavy"));
}

#[test]
fn a_bad_density_is_refused() {
    let d = drawing();
    let crate_ = |density| EntitySpec {
        loose: Some(Material { density, ..Material::default() }),
        ..solid("crate", (0.0, 0.0), Shape::Rect(8.0, 8.0), (0.0, 0.0), &d)
    };
    let mut world = World2d::default();
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let err = world.reconcile(vec![crate_(bad)]).unwrap_err().to_string();
        assert!(err.contains("density must be above 0"), "{bad}: {err}");
    }
}

// ── forces: push, blast, wells ─────────────────────────────────────────────────────────────────

/// Loose balls (radius 4) by their centres, at rest and friction-free, in a world with no gravity.
fn balls_at(centres: &[(&str, f64, f64)]) -> World2d {
    let d = drawing();
    let specs = centres
        .iter()
        .map(|&(id, x, y)| EntitySpec {
            loose: Some(DEAD),
            ..solid(id, (x - 4.0, y - 4.0), Shape::Circle(4.0), (4.0, 4.0), &d)
        })
        .collect();
    let mut world = World2d::default();
    world.reconcile(specs).unwrap();
    world
}

fn entity(world: &World2d, id: &str) -> EntityInspection {
    world.inspect().into_iter().find(|e| e.id == id).unwrap_or_else(|| panic!("no {id}"))
}

#[test]
fn a_push_adds_to_a_loose_things_velocity_and_wakes_it() {
    let mut world = balls_at(&[("ball", 50.0, 50.0)]);
    assert!(!world.needs_ticks(), "asleep");
    world.push("ball", (100.0, 0.0)).unwrap();
    assert_eq!(entity(&world, "ball").velocity, (100.0, 0.0));
    world.push("ball", (0.0, -40.0)).unwrap();
    assert_eq!(entity(&world, "ball").velocity, (100.0, -40.0), "adds, not sets");
    let mut clock = 0.0;
    run(&mut world, &mut clock, 1.0);
    let b = entity(&world, "ball");
    assert!((b.pos.0 - 146.0).abs() < 3.0 && (b.pos.1 - 6.0).abs() < 3.0, "went on: {b:?}");
}

#[test]
fn a_push_needs_a_loose_thing() {
    let d = drawing();
    let mut world = World2d::default();
    world.reconcile(vec![solid("wall", (0.0, 0.0), Shape::Rect(8.0, 8.0), (0.0, 0.0), &d)]).unwrap();
    let err = world.push("wall", (1.0, 0.0)).unwrap_err().to_string();
    assert!(err.contains("push needs a loose thing"), "{err}");
    let err = world.push("ghost", (1.0, 0.0)).unwrap_err().to_string();
    assert!(err.contains("there is no such entity"), "{err}");
    let mut world = balls_at(&[("ball", 0.0, 0.0)]);
    let err = world.push("ball", (f64::NAN, 0.0)).unwrap_err().to_string();
    assert!(err.contains("finite"), "{err}");
}

#[test]
fn a_blast_throws_what_is_near_away_and_leaves_the_rest() {
    let mut world = balls_at(&[
        ("near", 220.0, 200.0),  // 20 right of the blast
        ("above", 200.0, 150.0), // 50 above it
        ("far", 350.0, 200.0),   // beyond the radius
        ("centre", 200.0, 200.0), // exactly on it
    ]);
    let moved = world.blast((200.0, 200.0), 300.0, 100.0).unwrap();
    assert_eq!(moved, 3, "near, above and the one on it");
    let (near, above) = (entity(&world, "near").velocity, entity(&world, "above").velocity);
    assert!((near.0 - 240.0).abs() < 1e-6 && near.1.abs() < 1e-6, "300·(1 − 20/100), away: {near:?}");
    assert!(above.0.abs() < 1e-6 && (above.1 + 150.0).abs() < 1e-6, "300·(1 − 50/100), up: {above:?}");
    assert_eq!(entity(&world, "far").velocity, (0.0, 0.0));
    let on = entity(&world, "centre").velocity;
    assert!(on.0.is_finite() && on.1 < -100.0, "nudged up, never NaN: {on:?}");
}

#[test]
fn a_bad_blast_is_refused() {
    let mut world = balls_at(&[("ball", 0.0, 0.0)]);
    for (at, speed, radius, wanted) in [
        ((f64::NAN, 0.0), 1.0, 1.0, "finite"),
        ((0.0, 0.0), f64::INFINITY, 1.0, "finite"),
        ((0.0, 0.0), 1.0, 0.0, "radius must be above 0"),
        ((0.0, 0.0), 1.0, -5.0, "radius must be above 0"),
    ] {
        let err = world.blast(at, speed, radius).unwrap_err().to_string();
        assert!(err.contains(wanted), "{err}");
    }
}

#[test]
fn a_well_pulls_what_is_inside_its_radius_toward_its_centre() {
    let mut world = balls_at(&[("in", 260.0, 200.0), ("out", 350.0, 200.0), ("edge", 200.0, 110.0)]);
    world.set_wells(vec![Well { at: (200.0, 200.0), radius: 100.0, pull: 600.0 }]).unwrap();
    let mut clock = 0.0;
    run(&mut world, &mut clock, 0.1);
    // At 60 away the pull is 600·(1 − 0.6) = 240, so about −24 a tenth of a second on, and more
    // as it nears the centre.
    let v = entity(&world, "in").velocity;
    assert!(v.0 < -20.0 && v.1.abs() < 1.0, "pulled left: {v:?}");
    assert_eq!(entity(&world, "out").velocity, (0.0, 0.0), "beyond the radius: untouched");
    let top = entity(&world, "edge").velocity;
    assert!(top.1 > 0.0 && top.0.abs() < 1.0, "90 above is near the edge: pulled down, weakly: {top:?}");
}

#[test]
fn a_negative_well_repels_and_removing_it_ends_the_pull() {
    let mut world = balls_at(&[("ball", 230.0, 200.0)]);
    world.set_wells(vec![Well { at: (200.0, 200.0), radius: 100.0, pull: -600.0 }]).unwrap();
    let mut clock = 0.0;
    run(&mut world, &mut clock, 0.1);
    let v = entity(&world, "ball").velocity;
    assert!(v.0 > 20.0, "pushed away: {v:?}");
    world.set_wells(vec![]).unwrap();
    run(&mut world, &mut clock, 0.2);
    let after = entity(&world, "ball").velocity;
    assert!((after.0 - v.0).abs() < 1.0, "no well, no more change (friction-free): {v:?} -> {after:?}");
}

#[test]
fn a_bad_well_is_refused_and_the_last_good_ones_kept() {
    let mut world = balls_at(&[("ball", 0.0, 0.0)]);
    let fine = Well { at: (0.0, 0.0), radius: 10.0, pull: 5.0 };
    world.set_wells(vec![fine]).unwrap();
    for (bad, wanted) in [
        (Well { radius: 0.0, ..fine }, "radius must be above 0"),
        (Well { pull: f64::NAN, ..fine }, "finite"),
        (Well { at: (f64::INFINITY, 0.0), ..fine }, "finite"),
    ] {
        let err = world.set_wells(vec![fine, bad]).unwrap_err().to_string();
        assert!(err.contains("well 2") && err.contains(wanted), "{err}");
        assert_eq!(world.wells().len(), 1, "kept");
    }
}

// ── grabbing ───────────────────────────────────────────────────────────────────────────────────

fn centre_of(world: &World2d, id: &str) -> (f64, f64) {
    let e = entity(world, id);
    (e.pos.0 + 4.0, e.pos.1 + 4.0)
}

#[test]
fn a_grabbed_thing_is_brought_to_the_point_even_under_gravity() {
    let mut world = balls_at(&[("ball", 100.0, 100.0)]);
    world.set_gravity((0.0, 900.0)).unwrap();
    world.grab("ball", (300.0, 200.0)).unwrap();
    let mut clock = 0.0;
    run(&mut world, &mut clock, 1.0);
    let (x, y) = centre_of(&world, "ball");
    assert!((x - 300.0).abs() < 3.0 && (y - 200.0).abs() < 3.0, "held at the point: {x},{y}");
    // Let go, it falls: the hand was what held it.
    world.release("ball").unwrap();
    run(&mut world, &mut clock, 0.3);
    assert!(centre_of(&world, "ball").1 > 220.0, "{:?}", centre_of(&world, "ball"));
}

#[test]
fn letting_go_of_a_moving_grab_is_a_throw() {
    let mut world = balls_at(&[("ball", 100.0, 100.0)]);
    let mut clock = 0.0;
    // The hand drags it right at 200 a second for a second.
    for i in 0..60 {
        world.grab("ball", (100.0 + 200.0 * (i as f64 / 60.0), 100.0)).unwrap();
        run(&mut world, &mut clock, 1.0 / 60.0);
    }
    world.release("ball").unwrap();
    let thrown = entity(&world, "ball").velocity;
    assert!((thrown.0 - 200.0).abs() < 40.0, "it leaves at about the hand's speed: {thrown:?}");
    let at = centre_of(&world, "ball").0;
    run(&mut world, &mut clock, 0.5);
    assert!(centre_of(&world, "ball").0 > at + 70.0, "and goes on: {at} -> {:?}", centre_of(&world, "ball"));
}

#[test]
fn a_grab_is_refused_for_what_cannot_be_held_and_dropped_with_the_entity() {
    let d = drawing();
    let mut world = balls_at(&[("ball", 100.0, 100.0)]);
    let wall = || solid("wall", (0.0, 0.0), Shape::Rect(8.0, 8.0), (0.0, 0.0), &d);
    world.reconcile(vec![wall(), EntitySpec {
        loose: Some(DEAD),
        ..solid("ball", (96.0, 96.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    }]).unwrap();
    let err = world.grab("wall", (1.0, 1.0)).unwrap_err().to_string();
    assert!(err.contains("grab needs a loose thing"), "{err}");
    let err = world.grab("ghost", (1.0, 1.0)).unwrap_err().to_string();
    assert!(err.contains("there is no such entity"), "{err}");
    let err = world.grab("ball", (f64::NAN, 1.0)).unwrap_err().to_string();
    assert!(err.contains("finite"), "{err}");

    // Two grabs of one thing: the last point wins.
    world.grab("ball", (10.0, 10.0)).unwrap();
    world.grab("ball", (200.0, 50.0)).unwrap();
    assert_eq!(world.grabs(), 1);
    let mut clock = 0.0;
    run(&mut world, &mut clock, 1.0);
    let (x, y) = centre_of(&world, "ball");
    assert!((x - 200.0).abs() < 3.0 && (y - 50.0).abs() < 3.0, "{x},{y}");

    // The entity goes: the grab goes with it, and letting go of it is no error.
    world.reconcile(vec![wall()]).unwrap();
    run(&mut world, &mut clock, 0.1);
    assert_eq!(world.grabs(), 0);
    world.release("ball").unwrap();
}

// ── joints ─────────────────────────────────────────────────────────────────────────────────────

const GRIPPY: Material = Material { bounce: 0.0, friction: 0.0, grip: Some(0.5), density: 1.0, mass: None, center: None };

fn joint(id: &str, kind: JointKind, a: &str, a_at: (f64, f64), b: Option<&str>, b_at: (f64, f64)) -> JointSpec {
    JointSpec { id: id.into(), kind, a: a.into(), a_at, b: b.map(str::to_string), b_at, contacts: false, motor: None }
}

/// A plank (100 × 8, loose, with grip so it can turn) whose box top-left is `pos`.
fn plank(id: &str, pos: (f64, f64), d: &Arc<Drawing>) -> EntitySpec {
    EntitySpec {
        loose: Some(GRIPPY),
        ..solid(id, pos, Shape::Rect(100.0, 8.0), (0.0, 0.0), d)
    }
}

fn swing_world() -> World2d {
    let mut world = World2d::default();
    world.set_gravity((0.0, 900.0)).unwrap();
    world
}

#[test]
fn a_pin_lets_a_plank_swing_about_a_fixed_point() {
    let d = drawing();
    let mut world = swing_world();
    world.reconcile(vec![plank("plank", (100.0, 100.0), &d)]).unwrap();
    // Its left end, (0, 4) in its own box, pinned to the world at (100, 104).
    let pin = joint("hinge", JointKind::Pin { limits: None }, "plank", (0.0, 4.0), None, (100.0, 104.0));
    world.set_joints(vec![pin]).unwrap();
    let mut clock = 0.0;
    run(&mut world, &mut clock, 0.6);
    let p = entity(&world, "plank");
    assert!(p.rot.abs() > 60.0, "it swung down: {p:?}");
    // The pinned end stayed where it was pinned.
    let (cx, cy) = (p.pos.0 + 50.0, p.pos.1 + 4.0);
    let a = p.rot.to_radians();
    let end = (cx - 50.0 * a.cos(), cy - 50.0 * a.sin());
    assert!((end.0 - 100.0).abs() < 2.0 && (end.1 - 104.0).abs() < 2.0, "pinned end at {end:?}");
}

#[test]
fn a_rope_never_lets_a_thing_past_its_length() {
    let d = drawing();
    let mut world = swing_world();
    let ball = EntitySpec {
        loose: Some(DEAD),
        ..solid("ball", (256.0, 96.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    world.reconcile(vec![ball]).unwrap();
    let rope = joint("rope", JointKind::Rope { length: 80.0 }, "ball", (4.0, 4.0), None, (200.0, 100.0));
    world.set_joints(vec![rope]).unwrap();
    world.set("ball", Set { velocity: Some((300.0, -200.0)), ..Set::default() }).unwrap();
    let (mut clock, mut farthest) = (0.0, 0.0f64);
    for _ in 0..240 {
        run(&mut world, &mut clock, 1.0 / 60.0);
        let (x, y) = centre_of(&world, "ball");
        farthest = farthest.max(((x - 200.0).powi(2) + (y - 100.0).powi(2)).sqrt());
    }
    assert!(farthest < 81.5, "never farther than the rope: {farthest}");
    assert!(farthest > 60.0, "and it did swing out: {farthest}");
    // Hanging at last (damped by nothing but the solver, so allow some swing).
    let (x, y) = centre_of(&world, "ball");
    assert!(y > 100.0, "below the anchor: {x},{y}");
}

#[test]
fn a_spring_bobs_about_its_rest_length() {
    let d = drawing();
    let mut world = World2d::default();
    let ball = EntitySpec {
        loose: Some(DEAD),
        ..solid("ball", (196.0, 196.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    world.reconcile(vec![ball]).unwrap();
    let spring = joint(
        "spring",
        JointKind::Spring { length: 50.0, stiffness: 400.0, damping: 2.0 },
        "ball", (4.0, 4.0), None, (200.0, 100.0),
    );
    world.set_joints(vec![spring]).unwrap();
    let (mut clock, mut crossings, mut last) = (0.0, 0, 0.0f64);
    for _ in 0..180 {
        run(&mut world, &mut clock, 1.0 / 60.0);
        let (x, y) = centre_of(&world, "ball");
        let stretch = ((x - 200.0).powi(2) + (y - 100.0).powi(2)).sqrt() - 50.0;
        if stretch * last < 0.0 {
            crossings += 1;
        }
        if stretch != 0.0 {
            last = stretch;
        }
    }
    assert!(crossings >= 2, "it bobbed through its rest length: {crossings}");
}

#[test]
fn a_joint_is_kept_across_descriptions_and_ends_with_its_entity_or_its_line() {
    let d = drawing();
    let mut world = swing_world();
    let ball = || EntitySpec {
        loose: Some(DEAD),
        ..solid("ball", (256.0, 96.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    world.reconcile(vec![ball()]).unwrap();
    let rope = || joint("rope", JointKind::Rope { length: 80.0 }, "ball", (4.0, 4.0), None, (200.0, 100.0));
    world.set_joints(vec![rope()]).unwrap();
    let mut clock = 0.0;
    run(&mut world, &mut clock, 0.4);
    let before = centre_of(&world, "ball");
    // The same description again, as every frame does: nothing restarts.
    world.reconcile(vec![ball()]).unwrap();
    world.set_joints(vec![rope()]).unwrap();
    assert_eq!(centre_of(&world, "ball"), before, "not reset");
    assert_eq!(world.joint_count(), 1);
    run(&mut world, &mut clock, 0.4);
    let on = centre_of(&world, "ball");
    assert!(((on.0 - 200.0).powi(2) + (on.1 - 100.0).powi(2)).sqrt() < 81.5, "still held: {on:?}");

    // The line gone, the ball is free and falls past it.
    world.set_joints(vec![]).unwrap();
    assert_eq!(world.joint_count(), 0);
    run(&mut world, &mut clock, 1.0);
    assert!(centre_of(&world, "ball").1 > 190.0, "fell free: {:?}", centre_of(&world, "ball"));

    // Joined again, then the entity goes: its joint goes with it, quietly.
    world.reconcile(vec![ball()]).unwrap();
    world.set_joints(vec![rope()]).unwrap();
    world.reconcile(vec![]).unwrap();
    world.set_joints(vec![]).unwrap();
    assert_eq!(world.joint_count(), 0);
    run(&mut world, &mut clock, 0.1);
}

#[test]
fn a_bad_joint_is_refused_and_the_last_good_ones_kept() {
    let d = drawing();
    let mut world = swing_world();
    let ball = EntitySpec {
        loose: Some(DEAD),
        ..solid("ball", (256.0, 96.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let wall = solid("wall", (0.0, 0.0), Shape::Rect(8.0, 8.0), (0.0, 0.0), &d);
    world.reconcile(vec![ball, wall, plank("plank", (100.0, 200.0), &d)]).unwrap();
    let rope = |id: &str| joint(id, JointKind::Rope { length: 80.0 }, "ball", (4.0, 4.0), None, (200.0, 100.0));
    world.set_joints(vec![rope("good")]).unwrap();
    let cases: Vec<(Vec<JointSpec>, &str)> = vec![
        (vec![joint("j", JointKind::Rope { length: 80.0 }, "ghost", (0.0, 0.0), None, (0.0, 0.0))], "there is no such entity \"ghost\""),
        (vec![joint("j", JointKind::Rope { length: 80.0 }, "ball", (0.0, 0.0), Some("ghost"), (0.0, 0.0))], "there is no such entity \"ghost\""),
        (vec![joint("j", JointKind::Pin { limits: None }, "ball", (4.0, 4.0), None, (200.0, 100.0))], "needs grip"),
        (vec![joint("j", JointKind::Rope { length: 0.0 }, "ball", (4.0, 4.0), None, (0.0, 0.0))], "length must be above 0"),
        (vec![joint("j", JointKind::Spring { length: 5.0, stiffness: -1.0, damping: 0.0 }, "ball", (4.0, 4.0), None, (0.0, 0.0))], "stiffness must be 0 or more"),
        (vec![joint("j", JointKind::Pin { limits: Some((90.0, 10.0)) }, "plank", (0.0, 4.0), None, (0.0, 0.0))], "limits must be low then high"),
        (vec![rope("a"), rope("a")], "duplicate joint id \"a\""),
        (vec![joint("", JointKind::Rope { length: 1.0 }, "ball", (0.0, 0.0), None, (0.0, 0.0))], "joint id must not be empty"),
        (vec![joint("j", JointKind::Rope { length: 5.0 }, "wall", (0.0, 0.0), None, (0.0, 0.0))], "joins nothing that moves"),
        (vec![joint("j", JointKind::Rope { length: 5.0 }, "ball", (0.0, 0.0), Some("ball"), (0.0, 0.0))], "a thing cannot be joined to itself"),
        (vec![joint("j", JointKind::Rope { length: 5.0 }, "ball", (f64::NAN, 0.0), None, (0.0, 0.0))], "finite"),
    ];
    for (specs, wanted) in cases {
        let err = world.set_joints(specs).unwrap_err().to_string();
        assert!(err.contains(wanted), "wanted {wanted:?}, got {err}");
        assert_eq!(world.joint_count(), 1, "{wanted}: the last good kept");
    }
}

#[test]
fn a_world_with_a_well_or_a_hand_keeps_asking_for_ticks_even_when_all_is_asleep() {
    let mut world = balls_at(&[("ball", 100.0, 100.0)]);
    assert!(!world.needs_ticks(), "a world of sleepers needs no clock");
    world.set_wells(vec![Well { at: (0.0, 0.0), radius: 10.0, pull: 1.0 }]).unwrap();
    assert!(world.needs_ticks(), "a well can only work on a ticking world");
    world.set_wells(vec![]).unwrap();
    assert!(!world.needs_ticks());
    world.grab("ball", (50.0, 50.0)).unwrap();
    assert!(world.needs_ticks(), "a hand moves what it holds a step at a time");
    world.release("ball").unwrap();
    // Let go, it is awake and sliding until it rests again; the clock stops once it does.
    let mut clock = 0.0;
    run(&mut world, &mut clock, 3.0);
    assert!(!world.needs_ticks(), "and at rest again, it needs none");
}

// ── line ground ────────────────────────────────────────────────────────────────────────────────

fn line(points: &[(f64, f64)]) -> Shape {
    Shape::Line(points.into())
}

/// The ball's centre and the ground's height under it, once a frame, for `secs`.
fn roll(world: &mut World2d, secs: f64, ground: impl Fn(f64) -> f64) -> Vec<((f64, f64), f64)> {
    let mut clock = 0.0;
    (0..(secs * 60.0).round() as usize)
        .map(|_| {
            run(world, &mut clock, 1.0 / 60.0);
            let b = ball_of(world);
            let c = (b.pos.0 + 4.0, b.pos.1 + 4.0);
            (c, ground(c.0))
        })
        .collect()
}

#[test]
fn a_ball_rolls_down_a_line_slope_and_never_sinks_into_it() {
    let d = drawing();
    let slope = 30f64.to_radians().tan();
    let ground = solid("ground", (0.0, 300.0), line(&[(0.0, 0.0), (2000.0, 2000.0 * slope)]), (0.0, 0.0), &d);
    let ball = EntitySpec {
        loose: Some(Material { grip: Some(1.0), ..DEAD }),
        ..solid("ball", (100.0, 200.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let mut world = World2d::default();
    world.set_gravity((0.0, 981.0)).unwrap();
    world.reconcile(vec![ball, ground]).unwrap();
    let path = roll(&mut world, 3.0, |x| 300.0 + x * slope);
    for ((x, y), g) in &path {
        assert!(*y < g + 0.5, "the ball's centre went under the line at x {x}: {y} vs {g}");
    }
    let ((x, y), g) = *path.last().unwrap();
    assert!(x > 400.0, "rolled downhill to the right: x {x}");
    // Resting on a 30° slope, the centre is r / cos 30° above the line straight below it.
    assert!((g - y - 4.0 / 30f64.to_radians().cos()).abs() < 1.5, "touching the slope: {y} over {g}");
    assert!(ball_of(&world).rot.abs() > 90.0, "it rolled rather than slid: {:?}", ball_of(&world));
}

#[test]
fn a_fast_ball_does_not_tunnel_through_a_line() {
    let d = drawing();
    let ground = solid("ground", (-1000.0, 500.0), line(&[(0.0, 0.0), (2000.0, 0.0)]), (0.0, 0.0), &d);
    let ball = EntitySpec { loose: Some(DEAD), ..solid("ball", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d) };
    let mut world = World2d::default();
    world.set_gravity((0.0, 981.0)).unwrap();
    world.reconcile(vec![ball, ground]).unwrap();
    // 250 units a step at 120 Hz, far past a radius-4 ball's reach: Rapier's speculative
    // contacts hold a line without CCD (checked at 3000 and 30000 with CCD off).
    world.set("ball", Set { velocity: Some((0.0, 30000.0)), ..Set::default() }).unwrap();
    for ((x, y), g) in roll(&mut world, 1.0, |_| 500.0) {
        assert!(y < g + 0.5, "went through the line at x {x}: {y}");
    }
}

/// Height of the polyline through `points` (world units) at `x`.
fn height_on(points: &[(f64, f64)], x: f64) -> f64 {
    let seg = points.windows(2).find(|w| x <= w[1].0).unwrap_or(&points[points.len() - 2..]);
    let ((x0, y0), (x1, y1)) = (seg[0], seg[1]);
    y0 + (y1 - y0) * (x - x0) / (x1 - x0)
}

/// Where a gripping ball has rolled after 2 s down a 15° slope made of `pieces` collinear
/// segments, never sinking below it.
fn rolled_down(pieces: usize, at_joints: &[(usize, f64)]) -> f64 {
    let d = drawing();
    let slope = 15f64.to_radians().tan();
    let mut hills: Vec<_> = (0..=pieces).map(|i| {
        let x = 4000.0 * i as f64 / pieces as f64;
        (x, x * slope)
    }).collect();
    // Bumps at chosen joints: (index, how far the joint is lifted), to make a crest or a valley.
    for &(i, lift) in at_joints {
        hills[i].1 -= lift;
    }
    let ground = solid("ground", (0.0, 300.0), line(&hills), (0.0, 0.0), &d);
    // Its box's top-left, so the ball starts on the slope at x 24, at rest.
    let top = 300.0 + 24.0 * slope - 4.0 / 15f64.to_radians().cos() - 4.0 - 0.05;
    let ball = EntitySpec {
        loose: Some(Material { grip: Some(1.0), ..DEAD }),
        ..solid("ball", (20.0, top), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let mut world = World2d::default();
    world.set_gravity((0.0, 981.0)).unwrap();
    world.reconcile(vec![ball, ground]).unwrap();
    let path = roll(&mut world, 2.0, |x| 300.0 + height_on(&hills, x));
    for ((x, y), g) in &path {
        assert!(*y < g + 0.5, "under the line at x {x}: {y} vs {g}");
    }
    path.last().unwrap().0.0
}

#[test]
fn a_ball_rolls_across_joints_as_if_they_were_not_there() {
    // Joints in a straight line must not catch: the classic internal-edge snag.
    let whole = rolled_down(1, &[]);
    let jointed = rolled_down(160, &[]);
    // Rolling without slipping, and a 2D circle is a disc (I = ½·m·r²): a = (2/3)·g·sin θ along
    // the slope, so x = ½·a·t²·cos θ ≈ 327 from where it started.
    let theta = 15f64.to_radians();
    let expected = 20.0 + 4.0 + 0.5 * (2.0 / 3.0) * 981.0 * theta.sin() * 4.0 * theta.cos();
    assert!((whole - expected).abs() < expected * 0.03, "rolled to {whole}, expected {expected}");
    assert!((jointed - whole).abs() < whole * 0.02, "160 joints {jointed} vs one segment {whole}");
    // A shallow crest and valley (a 4-unit lift over 25-unit pieces, about 9° each way) are rolled
    // over: the ball gets as far, give or take what the bumps cost.
    let bumpy = rolled_down(160, &[(20, 4.0), (40, -4.0), (60, 4.0)]);
    assert!(bumpy > whole * 0.9, "over the bumps {bumpy} vs smooth {whole}");
}

#[test]
fn a_lines_points_are_from_its_at_and_its_entitys_pos() {
    let d = drawing();
    let ground = solid("ground", (100.0, 0.0), line(&[(0.0, 0.0), (500.0, 0.0)]), (0.0, 400.0), &d);
    let ball = EntitySpec { loose: Some(DEAD), ..solid("ball", (300.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d) };
    let mut world = World2d::default();
    world.set_gravity((0.0, 981.0)).unwrap();
    world.reconcile(vec![ball, ground]).unwrap();
    let ((_, y), _) = *roll(&mut world, 2.0, |_| 400.0).last().unwrap();
    assert!((y - 396.0).abs() < 1.0, "rests on y = 400 (pos 0 + at 400): centre {y}");
    // And off its ends there is nothing: a ball at x 50 falls past where the line would be.
    let ball = EntitySpec { loose: Some(DEAD), ..solid("ball", (50.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d) };
    let ground = solid("ground", (100.0, 0.0), line(&[(0.0, 0.0), (500.0, 0.0)]), (0.0, 400.0), &d);
    let mut world = World2d::default();
    world.set_gravity((0.0, 981.0)).unwrap();
    world.reconcile(vec![ball, ground]).unwrap();
    let ((_, y), _) = *roll(&mut world, 2.0, |_| 400.0).last().unwrap();
    assert!(y > 600.0, "nothing to land on left of x 100: centre {y}");
}

#[test]
fn a_carried_thing_let_go_as_a_line_is_fixed_ground() {
    let d = drawing();
    let hero = || EntitySpec {
        controller: Some(wasd(100.0)),
        ..solid("hero", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d)
    };
    let carried = EntitySpec {
        attach: Some(Attach { to: "hero".into(), part: "body".into(), at: (10.0, 0.0), pivot: (0.0, 0.0), turn: false }),
        ..solid("plank", (50.0, 0.0), Shape::Rect(8.0, 8.0), (0.0, 0.0), &d)
    };
    let mut world = World2d::default();
    world.set_gravity((0.0, 981.0)).unwrap();
    world.reconcile(vec![hero(), carried]).unwrap();
    world.key("KeyD", true);
    frames(&mut world, 0.3, 0.1);
    world.key("KeyD", false);
    let plank = solid("plank", (50.0, 0.0), line(&[(0.0, 0.0), (40.0, 0.0)]), (0.0, 0.0), &d);
    world.reconcile(vec![hero(), plank]).unwrap();
    let before = world.inspect().into_iter().find(|e| e.id == "plank").unwrap();
    assert_eq!(before.body, "fixed", "let go as a line, it is ground: {before:?}");
    frames(&mut world, 1.3, 0.1);
    let after = world.inspect().into_iter().find(|e| e.id == "plank").unwrap();
    assert_eq!(after.pos, before.pos, "and ground does not fall");
}

#[test]
fn get_answers_one_entity_as_the_dump_does() {
    let (specs, _) = ball_over_floor(500.0, Material { grip: Some(1.0), ..DEAD });
    let mut world = World2d::default();
    world.set_gravity((0.0, 981.0)).unwrap();
    world.reconcile(specs).unwrap();
    world.set("ball", Set { velocity: Some((80.0, 0.0)), spin: Some(45.0), ..Set::default() }).unwrap();
    let mut clock = 0.0;
    run(&mut world, &mut clock, 0.5);
    for id in ["ball", "floor"] {
        let dumped = world.inspect().into_iter().find(|e| e.id == id);
        assert_eq!(world.get(id), dumped, "{id}");
    }
    assert_eq!(world.get("ghost"), None);
}

#[test]
fn a_line_is_kept_while_the_same_and_replaced_when_changed() {
    let d = drawing();
    let mut world = World2d::default();
    let ground = |end| solid("ground", (0.0, 0.0), line(&[(0.0, 0.0), (end, 10.0)]), (0.0, 0.0), &d);
    let body = |w: &World2d| w.ecs.get::<Solid>(w.by_id["ground"]).map(|s| s.body);
    world.reconcile(vec![ground(100.0)]).unwrap();
    let first = body(&world);
    world.reconcile(vec![ground(100.0)]).unwrap();
    assert_eq!(body(&world), first, "the same line keeps its body");
    world.reconcile(vec![ground(200.0)]).unwrap();
    assert_ne!(body(&world), first, "a changed line replaces it");
}

#[test]
fn a_bad_line_is_refused_and_keeps_the_last_good_world() {
    let d = drawing();
    let mut world = World2d::default();
    let good = solid("ground", (0.0, 0.0), line(&[(0.0, 0.0), (10.0, 0.0)]), (0.0, 0.0), &d);
    world.reconcile(vec![good]).unwrap();
    let flat = || Some(Collider { shape: line(&[(0.0, 0.0), (10.0, 0.0)]), at: (0.0, 0.0) });
    let ball = || solid("ball", (0.0, 0.0), Shape::Circle(4.0), (4.0, 4.0), &d);
    let cases = [
        (solid("ground", (0.0, 0.0), line(&[(0.0, 0.0)]), (0.0, 0.0), &d), "a line needs at least 2 points"),
        (solid("ground", (0.0, 0.0), line(&[(0.0, 0.0), (f64::NAN, 1.0)]), (0.0, 0.0), &d), "line points must be finite"),
        (solid("ground", (0.0, 0.0), line(&[(1.0, 1.0), (1.0, 1.0)]), (0.0, 0.0), &d), "two points in the same place"),
        // Finite as f64, infinite as the f32 Rapier gets.
        (solid("ground", (0.0, 0.0), line(&[(0.0, 0.0), (1e39, 0.0)]), (0.0, 0.0), &d), "line points must be finite"),
        // Distinct as f64, one point as f32.
        (solid("ground", (0.0, 0.0), line(&[(1e8, 0.0), (1e8 + 1.0, 0.0)]), (0.0, 0.0), &d), "two points in the same place"),
        (EntitySpec { collider: flat(), loose: Some(DEAD), ..ball() }, "a line is ground: it cannot be loose"),
        (EntitySpec { collider: flat(), controller: Some(wasd(10.0)), ..ball() }, "a line is ground: it cannot have a controller"),
        (EntitySpec { sensor: flat(), ..spec("zone", (0.0, 0.0), &d) }, "a sensor needs an area: a line has none"),
    ];
    for (bad, wanted) in cases {
        let err = world.reconcile(vec![bad]).unwrap_err().to_string();
        assert!(err.contains(wanted), "wanted {wanted:?}, got {err}");
        assert_eq!(ids(&world), ["ground"], "the last good world stays");
    }
}

// ── mass and centre of mass ────────────────────────────────────────────────────────────────────

/// How far a box of `size` and mass `m` hangs below a spring's anchor once settled, minus the
/// rest length. Damped at ζ 0.6: overdamped, the slow creep falls asleep short of where it would
/// rest — 10% off either side.
fn sag(size: f64, material: Material, m: f64) -> f64 {
    let damping = 0.6 * 2.0 * (500.0 * m).sqrt();
    let d = drawing();
    let mut world = World2d::default();
    world.set_gravity((0.0, 981.0)).unwrap();
    let thing = EntitySpec { loose: Some(material), ..solid("box", (0.0, 0.0), Shape::Rect(size, size), (0.0, 0.0), &d) };
    world.reconcile(vec![thing]).unwrap();
    let mid = size / 2.0;
    let spring = JointKind::Spring { length: 50.0, stiffness: 500.0, damping };
    world.set_joints(vec![joint("spring", spring, "box", (mid, mid), None, (mid, -50.0))]).unwrap();
    let mut clock = 0.0;
    run(&mut world, &mut clock, 6.0);
    // The box's middle hangs at the anchor (y -50) plus rest length 50 plus the sag.
    entity(&world, "box").pos.1 + mid
}

#[test]
fn mass_sets_how_heavy_a_thing_is_whatever_its_size() {
    // A force-based spring settles where k·x = m·g: x = 5 · 981 / 500 = 9.81.
    let five = Material { mass: Some(5.0), ..DEAD };
    let (small, big) = (sag(8.0, five, 5.0), sag(80.0, five, 5.0));
    assert!((small - 9.81).abs() < 0.3, "an 8×8 box of mass 5 sags {small}");
    assert!((big - 9.81).abs() < 0.3, "an 80×80 box of mass 5 sags {big}");
    // Without `mass`, density × area decides: 8 × 8 × 1 = 64, so 64 · 981 / 500 ≈ 125.6.
    let by_area = sag(8.0, DEAD, 64.0);
    assert!((by_area - 125.6).abs() < 3.0, "density 1 over 64 square units sags {by_area}");
}

/// A 100 × 8 plank resting across a fixed peg under its middle, with its centre of mass at
/// `center` (drawing units); how far it has turned after a quarter second — before it can turn
/// past 180° and wrap.
fn tipped(center: Option<(f64, f64)>) -> f64 {
    let d = drawing();
    let plank = EntitySpec {
        loose: Some(Material { center, ..GRIPPY }),
        ..solid("plank", (0.0, 0.0), Shape::Rect(100.0, 8.0), (0.0, 0.0), &d)
    };
    let peg = solid("peg", (46.0, 8.0), Shape::Circle(4.0), (4.0, 4.0), &d);
    let mut world = World2d::default();
    world.set_gravity((0.0, 981.0)).unwrap();
    world.reconcile(vec![plank, peg]).unwrap();
    let mut clock = 0.0;
    run(&mut world, &mut clock, 0.25);
    entity(&world, "plank").rot
}

#[test]
fn center_moves_the_centre_of_mass_and_the_plank_tips_toward_it() {
    let level = tipped(None);
    assert!(level.abs() < 2.0, "balanced on its middle it stays level: {level}");
    let right = tipped(Some((90.0, 4.0)));
    let left = tipped(Some((10.0, 4.0)));
    // +y is down, so a positive turn takes the right end down. About the peg: torque m·g·40,
    // inertia m·(100² + 8²)/12 + m·40², so θ = ½·α·t² ≈ 29° at 0.25 s.
    assert!(right > 15.0, "heavy right end goes down: {right}");
    assert!(left < -15.0, "heavy left end goes down: {left}");
}

#[test]
fn bad_mass_or_center_is_refused() {
    let d = drawing();
    let mut world = World2d::default();
    let with = |m: Material| EntitySpec { loose: Some(m), ..solid("box", (0.0, 0.0), Shape::Rect(8.0, 8.0), (0.0, 0.0), &d) };
    let cases = [
        (Material { mass: Some(0.0), ..DEAD }, "mass must be above 0, got 0"),
        (Material { mass: Some(f64::NAN), ..DEAD }, "mass must be above 0"),
        (Material { center: Some((f64::INFINITY, 0.0)), ..DEAD }, "center must be finite"),
    ];
    for (m, wanted) in cases {
        let err = world.reconcile(vec![with(m)]).unwrap_err().to_string();
        assert!(err.contains(wanted), "wanted {wanted:?}, got {err}");
    }
}

// ── wheel joint ────────────────────────────────────────────────────────────────────────────────

/// A test cart: a 100 × 20 chassis of 100 kg on two 10 kg wheels of radius 10, sprung at
/// `stiffness`/`damping` per wheel, `drop` above flat line ground at y 40. Unloaded, each
/// wheel's centre is 30 below the chassis top (chassis-drawing point (10, 30) and (90, 30)), so
/// with no drop the wheels just touch.
struct Cart {
    stiffness: f64,
    damping: f64,
    travel: (f64, f64),
    drop: f64,
    /// The tyres' grip (the ground has none, so theirs decides).
    grip: f64,
}

const CHASSIS_KG: f64 = 100.0;
const MOUNTS: [(f64, f64); 2] = [(10.0, 30.0), (90.0, 30.0)];

impl Cart {
    fn world(&self) -> World2d {
        self.world_on((-1000.0, 40.0), &[(0.0, 0.0), (8000.0, 0.0)])
    }

    /// The cart over line ground through `points`, placed at `at`.
    fn world_on(&self, at: (f64, f64), points: &[(f64, f64)]) -> World2d {
        let d = drawing();
        let chassis = EntitySpec {
            loose: Some(Material { mass: Some(CHASSIS_KG), ..GRIPPY }),
            ..solid("chassis", (0.0, -self.drop), Shape::Rect(100.0, 20.0), (0.0, 0.0), &d)
        };
        let wheel = |id: &str, (x, y): (f64, f64)| EntitySpec {
            loose: Some(Material { mass: Some(10.0), grip: Some(self.grip), ..GRIPPY }),
            ..solid(id, (x - 10.0, y - 10.0 - self.drop), Shape::Circle(10.0), (10.0, 10.0), &d)
        };
        let ground = solid("ground", at, line(points), (0.0, 0.0), &d);
        let mut world = World2d::default();
        world.set_gravity((0.0, 981.0)).unwrap();
        world.reconcile(vec![chassis, wheel("rear", MOUNTS[0]), wheel("front", MOUNTS[1]), ground]).unwrap();
        self.drive(&mut world, None, None);
        world
    }

    /// Describes the two wheel joints again, with these motors.
    fn drive(&self, world: &mut World2d, rear: Option<Motor>, front: Option<Motor>) {
        let kind = JointKind::Wheel {
            axis: (0.0, 1.0),
            stiffness: self.stiffness,
            damping: self.damping,
            travel: self.travel,
        };
        let joints = [("rear", MOUNTS[0], rear), ("front", MOUNTS[1], front)]
            .map(|(id, at, motor)| JointSpec { motor, ..joint(id, kind, "chassis", at, Some(id), (10.0, 10.0)) });
        world.set_joints(joints.to_vec()).unwrap();
    }
}

/// Where `wheel`'s centre is in the chassis's own drawing frame, against its mount: (across the
/// axis, along it — positive is extended, down).
fn wheel_offset(world: &World2d, wheel: &str, mount: (f64, f64)) -> (f64, f64) {
    let (c, w) = (entity(world, "chassis"), entity(world, wheel));
    let centre = (c.pos.0 + 50.0, c.pos.1 + 10.0);
    let (dx, dy) = (w.pos.0 + 10.0 - centre.0, w.pos.1 + 10.0 - centre.1);
    let (s, co) = c.rot.to_radians().sin_cos();
    // Into the chassis frame: rotate by −rot, then measure from the mount.
    let (lx, ly) = (dx * co + dy * s, -dx * s + dy * co);
    (lx - (mount.0 - 50.0), ly - (mount.1 - 10.0))
}

const CORNER_KG: f64 = CHASSIS_KG / 2.0;

#[test]
fn a_wheel_joint_sags_by_weight_over_stiffness_at_rest() {
    let k = 5000.0;
    let cart = Cart { stiffness: k, damping: 0.5 * 2.0 * (k * CORNER_KG).sqrt(), travel: (30.0, 30.0), drop: 0.0, grip: 1.0 };
    let mut world = cart.world();
    let mut clock = 0.0;
    run(&mut world, &mut clock, 4.0);
    // Each spring carries half the chassis: m·g/k = 50 · 981 / 5000 = 9.81.
    let want = CORNER_KG * 981.0 / k;
    for (id, mount) in [("rear", MOUNTS[0]), ("front", MOUNTS[1])] {
        let (_, along) = wheel_offset(&world, id, mount);
        assert!((-along - want).abs() < want * 0.05, "{id} compressed {} for {want}", -along);
    }
    let c = entity(&world, "chassis");
    assert!((c.pos.1 - want).abs() < want * 0.05 && c.rot.abs() < 0.5, "chassis down by the sag, level: {c:?}");
}

/// The chassis's lowest point after a drop, past where it finally rests: the overshoot.
fn overshoot(zeta: f64) -> f64 {
    let k = 5000.0;
    // A 10-unit drop lands at 140 u/s: a swing of ~14 past a 9.8 sag, short of the 40-unit stop,
    // so the spring and damper alone decide it.
    let cart = Cart { stiffness: k, damping: zeta * 2.0 * (k * CORNER_KG).sqrt(), travel: (40.0, 40.0), drop: 10.0, grip: 1.0 };
    let mut world = cart.world();
    let mut clock = 0.0;
    let mut lowest = f64::MIN;
    for _ in 0..240 {
        run(&mut world, &mut clock, 1.0 / 60.0);
        lowest = lowest.max(entity(&world, "chassis").pos.1);
    }
    lowest - entity(&world, "chassis").pos.1
}

#[test]
fn damping_decides_how_far_a_landing_overshoots() {
    // A landing starts the spring with speed, so even critical damping dives past rest once;
    // what damping decides is how far.
    let (light, medium, critical) = (overshoot(0.1), overshoot(0.4), overshoot(1.0));
    assert!(light > medium && medium > critical, "more damping, less dive: {light} > {medium} > {critical}");
    assert!(light > 2.0 * critical, "lightly damped dives well past: {light} vs {critical}");
}

#[test]
fn a_wheel_stays_on_its_axis_and_inside_its_travel() {
    let k = 5000.0;
    let cart = Cart { stiffness: k, damping: 0.3 * 2.0 * (k * CORNER_KG).sqrt(), travel: (15.0, 20.0), drop: 500.0, grip: 1.0 };
    let mut world = cart.world();
    // Land moving fast sideways: a hard load across the axis.
    world.set("chassis", Set { velocity: Some((800.0, 0.0)), ..Set::default() }).unwrap();
    for id in ["rear", "front"] {
        world.set(id, Set { velocity: Some((800.0, 0.0)), ..Set::default() }).unwrap();
    }
    let mut clock = 0.0;
    let mut deepest = 0.0f64;
    for _ in 0..180 {
        run(&mut world, &mut clock, 1.0 / 60.0);
        for (id, mount) in [("rear", MOUNTS[0]), ("front", MOUNTS[1])] {
            let (across, along) = wheel_offset(&world, id, mount);
            deepest = deepest.min(along);
            assert!(across.abs() < 0.5, "{id} drifted {across} off its axis");
            assert!(along <= 21.0, "{id} out past its travel: {along}");
        }
    }
    // Rapier's limit is a constraint the solver meets over a few steps: a 10 m/s landing pushes
    // ~2 units past a 15-unit stop before it holds.
    assert!(deepest >= -18.0, "compressed past its stop by more than 3: {deepest}");
}

#[test]
fn the_same_cart_and_drop_run_the_same_twice() {
    let run_once = || {
        let cart = Cart { stiffness: 5000.0, damping: 300.0, travel: (15.0, 20.0), drop: 80.0, grip: 1.0 };
        let mut world = cart.world();
        world.set("chassis", Set { velocity: Some((300.0, 0.0)), ..Set::default() }).unwrap();
        let mut clock = 0.0;
        run(&mut world, &mut clock, 20.0);
        world.inspect().into_iter().map(|e| (e.id, e.pos, e.rot)).collect::<Vec<_>>()
    };
    assert_eq!(run_once(), run_once());
}

#[test]
fn a_bad_wheel_joint_is_refused() {
    let cart = Cart { stiffness: 5000.0, damping: 300.0, travel: (15.0, 20.0), drop: 0.0, grip: 1.0 };
    let mut world = cart.world();
    let wheel = |axis, stiffness, travel| {
        let kind = JointKind::Wheel { axis, stiffness, damping: 1.0, travel };
        vec![joint("rear", kind, "chassis", MOUNTS[0], Some("rear"), (10.0, 10.0))]
    };
    let cases = [
        (wheel((0.0, 0.0), 1.0, (1.0, 1.0)), "axis must be a direction, not 0, 0"),
        (wheel((f64::NAN, 1.0), 1.0, (1.0, 1.0)), "axis must be a direction"),
        (wheel((0.0, 1.0), -1.0, (1.0, 1.0)), "stiffness must be 0 or more"),
        (wheel((0.0, 1.0), 1.0, (-1.0, 1.0)), "travel must be up and down, each 0 or more"),
        (vec![joint("w", JointKind::Wheel { axis: (0.0, 1.0), stiffness: 1.0, damping: 1.0, travel: (1.0, 1.0) }, "chassis", (0.0, 0.0), None, (0.0, 0.0))], "a wheel joins two things"),
    ];
    for (joints, wanted) in cases {
        let err = world.set_joints(joints).unwrap_err().to_string();
        assert!(err.contains(wanted), "wanted {wanted:?}, got {err}");
    }
}

// ── motors ─────────────────────────────────────────────────────────────────────────────────────

/// A 10 kg wheel of radius 10 pinned at its centre to the world, driven by `motor`; its spin
/// (degrees a second) after `secs`.
fn pinned_spin(motor: Motor, secs: f64) -> f64 {
    let d = drawing();
    let wheel = EntitySpec {
        loose: Some(Material { mass: Some(10.0), grip: Some(1.0), ..GRIPPY }),
        ..solid("wheel", (0.0, 0.0), Shape::Circle(10.0), (10.0, 10.0), &d)
    };
    let mut world = World2d::default();
    world.reconcile(vec![wheel]).unwrap();
    let pin = JointSpec { motor: Some(motor), ..joint("axle", JointKind::Pin { limits: None }, "wheel", (10.0, 10.0), None, (10.0, 10.0)) };
    world.set_joints(vec![pin]).unwrap();
    let mut clock = 0.0;
    run(&mut world, &mut clock, secs);
    entity(&world, "wheel").spin
}

#[test]
fn a_motor_spins_up_at_torque_over_inertia_and_holds_its_speed() {
    // A disc: I = ½·m·r² = 500 kg·u². 1000 of torque gives α = 2 rad/s² ≈ 114.6 °/s², so
    // after half a second 57.3 °/s, far short of the 3600 asked.
    let limited = pinned_spin(Motor { speed: 3600.0, torque: 1000.0 }, 0.5);
    assert!((limited - 57.3).abs() < 57.3 * 0.05, "torque-limited spin-up: {limited}");
    // Ample torque reaches the speed and holds it, never past.
    let held = pinned_spin(Motor { speed: 360.0, torque: 1e7 }, 0.5);
    assert!((held - 360.0).abs() < 2.0, "held at 360: {held}");
    let backward = pinned_spin(Motor { speed: -360.0, torque: 1e7 }, 0.5);
    assert!((backward + 360.0).abs() < 2.0, "a negative speed turns it the other way: {backward}");
}

#[test]
fn grip_turns_wheel_torque_into_drive_and_none_is_wheelspin() {
    let go = Motor { speed: 720.0, torque: 2e5 };
    let drove = |grip: f64| {
        let cart = Cart { stiffness: 5000.0, damping: 300.0, travel: (15.0, 20.0), drop: 0.0, grip };
        let mut world = cart.world();
        let mut clock = 0.0;
        run(&mut world, &mut clock, 1.0);
        let before = world.joints["rear"].handle;
        cart.drive(&mut world, Some(go), Some(go));
        assert_eq!(world.joints["rear"].handle, before, "a new motor keeps the joint");
        run(&mut world, &mut clock, 2.0);
        (entity(&world, "chassis").pos.0, entity(&world, "rear").spin)
    };
    let (x, spin) = drove(1.0);
    // 720 °/s on radius 10 rolls at 2π·10·2 ≈ 126 u/s.
    assert!(x > 150.0, "with grip the cart drives off: x {x}");
    assert!((spin - 720.0).abs() < 20.0, "rolling at the motor's speed: {spin}");
    let (x, spin) = drove(0.0);
    assert!(x.abs() < 5.0, "with none it stays put: x {x}");
    assert!((spin - 720.0).abs() < 20.0, "while the wheels spin: {spin}");
}

/// A cart at 1000 u/s on flat ground, all wheels braked at `torque`; how far it goes.
fn stopping_distance(torque: f64) -> f64 {
    let cart = Cart { stiffness: 5000.0, damping: 300.0, travel: (15.0, 20.0), drop: 0.0, grip: 1.0 };
    let mut world = cart.world();
    let mut clock = 0.0;
    run(&mut world, &mut clock, 1.0);
    let rolling = 1000.0 / 10.0 * 180.0 / std::f64::consts::PI;
    for id in ["chassis", "rear", "front"] {
        let spin = (id != "chassis").then_some(rolling);
        world.set(id, Set { velocity: Some((1000.0, 0.0)), spin, ..Set::default() }).unwrap();
    }
    let start = entity(&world, "chassis").pos.0;
    let brake = Motor { speed: 0.0, torque };
    cart.drive(&mut world, Some(brake), Some(brake));
    for _ in 0..600 {
        run(&mut world, &mut clock, 1.0 / 60.0);
        if entity(&world, "chassis").velocity.0.abs() < 2.0 {
            break;
        }
    }
    entity(&world, "chassis").pos.0 - start
}

#[test]
fn braking_shortens_the_stop_until_the_wheels_lock_and_slide() {
    // Locked, 120 kg slides on grip 1: decel μ·g, so v²/(2·μ·g) = 1000² / 1962 ≈ 510.
    let (soft, hard, locked) = (stopping_distance(2e5), stopping_distance(2e6), stopping_distance(1e9));
    assert!(soft > 2.0 * hard, "harder braking stops sooner: {soft} vs {hard}");
    assert!((hard - 510.0).abs() < 510.0 * 0.15, "past locking it slides v²/2μg: {hard}");
    assert!((locked - hard).abs() < hard * 0.05, "and more brake no longer helps: {locked} vs {hard}");
}

#[test]
fn spinning_a_wheel_up_in_the_air_turns_the_body_the_other_way() {
    let cart = Cart { stiffness: 5000.0, damping: 300.0, travel: (15.0, 20.0), drop: 2000.0, grip: 1.0 };
    let mut world = cart.world();
    cart.drive(&mut world, Some(Motor { speed: 3600.0, torque: 1e5 }), None);
    let mut clock = 0.0;
    run(&mut world, &mut clock, 0.3);
    let (wheel, body) = (entity(&world, "rear").spin, entity(&world, "chassis").spin);
    assert!(wheel > 100.0 && body < -1.0, "wheel {wheel} forward, body {body} back");
}

#[test]
fn a_bad_motor_is_refused() {
    let cart = Cart { stiffness: 5000.0, damping: 300.0, travel: (15.0, 20.0), drop: 0.0, grip: 1.0 };
    let mut world = cart.world();
    let rope = JointSpec {
        motor: Some(Motor { speed: 1.0, torque: 1.0 }),
        ..joint("r", JointKind::Rope { length: 5.0 }, "chassis", (0.0, 0.0), None, (0.0, 0.0))
    };
    let cases = [
        (Motor { speed: f64::NAN, torque: 1.0 }, "motor speed must be a finite number"),
        (Motor { speed: 1.0, torque: -1.0 }, "motor torque must be 0 or more"),
    ];
    let kind = JointKind::Wheel { axis: (0.0, 1.0), stiffness: 1.0, damping: 1.0, travel: (1.0, 1.0) };
    for (motor, wanted) in cases {
        let j = JointSpec { motor: Some(motor), ..joint("rear", kind, "chassis", MOUNTS[0], Some("rear"), (10.0, 10.0)) };
        let err = world.set_joints(vec![j]).unwrap_err().to_string();
        assert!(err.contains(wanted), "wanted {wanted:?}, got {err}");
    }
    let err = world.set_joints(vec![rope]).unwrap_err().to_string();
    assert!(err.contains("a motor drives a pin or a wheel, not a rope"), "{err}");
}
