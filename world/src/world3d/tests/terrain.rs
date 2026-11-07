use super::*;
use runtime::scene3d::mesh::{MeshData, Vertex};

fn slope() -> Arc<MeshData> {
    MeshData::new(
        [
            [-12.0, 2.4, -6.0],
            [12.0, -2.4, -6.0],
            [-12.0, 2.4, 6.0],
            [12.0, -2.4, 6.0],
        ]
        .into_iter()
        .map(|position| Vertex {
            position,
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
        })
        .collect(),
        vec![0, 2, 1, 1, 2, 3],
    )
    .unwrap()
}

fn terrain_specs() -> Vec<EntitySpec3d> {
    vec![
        EntitySpec3d {
            id: "ground".into(),
            shape: Shape3d::Mesh(slope()),
            position: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            dynamic: false,
            sensor: false,
        },
        EntitySpec3d {
            id: "ball".into(),
            shape: Shape3d::Sphere(0.35),
            position: [-4.0, 3.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            dynamic: true,
            sensor: false,
        },
    ]
}

#[test]
fn terrain_rolls_downhill_and_equal_geometry_can_reload_without_reset() {
    let mut world = World3d::default();
    world.set_gravity([0.0, -1.62, 0.0]).unwrap();
    world.reconcile(terrain_specs()).unwrap();
    for _ in 0..720 {
        world.step();
    }
    let ball = world.body("ball").unwrap();
    assert!(ball.position[0] > -3.0, "{ball:?}");
    assert!(
        (ball.position[1] + 0.2 * ball.position[0] - 0.35 * 1.04_f32.sqrt()).abs() < 0.05,
        "{ball:?}"
    );
    let before = world.inspect();
    world.reconcile(terrain_specs()).unwrap();
    assert_eq!(world.inspect(), before);
}

#[test]
fn terrain_rejects_dynamic_sensor_oversize_and_overbudget_before_mutation() {
    let mut world = World3d::default();
    world.reconcile(terrain_specs()).unwrap();
    world.step();
    let before = world.inspect();
    for sensor in [false, true] {
        let mut bad = terrain_specs();
        bad[0].sensor = sensor;
        bad[0].dynamic = !sensor;
        assert!(world.reconcile(bad).is_err());
        assert_eq!(world.inspect(), before);
    }
    let mut bad = terrain_specs();
    let geometry = MeshData::new(
        vec![
            Vertex {
                position: [10001.0, 0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [0.0, 0.0, 1.0],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [0.0, 0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
            },
        ],
        vec![0, 1, 2],
    )
    .unwrap();
    bad[0].shape = Shape3d::Mesh(geometry);
    assert!(world.reconcile(bad).is_err());
    assert_eq!(world.inspect(), before);
    let triangle = MeshData::new(
        vec![
            Vertex {
                position: [0.0, 0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [0.0, 0.0, 1.0],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [1.0, 0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                uv: [0.0, 0.0],
            },
        ],
        [0, 1, 2].repeat(262_144),
    )
    .unwrap();
    let bad = (0..3)
        .map(|i| EntitySpec3d {
            id: format!("terrain-{i}"),
            shape: Shape3d::Mesh(triangle.clone()),
            position: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            dynamic: false,
            sensor: false,
        })
        .collect();
    assert!(world.reconcile(bad).is_err());
    assert_eq!(world.inspect(), before);
}

#[test]
fn transformed_terrain_stops_a_fast_falling_body() {
    let mut world = World3d::default();
    let mut specs = terrain_specs();
    specs[0].position = [0.0, 4.0, 0.0];
    specs[0].rotation = glam::Quat::from_rotation_y(std::f32::consts::PI).to_array();
    specs[1].position = [0.0, 12.0, 0.0];
    world.reconcile(specs).unwrap();
    world
        .set(
            "ball",
            Set3d {
                velocity: Some([0.0, -100.0, 0.0]),
                ..Default::default()
            },
        )
        .unwrap();
    for _ in 0..24 {
        world.step();
        let ball = world.body("ball").unwrap();
        assert!(
            ball.position[1] >= 4.0 + 0.2 * ball.position[0] + 0.30,
            "{ball:?}"
        );
    }
}

#[test]
fn rolling_on_a_subdivided_slope_matches_one_large_face() {
    let run = |n: usize| {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for i in 0..=n {
            let x = -12.0 + 24.0 * i as f32 / n as f32;
            for z in [-6.0, 6.0] {
                vertices.push(Vertex {
                    position: [x, -0.2 * x, z],
                    normal: [0.0, 1.0, 0.0],
                    uv: [0.0, 0.0],
                });
            }
            if i < n {
                let a = (2 * i) as u32;
                indices.extend([a, a + 1, a + 2, a + 2, a + 1, a + 3]);
            }
        }
        let mut specs = terrain_specs();
        specs[0].shape = Shape3d::Mesh(MeshData::new(vertices, indices).unwrap());
        specs[1].position = [-8.0, 2.0, 0.0];
        let mut world = World3d::default();
        world.set_gravity([0.0, -1.62, 0.0]).unwrap();
        world.reconcile(specs).unwrap();
        for _ in 0..960 {
            world.step();
        }
        world.body("ball").unwrap().position[0] + 8.0
    };
    let coarse = run(1);
    let fine = run(120);
    assert!(coarse > 4.0, "{coarse}");
    assert!(
        (coarse - fine).abs() < 0.03 * coarse,
        "triangle joins changed rolling distance: {coarse} vs {fine}"
    );
}
