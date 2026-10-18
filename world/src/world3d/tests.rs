use super::*;

mod runner;
mod commands;
mod rotation;
mod zones;

fn body_count(world: &mut World3d) -> usize {
    world.ecs.query::<&Body>().iter(&world.ecs).count()
}

fn specs() -> Vec<EntitySpec3d> {
    vec![
        EntitySpec3d {
            id: "platform".into(),
            shape: Shape3d::Box([6.0, 0.5, 4.0]),
            position: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            dynamic: false,
            sensor: false,
        },
        EntitySpec3d {
            id: "marble".into(),
            shape: Shape3d::Sphere(0.25),
            position: [0.0, 3.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            dynamic: true,
            sensor: false,
        },
    ]
}

#[test]
fn resolved_scene_is_observational_preserves_authored_data_and_shares_meshes() {
    use glam::{Quat, Vec3};
    use runtime::scene3d::{Camera3d, Object3d, mesh::MeshData};
    let mut world = World3d::default();
    world.reconcile(specs()).unwrap();
    world.advance(0.0, true).unwrap();
    for frame in 1..=30 {
        world.advance(f64::from(frame) / 60.0, true).unwrap();
    }
    let mesh = MeshData::cube();
    let object = |id: &str, position: Vec3| Object3d {
        id: id.into(),
        mesh: mesh.clone(),
        position,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
        color: [0.2, 0.4, 0.6, 1.0],
        surface: None,
    };
    let scene = Scene3d::new(
        Camera3d {
            eye: Vec3::new(8.0, 5.0, 8.0),
            target: Vec3::Y,
            up: Vec3::Y,
            fov_y_radians: 1.0,
            near: 0.1,
            far: 100.0,
        },
        vec![
            object("marble", Vec3::new(0.0, 3.0, 0.0)),
            object("platform", Vec3::ZERO),
            object("decoration", Vec3::new(1.0, 2.0, 3.0)),
        ],
    )
    .unwrap();
    let authored = scene.inspect();
    let state = world.inspect();
    let resolved = world.resolved_scene(&scene).unwrap();
    assert_eq!(
        resolved.objects[0].position.to_array(),
        world.body("marble").unwrap().position
    );
    assert!(resolved.objects[0].position.y < 3.0);
    assert_eq!(
        resolved.objects[0].rotation.to_array(),
        world.body("marble").unwrap().rotation
    );
    assert_eq!(resolved.objects[2].position, scene.objects[2].position);
    assert_eq!(resolved.objects[2].rotation, scene.objects[2].rotation);
    for (before, after) in scene.objects.iter().zip(resolved.objects.iter()) {
        assert!(Arc::ptr_eq(&before.mesh, &after.mesh));
        assert_eq!(before.scale, after.scale);
        assert_eq!(before.color, after.color);
    }
    assert_eq!(
        world.resolved_scene(&scene).unwrap().inspect(),
        resolved.inspect()
    );
    assert_eq!(scene.inspect(), authored);
    assert_eq!(world.inspect(), state);
    let mut invalid_camera = scene.camera.clone();
    invalid_camera.near = -1.0;
    let invalid = Scene3d {
        camera: invalid_camera,
        objects: scene.objects.clone(),
    };
    assert_eq!(
        world.resolved_scene(&invalid).unwrap_err(),
        World3dError::Scene(SceneError::InvalidCamera)
    );
    assert_eq!(world.inspect(), state);
}

#[test]
fn driven_clock_matches_manual_fixed_steps() {
    let mut driven = World3d::default();
    let mut manual = World3d::default();
    driven.reconcile(specs()).unwrap();
    manual.reconcile(specs()).unwrap();
    assert_eq!(driven.advance(0.0, true).unwrap().steps, 0);
    for frame in 1..=30 {
        assert_eq!(
            driven.advance(f64::from(frame) / 60.0, true).unwrap().steps,
            2
        );
        manual.step();
        manual.step();
    }
    assert_eq!(driven.inspect(), manual.inspect());
    assert_eq!(driven.tick(), 60);
}

#[test]
fn paused_world_and_inspection_do_not_move_bodies_or_catch_up() {
    let mut world = World3d::default();
    world.reconcile(specs()).unwrap();
    world.advance(0.0, true).unwrap();
    world.advance(1.0 / 60.0, true).unwrap();
    let before = world.inspect();
    assert_eq!(world.inspect(), before);
    assert_eq!(world.advance(10.0, false).unwrap().steps, 0);
    assert_eq!(world.advance(100.0, true).unwrap().steps, 0);
    assert_eq!(world.inspect(), before);
    assert_eq!(world.advance(100.0 + 1.0 / 60.0, true).unwrap().steps, 2);
    assert_eq!(world.tick(), 4);
    world.pause();
    let before = world.inspect();
    assert_eq!(world.advance(200.0, true).unwrap().steps, 0);
    assert_eq!(world.inspect(), before);
    assert_eq!(world.advance(199.0, true), Err(ClockError));
    assert_eq!(world.inspect(), before);
}

#[test]
fn settled_world_stops_requesting_ticks_and_reset_resumes_without_catch_up() {
    let mut world = World3d::default();
    world.reconcile(specs()).unwrap();
    assert!(world.needs_ticks());
    for frame in 0..=600 {
        world.advance(f64::from(frame) / 60.0, true).unwrap();
    }
    assert!(!world.needs_ticks());
    let before = world.inspect();
    assert_eq!(world.advance(100.0, false).unwrap().steps, 0);
    assert_eq!(world.inspect(), before);
    world.reset("marble").unwrap();
    assert!(world.needs_ticks());
    assert_eq!(world.body("marble").unwrap().position, [0.0, 3.0, 0.0]);
    assert_eq!(world.advance(200.0, true).unwrap().steps, 0);
    assert_eq!(world.tick(), before.tick);
    assert_eq!(world.advance(200.0 + 1.0 / 60.0, true).unwrap().steps, 2);
    assert!(world.body("marble").unwrap().position[1] < 3.0);
    assert_eq!(world.dropped_seconds(), 0.0);
}

#[test]
fn native_world_reports_stall_loss_and_keeps_authored_and_resolved_state_distinct() {
    let mut world = World3d::default();
    world.reconcile(specs()).unwrap();
    world.advance(0.0, true).unwrap();
    let report = world.advance(1.0, true).unwrap();
    assert_eq!(report.steps, 8);
    let snapshot = world.inspect();
    assert_eq!(snapshot.tick, 8);
    assert_eq!(snapshot.dropped_seconds, report.dropped_seconds);
    assert_eq!(snapshot.entities[0].authored.id, "marble");
    assert_eq!(snapshot.entities[1].authored.id, "platform");
    assert_eq!(snapshot.entities[0].authored.position, [0.0, 3.0, 0.0]);
    assert!(snapshot.entities[0].resolved.position[1] < 3.0);
    assert_eq!(world.inspect(), snapshot);
}

#[test]
fn reconciliation_retains_simulated_placement_and_native_identity() {
    let mut world = World3d::default();
    world.reconcile(specs()).unwrap();
    let entity = world.ids["marble"];
    let handle = world.ecs.get::<Body>(entity).unwrap().handle;
    for _ in 0..60 {
        world.step();
    }
    let falling = world.body("marble").unwrap();
    assert!(falling.position[1] < 3.0);
    for _ in 0..20 {
        world.reconcile(specs()).unwrap();
    }
    assert_eq!(world.body("marble").unwrap(), falling);
    assert_eq!(world.ids["marble"], entity);
    assert_eq!(world.ecs.get::<Body>(entity).unwrap().handle, handle);
    assert_eq!(world.tick(), 60);
    assert_eq!(body_count(&mut world), 2);
}

#[test]
fn authored_position_changes_the_reset_target_not_the_live_pose() {
    let mut world = World3d::default();
    world.reconcile(specs()).unwrap();
    for _ in 0..60 {
        world.step();
    }
    let before = world.body("marble").unwrap();
    let mut changed = specs();
    changed[1].position = [1.0, 5.0, 2.0];
    world.reconcile(changed).unwrap();
    assert_eq!(world.body("marble").unwrap(), before);
    world.reset("marble").unwrap();
    let reset = world.body("marble").unwrap();
    assert_eq!(reset.position, [1.0, 5.0, 2.0]);
    assert_eq!(reset.velocity, [0.0; 3]);
    assert!(!reset.sleeping);
    assert_eq!(world.tick(), 60);
}

#[test]
fn removing_then_reintroducing_a_body_starts_a_new_entity() {
    let mut world = World3d::default();
    world.reconcile(specs()).unwrap();
    let entity = world.ids["marble"];
    for _ in 0..1200 {
        world.step();
    }
    world.reconcile(vec![specs().remove(0)]).unwrap();
    assert_eq!(body_count(&mut world), 1);
    assert!(matches!(
        world.body("marble"),
        Err(World3dError::Missing(_))
    ));
    assert!(matches!(
        world.reset("marble"),
        Err(World3dError::Missing(_))
    ));
    world.step(); // Removing a settled, contacting collider must remain safe.
    let mut recreated = specs();
    recreated[1].shape = Shape3d::Sphere(0.3);
    world.reconcile(recreated).unwrap();
    assert_ne!(world.ids["marble"], entity);
    assert_eq!(world.body("marble").unwrap().position, [0.0, 3.0, 0.0]);
    world.reconcile(vec![]).unwrap();
    assert_eq!(body_count(&mut world), 0);
    world.step();
}

#[test]
fn invalid_batch_does_not_change_membership_state_or_reset_targets() {
    let mut world = World3d::default();
    world.reconcile(specs()).unwrap();
    world.step();
    let before = world.body("marble").unwrap();
    let entity = world.ids["marble"];
    let mut invalid = specs();
    invalid[1].position = [1.0, 5.0, 2.0];
    invalid.push(EntitySpec3d {
        id: "bad".into(),
        shape: Shape3d::Sphere(f32::NAN),
        position: [0.0; 3],
        rotation: [0.0, 0.0, 0.0, 1.0],
        dynamic: true,
        sensor: false,
    });
    assert_eq!(
        world.reconcile(invalid),
        Err(World3dError::Body("bad".into()))
    );
    assert_eq!(body_count(&mut world), 2);
    assert_eq!(world.ids["marble"], entity);
    assert_eq!(world.body("marble").unwrap(), before);
    assert_eq!(world.tick(), 1);
    world.reset("marble").unwrap();
    assert_eq!(world.body("marble").unwrap().position, [0.0, 3.0, 0.0]);
}

#[test]
fn invalid_ids_and_budget_reject_before_removing_old_bodies() {
    let mut world = World3d::default();
    world.reconcile(specs()).unwrap();
    let before = world.body("marble").unwrap();
    for id in ["".to_owned(), "x".repeat(129)] {
        let mut invalid = specs();
        invalid[0].id = id.clone();
        assert_eq!(world.reconcile(invalid), Err(World3dError::Id(id)));
    }
    let repeated = vec![specs()[1].clone(), specs()[1].clone()];
    assert_eq!(
        world.reconcile(repeated),
        Err(World3dError::Id("marble".into()))
    );
    let too_many: Vec<_> = (0..257)
        .map(|i| EntitySpec3d {
            id: i.to_string(),
            shape: Shape3d::Sphere(0.25),
            position: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            dynamic: true,
            sensor: false,
        })
        .collect();
    assert_eq!(world.reconcile(too_many), Err(World3dError::Budget));
    assert_eq!(world.body("marble").unwrap(), before);
    assert_eq!(body_count(&mut world), 2);
}

#[test]
fn retained_collider_and_body_type_changes_reject_atomically() {
    let mut world = World3d::default();
    world.reconcile(specs()).unwrap();
    world.step();
    let before = world.body("marble").unwrap();
    for change_type in [false, true] {
        let mut changed = specs();
        changed.remove(0); // Even this platform removal must not land on failure.
        if change_type {
            changed[0].dynamic = false;
        } else {
            changed[0].shape = Shape3d::Sphere(0.5);
        }
        assert_eq!(
            world.reconcile(changed),
            Err(World3dError::Changed("marble".into()))
        );
        assert_eq!(world.body("marble").unwrap(), before);
        assert_eq!(body_count(&mut world), 2);
    }
}
