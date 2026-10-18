use super::*;

const IDENTITY: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

fn drop_scene() -> (Physics3d, RigidBodyHandle, RigidBodyHandle) {
    let mut physics = Physics3d::default();
    let platform = physics
        .add(Shape3d::Box([6.0, 0.5, 4.0]), [0.0; 3], IDENTITY, false, false)
        .unwrap();
    let marble = physics
        .add(Shape3d::Sphere(0.25), [0.0, 3.0, 0.0], IDENTITY, true, false)
        .unwrap();
    (physics, platform, marble)
}

#[test]
fn marble_falls_bounces_and_sleeps_on_full_size_platform() {
    let (mut physics, platform, marble) = drop_scene();
    physics.step();
    let falling = physics.inspect(marble);
    assert!(falling.position[1] < 3.0 && falling.velocity[1] < 0.0);
    let mut bounced = false;
    for _ in 0..1199 {
        physics.step();
        bounced |= physics.inspect(marble).velocity[1] > 0.2;
    }
    let settled = physics.inspect(marble);
    assert!(bounced, "restitution must produce an actual bounce");
    assert!((settled.position[1] - 0.5).abs() < 0.01, "{settled:?}");
    assert!(settled.velocity.into_iter().all(|v| v.abs() < 0.01));
    assert!(settled.sleeping, "{settled:?}");
    assert_eq!(physics.inspect(platform).position, [0.0; 3]);
    assert_eq!(physics.tick, 1200);
}

#[test]
fn inspection_is_observational_and_repeated_steps_are_deterministic() {
    let (mut a, _, marble_a) = drop_scene();
    let (mut b, _, marble_b) = drop_scene();
    for _ in 0..360 {
        let before = a.inspect(marble_a);
        let tick = a.tick;
        assert_eq!(a.inspect(marble_a), before);
        assert_eq!(a.tick, tick);
        a.step();
        b.step();
        assert_eq!(a.inspect(marble_a), b.inspect(marble_b));
    }
}

#[test]
fn reset_wakes_a_settled_body_and_clears_all_motion_and_forces() {
    let (mut physics, _, marble) = drop_scene();
    for _ in 0..1200 {
        physics.step();
    }
    assert!(physics.inspect(marble).sleeping);
    let body = &mut physics.world.bodies[marble];
    body.set_rotation(Rotation::from_rotation_z(0.8), true);
    body.set_linvel(Vector::new(20.0, 10.0, 3.0), true);
    body.set_angvel(Vector::new(1.0, 2.0, 3.0), true);
    body.add_force(Vector::new(100.0, 0.0, 0.0), true);
    body.add_torque(Vector::new(0.0, 100.0, 0.0), true);
    physics.reset(marble, [0.0, 3.0, 0.0], IDENTITY).unwrap();
    let reset = physics.inspect(marble);
    assert_eq!(reset.position, [0.0, 3.0, 0.0]);
    assert_eq!(reset.rotation, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(reset.velocity, [0.0; 3]);
    assert_eq!(reset.angular_velocity, [0.0; 3]);
    assert!(!reset.sleeping);
    assert_eq!(
        physics.tick, 1200,
        "reset is a command, not a simulation step"
    );
    for _ in 0..30 {
        physics.step();
    }
    let falling = physics.inspect(marble);
    assert!(falling.position[1] < 3.0);
    assert_eq!(
        falling.position[0], 0.0,
        "old force must not move it sideways"
    );
    assert_eq!(falling.angular_velocity, [0.0; 3]);
}

#[test]
fn invalid_spawn_and_reset_reject_without_mutating_the_solver() {
    let (mut physics, _, marble) = drop_scene();
    for bad in [f32::NAN, f32::INFINITY, -1.0, 0.0, 10001.0] {
        assert!(physics.add(Shape3d::Sphere(bad), [0.0; 3], IDENTITY, true, false).is_err());
        assert!(
            physics
                .add(Shape3d::Box([1.0, bad, 1.0]), [0.0; 3], IDENTITY, false, false)
                .is_err()
        );
    }
    for bad in [f32::NAN, f32::INFINITY, 10001.0] {
        assert!(
            physics
                .add(Shape3d::Sphere(1.0), [bad, 0.0, 0.0], IDENTITY, true, false)
                .is_err()
        );
        let before = physics.inspect(marble);
        assert!(physics.reset(marble, [0.0, bad, 0.0], IDENTITY).is_err());
        assert_eq!(physics.inspect(marble), before);
    }
    assert_eq!(physics.world.bodies.len(), 2);
    assert_eq!(physics.world.colliders.len(), 2);
    assert_eq!(physics.tick, 0);
}

#[test]
fn removing_a_contacting_body_removes_its_collider_and_steps_safely() {
    let (mut physics, platform, marble) = drop_scene();
    for _ in 0..1200 {
        physics.step();
    }
    physics.remove(marble);
    assert_eq!(physics.world.bodies.len(), 1);
    assert_eq!(physics.world.colliders.len(), 1);
    physics.step();
    physics.remove(platform);
    physics.step();
    assert!(physics.world.bodies.is_empty());
    assert!(physics.world.colliders.is_empty());
}
