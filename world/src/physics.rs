//! Rapier's side of the world: one body per entity that has a collider — fixed, or kinematic
//! when a controller moves it. Only shapes live here; drawings never reach Rapier, and no Rapier
//! type leaves this crate.

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

#[derive(Default)]
pub(crate) struct Physics {
    bodies: RigidBodySet,
    colliders: ColliderSet,
    islands: IslandManager,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
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
        let body = self.bodies.insert(body.translation(at));
        let shape = match c.shape {
            Shape::Circle(r) => ColliderBuilder::ball(r as f32),
            Shape::Rect(w, h) => ColliderBuilder::cuboid((w / 2.0) as f32, (h / 2.0) as f32),
        };
        self.colliders
            .insert_with_parent(shape, body, &mut self.bodies);
        body
    }

    pub(crate) fn remove(&mut self, body: RigidBodyHandle) {
        self.bodies.remove(
            body,
            &mut self.islands,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            true,
        );
    }

    /// Every collider's centre, for tests.
    #[cfg(test)]
    pub(crate) fn centres(&self) -> Vec<(f32, f32)> {
        let mut centres: Vec<_> = self
            .colliders
            .iter()
            .map(|(_, c)| (c.translation().x, c.translation().y))
            .collect();
        centres.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        centres
    }
}
