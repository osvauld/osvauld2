//! Validated immutable triangle geometry. GPU buffers and asset formats live outside this boundary.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicU64, Ordering},
};
use thiserror::Error;

// Per-mesh cap: at most ~1.9 MiB of vertex/index payload; scene-wide GPU budget is still open.
pub const MAX_VERTICES: usize = 65_536;
pub const MAX_INDICES: usize = 196_608;
const MAX_COORD: f32 = 1_000_000.0;

const fn vertex(position: [f32; 3], normal: [f32; 3]) -> Vertex {
    Vertex { position, normal }
}

const CUBE_VERTICES: [Vertex; 24] = [
    vertex([-0.5, -0.5, 0.5], [0.0, 0.0, 1.0]),
    vertex([0.5, -0.5, 0.5], [0.0, 0.0, 1.0]),
    vertex([0.5, 0.5, 0.5], [0.0, 0.0, 1.0]),
    vertex([-0.5, 0.5, 0.5], [0.0, 0.0, 1.0]),
    vertex([0.5, -0.5, -0.5], [0.0, 0.0, -1.0]),
    vertex([-0.5, -0.5, -0.5], [0.0, 0.0, -1.0]),
    vertex([-0.5, 0.5, -0.5], [0.0, 0.0, -1.0]),
    vertex([0.5, 0.5, -0.5], [0.0, 0.0, -1.0]),
    vertex([0.5, -0.5, 0.5], [1.0, 0.0, 0.0]),
    vertex([0.5, -0.5, -0.5], [1.0, 0.0, 0.0]),
    vertex([0.5, 0.5, -0.5], [1.0, 0.0, 0.0]),
    vertex([0.5, 0.5, 0.5], [1.0, 0.0, 0.0]),
    vertex([-0.5, -0.5, -0.5], [-1.0, 0.0, 0.0]),
    vertex([-0.5, -0.5, 0.5], [-1.0, 0.0, 0.0]),
    vertex([-0.5, 0.5, 0.5], [-1.0, 0.0, 0.0]),
    vertex([-0.5, 0.5, -0.5], [-1.0, 0.0, 0.0]),
    vertex([-0.5, 0.5, 0.5], [0.0, 1.0, 0.0]),
    vertex([0.5, 0.5, 0.5], [0.0, 1.0, 0.0]),
    vertex([0.5, 0.5, -0.5], [0.0, 1.0, 0.0]),
    vertex([-0.5, 0.5, -0.5], [0.0, 1.0, 0.0]),
    vertex([-0.5, -0.5, -0.5], [0.0, -1.0, 0.0]),
    vertex([0.5, -0.5, -0.5], [0.0, -1.0, 0.0]),
    vertex([0.5, -0.5, 0.5], [0.0, -1.0, 0.0]),
    vertex([-0.5, -0.5, 0.5], [0.0, -1.0, 0.0]),
];
const CUBE_INDICES: [u16; 36] = [
    0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7, 8, 9, 10, 8, 10, 11, 12, 13, 14, 12, 14, 15, 16, 17, 18,
    16, 18, 19, 20, 21, 22, 20, 22, 23,
];

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

#[derive(Debug)]
pub struct MeshData {
    id: u64,
    pub(super) vertices: Box<[Vertex]>,
    pub(super) indices: Box<[u16]>,
    min: Vec3,
    max: Vec3,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MeshError {
    #[error("mesh vertex or index count is invalid")]
    InvalidCount,
    #[error("mesh contains a non-finite or out-of-range vertex or normal")]
    InvalidVertex,
    #[error("mesh has an out-of-range index or degenerate triangle")]
    InvalidTriangle,
}

impl MeshData {
    pub fn cube() -> Arc<Self> {
        static CUBE: OnceLock<Arc<MeshData>> = OnceLock::new();
        CUBE.get_or_init(|| {
            Self::new(CUBE_VERTICES.to_vec(), CUBE_INDICES.to_vec())
                .expect("built-in cube must be valid")
        })
        .clone()
    }

    /// Returns t in origin + t * direction; preserve direction magnitude through inverse transforms.
    pub(super) fn raycast(&self, origin: Vec3, direction: Vec3) -> Option<(f32, Vec3)> {
        self.indices
            .chunks_exact(3)
            .filter_map(|tri| {
                let [a, b, c] = [tri[0], tri[1], tri[2]]
                    .map(|i| Vec3::from_array(self.vertices[i as usize].position));
                let e1 = b - a;
                let e2 = c - a;
                let p = direction.cross(e2);
                let det = e1.dot(p);
                // Match the renderer's back-face culling, including rays starting inside a solid.
                if det <= 0.0 {
                    return None;
                }
                let from_a = origin - a;
                let u = from_a.dot(p) / det;
                let q = from_a.cross(e1);
                let v = direction.dot(q) / det;
                let t = e2.dot(q) / det;
                if !t.is_finite()
                    || t <= 0.0
                    || !(0.0..=1.0).contains(&u)
                    || !v.is_finite()
                    || v < 0.0
                    || u + v > 1.0
                {
                    return None;
                }
                Some((t, e1.cross(e2).normalize()))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
    }

    /// Process-local inspection identity, not a durable asset ID or memory address.
    pub fn resource_id(&self) -> u64 {
        self.id
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
    pub fn bounds(&self) -> (Vec3, Vec3) {
        (self.min, self.max)
    }
    pub fn payload_bytes(&self) -> usize {
        self.vertices.len() * std::mem::size_of::<Vertex>()
            + self.indices.len() * std::mem::size_of::<u16>()
    }

    pub fn new(vertices: Vec<Vertex>, indices: Vec<u16>) -> Result<Arc<Self>, MeshError> {
        if vertices.is_empty()
            || vertices.len() > MAX_VERTICES
            || indices.is_empty()
            || indices.len() > MAX_INDICES
            || !indices.len().is_multiple_of(3)
        {
            return Err(MeshError::InvalidCount);
        }
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for vertex in &vertices {
            let p = Vec3::from_array(vertex.position);
            let n = Vec3::from_array(vertex.normal);
            if !p.is_finite()
                || p.abs().max_element() > MAX_COORD
                || !n.is_finite()
                || n.abs().max_element() > MAX_COORD
                || n.length_squared() < 1e-12
            {
                return Err(MeshError::InvalidVertex);
            }
            min = min.min(p);
            max = max.max(p);
        }
        for tri in indices.chunks_exact(3) {
            let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
            if a >= vertices.len() || b >= vertices.len() || c >= vertices.len() {
                return Err(MeshError::InvalidTriangle);
            }
            let a = Vec3::from_array(vertices[a].position);
            let b = Vec3::from_array(vertices[b].position);
            let c = Vec3::from_array(vertices[c].position);
            if (b - a).cross(c - a).length_squared() <= 1e-12 {
                return Err(MeshError::InvalidTriangle);
            }
        }
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Ok(Arc::new(Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            vertices: vertices.into(),
            indices: indices.into(),
            min,
            max,
        }))
    }
}

#[cfg(test)]
mod tests;
