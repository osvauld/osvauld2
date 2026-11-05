//! A world under a match (`jetpack-mp.md` §5): stepped by the match, not the frame clock; saved
//! before every match frame and put back on a rewind; read as bytes for the match's hash.
//!
//! A snapshot holds what moves — Rapier whole, each entity's place, velocity and steering, who
//! is in which zone and touching what — and not what the description gives. Restoring expects
//! the world already reconciled from the same frame's state, so its entities are the snapshot's;
//! a driven world is a function of the match's state, so the platform reconciles first. Entities
//! spawned anew by that reconcile get their old bodies back, re-tagged with their new owner.

use std::collections::HashMap;

use bevy_ecs::prelude::Entity;
use rapier2d::prelude::RigidBodyHandle;

use crate::physics::Physics;
use crate::{Heading, Joined, Name, STEP, Solid, Steer, Transform, Velocity, World2d, WorldError};

#[derive(Clone)]
struct Kept {
    owner: u64,
    body: Option<RigidBodyHandle>,
    sliding: bool,
    transform: Transform,
    velocity: Velocity,
    heading: Option<Heading>,
    steer: Option<Steer>,
}

#[derive(Clone)]
pub struct WorldSnapshot {
    physics: Physics,
    steps: u64,
    entities: HashMap<String, Kept>,
    inside: Vec<(String, String)>,
    touching: Vec<(String, String)>,
    joints: HashMap<String, Joined>,
}

impl World2d {
    fn name_of(&self, e: Entity) -> Option<String> {
        self.ecs.get::<Name>(e).map(|n| n.0.clone())
    }

    fn named_pairs(&self, pairs: &std::collections::HashSet<(Entity, Entity)>) -> Vec<(String, String)> {
        let mut out: Vec<_> = pairs.iter().filter_map(|&(a, b)| Some((self.name_of(a)?, self.name_of(b)?))).collect();
        out.sort();
        out
    }

    pub fn snapshot(&self) -> WorldSnapshot {
        let entities = self
            .order
            .iter()
            .map(|&e| {
                let r = self.ecs.entity(e);
                let solid = r.get::<Solid>();
                let kept = Kept {
                    owner: e.to_bits(),
                    body: solid.map(|s| s.body),
                    sliding: solid.is_some_and(|s| s.sliding),
                    transform: *r.get::<Transform>().expect("every entity has a Transform"),
                    velocity: r.get::<Velocity>().copied().unwrap_or_default(),
                    heading: r.get::<Heading>().copied(),
                    steer: r.get::<Steer>().copied(),
                };
                (r.get::<Name>().expect("every entity has a Name").0.clone(), kept)
            })
            .collect();
        WorldSnapshot {
            physics: self.physics.clone(),
            steps: self.steps,
            entities,
            inside: self.named_pairs(&self.inside),
            touching: self.named_pairs(&self.touching),
            joints: self.joints.clone(),
        }
    }

    /// Back to `snap`. The world must hold the snapshot's entities (see the module doc); one it
    /// does not know is an error, and the world is then left as it was.
    pub fn restore(&mut self, snap: &WorldSnapshot) -> Result<(), WorldError> {
        let names: Vec<(Entity, String)> = self.order.iter().map(|&e| (e, self.name_of(e).expect("named"))).collect();
        if names.len() != snap.entities.len() || names.iter().any(|(_, n)| !snap.entities.contains_key(n)) {
            return Err(WorldError::Restore("the world's entities are not the snapshot's".into()));
        }
        self.physics = snap.physics.clone();
        self.steps = snap.steps;
        self.carry = 0.0;
        self.grabs.clear();
        self.events.clear();
        self.joints = snap.joints.clone();
        for (e, name) in &names {
            let k = &snap.entities[name];
            let mut r = self.ecs.entity_mut(*e);
            if let (Some(mut s), Some(body)) = (r.get_mut::<Solid>(), k.body) {
                s.body = body;
                s.sliding = k.sliding;
            }
            r.insert(k.transform);
            r.insert(k.velocity);
            match k.heading {
                Some(h) => r.insert(h),
                None => r.remove::<Heading>(),
            };
            match k.steer {
                Some(s) => r.insert(s),
                None => r.remove::<Steer>(),
            };
            if let (Some(body), true) = (k.body, k.owner != e.to_bits()) {
                self.physics.set_owner(body, e.to_bits());
            }
        }
        let by: HashMap<&str, Entity> = names.iter().map(|(e, n)| (n.as_str(), *e)).collect();
        let pairs = |list: &[(String, String)]| {
            list.iter().filter_map(|(a, b)| Some((*by.get(a.as_str())?, *by.get(b.as_str())?))).collect()
        };
        self.inside = pairs(&snap.inside);
        self.touching = pairs(&snap.touching);
        Ok(())
    }

    pub fn set_driven(&mut self, driven: bool) {
        self.driven = driven;
    }

    pub fn is_driven(&self) -> bool {
        self.driven
    }

    /// A driven world takes commands only while its match runs `step`.
    pub fn open_commands(&mut self, open: bool) {
        self.commands_open = open;
    }

    pub fn takes_commands(&self) -> bool {
        !self.driven || self.commands_open
    }

    /// A driven world's frame clock: clips play on it and carried things follow, and nothing
    /// queues — what happens in a match is the match's.
    pub fn show_at(&mut self, elapsed: f64) {
        self.clock = elapsed;
        self.follow();
    }

    /// `steps` fixed steps, for a match: no timers, no frame clock. What happened queues as
    /// events, as on a ticked world.
    pub fn step_driven(&mut self, steps: u32) {
        for _ in 0..steps {
            self.step(STEP);
        }
        self.follow();
    }

    /// A controller's axes from now on, each -1..1, in place of held keys: a match's input.
    pub fn steer(&mut self, id: &str, axes: (f64, f64)) -> Result<(), WorldError> {
        let err = |why: &str| WorldError::Set(id.to_string(), why.to_string());
        let &entity = self.by_id.get(id).ok_or_else(|| err("there is no such entity"))?;
        if self.ecs.get::<crate::Controller>(entity).is_none() {
            return Err(err("move needs a controller: it steers one"));
        }
        if !(axes.0.is_finite() && axes.1.is_finite()) {
            return Err(err("move must be finite numbers"));
        }
        let clamp = |v: f64| v.clamp(-1.0, 1.0);
        self.ecs.entity_mut(entity).insert(Steer(clamp(axes.0), clamp(axes.1)));
        Ok(())
    }

    /// The world as bytes two devices must agree on: Rapier's bodies, then each entity's place
    /// in list order, and the step count.
    pub fn digest(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(self.steps.to_le_bytes());
        self.physics.digest(&mut out);
        for &e in &self.order {
            let t = self.ecs.get::<Transform>(e).expect("every entity has a Transform");
            for f in [t.x, t.y, t.rot] {
                out.extend(f.to_bits().to_le_bytes());
            }
        }
        out
    }
}
