//! Bounded retained descriptions for the experimental 3D viewer. This module owns scene data and
//! agent-readable inspection, not GPU resources, Lua callbacks, or application meaning.

use std::collections::HashSet;
use std::sync::Arc;

use glam::{Mat3, Mat4, Quat, Vec3};
use thiserror::Error;

mod glyph_mesh;
mod glyph_outline;
mod gpu;
mod text_mesh;
pub(crate) use gpu::SceneRenderer;

const MAX_OBJECTS: usize = 256;
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
pub enum BuiltinMesh {
    Cube,
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
    pub mesh: BuiltinMesh,
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    pub color: [f32; 4],
    /// A GPU-rendered text panel attached to the cube's local +Z face.
    pub surface: Option<Arc<TextSurface>>,
}

#[derive(Clone, Debug)]
pub struct Scene3d {
    pub camera: Camera3d,
    pub objects: Arc<[Object3d]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectInspection {
    pub id: Arc<str>,
    pub mesh: BuiltinMesh,
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
        for object in &objects {
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
        if width <= 0.0 || height <= 0.0 {
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
        self.objects
            .iter()
            .filter_map(|object| {
                let model = Mat4::from_scale_rotation_translation(
                    object.scale,
                    object.rotation,
                    object.position,
                );
                let inverse = model.inverse();
                let origin = inverse.transform_point3(camera.eye);
                let local_direction = inverse.transform_vector3(direction);
                let (distance, local_normal) = cube_hit(origin, local_direction)?;
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
                    mesh: object.mesh,
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

fn cube_hit(origin: Vec3, direction: Vec3) -> Option<(f32, Vec3)> {
    let mut near = f32::NEG_INFINITY;
    let mut far = f32::INFINITY;
    let mut near_normal = Vec3::ZERO;
    let mut far_normal = Vec3::ZERO;
    for (axis, normal) in [(0, Vec3::X), (1, Vec3::Y), (2, Vec3::Z)] {
        let o = origin[axis];
        let d = direction[axis];
        if d.abs() < 1e-7 {
            if !(-0.5..=0.5).contains(&o) {
                return None;
            }
            continue;
        }
        let mut a = (-0.5 - o) / d;
        let mut b = (0.5 - o) / d;
        let (mut a_normal, mut b_normal) = (-normal, normal);
        if a > b {
            std::mem::swap(&mut a, &mut b);
            std::mem::swap(&mut a_normal, &mut b_normal);
        }
        if a > near {
            near = a;
            near_normal = a_normal;
        }
        if b < far {
            far = b;
            far_normal = b_normal;
        }
        if near > far {
            return None;
        }
    }
    if near >= 0.0 {
        Some((near, near_normal))
    } else if far >= 0.0 {
        Some((far, far_normal))
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
