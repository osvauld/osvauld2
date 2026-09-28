//! A retained 2D world: entities kept by author id across frames in a `bevy_ecs` World. Lua
//! describes; `reconcile` spawns, updates and despawns to match. `pos` is read only at spawn —
//! once an entity exists its placement belongs to the world. List order is draw order. Nothing
//! outside this crate sees ECS types.

pub mod clip;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy_ecs::prelude::{Component, Entity, World};
use runtime::drawing::{Drawing, DrawingError};
use runtime::frame::{Frame, FrameError, Item};
use runtime::vello::kurbo::Affine;

use crate::clip::Clip;

pub struct EntitySpec {
    pub id: String,
    pub pos: (f64, f64),
    pub drawing: Arc<Drawing>,
    /// Played from when this handle first appears on the entity; the same handle keeps playing.
    pub clip: Option<Arc<Clip>>,
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub x: f64,
    pub y: f64,
    pub rot: f64,
    pub scale: f64,
}

impl Transform {
    fn affine(&self) -> Affine {
        Affine::translate((self.x, self.y))
            * Affine::rotate(self.rot.to_radians())
            * Affine::scale(self.scale)
    }
}

#[derive(Component)]
struct Name(String);

#[derive(Component)]
struct Look {
    drawing: Arc<Drawing>,
    rest: Arc<Frame>,
}

#[derive(Component)]
struct Animator {
    clip: Arc<Clip>,
    started: f64,
}

#[derive(Default)]
pub struct World2d {
    ecs: World,
    clock: f64,
    animating: bool,
    by_id: HashMap<String, Entity>,
    order: Vec<Entity>,
}

#[derive(Debug, thiserror::Error)]
pub enum WorldError {
    #[error("duplicate entity id {0:?}")]
    DuplicateId(String),
    #[error("entity id must not be empty")]
    EmptyId,
    #[error(transparent)]
    Drawing(#[from] DrawingError),
    #[error(transparent)]
    Frame(#[from] FrameError),
}

impl World2d {
    /// All-or-nothing: every check and every pose runs before the world is touched, so a bad
    /// description leaves the last good world in place.
    pub fn reconcile(&mut self, specs: Vec<EntitySpec>) -> Result<(), WorldError> {
        let mut seen = HashSet::with_capacity(specs.len());
        let mut looks = Vec::with_capacity(specs.len());
        for spec in &specs {
            if spec.id.is_empty() {
                return Err(WorldError::EmptyId);
            }
            if !seen.insert(spec.id.as_str()) {
                return Err(WorldError::DuplicateId(spec.id.clone()));
            }
            let current = self
                .by_id
                .get(&spec.id)
                .and_then(|&e| self.ecs.get::<Look>(e));
            let unchanged = current.is_some_and(|look| Arc::ptr_eq(&look.drawing, &spec.drawing));
            if let Some(clip) = &spec.clip
                && !(unchanged
                    && self
                        .by_id
                        .get(&spec.id)
                        .is_some_and(|&e| self.playing(e, clip)))
            {
                spec.drawing.pose(&clip.sample(0.0))?; // a clip may move only parts its drawing has
            }
            looks.push(match unchanged {
                true => None,
                false => Some(Arc::new(spec.drawing.pose(&HashMap::new())?)),
            });
        }
        self.animating = specs.iter().any(|s| s.clip.is_some());
        let mut next = HashMap::with_capacity(specs.len());
        let mut order = Vec::with_capacity(specs.len());
        for (spec, rest) in specs.into_iter().zip(looks) {
            let look = rest.map(|rest| Look {
                drawing: spec.drawing,
                rest,
            });
            let entity = match (self.by_id.remove(&spec.id), look) {
                (Some(entity), Some(look)) => {
                    self.ecs.entity_mut(entity).insert(look);
                    entity
                }
                (Some(entity), None) => entity,
                (None, look) => {
                    let (x, y) = spec.pos;
                    let transform = Transform {
                        x,
                        y,
                        rot: 0.0,
                        scale: 1.0,
                    };
                    let look = look.expect("a new entity always has a fresh look");
                    self.ecs
                        .spawn((transform, Name(spec.id.clone()), look))
                        .id()
                }
            };
            match spec.clip {
                None => {
                    self.ecs.entity_mut(entity).remove::<Animator>();
                }
                Some(clip) if !self.playing(entity, &clip) => {
                    let started = self.clock;
                    self.ecs
                        .entity_mut(entity)
                        .insert(Animator { clip, started });
                }
                Some(_) => {}
            }
            next.insert(spec.id, entity);
            order.push(entity);
        }
        for (_, gone) in self.by_id.drain() {
            self.ecs.despawn(gone);
        }
        (self.by_id, self.order) = (next, order);
        Ok(())
    }

    /// Each entity is an instance named by its id, so a hit on the world reports which entity.
    pub fn frame(&self, width: f64, height: f64) -> Result<Frame, WorldError> {
        let items = self
            .order
            .iter()
            .map(|&entity| {
                let (Some(t), Some(name), Some(look)) = (
                    self.ecs.get::<Transform>(entity),
                    self.ecs.get::<Name>(entity),
                    self.ecs.get::<Look>(entity),
                ) else {
                    unreachable!("reconcile spawns every entity with all three");
                };
                let posed = match self.ecs.get::<Animator>(entity) {
                    Some(a) => Arc::new(look.drawing.pose(&a.clip.sample(self.clock - a.started))?),
                    None => look.rest.clone(),
                };
                Ok(Item::instance(t.affine(), posed)?.with_id(name.0.as_str()))
            })
            .collect::<Result<Vec<_>, WorldError>>()?;
        Ok(Frame::new(width, height, None, items)?)
    }

    /// Sets the world clock, in seconds on the runtime's frame clock. Clips play against it.
    pub fn tick(&mut self, elapsed: f64) {
        self.clock = elapsed;
    }

    /// Whether any entity plays a clip — only then does the world need the frame clock.
    pub fn animating(&self) -> bool {
        self.animating
    }

    fn playing(&self, entity: Entity, clip: &Arc<Clip>) -> bool {
        self.ecs
            .get::<Animator>(entity)
            .is_some_and(|a| Arc::ptr_eq(&a.clip, clip))
    }

    pub fn transform(&self, id: &str) -> Option<Transform> {
        self.ecs.get::<Transform>(*self.by_id.get(id)?).copied()
    }
}

#[cfg(test)]
mod tests;
