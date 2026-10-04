//! Bounded retained descriptions for the experimental 3D viewer. This module owns scene data and
//! agent-readable inspection, not GPU resources, Lua callbacks, or application meaning.

use std::collections::HashSet;
use std::sync::Arc;

use glam::{Mat3, Mat4, Quat, Vec3};
use thiserror::Error;

mod glyph_mesh;
mod glyph_outline;
mod gpu;
pub mod mesh;
mod text_mesh;
pub(crate) use gpu::SceneRenderer;

const MAX_OBJECTS: usize = 256;
const MAX_SCENE_MESH_BYTES: usize = 16 * 1024 * 1024;
const MAX_SCENE_TRIANGLES: usize = 131_072;
pub(crate) const MAX_TEXT_SURFACES: usize = 8;
const MAX_ID_BYTES: usize = 128;
const MAX_SURFACE_TEXT_BYTES: usize = 1024;

#[derive(Clone, Debug)]
pub struct Camera3d {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y_radians: f32,
    pub near: f32,
    pub far: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshKind {
    Cube,
    Triangles,
}

#[derive(Clone, Debug)]
pub struct TextSurface {
    pub text: Arc<str>,
    pub font_size: f32,
    pub color: [f32; 4],
    pub background: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct Object3d {
    pub id: Arc<str>,
    pub mesh: Arc<mesh::MeshData>,
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    pub color: [f32; 4],
    /// A GPU-rendered text panel attached to the cube's local +Z face.
    pub surface: Option<Arc<TextSurface>>,
}

impl Object3d {
    pub(super) fn mesh_data(&self) -> Arc<mesh::MeshData> {
        self.mesh.clone()
    }
}

#[derive(Clone, Debug)]
pub struct Scene3d {
    pub camera: Camera3d,
    pub objects: Arc<[Object3d]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectInspection {
    pub id: Arc<str>,
    pub mesh: MeshKind,
    pub mesh_resource: u64,
    pub vertex_count: usize,
    pub triangle_count: usize,
    pub mesh_bytes: usize,
    pub local_bounds: ([f32; 3], [f32; 3]),
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub color: [f32; 4],
    pub surface_text: Option<Arc<str>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneHit {
    pub id: Arc<str>,
    pub distance: f32,
    pub world_position: Vec3,
    pub world_normal: Vec3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneInspection {
    pub eye: [f32; 3],
    pub target: [f32; 3],
    pub up: [f32; 3],
    pub fov_y_radians: f32,
    pub near: f32,
    pub far: f32,
    pub objects: Vec<ObjectInspection>,
}

#[derive(Debug, Error, PartialEq)]
pub enum SceneError {
    #[error("scene has more than {MAX_OBJECTS} objects")]
    TooManyObjects,
    #[error("scene mesh budget exceeded (16 MiB unique payload or 131072 instanced triangles)")]
    MeshBudget,
    #[error("camera is invalid")]
    InvalidCamera,
    #[error("object id is empty, too long, or duplicated: {0}")]
    InvalidId(String),
    #[error("object {0} has an invalid transform, color, or text surface")]
    InvalidObject(String),
    #[error("scene has more than {MAX_TEXT_SURFACES} text surfaces")]
    TooManyTextSurfaces,
}

impl Scene3d {
    pub fn new(camera: Camera3d, objects: Vec<Object3d>) -> Result<Arc<Self>, SceneError> {
        if objects.len() > MAX_OBJECTS {
            return Err(SceneError::TooManyObjects);
        }
        if !camera.eye.is_finite()
            || !camera.target.is_finite()
            || !camera.up.is_finite()
            || camera.eye == camera.target
            || camera.up.length_squared() < 1e-6
            || !camera.fov_y_radians.is_finite()
            || !(0.01..3.13).contains(&camera.fov_y_radians)
            || !camera.near.is_finite()
            || !camera.far.is_finite()
            || camera.near <= 0.0
            || camera.far <= camera.near
        {
            return Err(SceneError::InvalidCamera);
        }
        let mut ids = HashSet::with_capacity(objects.len());
        if objects
            .iter()
            .filter(|object| object.surface.is_some())
            .count()
            > MAX_TEXT_SURFACES
        {
            return Err(SceneError::TooManyTextSurfaces);
        }
        let mut meshes = HashSet::new();
        let mut mesh_bytes = 0;
        let mut triangles = 0;
        for object in &objects {
            if meshes.insert(object.mesh.resource_id()) {
                mesh_bytes += object.mesh.payload_bytes();
            }
            triangles += object.mesh.triangle_count();
            if mesh_bytes > MAX_SCENE_MESH_BYTES || triangles > MAX_SCENE_TRIANGLES {
                return Err(SceneError::MeshBudget);
            }
            if object.id.is_empty()
                || object.id.len() > MAX_ID_BYTES
                || !ids.insert(object.id.clone())
            {
                return Err(SceneError::InvalidId(object.id.to_string()));
            }
            if !object.position.is_finite()
                || !object.rotation.is_finite()
                || object.rotation.length_squared() < 1e-6
                || !object.scale.is_finite()
                || object.scale.min_element() <= 0.0
                || object
                    .color
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                || (object.surface.is_some() && !Arc::ptr_eq(&object.mesh, &mesh::MeshData::cube()))
                || object.surface.as_ref().is_some_and(|surface| {
                    surface.text.len() > MAX_SURFACE_TEXT_BYTES
                        || !surface.font_size.is_finite()
                        || !(8.0..=96.0).contains(&surface.font_size)
                        || surface
                            .color
                            .iter()
                            .chain(&surface.background)
                            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
                })
            {
                return Err(SceneError::InvalidObject(object.id.to_string()));
            }
        }
        let objects = objects
            .into_iter()
            .map(|mut object| {
                object.rotation = object.rotation.normalize();
                object
            })
            .collect::<Vec<_>>();
        Ok(Arc::new(Self {
            camera,
            objects: objects.into(),
        }))
    }

    pub fn raycast(&self, viewport: (f32, f32), point: (f32, f32)) -> Option<SceneHit> {
        let (width, height) = viewport;
        if !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
            || !point.0.is_finite()
            || !point.1.is_finite()
        {
            return None;
        }
        let ndc = Vec3::new(
            2.0 * point.0 / width - 1.0,
            1.0 - 2.0 * point.1 / height,
            1.0,
        );
        let camera = &self.camera;
        let view = Mat4::look_at_rh(camera.eye, camera.target, camera.up.normalize());
        let projection = Mat4::perspective_rh(
            camera.fov_y_radians,
            width / height,
            camera.near,
            camera.far,
        );
        let direction =
            ((projection * view).inverse().project_point3(ndc) - camera.eye).normalize();
        let forward = (camera.target - camera.eye).normalize();
        let along = direction.dot(forward);
        if !along.is_finite() || along <= 0.0 {
            return None;
        }
        let near_t = camera.near / along;
        let far_t = camera.far / along;
        if !near_t.is_finite() || !far_t.is_finite() {
            return None;
        }
        let ray_origin = camera.eye + direction * near_t;
        self.objects
            .iter()
            .filter_map(|object| {
                let model = Mat4::from_scale_rotation_translation(
                    object.scale,
                    object.rotation,
                    object.position,
                );
                let inverse = model.inverse();
                let origin = inverse.transform_point3(ray_origin);
                let local_direction = inverse.transform_vector3(direction);
                let (t, local_normal) = object.mesh_data().raycast(origin, local_direction)?;
                let distance = t + near_t;
                if distance > far_t {
                    return None;
                }
                Some(SceneHit {
                    id: object.id.clone(),
                    distance,
                    world_position: camera.eye + direction * distance,
                    world_normal: (Mat3::from_mat4(model).inverse().transpose() * local_normal)
                        .normalize(),
                })
            })
            .min_by(|a, b| a.distance.total_cmp(&b.distance))
    }

    pub fn inspect(&self) -> SceneInspection {
        SceneInspection {
            eye: self.camera.eye.to_array(),
            target: self.camera.target.to_array(),
            up: self.camera.up.to_array(),
            fov_y_radians: self.camera.fov_y_radians,
            near: self.camera.near,
            far: self.camera.far,
            objects: self
                .objects
                .iter()
                .map(|object| ObjectInspection {
                    id: object.id.clone(),
                    mesh: if Arc::ptr_eq(&object.mesh, &mesh::MeshData::cube()) {
                        MeshKind::Cube
                    } else {
                        MeshKind::Triangles
                    },
                    mesh_resource: object.mesh.resource_id(),
                    vertex_count: object.mesh.vertex_count(),
                    triangle_count: object.mesh.triangle_count(),
                    mesh_bytes: object.mesh.payload_bytes(),
                    local_bounds: (
                        object.mesh.bounds().0.to_array(),
                        object.mesh.bounds().1.to_array(),
                    ),
                    position: object.position.to_array(),
                    rotation: object.rotation.to_array(),
                    scale: object.scale.to_array(),
                    color: object.color,
                    surface_text: object.surface.as_ref().map(|surface| surface.text.clone()),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests;
