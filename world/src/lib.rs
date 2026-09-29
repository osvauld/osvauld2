//! A retained 2D world: entities kept by author id across frames in a `bevy_ecs` World. Lua
//! describes; `reconcile` spawns, updates and despawns to match. `pos` is read only at spawn —
//! once an entity exists its placement belongs to the world. List order is draw order unless the
//! world sorts by feet (`Order::Feet`). Nothing
//! outside this crate sees ECS types. Held keys and motion live here too: Lua describes a
//! controller once, and `tick` moves the entity every frame without Lua. Moments — a press, a
//! change of direction — queue as `WorldEvent`s for Lua to decide on.

pub mod clip;
pub mod facing;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy_ecs::prelude::{Component, Entity, World};
use runtime::drawing::{Drawing, DrawingError};
use runtime::frame::{Frame, FrameError, Item};
use runtime::vello::kurbo::Affine;

use crate::clip::Clip;
use crate::facing::{Dir, Facings, Shown};

pub struct EntitySpec {
    pub id: String,
    pub pos: (f64, f64),
    pub drawing: Arc<Drawing>,
    /// Played from when this handle first appears on the entity; the same handle keeps playing.
    pub clip: Option<Arc<Clip>>,
    pub controller: Option<Controller>,
    pub facing: Facings,
}

/// Two key codes driving one axis: `neg` held is -1, `pos` held is +1, both or neither is 0.
#[derive(Clone, Debug, PartialEq)]
pub struct Axis {
    pub neg: String,
    pub pos: String,
}

/// Moves its entity at `speed` units a second along the held axes; a diagonal is no faster.
/// `moving` is the clip played while it moves, in place of the entity's own.
#[derive(Component, Clone, Debug)]
pub struct Controller {
    pub speed: f64,
    pub axis_x: Option<Axis>,
    pub axis_y: Option<Axis>,
    pub moving: Option<Arc<Clip>>,
}

/// A moment for Lua; per-frame work never makes one.
#[derive(Clone, Debug, PartialEq)]
pub enum WorldEvent {
    /// An entity's held direction changed: `dx`, `dy` are each -1, 0 or 1, and both 0 is stopped.
    Move { id: String, dx: i8, dy: i8 },
    /// A fresh press of the key an action names; a held key's repeats are not presses.
    Action(String),
}

/// How entities stack when drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Order {
    /// The description's list order, back to front.
    #[default]
    List,
    /// The lower an entity's feet (the bottom of its box) on screen, the nearer it draws — a
    /// top-down room. Ties keep list order. Rotation is ignored.
    Feet,
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

/// Everything the entity can show, plus the rest pose of each distinct drawing among its views —
/// posing is the cost, so reconcile carries unchanged ones over.
#[derive(Component)]
struct Appearance {
    drawing: Arc<Drawing>,
    clip: Option<Arc<Clip>>,
    facing: Facings,
    rests: Vec<(Arc<Drawing>, Arc<Frame>)>,
}

impl Appearance {
    fn shown<'a>(&'a self, dir: Dir, controller: Option<&'a Controller>) -> Shown<'a> {
        let moving = controller.and_then(|c| c.moving.as_ref());
        self.facing
            .shown(dir, &self.drawing, self.clip.as_ref(), moving)
    }

    fn rest(&self, drawing: &Arc<Drawing>) -> Option<&Arc<Frame>> {
        let mut rests = self.rests.iter();
        rests.find(|(d, _)| Arc::ptr_eq(d, drawing)).map(|(_, f)| f)
    }
}

/// Set by `tick` while a controller is moving its entity; kept across reconciles.
#[derive(Component)]
struct Moving;

/// The held direction `tick` last reported, so it reports only a change.
#[derive(Component, Default, PartialEq)]
struct Heading(i8, i8);

#[derive(Component)]
struct Animator {
    clip: Arc<Clip>,
    started: f64,
}

#[derive(Default)]
pub struct World2d {
    ecs: World,
    clock: f64,
    ticking: bool,
    controlled: bool,
    held: HashSet<String>,
    /// `(action, key code)`, as the world's `actions` name them.
    actions: Vec<(String, String)>,
    events: Vec<WorldEvent>,
    by_id: HashMap<String, Entity>,
    order: Vec<Entity>,
    stacking: Order,
}

