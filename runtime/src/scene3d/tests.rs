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
        mesh: mesh::MeshData::cube(),
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
fn built_in_cube_is_a_valid_mesh() {
    let mesh = mesh::MeshData::cube();
    assert_eq!(mesh.bounds(), (Vec3::splat(-0.5), Vec3::splat(0.5)));
    assert_eq!(mesh.vertex_count(), 24);
    assert_eq!(mesh.triangle_count(), 12);
    assert_eq!(mesh.payload_bytes(), 24 * 24 + 36 * 2);
}

fn triangle_mesh() -> Arc<mesh::MeshData> {
    let normal = [0.0, 0.0, 1.0];
    mesh::MeshData::new(
        vec![
            mesh::Vertex {
                position: [-2.0, -2.0, 0.0],
                normal,
            },
            mesh::Vertex {
                position: [2.0, -2.0, 0.0],
                normal,
            },
            mesh::Vertex {
                position: [0.0, 2.0, 0.0],
                normal,
            },
        ],
        vec![0, 1, 2],
    )
    .unwrap()
}

#[test]
fn arbitrary_meshes_are_shared_inspectable_and_picked_as_triangles() {
    let mut object = cube("triangle");
    object.mesh = triangle_mesh();
    let mut other = object.clone();
    other.id = "second".into();
    other.position.z = -1.0;
    let mut cam = camera();
    cam.eye = Vec3::new(1.0, -1.0, 5.0);
    cam.target = Vec3::new(1.0, -1.0, 0.0);
    let scene = Scene3d::new(cam, vec![other, object]).unwrap();
    let info = scene.inspect();
    let a = &info.objects[0];
    let b = &info.objects[1];
    assert_eq!(a.mesh, MeshKind::Triangles);
    assert_eq!(a.mesh_resource, b.mesh_resource);
    assert_eq!((a.vertex_count, a.triangle_count, a.mesh_bytes), (3, 1, 78));
    assert_eq!(a.local_bounds, ([-2.0, -2.0, 0.0], [2.0, 2.0, 0.0]));
    let hit = scene.raycast((800.0, 600.0), (400.0, 300.0)).unwrap();
    assert_eq!(
        hit.id.as_ref(),
        "triangle",
        "hits outside the old cube footprint"
    );
    assert!((hit.distance - 5.0).abs() < 1e-5);
    assert_eq!(hit.world_normal, Vec3::Z);
}

#[test]
fn clipped_projection_preserves_full_viewport_screen_points_and_rays() {
    let mut cam = camera();
    cam.eye = Vec3::new(0.0, 0.0, 5.0);
    let scene = Scene3d::new(cam.clone(), vec![cube("cube")]).unwrap();
    let matrix = Mat4::perspective_rh(cam.fov_y_radians, 800.0 / 600.0, cam.near, cam.far)
        * Mat4::look_at_rh(cam.eye, cam.target, cam.up);
    for (full, clip) in [
        ([-100.0, 20.0, 800.0, 600.0], [0, 20, 700, 600]),
        ([20.0, -100.0, 800.0, 600.0], [20, 0, 800, 400]),
        ([20.5, 30.25, 800.0, 600.0], [50, 80, 500, 400]),
    ] {
        let point = Vec3::new(0.2, 0.1, 0.5);
        let ndc = matrix.project_point3(point);
        let pixel = [
            full[0] + (ndc.x + 1.0) * full[2] / 2.0,
            full[1] + (1.0 - ndc.y) * full[3] / 2.0,
        ];
        let cropped = (gpu::viewport_crop(full, clip) * matrix).project_point3(point);
        let resolved = [
            clip[0] as f32 + (cropped.x + 1.0) * clip[2] as f32 / 2.0,
            clip[1] as f32 + (1.0 - cropped.y) * clip[3] as f32 / 2.0,
        ];
        assert!((pixel[0] - resolved[0]).abs() < 1e-3);
        assert!((pixel[1] - resolved[1]).abs() < 1e-3);
        let local = (pixel[0] - full[0], pixel[1] - full[1]);
        let hit = scene.raycast((full[2], full[3]), local).unwrap();
        assert!((hit.world_position - point).length() < 1e-4);
    }
}

#[test]
fn picking_respects_camera_clipping_and_back_faces() {
    let mut cam = camera();
    cam.eye = Vec3::new(0.0, 0.0, 5.0);
    cam.near = 4.8;
    cam.far = 5.2;
    let scene = Scene3d::new(cam, vec![cube("clipped")]).unwrap();
    assert!(scene.raycast((800.0, 600.0), (400.0, 300.0)).is_none());
    let mut cam = scene.camera.clone();
    cam.near = 0.1;
    cam.far = 4.0;
    let scene = Scene3d::new(cam, vec![cube("too-far")]).unwrap();
    assert!(scene.raycast((800.0, 600.0), (400.0, 300.0)).is_none());
}

#[test]
fn scene_triangle_budget_counts_instances_not_just_shared_resources() {
    let base = triangle_mesh();
    let vertices = base.vertices.to_vec();
    let big = mesh::MeshData::new(vertices, [0, 1, 2].repeat(65_536)).unwrap();
    let mut object = cube("a");
    object.mesh = big;
    let mut other = object.clone();
    other.id = "b".into();
    let scene = Scene3d::new(camera(), vec![object.clone(), other.clone()]).unwrap();
    assert!(Arc::ptr_eq(&scene.objects[0].mesh, &scene.objects[1].mesh));
    other.id = "c".into();
    assert_eq!(
        Scene3d::new(camera(), vec![object, scene.objects[1].clone(), other]).unwrap_err(),
        SceneError::MeshBudget
    );
}

#[test]
fn text_surface_is_only_supported_on_the_cube_shorthand_resource() {
    let mut object = cube("custom");
    object.mesh = triangle_mesh();
    object.surface = Some(Arc::new(TextSurface {
        text: "hello".into(),
        font_size: 32.0,
        color: [1.0; 4],
        background: [0.0, 0.0, 0.0, 1.0],
    }));
    assert!(matches!(
        Scene3d::new(camera(), vec![object]),
        Err(SceneError::InvalidObject(_))
    ));
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
