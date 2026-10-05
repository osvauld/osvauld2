use super::*;

#[test]
fn commands_preserve_native_identity_clock_and_authored_reset_targets() {
    let mut world = World3d::default();
    world.reconcile(specs()).unwrap();
    world.step();
    let entity = world.ids["marble"];
    let before = world.inspect();
    world
        .set(
            "marble",
            Set3d {
                pos: Some([1.0, 8.0, 2.0]),
                velocity: Some([3.0, 4.0, 0.0]),
                spin: Some([0.0, 90.0, 0.0]),
                rotation: Some([0.0, 0.0, 2.0, 2.0]),
            },
        )
        .unwrap();
    let state = world.body("marble").unwrap();
    assert_eq!(state.position, [1.0, 8.0, 2.0]);
    assert_eq!(state.velocity, [3.0, 4.0, 0.0]);
    assert!((state.angular_velocity[1] - std::f32::consts::FRAC_PI_2).abs() < 0.00001);
    assert!((glam::Quat::from_array(state.rotation).length() - 1.0).abs() < 0.00001);
    assert_eq!(world.ids["marble"], entity);
    assert_eq!(world.tick(), before.tick);
    assert_eq!(
        world.inspect().entities[0].authored,
        before.entities[0].authored
    );
    world.reset("marble").unwrap();
    assert_eq!(world.body("marble").unwrap().position, [0.0, 3.0, 0.0]);
    assert_eq!(world.body("marble").unwrap().angular_velocity, [0.0; 3]);
}

#[test]
fn all_command_fields_validate_before_any_pose_sleep_or_clock_changes() {
    let mut world = World3d::default();
    world.reconcile(specs()).unwrap();
    for _ in 0..1200 {
        world.step();
    }
    let before = world.inspect();
    for bad in [f32::NAN, f32::INFINITY, 10_001.0] {
        for field in [0, 1, 2] {
            let mut to = Set3d {
                pos: Some([8.0; 3]),
                ..Set3d::default()
            };
            match field {
                0 => to.pos = Some([bad, 1.0, 0.0]),
                1 => to.velocity = Some([bad, 1.0, 0.0]),
                _ => to.spin = Some([bad, 1.0, 0.0]),
            }
            assert!(world.set("marble", to).is_err());
            assert_eq!(world.inspect(), before);
            assert!(!world.dirty);
        }
    }
    assert!(
        world
            .set(
                "marble",
                Set3d {
                    pos: Some([8.0; 3]),
                    rotation: Some([0.0; 4]),
                    ..Set3d::default()
                }
            )
            .is_err()
    );
    assert!(
        world
            .set(
                "platform",
                Set3d {
                    pos: Some([8.0; 3]),
                    velocity: Some([0.0; 3]),
                    ..Set3d::default()
                }
            )
            .is_err()
    );
    assert!(world.set("missing", Set3d::default()).is_err());
    assert_eq!(world.inspect(), before);
}

#[test]
fn fixed_sensor_pose_commands_refresh_overlap_without_waking_sleepers() {
    let mut recipe = specs();
    recipe.push(EntitySpec3d {
        id: "zone".into(),
        shape: Shape3d::Box([1.0; 3]),
        position: [0.0, 8.0, 0.0],
        sensor: true,
        ..recipe[0].clone()
    });
    let mut world = World3d::default();
    world.reconcile(recipe).unwrap();
    for _ in 0..1200 {
        world.step();
    }
    assert!(world.body("marble").unwrap().sleeping);
    world
        .set(
            "zone",
            Set3d {
                pos: Some([0.0, 0.5, 0.0]),
                ..Set3d::default()
            },
        )
        .unwrap();
    assert!(world.body("marble").unwrap().sleeping);
    assert!(world.needs_ticks());
    world.step();
    assert!(world.body("marble").unwrap().sleeping);
    assert_eq!(world.drain_zone_events()[0].phase, ZonePhase3d::Enter);
}

#[test]
fn fixed_solid_edits_wake_sleepers_but_empty_commands_do_not() {
    let mut world = World3d::default();
    world.reconcile(specs()).unwrap();
    for _ in 0..1200 {
        world.step();
    }
    assert!(world.body("marble").unwrap().sleeping);
    world.set("marble", Set3d::default()).unwrap();
    assert!(world.body("marble").unwrap().sleeping);
    world
        .set(
            "platform",
            Set3d {
                rotation: Some(glam::Quat::from_rotation_z(-0.4).to_array()),
                ..Set3d::default()
            },
        )
        .unwrap();
    assert!(world.needs_ticks());
    assert!(!world.body("marble").unwrap().sleeping);
    for _ in 0..240 {
        world.step();
    }
    assert!(world.body("marble").unwrap().position[0] > 0.5);
}