#[derive(Debug, thiserror::Error)]
pub enum WorldError {
    #[error("duplicate entity id {0:?}")]
    DuplicateId(String),
    #[error("entity id must not be empty")]
    EmptyId,
    #[error("entity {0:?}: controller speed must be a finite number, zero or more")]
    Speed(String),
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
        let mut all_rests = Vec::with_capacity(specs.len());
        for spec in &specs {
            if spec.id.is_empty() {
                return Err(WorldError::EmptyId);
            }
            if !seen.insert(spec.id.as_str()) {
                return Err(WorldError::DuplicateId(spec.id.clone()));
            }
            if let Some(c) = &spec.controller
                && !(c.speed.is_finite() && c.speed >= 0.0)
            {
                return Err(WorldError::Speed(spec.id.clone()));
            }
            let current = self
                .by_id
                .get(&spec.id)
                .and_then(|&e| self.ecs.get::<Appearance>(e));
            let moving = spec.controller.as_ref().and_then(|c| c.moving.as_ref());
            let mut rests: Vec<(Arc<Drawing>, Arc<Frame>)> = Vec::new();
            for dir in Dir::ALL {
                let shown = spec
                    .facing
                    .shown(dir, &spec.drawing, spec.clip.as_ref(), moving);
                // A clip may move only parts the drawing it plays on has, in every facing.
                for clip in shown.clip.iter().chain(shown.moving.iter()) {
                    if let Some(part) = clip.parts().find(|p| !shown.drawing.has_part(p)) {
                        return Err(DrawingError::UnknownPart(part.to_string()).into());
                    }
                }
                if rests.iter().any(|(d, _)| Arc::ptr_eq(d, shown.drawing)) {
                    continue;
                }
                let rest = match current.and_then(|a| a.rest(shown.drawing)) {
                    Some(rest) => rest.clone(),
                    None => Arc::new(shown.drawing.pose(&HashMap::new())?),
                };
                rests.push((shown.drawing.clone(), rest));
            }
            all_rests.push(rests);
        }
        self.controlled = specs.iter().any(|s| s.controller.is_some());
        if !self.wants_keys() {
            self.held.clear(); // key events stop arriving, so nothing can release them later
        }
        self.ticking = specs
            .iter()
            .any(|s| s.clip.is_some() || s.controller.is_some());
        let mut next = HashMap::with_capacity(specs.len());
        let mut order = Vec::with_capacity(specs.len());
        for (spec, rests) in specs.into_iter().zip(all_rests) {
            let entity = match self.by_id.remove(&spec.id) {
                Some(entity) => entity,
                None => {
                    let (x, y) = spec.pos;
                    let transform = Transform {
                        x,
                        y,
                        rot: 0.0,
                        scale: 1.0,
                    };
                    let name = Name(spec.id.clone());
                    self.ecs.spawn((transform, name, Dir::default())).id()
                }
            };
            let mut e = self.ecs.entity_mut(entity);
            e.insert(Appearance {
                drawing: spec.drawing,
                clip: spec.clip,
                facing: spec.facing,
                rests,
            });
            match spec.controller {
                Some(controller) => e.insert(controller),
                None => e.remove::<(Controller, Moving, Heading)>(),
            };
            self.animate(entity);
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
        let mut items = self
            .order
            .iter()
            .map(|&entity| {
                let e = self.ecs.entity(entity);
                let (Some(t), Some(name), Some(look), Some(&dir)) = (
                    e.get::<Transform>(),
                    e.get::<Name>(),
                    e.get::<Appearance>(),
                    e.get::<Dir>(),
                ) else {
                    unreachable!("reconcile gives every entity all four");
                };
                let shown = look.shown(dir, e.get::<Controller>());
                let posed = match e.get::<Animator>() {
                    Some(a) => {
                        let poses = a.clip.sample(self.clock - a.started);
                        Arc::new(shown.drawing.pose(&poses)?)
                    }
                    None => look
                        .rest(shown.drawing)
                        .expect("posed at reconcile")
                        .clone(),
                };
                let mut place = t.affine();
                let (w, h) = shown.drawing.size();
                if shown.mirrored {
                    place *= Affine::new([-1.0, 0.0, 0.0, 1.0, w, 0.0]); // flip within its box
                }
                let feet = t.y + h * t.scale;
                Ok((feet, Item::instance(place, posed)?.with_id(name.0.as_str())))
            })
            .collect::<Result<Vec<_>, WorldError>>()?;
        if self.stacking == Order::Feet {
            items.sort_by(|a, b| a.0.total_cmp(&b.0)); // stable, so ties keep list order
        }
        let items = items.into_iter().map(|(_, item)| item).collect();
        Ok(Frame::new(width, height, None, items)?)
    }

