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

/// What a body Rapier moves is made of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// The share of speed it keeps off what it hits: 0 dead, 1 as fast as it came.
    pub bounce: f64,
    /// How fast the floor slows it, as speed lost per second: 0 never slows; 6 stops a walker's
    /// pace in about half a second. It slows a spin the same.
    pub friction: f64,
    /// How much its surface catches at a contact: a glancing hit sets it turning. Without, it
    /// never turns — the drawing stays upright.
    pub grip: Option<f64>,
}

impl Default for Material {
    /// A let-go chest's: a modest bounce, at rest in about half a second.
    fn default() -> Self {
        Self { bounce: 0.5, friction: 6.0, grip: None }
    }
}

impl Material {
    pub(crate) fn check(&self) -> Result<(), String> {
        if !(0.0..=1.0).contains(&self.bounce) {
            return Err(format!("bounce must be from 0 to 1, got {}", self.bounce));
        }
        if !(self.friction.is_finite() && self.friction >= 0.0) {
            return Err(format!("friction must be 0 or more, got {}", self.friction));
        }
        if let Some(g) = self.grip.filter(|g| !(g.is_finite() && *g >= 0.0)) {
            return Err(format!("grip must be 0 or more, got {g}"));
        }
        Ok(())
    }
}

/// Slower than this, in drawing units a second, a sliding body has come to rest.
const AT_REST: f32 = 4.0;

/// What moves a body: nothing, the world's controller, or Rapier from a starting velocity.
#[derive(Clone, Copy)]
pub(crate) enum Body {
    Fixed,
    Moved,
    Dynamic((f64, f64), Material),
}

pub(crate) struct Physics {
    world: PhysicsWorld,
}

