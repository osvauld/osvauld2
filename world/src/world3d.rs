//! Retained 3D bodies by author ID. Reconcile changes membership, never simulated placement.
//! ECS and Rapier handles are private; mesh/camera declarations remain a separate concern.
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy_ecs::prelude::{Component, Entity, World};
use rapier3d::prelude::RigidBodyHandle;
use runtime::scene3d::{Scene3d, SceneError};

use crate::physics3d::Physics3d;
pub use crate::physics3d::{
    BodyProperties3d, WheelInput3d, WheelMotor3d, WheelSpec3d, WheelState3d,
};
pub use crate::physics3d::{BodyState, Set3d, Shape3d};

mod clock;
mod components;
mod worlds;
mod zones;
use clock::FixedClock;
pub use clock::{AdvanceReport, ClockError};
pub use worlds::{WorldRecipe3d, WorldRecipes3d, Worlds3d, Worlds3dError};
pub use zones::{ZoneEvent3d, ZonePhase3d};

#[derive(Clone, Debug, PartialEq)]
pub struct EntitySpec3d {
    pub id: String,
    pub shape: Shape3d,
    /// Metre-scale body centre: spawn position for a new ID, reset target for a retained ID.
    pub position: [f32; 3],
    /// Quaternion x/y/z/w, normalized by physics; retained edits change only the reset target.
    pub rotation: [f32; 4],
    pub dynamic: bool,
    pub sensor: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityInspection3d {
    pub authored: EntitySpec3d,
    pub properties: BodyProperties3d,
    pub mass: f32,
    pub resolved: BodyState,
    pub zones: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldInspection3d {
    pub wheels: Vec<WheelState3d>,
    pub gravity: [f32; 3],
    pub tick: u64,
    pub dropped_seconds: f64,
    pub dropped_zone_events: u64,
    pub entities: Vec<EntityInspection3d>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum World3dError {
    #[error("invalid 3D wheel configuration or actuation: {0}")]
    Wheel(String),
    #[error("invalid 3D body properties: {0}")]
    Properties(String),
    #[error("3D mesh collider {0} must be fixed and cannot be a sensor")]
    MeshBody(String),
    #[error("3D world exceeds its 131072 collision-triangle budget")]
    CollisionBudget,
    #[error("3D gravity components must be finite numbers bounded to 10000 m/s²")]
    Gravity,
    #[error(transparent)]
    Scene(#[from] SceneError),
    #[error("3D world is limited to 256 bodies")]
    Budget,
    #[error("3D body id is empty, too long, or repeated: {0}")]
    Id(String),
    #[error("3D body {0} has invalid position, rotation or dimensions")]
    Body(String),
    #[error("remove body {0} before changing its collider or body type")]
    Changed(String),
    #[error("3D body {0} needs finite bounded commands; velocity/spin require a dynamic body")]
    Command(String),
    #[error("3D sensor {0} must be fixed")]
    Sensor(String),
    #[error("3D body does not exist: {0}")]
    Missing(String),
}

#[derive(Component)]
struct Body {
    spec: EntitySpec3d,
    handle: RigidBodyHandle,
}

#[derive(Default)]
pub struct World3d {
    ecs: World,
    ids: HashMap<String, Entity>,
    physics: Physics3d,
    properties: std::collections::BTreeMap<String, BodyProperties3d>,
    clock: FixedClock,
    zones: std::collections::BTreeSet<(String, String)>,
    events: std::collections::VecDeque<ZoneEvent3d>,
    dropped_zone_events: u64,
    dirty: bool,
}

impl World3d {
    pub fn validate_gravity(gravity: [f32; 3]) -> Result<(), World3dError> {
        if gravity
            .into_iter()
            .all(|v| v.is_finite() && v.abs() <= 10_000.0)
        {
            Ok(())
        } else {
            Err(World3dError::Gravity)
        }
    }

    pub fn set_gravity(&mut self, gravity: [f32; 3]) -> Result<(), World3dError> {
        Self::validate_gravity(gravity)?;
        self.physics.set_gravity(gravity);
        Ok(())
    }

    /// Preflight a recipe without changing membership, reset targets, clocks or solver state.
    /// Hosts use this to validate every staged world before accepting a whole-VM reload.
    pub fn validate_reconcile(&self, specs: &[EntitySpec3d]) -> Result<(), World3dError> {
        if specs.len() > 256 {
            return Err(World3dError::Budget);
        }
        let mut seen = HashSet::new();
        let mut collision_triangles = 0;
        // Validate the whole batch before membership, reset targets or handles can change.
        for spec in specs {
            if spec.id.is_empty() || spec.id.len() > 128 || !seen.insert(spec.id.clone()) {
                return Err(World3dError::Id(spec.id.clone()));
            }
            if spec.sensor && spec.dynamic {
                return Err(World3dError::Sensor(spec.id.clone()));
            }
            if let Shape3d::Mesh(mesh) = &spec.shape {
                if spec.dynamic || spec.sensor {
                    return Err(World3dError::MeshBody(spec.id.clone()));
                }
                collision_triangles += mesh.triangle_count();
                if collision_triangles > 131_072 {
                    return Err(World3dError::CollisionBudget);
                }
            }
            Physics3d::validate(&spec.shape, spec.position, spec.rotation)
                .map_err(|_| World3dError::Body(spec.id.clone()))?;
            if let Some(e) = self.ids.get(&spec.id) {
                let old = &self.ecs.get::<Body>(*e).unwrap().spec;
                if old.shape != spec.shape
                    || old.dynamic != spec.dynamic
                    || old.sensor != spec.sensor
                {
                    return Err(World3dError::Changed(spec.id.clone()));
                }
            }
        }
        Ok(())
    }

    pub fn reconcile(&mut self, specs: Vec<EntitySpec3d>) -> Result<(), World3dError> {
        self.validate_reconcile(&specs)?;
        let seen: HashSet<_> = specs.iter().map(|spec| spec.id.clone()).collect();
        let mut gone: Vec<_> = self
            .ids
            .keys()
            .filter(|id| !seen.contains(*id))
            .cloned()
            .collect();
        gone.sort();
        for id in gone {
            let e = self.ids.remove(&id).unwrap();
            self.properties.remove(&id);
            self.physics.remove(self.ecs.get::<Body>(e).unwrap().handle);
            self.ecs.despawn(e);
            self.dirty = true;
        }
        self.prune_zones();
        for spec in specs {
            if let Some(e) = self.ids.get(&spec.id) {
                self.ecs.get_mut::<Body>(*e).unwrap().spec = spec;
            } else {
                let handle = self
                    .physics
                    .add(
                        spec.shape.clone(),
                        spec.position,
                        spec.rotation,
                        spec.dynamic,
                        spec.sensor,
                    )
                    .expect("validated before reconciliation");
                self.dirty = true;
                let id = spec.id.clone();
                self.ids
                    .insert(id, self.ecs.spawn(Body { spec, handle }).id());
            }
        }
        Ok(())
    }

    pub fn step(&mut self) {
        self.physics.step();
        self.dirty = false;
        self.update_zones();
    }
    pub fn tick(&self) -> u64 {
        self.physics.tick
    }

    pub fn advance(&mut self, elapsed: f64, running: bool) -> Result<AdvanceReport, ClockError> {
        let report = self.clock.advance(elapsed, running)?;
        for _ in 0..report.steps {
            self.step();
        }
        if !self.needs_ticks() {
            self.clock.pause();
        }
        Ok(report)
    }

    pub fn pause(&mut self) {
        self.clock.pause();
    }
    pub fn dropped_seconds(&self) -> f64 {
        self.clock.dropped_seconds
    }

    pub fn needs_ticks(&self) -> bool {
        self.dirty
            || self.ids.values().any(|e| {
                let body = self.ecs.get::<Body>(*e).unwrap();
                body.spec.dynamic && !self.physics.inspect(body.handle).sleeping
            })
    }

    pub fn body(&self, id: &str) -> Result<BodyState, World3dError> {
        let e = self
            .ids
            .get(id)
            .ok_or_else(|| World3dError::Missing(id.to_owned()))?;
        Ok(self
            .physics
            .inspect(self.ecs.get::<Body>(*e).unwrap().handle))
    }

    /// Matching visual IDs take the body's simulated world pose; unbound visuals stay authored.
    /// Camera, meshes, scale and appearance remain presentation data, independent of colliders.
    pub fn resolved_scene(&self, authored: &Scene3d) -> Result<Arc<Scene3d>, World3dError> {
        let objects = authored
            .objects
            .iter()
            .cloned()
            .map(|mut object| {
                if let Some(e) = self.ids.get(object.id.as_ref()) {
                    let state = self
                        .physics
                        .inspect(self.ecs.get::<Body>(*e).unwrap().handle);
                    object.position = state.position.into();
                    object.rotation = glam::Quat::from_array(state.rotation);
                } else if let Some((position, rotation)) =
                    self.physics.wheel_pose(object.id.as_ref())
                {
                    object.position = position.into();
                    object.rotation = glam::Quat::from_array(rotation);
                }
                object
            })
            .collect();
        Ok(Scene3d::new(authored.camera.clone(), objects)?)
    }

    pub fn inspect(&self) -> WorldInspection3d {
        let mut entities: Vec<_> = self
            .ids
            .values()
            .map(|e| {
                let body = self.ecs.get::<Body>(*e).unwrap();
                EntityInspection3d {
                    authored: body.spec.clone(),
                    properties: self
                        .properties
                        .get(&body.spec.id)
                        .cloned()
                        .unwrap_or_default(),
                    mass: self.physics.mass(body.handle),
                    resolved: self.physics.inspect(body.handle),
                    zones: self
                        .zones
                        .iter()
                        .filter(|(_, who)| who == &body.spec.id)
                        .map(|(id, _)| id.clone())
                        .collect(),
                }
            })
            .collect();
        entities.sort_by(|a, b| a.authored.id.cmp(&b.authored.id));
        WorldInspection3d {
            wheels: self.physics.inspect_wheels(),
            gravity: self.physics.gravity(),
            tick: self.tick(),
            dropped_seconds: self.dropped_seconds(),
            dropped_zone_events: self.dropped_zone_events,
            entities,
        }
    }

    pub fn set(&mut self, id: &str, to: Set3d) -> Result<(), World3dError> {
        let e = self
            .ids
            .get(id)
            .ok_or_else(|| World3dError::Missing(id.to_owned()))?;
        let handle = self.ecs.get::<Body>(*e).unwrap().handle;
        self.physics
            .set(handle, &to)
            .map_err(|_| World3dError::Command(id.to_owned()))?;
        self.dirty |= to.pos.is_some() || to.rotation.is_some();
        Ok(())
    }

    pub fn reset(&mut self, id: &str) -> Result<(), World3dError> {
        let e = self
            .ids
            .get(id)
            .ok_or_else(|| World3dError::Missing(id.to_owned()))?;
        let body = self.ecs.get::<Body>(*e).unwrap();
        self.physics
            .reset(body.handle, body.spec.position, body.spec.rotation)
            .map_err(|_| World3dError::Body(id.to_owned()))?;
        self.dirty = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