    /// One frame: `elapsed` is the runtime's frame clock, which clips play against; `dt` is the
    /// step controllers move by (the runtime caps it after a stall).
    pub fn tick(&mut self, elapsed: f64, dt: f64) {
        self.clock = elapsed;
        for entity in self.order.clone() {
            let Some(c) = self.ecs.get::<Controller>(entity) else {
                continue;
            };
            let (dx, dy) = (self.axis(&c.axis_x), self.axis(&c.axis_y));
            let (length, step) = ((dx * dx + dy * dy).sqrt(), c.speed * dt);
            let heading = Heading(dx as i8, dy as i8);
            let mut e = self.ecs.entity_mut(entity);
            if e.get::<Heading>() != Some(&heading) && (e.contains::<Heading>() || length > 0.0) {
                let id = e.get::<Name>().expect("every entity has a Name").0.clone();
                let (dx, dy) = (heading.0, heading.1);
                self.events.push(WorldEvent::Move { id, dx, dy });
            }
            e.insert(heading);
            if length > 0.0 {
                let mut t = e
                    .get_mut::<Transform>()
                    .expect("every entity has a Transform");
                (t.x, t.y) = (t.x + dx / length * step, t.y + dy / length * step);
                let dir = Dir::of(dx, dy, *e.get::<Dir>().expect("every entity has a Dir"));
                e.insert((Moving, dir));
            } else {
                e.remove::<Moving>();
            }
            self.animate(entity);
        }
    }

    /// A key went down or up; `code` is the physical key name, as axes and actions name it.
    pub fn key(&mut self, code: &str, down: bool) {
        if !down {
            self.held.remove(code);
            return;
        }
        if self.held.insert(code.to_string()) {
            for (action, _) in self.actions.iter().filter(|(_, key)| key == code) {
                self.events.push(WorldEvent::Action(action.clone()));
            }
        }
    }

    /// Names the keys whose presses reach Lua as actions.
    pub fn set_actions(&mut self, actions: Vec<(String, String)>) {
        self.actions = actions;
        if !self.wants_keys() {
            self.held.clear();
        }
    }

    /// The moments since the last drain, oldest first.
    pub fn drain_events(&mut self) -> Vec<WorldEvent> {
        std::mem::take(&mut self.events)
    }

    /// Focus left, so no key is known to be down any more.
    pub fn release_all(&mut self) {
        self.held.clear();
    }

    pub fn set_order(&mut self, order: Order) {
        self.stacking = order;
    }

    /// Whether anything plays or moves — only then does the world need the frame clock.
    pub fn needs_ticks(&self) -> bool {
        self.ticking
    }

    /// Whether any entity has a controller or the world has actions — only then should it take
    /// keys.
    pub fn wants_keys(&self) -> bool {
        self.controlled || !self.actions.is_empty()
    }

    fn axis(&self, axis: &Option<Axis>) -> f64 {
        axis.as_ref().map_or(0.0, |a| {
            let held = |code: &String| self.held.contains(code) as u8 as f64;
            held(&a.pos) - held(&a.neg)
        })
    }

    /// Plays the clip the entity should show now, for its facing: the `moving` clip while it
    /// moves, else its own. Switching handles restarts from the current clock; the same continues.
    fn animate(&mut self, entity: Entity) {
        let e = self.ecs.entity(entity);
        let moving = e.contains::<Moving>();
        let (Some(look), Some(&dir)) = (e.get::<Appearance>(), e.get::<Dir>()) else {
            unreachable!("reconcile gives every entity an Appearance and a Dir");
        };
        let shown = look.shown(dir, e.get::<Controller>());
        let wanted = shown.moving.filter(|_| moving).or(shown.clip).cloned();
        match wanted {
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
    }

    fn playing(&self, entity: Entity, clip: &Arc<Clip>) -> bool {
        self.ecs
            .get::<Animator>(entity)
            .is_some_and(|a| Arc::ptr_eq(&a.clip, clip))
    }

    pub fn transform(&self, id: &str) -> Option<Transform> {
        self.ecs.get::<Transform>(*self.by_id.get(id)?).copied()
    }

    pub fn facing(&self, id: &str) -> Option<Dir> {
        self.ecs.get::<Dir>(*self.by_id.get(id)?).copied()
    }
}

#[cfg(test)]
mod tests;
