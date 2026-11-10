use super::*;

fn recipe() -> WorldRecipe3d {
    let mounts = [
        [-0.9, -0.15, 0.9],
        [0.9, -0.15, 0.9],
        [-0.9, -0.15, -0.9],
        [0.9, -0.15, -0.9],
    ];
    WorldRecipe3d {
        gravity: [0.0, -1.62, 0.0],
        bodies: vec![
            EntitySpec3d {
                id: "ground".into(),
                shape: Shape3d::Box([100.0, 1.0, 100.0]),
                position: [0.0, -0.5, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                dynamic: false,
                sensor: false,
            },
            EntitySpec3d {
                id: "platform".into(),
                shape: Shape3d::Box([1.6, 0.35, 2.6]),
                position: [0.0, 1.2, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                dynamic: true,
                sensor: false,
            },
        ],
        properties: std::collections::BTreeMap::from([(
            "platform".into(),
            BodyProperties3d {
                mass: Some(400.0),
                center: [0.0, -0.1, 0.0],
                ..Default::default()
            },
        )]),
        wheels: mounts
            .into_iter()
            .enumerate()
            .map(|(i, at)| WheelSpec3d {
                id: format!("wheel{i}"),
                body: "platform".into(),
                at,
                radius: 0.35,
                width: 0.26,
                rest: 0.28,
                travel: 0.18,
                stiffness: 2500.0,
                damping: 500.0,
                grip: 1.1,
            })
            .collect(),
    }
}
fn advance(w: &mut World3d, n: usize) {
    for _ in 0..n {
        w.step();
    }
}
fn actuate(w: &mut World3d, input: WheelInput3d) {
    let commands = (0..4)
        .map(|i| (format!("wheel{i}"), input))
        .collect::<Vec<_>>();
    w.set_wheels(&commands).unwrap();
}
fn settled() -> World3d {
    let mut w = World3d::default();
    w.reconcile_recipe(recipe()).unwrap();
    advance(&mut w, 480);
    w
}
#[test]
fn wheel_components_sag_support_render_and_reset_independently() {
    let mut w = settled();
    let state = w.inspect();
    assert!(
        (state
            .entities
            .iter()
            .find(|e| e.authored.id == "platform")
            .unwrap()
            .mass
            - 400.0)
            .abs()
            < 0.01
    );
    for wheel in &state.wheels {
        assert!(wheel.contact);
        assert!(
            (wheel.suspension_length - (0.28 - 100.0 * 1.62 / 2500.0)).abs() < 0.02,
            "{wheel:?}"
        );
        assert!((wheel.position[1] - 0.35).abs() < 0.03);
    }
    w.set_wheels(&[(
        "wheel0".into(),
        WheelInput3d {
            steering: -0.2,
            torque: 20.0,
            ..Default::default()
        },
    )])
    .unwrap();
    assert_eq!(w.inspect().wheels[0].input.torque, 20.0);
    assert_eq!(w.inspect().wheels[1].input, WheelInput3d::default());
    w.reset("platform").unwrap();
    assert_eq!(w.body("platform").unwrap().position, [0.0, 1.2, 0.0]);
    assert!(
        w.inspect()
            .wheels
            .iter()
            .all(|s| s.input == WheelInput3d::default() && !s.contact && s.angle == 0.0)
    );
    let mut single = recipe();
    single.wheels.truncate(1);
    World3d::default().reconcile_recipe(single).unwrap();
}
#[test]
fn wheel_torque_coast_brake_reverse_match_si_force_model() {
    let mut w = settled();
    actuate(
        &mut w,
        WheelInput3d {
            torque: 42.0,
            ..Default::default()
        },
    );
    advance(&mut w, 120);
    let one = w.body("platform").unwrap().velocity[2];
    advance(&mut w, 120);
    let two = w.body("platform").unwrap().velocity[2];
    assert!((one - 1.2).abs() < 0.06, "{one}");
    assert!((two - 2.4).abs() < 0.12, "{two}");
    actuate(&mut w, WheelInput3d::default());
    advance(&mut w, 120);
    let coast = w.body("platform").unwrap().velocity[2];
    assert!((coast - two).abs() < 0.03);
    actuate(
        &mut w,
        WheelInput3d {
            brake: 105.0,
            ..Default::default()
        },
    );
    advance(&mut w, 60);
    let braking = w.body("platform").unwrap().velocity[2];
    assert!(
        (1.0..=3.1).contains(&((coast - braking) / 0.5)),
        "{coast}->{braking}"
    );
    advance(&mut w, 300);
    assert!(w.body("platform").unwrap().velocity[2].abs() < 0.15);
    w.reset("platform").unwrap();
    advance(&mut w, 480);
    actuate(
        &mut w,
        WheelInput3d {
            torque: 42.0,
            ..Default::default()
        },
    );
    advance(&mut w, 240);
    actuate(
        &mut w,
        WheelInput3d {
            torque: -42.0,
            ..Default::default()
        },
    );
    advance(&mut w, 120);
    assert!((w.body("platform").unwrap().velocity[2] - 1.2).abs() < 0.12);
    advance(&mut w, 360);
    assert!((w.body("platform").unwrap().velocity[2] + 2.4).abs() < 0.15);
}
#[test]
fn velocity_motor_tracks_target_with_bounded_torque_in_both_directions() {
    for sign in [-1.0, 1.0] {
        let mut w = settled();
        actuate(
            &mut w,
            WheelInput3d {
                motor: Some(WheelMotor3d {
                    velocity: sign / 0.35,
                    max_torque: 21.0,
                }),
                ..Default::default()
            },
        );
        advance(&mut w, 120);
        let speed = w.body("platform").unwrap().velocity[2];
        assert!(
            speed.abs() < 0.7,
            "motor exceeded its torque bound: {speed}"
        );
        advance(&mut w, 2280);
        let speed = w.body("platform").unwrap().velocity[2];
        assert!((speed - sign).abs() < 0.05, "{speed}");
    }
}
#[test]
fn physical_steering_radians_rotate_wheel_axis_and_heading() {
    let mut w = settled();
    let commands = (0..4)
        .map(|i| {
            (
                format!("wheel{i}"),
                WheelInput3d {
                    torque: 42.0,
                    steering: if i < 2 { -0.3 } else { 0.0 },
                    ..Default::default()
                },
            )
        })
        .collect::<Vec<_>>();
    w.set_wheels(&commands).unwrap();
    advance(&mut w, 240);
    let body = w.body("platform").unwrap();
    assert!((glam::Quat::from_array(body.rotation) * glam::Vec3::Z).x < -0.1);
}
#[test]
fn wheel_and_body_preflight_rejects_bad_last_component_and_preserves_reset_targets() {
    let mut w = settled();
    let before = w.inspect();
    for bad in [
        WheelInput3d {
            torque: f32::NAN,
            ..Default::default()
        },
        WheelInput3d {
            steering: 4.0,
            ..Default::default()
        },
        WheelInput3d {
            brake: -1.0,
            ..Default::default()
        },
        WheelInput3d {
            torque: 1.0,
            motor: Some(WheelMotor3d {
                velocity: 1.0,
                max_torque: 2.0,
            }),
            ..Default::default()
        },
    ] {
        assert!(
            w.set_wheels(&[
                (
                    "wheel0".into(),
                    WheelInput3d {
                        torque: 40.0,
                        ..Default::default()
                    }
                ),
                ("wheel3".into(), bad)
            ])
            .is_err()
        );
        assert_eq!(w.inspect(), before);
    }
    assert!(
        w.set_wheels(&[
            ("wheel0".into(), WheelInput3d::default()),
            ("wheel0".into(), WheelInput3d::default())
        ])
        .is_err()
    );
    assert!(
        w.set_wheels(&[("missing".into(), WheelInput3d::default())])
            .is_err()
    );
    for change in 0..6 {
        let mut spec = recipe();
        spec.bodies[1].position = [9.0; 3];
        match change {
            0 => spec.properties.get_mut("platform").unwrap().mass = Some(401.0),
            1 => spec.wheels[3].body = "ground".into(),
            2 => spec.wheels[3].body = "missing".into(),
            3 => spec.wheels[3].radius = 0.0,
            4 => spec.wheels[3].id = "ground".into(),
            _ => spec.wheels[3].stiffness = 2600.0,
        }
        assert!(w.reconcile_recipe(spec).is_err());
        assert_eq!(w.inspect(), before);
    }
    w.reset("platform").unwrap();
    assert_eq!(w.body("platform").unwrap().position, [0.0, 1.2, 0.0]);
}
#[test]
fn wheel_setup_preserves_worlds_atomically_and_stepping_is_repeatable() {
    use std::collections::BTreeMap;
    let mut worlds = Worlds3d::default();
    worlds
        .reconcile(BTreeMap::from([
            ("a".into(), recipe()),
            ("b".into(), recipe()),
        ]))
        .unwrap();
    for _ in 0..240 {
        worlds.get_mut("a").unwrap().step();
        worlds.get_mut("b").unwrap().step();
    }
    assert_eq!(
        worlds.get("a").unwrap().inspect(),
        worlds.get("b").unwrap().inspect()
    );
    let before = worlds.get("a").unwrap().inspect();
    let mut moved = recipe();
    moved.bodies[1].position = [9.0; 3];
    let mut invalid = recipe();
    invalid.wheels[3].width = -1.0;
    assert!(
        worlds
            .reconcile(BTreeMap::from([("a".into(), moved), ("b".into(), invalid)]))
            .is_err()
    );
    assert_eq!(worlds.get("a").unwrap().inspect(), before);
    assert_eq!(worlds.get("b").unwrap().inspect(), before);
}
#[test]
fn wheel_obstacle_contacts_push_blocks_below_the_parent_collider() {
    let mut spec = recipe();
    spec.bodies.push(EntitySpec3d {
        id: "block".into(),
        shape: Shape3d::Box([0.7, 0.4, 0.7]),
        position: [0.9, 0.2, 2.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        dynamic: true,
        sensor: false,
    });
    let mut w = World3d::default();
    w.reconcile_recipe(spec).unwrap();
    advance(&mut w, 480);
    let before = w.body("block").unwrap();
    assert!(w.body("platform").unwrap().position[1] - 0.175 > before.position[1] + 0.2);
    actuate(
        &mut w,
        WheelInput3d {
            torque: 42.0,
            ..Default::default()
        },
    );
    advance(&mut w, 300);
    assert!(w.body("block").unwrap().position[2] > before.position[2] + 0.5);
}
#[test]
fn body_mass_and_material_are_independent_of_wheels() {
    let mut spec = recipe();
    spec.wheels.clear();
    spec.gravity = [0.0; 3];
    let mut w = World3d::default();
    w.reconcile_recipe(spec).unwrap();
    assert!(w.inspect().wheels.is_empty());
    assert!(
        (w.inspect()
            .entities
            .iter()
            .find(|e| e.authored.id == "platform")
            .unwrap()
            .mass
            - 400.0)
            .abs()
            < 0.01
    );
}

#[test]
fn motor_and_friction_brake_compete_physically_without_game_priority() {
    let mut w = settled();
    actuate(
        &mut w,
        WheelInput3d {
            torque: 42.0,
            brake: 21.0,
            ..Default::default()
        },
    );
    advance(&mut w, 120);
    let speed = w.body("platform").unwrap().velocity[2];
    assert!(
        (speed - 0.6).abs() < 0.08,
        "weak brake suppressed the motor: {speed}"
    );
    actuate(
        &mut w,
        WheelInput3d {
            torque: 42.0,
            brake: 84.0,
            ..Default::default()
        },
    );
    advance(&mut w, 360);
    assert!(w.body("platform").unwrap().velocity[2].abs() < 0.05);
}

#[test]
fn adding_wheels_to_a_sleeping_body_refreshes_the_solver() {
    let mut spec = recipe();
    spec.wheels.clear();
    let mut w = World3d::default();
    w.reconcile_recipe(spec).unwrap();
    advance(&mut w, 1200);
    assert!(w.body("platform").unwrap().sleeping);
    w.reconcile_recipe(recipe()).unwrap();
    assert!(w.needs_ticks());
    assert!(!w.body("platform").unwrap().sleeping);
}