impl Default for Physics {
    fn default() -> Self {
        let mut world = PhysicsWorld::new();
        world.gravity = Vector::ZERO;
        // Rapier's tolerances are in metres; ours are drawing units, about a hundred to the metre.
        // Left at 1, a body could go no faster than 400 units a second, and an overlap was pushed
        // apart at 3. Resting overlap stays a tenth of a unit, not half: less than a pixel shows.
        world.integration_parameters.length_unit = 100.0;
        world.integration_parameters.normalized_allowed_linear_error = 0.001;
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
    /// `groups` is the solid shape's `(memberships, filter)` bits: two solids meet only when each
    /// is in a group the other blocks.
    pub(crate) fn add(
        &mut self,
        solid: Option<&Collider>,
        sensor: Option<&Collider>,
        origin: (f64, f64),
        kind: Body,
        owner: u64,
        groups: (u32, u32),
    ) -> RigidBodyHandle {
        let anchor = solid.or(sensor).expect("a body has a shape").centre();
        let at = Vector::new((origin.0 + anchor.0) as f32, (origin.1 + anchor.1) as f32);
        let body = match kind {
            Body::Fixed => RigidBodyBuilder::fixed(),
            Body::Moved => RigidBodyBuilder::kinematic_position_based(),
            Body::Dynamic((vx, vy), m) => {
                let b = RigidBodyBuilder::dynamic()
                    .linvel(Vector::new(vx as f32, vy as f32))
                    .linear_damping(m.friction as f32)
                    .angular_damping(m.friction as f32);
                if m.grip.is_some() { b } else { b.lock_rotations() }
            }
        };
        // Walls and walkers keep nothing; a bounce is the moving body's own (the larger of two).
        let (bounce, grip) = match kind {
            Body::Dynamic(_, m) => (m.bounce as f32, m.grip.unwrap_or(0.0) as f32),
            _ => (0.0, 0.0),
        };
        let body = self.world.insert_body(body.translation(at));
        let (bodies, colliders) = (&mut self.world.bodies, &mut self.world.colliders);
        if let Some(c) = solid {
            let solid = shape(c, anchor)
                .restitution(bounce)
                .restitution_combine_rule(CoefficientCombineRule::Max)
                .friction(grip)
                .friction_combine_rule(CoefficientCombineRule::Max)
                .collision_groups(InteractionGroups::new(
                    Group::from_bits_retain(groups.0),
                    Group::from_bits_retain(groups.1),
                    InteractionTestMode::And,
                ))
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
    /// each other. A pair whose collider was removed since goes: Rapier drops it only at a step,
    /// and a frame with no time passing has none.
    pub(crate) fn overlaps(&self) -> Vec<(u64, u64)> {
        let colliders = &self.world.colliders;
        let pairs = self.world.narrow_phase.intersection_pairs();
        let pairs = pairs
            .filter(|&(_, _, touching)| touching)
            .filter_map(|(a, b, _)| {
                let (a, b) = (colliders.get(a)?, colliders.get(b)?);
                let (zone, other) = match (a.is_sensor(), b.is_sensor()) {
                    (true, false) => (a, b),
                    (false, true) => (b, a),
                    _ => return None,
                };
                Some((zone.user_data as u64, other.user_data as u64))
            });
        pairs.collect()
    }

    /// Every pair of solid colliders in contact as of the last step, as `(owner, owner, normal)`.
    /// Speculative contacts count: Rapier makes them a step ahead of a fast meeting.
    pub(crate) fn contacts(&self) -> Vec<(u64, u64, (f64, f64))> {
        let colliders = &self.world.colliders;
        (self.world.narrow_phase.contact_pairs())
            .filter(|p| p.has_any_active_contact())
            .filter_map(|p| {
                let (a, b) = (colliders.get(p.collider1)?, colliders.get(p.collider2)?);
                let n = p.manifolds.first()?.data.normal;
                Some((a.user_data as u64, b.user_data as u64, (n.x as f64, n.y as f64)))
            })
            .collect()
    }

    /// The first solid collider on the line from `from` to `to`, not `skip`'s: its owner, where
    /// the line met it, the surface's normal there and how far along. Sensors do not stop a ray;
    /// a line starting inside a collider meets it at once. As of the last step.
    pub(crate) fn ray(
        &self,
        from: (f64, f64),
        to: (f64, f64),
        skip: Option<RigidBodyHandle>,
    ) -> Option<(u64, (f64, f64), (f64, f64), f64)> {
        let filter = QueryFilter::default().exclude_sensors();
        let filter = match skip {
            Some(body) => filter.exclude_rigid_body(body),
            None => filter,
        };
        let queries = self.world.query_pipeline_with_filter(filter);
        let (o, d) = (Vector::new(from.0 as f32, from.1 as f32), Vector::new((to.0 - from.0) as f32, (to.1 - from.1) as f32));
        let (c, hit) = queries.cast_ray_and_get_normal(&Ray::new(o, d), 1.0, true)?;
        let (at, n) = (o + d * hit.time_of_impact, hit.normal);
        let owner = self.world.colliders.get(c)?.user_data as u64;
        let dist = hit.time_of_impact as f64 * (d.length() as f64);
        Some((owner, (at.x as f64, at.y as f64), (n.x as f64, n.y as f64), dist))
    }

    /// The owners of every collider, solid or sensor, that covers `point`, as of the last step.
    pub(crate) fn at(&self, point: (f64, f64)) -> Vec<u64> {
        let queries = self.world.query_pipeline_with_filter(QueryFilter::default());
        let point = Vector::new(point.0 as f32, point.1 as f32);
        queries.intersect_point(point).map(|(_, c)| c.user_data as u64).collect()
    }

    /// Where Rapier has a body now: its shape's centre.
    pub(crate) fn centre_of(&self, body: RigidBodyHandle) -> (f64, f64) {
        let at = self.world.bodies[body].translation();
        (at.x as f64, at.y as f64)
    }

    /// At rest until something touches it: nothing to step meanwhile. Not if it is already in
    /// something — a puck put down under a paddle — or it would sleep there for good; awake,
    /// Rapier pushes it out, and it sleeps by itself once at rest.
    pub(crate) fn sleep_if_clear(&mut self, body: RigidBodyHandle) {
        let me = &self.world.colliders[self.world.bodies[body].colliders()[0]];
        let filter = QueryFilter::default().exclude_rigid_body(body).exclude_sensors();
        let queries = self.world.query_pipeline_with_filter(filter);
        if queries.intersect_shape(*me.position(), me.shape()).next().is_none() {
            self.world.bodies[body].sleep();
        }
    }

    /// Puts a body's centre at `at` and sets its velocity and spin (degrees a second), each if
    /// given, and wakes it.
    pub(crate) fn set(
        &mut self,
        body: RigidBodyHandle,
        at: Option<(f64, f64)>,
        v: Option<(f64, f64)>,
        spin: Option<f64>,
    ) {
        let rb = &mut self.world.bodies[body];
        if let Some(spin) = spin {
            rb.set_angvel(spin.to_radians() as f32, true);
        }
        if let Some((x, y)) = at {
            rb.set_translation(Vector::new(x as f32, y as f32), true);
        }
        if let Some((vx, vy)) = v {
            rb.set_linvel(Vector::new(vx as f32, vy as f32), true);
        }
        rb.wake_up(true);
    }

    pub(crate) fn asleep(&self, body: RigidBodyHandle) -> bool {
        self.world.bodies[body].is_sleeping()
    }

    /// How far a body has turned and how fast it turns, in degrees and degrees a second.
    pub(crate) fn turn(&self, body: RigidBodyHandle) -> (f64, f64) {
        let rb = &self.world.bodies[body];
        ((rb.rotation().angle() as f64).to_degrees(), (rb.angvel() as f64).to_degrees())
    }

    pub(crate) fn velocity(&self, body: RigidBodyHandle) -> (f64, f64) {
        let v = self.world.bodies[body].linvel();
        (v.x as f64, v.y as f64)
    }

    /// A thrown body that has slowed to rest becomes fixed — an obstacle again, and no more work.
    /// Not while it is still pressed into something: Rapier pushes it out slowly, so it would
    /// otherwise settle inside a wall. Resting contact keeps some overlap on purpose — Rapier's
    /// allowed error — so pressed is deeper than twice that. Measured afresh: Rapier keeps a
    /// contact's old depth until the pair moves past its tolerance, so a body pushed out after it
    /// first touched can look pressed in for good.
    pub(crate) fn settle(&mut self, body: RigidBodyHandle) -> bool {
        let colliders = &self.world.colliders;
        let me = self.world.bodies[body].colliders()[0];
        let deep = -2.0 * self.world.integration_parameters.allowed_linear_error();
        let pressed = self.world.narrow_phase.contact_pairs_with(me).any(|pair| {
            let other = if pair.collider1 == me { pair.collider2 } else { pair.collider1 };
            let (a, b) = (&colliders[me], &colliders[other]);
            let now = rapier2d::parry::query::contact(a.position(), a.shape(), b.position(), b.shape(), 0.0);
            now.ok().flatten().is_some_and(|c| c.dist < deep)
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
    /// meets, and says how far that was. What Rapier moves it does not stop at: Rapier's step
    /// meets those with the walker's velocity, so it strikes them rather than shoving.
    pub(crate) fn slide(
        &mut self,
        body: RigidBodyHandle,
        wanted: (f64, f64),
        dt: f64,
    ) -> (f64, f64) {
        let rb = &self.world.bodies[body];
        let solid = &self.world.colliders[rb.colliders()[0]];
        let (pose, shape, groups) = (*rb.position(), solid.shared_shape().clone(), solid.collision_groups());
        let filter = |only: QueryFilterFlags| {
            let filter = QueryFilter::from(only).exclude_rigid_body(body).exclude_sensors();
            filter.groups(groups)
        };
        let wanted = Vector::new(wanted.0 as f32, wanted.1 as f32);
        // A loose thing at rest is asleep, and Rapier lets a moving walker through a sleeper.
        let loose = self.world.query_pipeline_with_filter(filter(QueryFilterFlags::ONLY_DYNAMIC));
        let mut met = vec![];
        WALKER.move_shape(dt as f32, &loose, &*shape, &pose, wanted, |c| met.push(c.handle));
        for c in met {
            let parent = self.world.colliders.get(c).and_then(|c| c.parent());
            parent.and_then(|b| self.world.bodies.get_mut(b)).map(|b| b.wake_up(true));
        }
        let queries = self.world.query_pipeline_with_filter(filter(QueryFilterFlags::EXCLUDE_DYNAMIC));
        let moved = WALKER.move_shape(dt as f32, &queries, &*shape, &pose, wanted, |_| {});
        // Where it will be after the step, so Rapier knows its velocity.
        let to = pose.translation + moved.translation;
        self.world.bodies[body].set_next_kinematic_translation(to);
        (moved.translation.x as f64, moved.translation.y as f64)
    }

    /// A let-go body's spot may be in a wall: carried, it had no body to stop it. So it comes from
    /// its carrier's body to that spot, stopping at what is in the way — against the wall, never
    /// inside it or past it.
    pub(crate) fn bring_in(&mut self, body: RigidBodyHandle, from: RigidBodyHandle) {
        let rb = &self.world.bodies[body];
        let (mut pose, spot) = (*rb.position(), rb.translation());
        let solid = &self.world.colliders[rb.colliders()[0]];
        let (shape, groups) = (solid.shared_shape().clone(), solid.collision_groups());
        pose.translation = self.world.bodies[from].translation();
        let neither = |_, c: &rapier2d::geometry::Collider| ![Some(body), Some(from)].contains(&c.parent());
        let filter = QueryFilter::default().exclude_sensors().predicate(&neither).groups(groups);
        let queries = self.world.query_pipeline_with_filter(filter);
        let moved = WALKER.move_shape(1.0, &queries, &*shape, &pose, spot - pose.translation, |_| {});
        self.world.bodies[body].set_translation(pose.translation + moved.translation, true);
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
