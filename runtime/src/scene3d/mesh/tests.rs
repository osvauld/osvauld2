use super::*;

fn triangle() -> (Vec<Vertex>, Vec<u16>) {
    (
        vec![
            Vertex {
                position: [0.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
            },
            Vertex {
                position: [2.0, 0.0, 0.0],
                normal: [0.0, 0.0, 1.0],
            },
            Vertex {
                position: [0.0, 3.0, 0.0],
                normal: [0.0, 0.0, 1.0],
            },
        ],
        vec![0, 1, 2],
    )
}

#[test]
fn accepts_triangle_and_measures_bounds() {
    let (v, i) = triangle();
    let mesh = MeshData::new(v, i).unwrap();
    assert_eq!(mesh.min, Vec3::ZERO);
    assert_eq!(mesh.max, Vec3::new(2.0, 3.0, 0.0));
}

#[test]
fn accepts_exact_limits_and_shares_the_immutable_payload() {
    let (v, _) = triangle();
    let mut vertices = vec![v[0]; MAX_VERTICES];
    vertices[1] = v[1];
    vertices[MAX_VERTICES - 1] = v[2];
    let indices = [0, 1, u16::MAX].repeat(MAX_INDICES / 3);
    let mesh = MeshData::new(vertices, indices).unwrap();
    let shared = mesh.clone();
    assert!(Arc::ptr_eq(&mesh, &shared));
    assert_eq!(mesh.vertex_count(), MAX_VERTICES);
    assert_eq!(mesh.triangle_count(), MAX_INDICES / 3);
    assert_eq!(mesh.payload_bytes(), 1_966_080);
    assert_eq!(shared.bounds(), (Vec3::ZERO, Vec3::new(2.0, 3.0, 0.0)));
}

#[test]
fn triangle_raycast_respects_geometry_and_back_face_culling() {
    let (v, i) = triangle();
    let mesh = MeshData::new(v, i).unwrap();
    let hit = mesh.raycast(Vec3::new(0.5, 0.5, 2.0), -Vec3::Z).unwrap();
    assert_eq!(hit, (2.0, Vec3::Z));
    assert!(mesh.raycast(Vec3::new(1.9, 2.9, 2.0), -Vec3::Z).is_none());
    assert!(mesh.raycast(Vec3::new(0.5, 0.5, -2.0), Vec3::Z).is_none());
    assert!(mesh.raycast(Vec3::new(0.5, 0.5, 2.0), Vec3::X).is_none());
    assert!(mesh.raycast(Vec3::new(0.5, 0.5, -2.0), -Vec3::Z).is_none());
    let a = MeshData::cube();
    let b = MeshData::cube();
    assert!(Arc::ptr_eq(&a, &b));
    assert!(
        a.raycast(Vec3::ZERO, Vec3::Z).is_none(),
        "inside sees only culled faces"
    );
}

#[test]
fn rejects_invalid_counts_and_indices() {
    let (v, i) = triangle();
    assert_eq!(
        MeshData::new(vec![], i.clone()).unwrap_err(),
        MeshError::InvalidCount
    );
    assert_eq!(
        MeshData::new(v.clone(), vec![0, 1]).unwrap_err(),
        MeshError::InvalidCount
    );
    assert_eq!(
        MeshData::new(v.clone(), vec![0, 1, 3]).unwrap_err(),
        MeshError::InvalidTriangle
    );
    assert_eq!(
        MeshData::new(v.clone(), vec![0, 1, 1]).unwrap_err(),
        MeshError::InvalidTriangle
    );
    assert_eq!(
        MeshData::new(v.clone(), vec![0, 1, 0]).unwrap_err(),
        MeshError::InvalidTriangle
    );
    assert_eq!(
        MeshData::new(v.clone(), vec![0; MAX_INDICES + 1]).unwrap_err(),
        MeshError::InvalidCount
    );
    assert_eq!(
        MeshData::new(vec![v[0]; MAX_VERTICES + 1], i).unwrap_err(),
        MeshError::InvalidCount
    );
}

#[test]
fn rejects_bad_vertex_data() {
    let (mut v, i) = triangle();
    v[0].position[0] = f32::NAN;
    assert_eq!(
        MeshData::new(v.clone(), i.clone()).unwrap_err(),
        MeshError::InvalidVertex
    );
    v[0].position[0] = MAX_COORD + 1.0;
    assert_eq!(
        MeshData::new(v.clone(), i.clone()).unwrap_err(),
        MeshError::InvalidVertex
    );
    v[0].position[0] = 0.0;
    v[0].normal[1] = f32::INFINITY;
    assert_eq!(
        MeshData::new(v.clone(), i.clone()).unwrap_err(),
        MeshError::InvalidVertex
    );
    v[0].normal = [0.0; 3];
    assert_eq!(MeshData::new(v, i).unwrap_err(), MeshError::InvalidVertex);
}
