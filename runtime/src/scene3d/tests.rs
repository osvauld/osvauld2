use super::*;

fn camera() -> Camera3d {
    Camera3d {
        eye: Vec3::new(3.0, 2.0, 4.0),
        target: Vec3::ZERO,
        up: Vec3::Y,
        fov_y_radians: 0.8,
        near: 0.1,
        far: 100.0,
    }
}

fn cube(id: &str) -> Object3d {
    Object3d {
        id: id.into(),
        mesh: BuiltinMesh::Cube,
        position: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
        color: [1.0, 0.0, 0.0, 1.0],
        surface: None,
    }
}

#[test]
fn rejects_bad_cameras_duplicate_ids_and_non_finite_objects() {
    let mut bad_camera = camera();
    bad_camera.eye = Vec3::ZERO;
    assert_eq!(
        Scene3d::new(bad_camera, vec![]).unwrap_err(),
        SceneError::InvalidCamera
    );

    assert_eq!(
        Scene3d::new(camera(), vec![cube("same"), cube("same")]).unwrap_err(),
        SceneError::InvalidId("same".into())
    );

    let mut bad = cube("bad");
    bad.position.x = f32::NAN;
    assert_eq!(
        Scene3d::new(camera(), vec![bad]).unwrap_err(),
        SceneError::InvalidObject("bad".into())
    );
}

#[test]
fn inspection_comes_from_the_validated_scene() {
    let mut object = cube("left");
    object.position = Vec3::new(-0.5, 0.0, 1.0);
    object.rotation = Quat::from_rotation_y(0.4) * 2.0;
    let scene = Scene3d::new(camera(), vec![object]).unwrap();

    let inspected = scene.inspect();
    assert_eq!(inspected.eye, [3.0, 2.0, 4.0]);
    assert_eq!(inspected.objects[0].id.as_ref(), "left");
    assert_eq!(inspected.objects[0].position, [-0.5, 0.0, 1.0]);
    assert!((scene.objects[0].rotation.length() - 1.0).abs() < 1e-6);
}

#[test]
fn raycast_returns_the_nearest_visible_stable_id() {
    let mut near = cube("near");
    near.position.z = 1.0;
    let mut far = cube("far");
    far.position.z = -1.0;
    let mut straight = camera();
    straight.eye = Vec3::new(0.0, 0.0, 5.0);
    let scene = Scene3d::new(straight, vec![far, near]).unwrap();

    let hit = scene.raycast((800.0, 600.0), (400.0, 300.0)).unwrap();
    assert_eq!(hit.id.as_ref(), "near");
    assert!(hit.distance > 0.0);
    assert!((hit.world_normal.length() - 1.0).abs() < 1e-6);
    assert!(scene.raycast((800.0, 600.0), (0.0, 0.0)).is_none());
}

#[test]
fn allocation_is_bounded() {
    let objects = (0..=MAX_OBJECTS)
        .map(|i| cube(&format!("cube-{i}")))
        .collect();
    assert_eq!(
        Scene3d::new(camera(), objects).unwrap_err(),
        SceneError::TooManyObjects
    );
}
