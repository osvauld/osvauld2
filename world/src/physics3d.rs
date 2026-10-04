//! Metre-scale, Y-up 3D rigid bodies. Rapier handles stay inside `world`; visuals are separate.
use rapier3d::prelude::*;

pub(crate) const HZ: u16 = 120;
pub(crate) const STEP: f32 = 1.0 / HZ as f32;
const LIMIT: f32 = 10_000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape3d {
    Sphere(f32),
    /// Full dimensions, not half-extents.
    Box([f32; 3]),
}

#[derive(Debug, thiserror::Error)]
#[error("3D body needs finite bounded position and positive bounded dimensions")]
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
    pub tick: u64,
}

impl Default for Physics3d {
    fn default() -> Self {
        let mut world = PhysicsWorld::new();
        world.gravity = Vector::new(0.0, -9.81, 0.0);
        world.integration_parameters.dt = STEP;
        Self { world, tick: 0 }
    }
}

impl Physics3d {
    pub fn add(
        &mut self,
        shape: Shape3d,
        position: [f32; 3],
        dynamic: bool,
    ) -> Result<RigidBodyHandle, InvalidBody> {
        Self::validate(shape, position)?;
        let collider = match shape {
            Shape3d::Sphere(r) => ColliderBuilder::ball(r),
            Shape3d::Box(size) => {
                ColliderBuilder::cuboid(size[0] / 2.0, size[1] / 2.0, size[2] / 2.0)
            }
        };
        let body = if dynamic {
            RigidBodyBuilder::dynamic().ccd_enabled(true)
        } else {
            RigidBodyBuilder::fixed()
        };
        let handle = self
            .world
            .insert_body(body.translation(Vector::from_array(position)));
        self.world
            .insert_collider(collider.restitution(0.35).friction(0.5), Some(handle));
        Ok(handle)
    }

    pub fn validate(shape: Shape3d, position: [f32; 3]) -> Result<(), InvalidBody> {
        let bounded = |v: f32| v.is_finite() && v.abs() <= LIMIT;
        let dimension = |v: f32| bounded(v) && v >= 0.0001;
        let valid = match shape {
            Shape3d::Sphere(r) => dimension(r),
            Shape3d::Box(size) => size.into_iter().all(dimension),
        };
        if valid && position.into_iter().all(bounded) {
            Ok(())
        } else {
            Err(InvalidBody)
        }
    }

    pub fn step(&mut self) {
        self.world.step();
        self.tick += 1;
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
    ) -> Result<(), InvalidBody> {
        if !position
            .into_iter()
            .all(|v| v.is_finite() && v.abs() <= LIMIT)
        {
            return Err(InvalidBody);
        }
        let body = &mut self.world.bodies[handle];
        body.set_translation(Vector::from_array(position), true);
        body.set_rotation(Rotation::IDENTITY, true);
        body.set_linvel(Vector::ZERO, true);
        body.set_angvel(Vector::ZERO, true);
        body.reset_forces(true);
        body.reset_torques(true);
        Ok(())
    }

    pub fn remove(&mut self, handle: RigidBodyHandle) {
        self.world.remove_body(handle);
    }
}

#[cfg(test)]
mod tests;
