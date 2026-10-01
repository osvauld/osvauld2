//! Rapier's side of the world: one body per entity that has a collider — fixed, or kinematic
//! when a controller moves it. Only shapes live here; drawings never reach Rapier, and no Rapier
//! type leaves this crate.

use rapier2d::control::{CharacterLength, KinematicCharacterController};
use rapier2d::prelude::*;

/// A collision shape in the entity's drawing units. Plain data, not tied to an entity, so an
/// asset can carry a default one in the same words.
#[derive(Clone, Debug, PartialEq)]
pub struct Collider {
    pub shape: Shape,
    /// From the entity's origin: a circle's centre, a rect's top-left corner.
    pub at: (f64, f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Circle(f64),
    Rect(f64, f64),
}

impl Collider {
    pub(crate) fn check(&self) -> Result<(), String> {
        let size = |v: f64| v.is_finite() && v > 0.0;
        let sized = match self.shape {
            Shape::Circle(r) => size(r),
            Shape::Rect(w, h) => size(w) && size(h),
        };
        if !sized {
            return Err("collider size must be a finite number above zero".into());
        }
        if !(self.at.0.is_finite() && self.at.1.is_finite()) {
            return Err("collider at must be finite".into());
        }
        Ok(())
    }

    /// Rapier places shapes by their centre.
    fn centre(&self) -> (f64, f64) {
        match self.shape {
            Shape::Circle(_) => self.at,
            Shape::Rect(w, h) => (self.at.0 + w / 2.0, self.at.1 + h / 2.0),
        }
    }
}

/// Top-down: nothing falls, and every surface is a wall to slide along — no ground to snap to,
/// no slope too steep to climb.
const WALKER: KinematicCharacterController = KinematicCharacterController {
    up: Vector::Y,
    offset: CharacterLength::Relative(0.01),
    slide: true,
    autostep: None,
    max_slope_climb_angle: std::f32::consts::FRAC_PI_2,
    min_slope_slide_angle: std::f32::consts::FRAC_PI_2,
    snap_to_ground: None,
    normal_nudge_factor: 1.0e-4,
};

pub(crate) struct Physics {
    world: PhysicsWorld,
}

impl Default for Physics {
    fn default() -> Self {
        let mut world = PhysicsWorld::new();
        world.gravity = Vector::ZERO;
        Self { world }
    }
}

impl Physics {
    /// A body for an entity whose origin is at `origin`. The entity's scale is not applied.
    pub(crate) fn add(&mut self, c: &Collider, origin: (f64, f64), moves: bool) -> RigidBodyHandle {
        let (cx, cy) = c.centre();
        let at = Vector::new((origin.0 + cx) as f32, (origin.1 + cy) as f32);
        let body = match moves {
            true => RigidBodyBuilder::kinematic_position_based(),
            false => RigidBodyBuilder::fixed(),
        };
        let shape = match c.shape {
            Shape::Circle(r) => ColliderBuilder::ball(r as f32),
            Shape::Rect(w, h) => ColliderBuilder::cuboid((w / 2.0) as f32, (h / 2.0) as f32),
        };
        self.world.insert(body.translation(at), shape).0
    }

    pub(crate) fn remove(&mut self, body: RigidBodyHandle) {
        self.world.remove_body(body);
    }

    /// Brings Rapier's picture up to date — bodies added or moved since — before anything asks it.
    pub(crate) fn step(&mut self, dt: f64) {
        if dt > 0.0 {
            self.world.integration_parameters.dt = dt as f32;
            self.world.step();
        }
    }

    /// Moves `body` as far along `wanted` as the other colliders allow, sliding along what it
    /// meets, and says how far that was.
    pub(crate) fn slide(
        &mut self,
        body: RigidBodyHandle,
        wanted: (f64, f64),
        dt: f64,
    ) -> (f64, f64) {
        let rb = &self.world.bodies[body];
        let (pose, shape) = (
            *rb.position(),
            self.world.colliders[rb.colliders()[0]].shared_shape(),
        );
        let queries = self
            .world
            .query_pipeline_with_filter(QueryFilter::default().exclude_rigid_body(body));
        let wanted = Vector::new(wanted.0 as f32, wanted.1 as f32);
        let moved = WALKER.move_shape(dt as f32, &queries, &**shape, &pose, wanted, |_| {});
        let to = pose.translation + moved.translation;
        self.world.bodies[body].set_translation(to, true);
        (moved.translation.x as f64, moved.translation.y as f64)
    }

    /// Every collider's centre, for tests.
    #[cfg(test)]
    pub(crate) fn centres(&self) -> Vec<(f32, f32)> {
        let mut centres: Vec<_> = self
            .world
            .colliders
            .iter()
            .map(|(_, c)| (c.translation().x, c.translation().y))
            .collect();
        centres.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        centres
    }
}
