use super::*;

fn goal() -> EntitySpec3d {
    EntitySpec3d {
        id: "goal".into(),
        shape: Shape3d::Box([1.0; 3]),
        position: [0.0, 1.5, 0.0],
        sensor: true,
        ..specs()[0].clone()
    }
}

#[test]
fn sensor_reports_one_enter_then_leave_without_affecting_motion() {
    let mut world = World3d::default();
    let mut plain = World3d::default();
    world.reconcile(vec![specs()[1].clone(), goal()]).unwrap();
    plain.reconcile(vec![specs()[1].clone()]).unwrap();
    let mut events = Vec::new();
    let mut saw_overlap = false;
    for _ in 0..180 {
        world.step();
        plain.step();
        assert_eq!(world.body("marble").unwrap(), plain.body("marble").unwrap());
        saw_overlap |= world.inspect().entities.iter().any(|e| !e.zones.is_empty());
        events.extend(world.drain_zone_events());
    }
    assert!(saw_overlap);
    assert_eq!(events.len(), 2, "{events:?}");
    assert_eq!(events[0].phase, ZonePhase3d::Enter);
    assert_eq!(events[1].phase, ZonePhase3d::Leave);
    assert!(events[0].tick < events[1].tick);
    assert!(events.iter().all(|e| e.id == "goal" && e.who == "marble"));
    assert!(world.inspect().entities.iter().all(|e| e.zones.is_empty()));
}

fn inside() -> World3d {
    let mut world = World3d::default();
    world.reconcile(vec![specs()[1].clone(), goal()]).unwrap();
    for _ in 0..120 {
        world.step();
        if !world.inspect().entities[1].zones.is_empty() {
            world.drain_zone_events();
            return world;
        }
    }
    panic!("marble never entered goal");
}

#[test]
fn sphere_sensor_also_reports_enter_and_leave() {
    let mut sensor = goal();
    sensor.shape = Shape3d::Sphere(0.75);
    let mut world = World3d::default();
    world.reconcile(vec![specs()[1].clone(), sensor]).unwrap();
    for _ in 0..180 {
        world.step();
    }
    let events = world.drain_zone_events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].phase, ZonePhase3d::Enter);
    assert_eq!(events[1].phase, ZonePhase3d::Leave);
}

#[test]
fn sensor_removal_emits_leave_and_body_removal_is_quiet() {
    let mut world = inside();
    world.reconcile(vec![specs()[1].clone()]).unwrap();
    let events = world.drain_zone_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].phase, ZonePhase3d::Leave);
    assert_eq!(events[0].who, "marble");
    world.step();
    assert!(world.drain_zone_events().is_empty());
    let mut world = inside();
    world.reconcile(vec![goal()]).unwrap();
    assert!(world.drain_zone_events().is_empty());
    world.step();
    assert!(world.drain_zone_events().is_empty());
}

#[test]
fn pause_and_inspection_do_not_create_events_or_change_overlaps() {
    let mut world = inside();
    let before = world.inspect();
    world.advance(0.0, false).unwrap();
    world.advance(20.0, false).unwrap();
    assert_eq!(world.inspect(), before);
    assert!(world.drain_zone_events().is_empty());
}

#[test]
fn sensor_role_edits_and_dynamic_sensors_reject_atomically() {
    let mut world = inside();
    let before = world.inspect();
    let mut changed = goal();
    changed.sensor = false;
    assert!(world.reconcile(vec![specs()[1].clone(), changed]).is_err());
    let mut dynamic = goal();
    dynamic.dynamic = true;
    assert!(world.reconcile(vec![specs()[1].clone(), dynamic]).is_err());
    assert_eq!(world.inspect(), before);
}

#[test]
fn overlapping_zone_ids_and_events_are_sorted() {
    let mut a = goal();
    a.id = "a".into();
    let mut z = goal();
    z.id = "z".into();
    let mut world = World3d::default();
    world.reconcile(vec![z, specs()[1].clone(), a]).unwrap();
    for _ in 0..120 {
        world.step();
        let events = world.drain_zone_events();
        if !events.is_empty() {
            assert_eq!(
                events.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
                ["a", "z"]
            );
            let state = world.inspect();
            let marble = state
                .entities
                .iter()
                .find(|e| e.authored.id == "marble")
                .unwrap();
            assert_eq!(marble.zones, ["a", "z"]);
            return;
        }
    }
    panic!("no overlap");
}

#[test]
fn resetting_a_fixed_sensor_refreshes_overlaps_even_when_the_marble_sleeps() {
    let mut recipe = specs();
    let mut sensor = goal();
    sensor.position[1] = 8.0;
    recipe.push(sensor);
    let mut world = World3d::default();
    world.reconcile(recipe.clone()).unwrap();
    for _ in 0..1200 {
        world.step();
    }
    assert!(world.body("marble").unwrap().sleeping);
    assert!(world.drain_zone_events().is_empty());
    recipe[2].position[1] = 0.5;
    world.reconcile(recipe).unwrap();
    world.reset("goal").unwrap();
    assert!(world.needs_ticks());
    world.advance(0.0, true).unwrap();
    world.advance(1.0 / 60.0, true).unwrap();
    let events = world.drain_zone_events();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].phase, ZonePhase3d::Enter);
}

#[test]
fn event_queue_is_bounded_and_overflow_is_observable() {
    let mut world = World3d::default();
    for _ in 0..4106 {
        world.push_zone_event(ZoneEvent3d {
            phase: ZonePhase3d::Enter,
            id: "goal".into(),
            who: "marble".into(),
            tick: 1,
        });
    }
    assert_eq!(world.drain_zone_events().len(), 4096);
    assert_eq!(world.inspect().dropped_zone_events, 10);
}
