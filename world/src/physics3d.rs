//! Metre-scale, Y-up 3D rigid bodies. Rapier handles stay inside `world`; visuals are separate.
use rapier3d::prelude::*;
use runtime::scene3d::mesh::MeshData;
use std::sync::Arc;

mod commands;
mod properties;
mod wheel;
pub use commands::Set3d;
pub use properties::BodyProperties3d;
pub use wheel::{WheelInput3d, WheelMotor3d, WheelSpec3d, WheelState3d};

pub(crate) const HZ: u16 = 120;
pub(crate) const STEP: f32 = 1.0 / HZ as f32;
const LIMIT: f32 = 10_000.0;
pub(crate) const DEFAULT_GRAVITY: [f32; 3] = [0.0, -9.81, 0.0];

#[derive(Clone, Debug)]
pub enum Shape3d {
    Sphere(f32),
    /// Full dimensions, not half-extents.
    Box([f32; 3]),
    /// Fixed, nonsensor triangle surface in local metres; normals are visual data only.
    Mesh(Arc<MeshData>),
}

impl PartialEq for Shape3d {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Sphere(a), Self::Sphere(b)) => a == b,
            (Self::Box(a), Self::Box(b)) => a == b,
            (Self::Mesh(a), Self::Mesh(b)) => {
                Arc::ptr_eq(a, b)
                    || (a.indices() == b.indices()
                        && a.vertices()
                            .iter()
                            .map(|v| v.position)
                            .eq(b.vertices().iter().map(|v| v.position)))
            }
            _ => false,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("3D body needs finite bounded position, nonzero rotation and positive bounded dimensions")]
pub(crate) struct InvalidBody;

#[derive(Clone, Debug, PartialEq)]
pub struct BodyState {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub velocity: [f32; 3],
    pub angular_velocity: [f32; 3],
    pub sleeping: bool,
}

pub(crate) struct Physics3d {
    world: PhysicsWorld,
    wheel_groups: std::collections::BTreeMap<String, wheel::WheelGroup>,
    pub tick: u64,
}

impl Default for Physics3d {
    fn default() -> Self {
        let mut world = PhysicsWorld::new();
        world.gravity = Vector::from_array(DEFAULT_GRAVITY);
        world.integration_parameters.dt = STEP;
        Self {
            world,
            tick: 0,
            wheel_groups: Default::default(),
        }
    }
}

impl Physics3d {
    pub fn gravity(&self) -> [f32; 3] {
        self.world.gravity.to_array()
    }

    pub fn set_gravity(&mut self, gravity: [f32; 3]) {
        if self.gravity() != gravity {
            self.world.gravity = Vector::from_array(gravity);
            for (_, body) in self.world.bodies.iter_mut() {
                if body.is_dynamic() {
                    body.wake_up(true);
                }
            }
        }
    }

    pub fn add(
        &mut self,
        shape: Shape3d,
        position: [f32; 3],
        rotation: [f32; 4],
        dynamic: bool,
        sensor: bool,
    ) -> Result<RigidBodyHandle, InvalidBody> {
        Self::validate(&shape, position, rotation)?;
        if (sensor && dynamic) || (matches!(shape, Shape3d::Mesh(_)) && (dynamic || sensor)) {
            return Err(InvalidBody);
        }
        let collider = match shape {
            Shape3d::Sphere(r) => ColliderBuilder::ball(r),
            Shape3d::Box(size) => {
                ColliderBuilder::cuboid(size[0] / 2.0, size[1] / 2.0, size[2] / 2.0)
            }
            Shape3d::Mesh(mesh) => ColliderBuilder::trimesh(
                mesh.vertices()
                    .iter()
                    .map(|v| Vector::from_array(v.position))
                    .collect(),
                mesh.indices()
                    .chunks_exact(3)
                    .map(|t| [t[0], t[1], t[2]])
                    .collect(),
            )
            .map_err(|_| InvalidBody)?,
        };
        let body = if dynamic {
            RigidBodyBuilder::dynamic().ccd_enabled(true)
        } else {
            RigidBodyBuilder::fixed()
        };
        let handle = self.world.insert_body(body.pose(Pose::from_parts(
            Vector::from_array(position),
            Rotation::from_array(rotation).normalize(),
        )));
        self.world.insert_collider(
            collider.restitution(0.35).friction(0.5).sensor(sensor),
            Some(handle),
        );
        Ok(handle)
    }

    pub fn validate(
        shape: &Shape3d,
        position: [f32; 3],
        rotation: [f32; 4],
    ) -> Result<(), InvalidBody> {
        let bounded = |v: f32| v.is_finite() && v.abs() <= LIMIT;
        let dimension = |v: f32| bounded(v) && v >= 0.0001;
        let valid = match shape {
            Shape3d::Sphere(r) => dimension(*r),
            Shape3d::Box(size) => size.iter().copied().all(dimension),
            Shape3d::Mesh(mesh) => {
                let (min, max) = mesh.bounds();
                min.abs().max_element() <= LIMIT && max.abs().max_element() <= LIMIT
            }
        };
        if valid && position.into_iter().all(bounded) && Self::valid_rotation(rotation) {
            Ok(())
        } else {
            Err(InvalidBody)
        }
    }

    fn valid_rotation(rotation: [f32; 4]) -> bool {
        rotation
            .into_iter()
            .all(|v| v.is_finite() && v.abs() <= LIMIT)
            && Rotation::from_array(rotation).length_squared() >= 1e-6
    }

    pub fn step(&mut self) {
        self.step_wheels();
        self.world.step();
        self.tick += 1;
    }

    pub fn sensor_pairs(&self) -> impl Iterator<Item = (RigidBodyHandle, RigidBodyHandle)> + '_ {
        self.world
            .narrow_phase
            .intersection_pairs()
            .filter_map(|(a, b, intersects)| {
                if !intersects {
                    return None;
                }
                Some((
                    self.world.colliders[a].parent()?,
                    self.world.colliders[b].parent()?,
                ))
            })
    }

    pub fn inspect(&self, handle: RigidBodyHandle) -> BodyState {
        let body = &self.world.bodies[handle];
        BodyState {
            position: body.translation().to_array(),
            rotation: body.rotation().to_array(),
            velocity: body.linvel().to_array(),
            angular_velocity: body.angvel().to_array(),
            sleeping: body.is_sleeping(),
        }
    }

    pub fn reset(
        &mut self,
        handle: RigidBodyHandle,
        position: [f32; 3],
        rotation: [f32; 4],
    ) -> Result<(), InvalidBody> {
        if !position
            .into_iter()
            .all(|v| v.is_finite() && v.abs() <= LIMIT)
            || !Self::valid_rotation(rotation)
        {
            return Err(InvalidBody);
        }
        let body = &mut self.world.bodies[handle];
        body.set_translation(Vector::from_array(position), true);
        body.set_rotation(Rotation::from_array(rotation).normalize(), true);
        body.set_linvel(Vector::ZERO, true);
        body.set_angvel(Vector::ZERO, true);
        body.reset_forces(true);
        body.reset_torques(true);
        self.reset_wheels(handle);
        Ok(())
    }

    pub fn remove(&mut self, handle: RigidBodyHandle) {
        self.remove_wheels(handle);
        self.world.remove_body(handle);
    }
}

#[cfg(test)]
mod tests;
