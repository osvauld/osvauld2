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
    pub(crate) fn centre(&self) -> (f64, f64) {
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

/// How a let-go entity slides: speed lost per second (it slows to rest in about half a second),
/// and the share of speed kept off a wall. It does not spin — the drawing stays upright.
const FRICTION: f32 = 6.0;
const BOUNCE: f32 = 0.5;
/// Slower than this, in drawing units a second, a sliding body has come to rest.
const AT_REST: f32 = 4.0;

/// What moves a body: nothing, the world's controller, or Rapier from a starting velocity.
pub(crate) enum Body {
    Fixed,
    Moved,
    Thrown(f64, f64),
}

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

/// A shape placed relative to its body, which sits at `anchor`.
fn shape(c: &Collider, anchor: (f64, f64)) -> ColliderBuilder {
    let shape = match c.shape {
        Shape::Circle(r) => ColliderBuilder::ball(r as f32),
        Shape::Rect(w, h) => ColliderBuilder::cuboid((w / 2.0) as f32, (h / 2.0) as f32),
    };
    let (cx, cy) = c.centre();
    shape.translation(Vector::new((cx - anchor.0) as f32, (cy - anchor.1) as f32))
}

impl Physics {
    /// A body for an entity whose origin is at `origin`, sitting at its solid shape's centre (or
    /// its sensor's, with none). `owner` comes back with every overlap. Scale is not applied.
    pub(crate) fn add(
        &mut self,
        solid: Option<&Collider>,
        sensor: Option<&Collider>,
        origin: (f64, f64),
        kind: Body,
        owner: u64,
    ) -> RigidBodyHandle {
        let anchor = solid.or(sensor).expect("a body has a shape").centre();
        let at = Vector::new((origin.0 + anchor.0) as f32, (origin.1 + anchor.1) as f32);
        let body = match kind {
            Body::Fixed => RigidBodyBuilder::fixed(),
            Body::Moved => RigidBodyBuilder::kinematic_position_based(),
            Body::Thrown(vx, vy) => RigidBodyBuilder::dynamic()
                .linvel(Vector::new(vx as f32, vy as f32))
                .linear_damping(FRICTION)
                .lock_rotations(),
        };
        let body = self.world.insert_body(body.translation(at));
        let (bodies, colliders) = (&mut self.world.bodies, &mut self.world.colliders);
        if let Some(c) = solid {
            let solid = shape(c, anchor)
                .restitution(BOUNCE)
                .restitution_combine_rule(CoefficientCombineRule::Max)
                .friction(0.0)
                .user_data(owner as u128);
            colliders.insert_with_parent(solid, body, bodies);
        }
        if let Some(c) = sensor {
            // Rapier skips a moved body meeting a fixed one by default: neither can push the
            // other. A zone wants to hear of it all the same — but not of walls, fixed on fixed.
            let kinds = ActiveCollisionTypes::default()
                | ActiveCollisionTypes::KINEMATIC_FIXED
                | ActiveCollisionTypes::KINEMATIC_KINEMATIC;
            let zone = shape(c, anchor).sensor(true).active_collision_types(kinds);
            colliders.insert_with_parent(zone.user_data(owner as u128), body, bodies);
        }
        body
    }

    /// Moves a body that does not collide — one with only a sensor — by `by`.
    pub(crate) fn shift(&mut self, body: RigidBodyHandle, by: (f64, f64)) {
        let rb = &mut self.world.bodies[body];
        let to = rb.translation() + Vector::new(by.0 as f32, by.1 as f32);
        rb.set_translation(to, true);
    }

    /// Every `(zone owner, owner of what is in it)`, as of the last step. Sensors do not sense
    /// each other.
    pub(crate) fn overlaps(&self) -> Vec<(u64, u64)> {
        let colliders = &self.world.colliders;
        let pairs = self.world.narrow_phase.intersection_pairs();
        let pairs = pairs
            .filter(|&(_, _, touching)| touching)
            .filter_map(|(a, b, _)| {
                let (a, b) = (&colliders[a], &colliders[b]);
                let (zone, other) = match (a.is_sensor(), b.is_sensor()) {
                    (true, false) => (a, b),
                    (false, true) => (b, a),
                    _ => return None,
                };
                Some((zone.user_data as u64, other.user_data as u64))
            });
        pairs.collect()
    }

    /// Where Rapier has a body now: its shape's centre.
    pub(crate) fn centre_of(&self, body: RigidBodyHandle) -> (f64, f64) {
        let at = self.world.bodies[body].translation();
        (at.x as f64, at.y as f64)
    }

    /// A thrown body that has slowed to rest becomes fixed — an obstacle again, and no more work.
    /// Not while it is still pressed into something: Rapier pushes it out slowly, so it would
    /// otherwise settle inside a wall. Resting contact keeps about 0.012 of overlap on purpose.
    pub(crate) fn settle(&mut self, body: RigidBodyHandle) -> bool {
        let collider = self.world.bodies[body].colliders()[0];
        let pressed = self
            .world
            .narrow_phase
            .contact_pairs_with(collider)
            .any(|pair| {
                let mut points = pair.manifolds.iter().flat_map(|m| m.points.iter());
                points.any(|p| p.dist < -0.05)
            });
        let rb = &mut self.world.bodies[body];
        let resting = !pressed && rb.linvel().length() < AT_REST;
        if resting {
            rb.set_body_type(RigidBodyType::Fixed, true);
        }
        resting
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
        let queries = self.world.query_pipeline_with_filter(
            QueryFilter::default()
                .exclude_rigid_body(body)
                .exclude_sensors(),
        );
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
