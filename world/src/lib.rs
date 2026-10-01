//! A retained 2D world: entities kept by author id across frames in a `bevy_ecs` World. Lua
//! describes; `reconcile` spawns, updates and despawns to match. `pos` is read only at spawn —
//! once an entity exists its placement belongs to the world. List order is draw order unless the
//! world sorts by feet (`Order::Feet`). Nothing
//! outside this crate sees ECS types. Held keys and motion live here too: Lua describes a
//! controller once, and `tick` moves the entity every frame without Lua. Moments — a press, a
//! change of direction — queue as `WorldEvent`s for Lua to decide on.

pub mod clip;
mod physics;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy_ecs::prelude::{Component, Entity, World};
use runtime::drawing::{Drawing, DrawingError, Pose};
use runtime::frame::{Frame, FrameError, Item};
use runtime::vello::kurbo::{Affine, Point};

use crate::clip::Clip;
use crate::physics::{Body, Physics};
pub use crate::physics::{Collider, Shape};

pub struct EntitySpec {
    pub id: String,
    pub pos: (f64, f64),
    pub drawing: Arc<Drawing>,
    /// Played from when this handle first appears on the entity; the same handle keeps playing.
    pub clip: Option<Arc<Clip>>,
    pub controller: Option<Controller>,
    /// Mirrored within the drawing's box: a side view drawn facing right, shown facing left.
    pub flip: bool,
    pub attach: Option<Attach>,
    pub collider: Option<Collider>,
    /// A zone that blocks nothing; what moves into or out of it is reported.
    pub sensor: Option<Collider>,
}

/// Carried: the entity's `pivot` — a point in its own drawing — rides at `at`, a point in the
/// carrier's drawing at rest, as the carrier's `part` moves and animates. With `turn` it also
/// takes on the part's rotation, scale and mirror (a hat on a nodding head); without, it stays
/// upright (a chest in the arms). Removing it leaves the entity where it was last carried.
#[derive(Clone, Debug, PartialEq)]
pub struct Attach {
    pub to: String,
    pub part: String,
    pub at: (f64, f64),
    pub pivot: (f64, f64),
    pub turn: bool,
}

/// Two key codes driving one axis: `neg` held is -1, `pos` held is +1, both or neither is 0.
#[derive(Clone, Debug, PartialEq)]
pub struct Axis {
    pub neg: String,
    pub pos: String,
}

/// Moves its entity at `speed` units a second along the held axes; a diagonal is no faster.
#[derive(Component, Clone, Debug)]
pub struct Controller {
    pub speed: f64,
    pub axis_x: Option<Axis>,
    pub axis_y: Option<Axis>,
}

/// A moment for Lua; per-frame work never makes one.
#[derive(Clone, Debug, PartialEq)]
pub enum WorldEvent {
    /// An entity's held direction changed: `dx`, `dy` are each -1, 0 or 1, and both 0 is stopped.
    Move { id: String, dx: i8, dy: i8 },
    /// A fresh press of the key an action names; a held key's repeats are not presses.
    Action(String),
    /// A once clip on this entity reached its end and now holds; reported once per play.
    ClipEnd(String),
    /// Something solid or moving came into this entity's sensor: `who` is its id. Lua gets both
    /// this and `Exit` as `on_zone`, with a phase.
    Enter { id: String, who: String },
    /// It left the sensor — or one of the two stopped being there, carried or despawned.
    Exit { id: String, who: String },
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

/// What the entity shows, plus its drawing's rest pose — posing is the cost, so reconcile carries
/// an unchanged drawing's over.
#[derive(Component)]
struct Appearance {
    drawing: Arc<Drawing>,
    flip: bool,
    rest: Arc<Frame>,
}

/// The entity's body in `Physics`, rebuilt only when its collider or kind of body changes.
#[derive(Component)]
struct Solid {
    collider: Option<Collider>,
    sensor: Option<Collider>,
    moves: bool,
    body: rapier2d::prelude::RigidBodyHandle,
    /// Let go and still sliding: Rapier moves it until it settles.
    sliding: bool,
}

/// How fast a controlled entity moved on its last tick, so what it lets go of keeps that speed.
#[derive(Component, Default)]
struct Velocity(f64, f64);

#[derive(Component)]
struct Attached {
    to: Entity,
    part: String,
    at: Point,
    pivot: Point,
    /// With `turn`: the whole placement, as `follow` last composed it.
    turned: Option<Affine>,
}

/// The held direction `tick` last reported, so it reports only a change.
#[derive(Component, Default, PartialEq)]
struct Heading(i8, i8);

#[derive(Component)]
struct Animator {
    clip: Arc<Clip>,
    started: f64,
    ended: bool,
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
    physics: Physics,
    /// `(zone, who)` sensor overlaps as of the last tick, to report only changes.
    inside: HashSet<(Entity, Entity)>,
    /// `(entity id, part)` clip tracks skipped because the drawing lacks the part — noted once,
    /// and again only if the part comes back and goes missing anew.
    skipped: HashSet<(String, String)>,
    notes: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum WorldError {
    #[error("duplicate entity id {0:?}")]
    DuplicateId(String),
    #[error("entity id must not be empty")]
    EmptyId,
    #[error("entity {0:?}: controller speed must be a finite number, zero or more")]
    Speed(String),
    #[error("entity {0:?}: {1}")]
    Attach(String, String),
    #[error("entity {0:?}: {1}")]
    Collider(String, String),
    #[error(transparent)]
    Drawing(#[from] DrawingError),
    #[error(transparent)]
    Frame(#[from] FrameError),
}

impl World2d {
    /// All-or-nothing: every check and every pose runs before the world is touched, so a bad
    /// description leaves the last good world in place.
    pub fn reconcile(&mut self, specs: Vec<EntitySpec>) -> Result<(), WorldError> {
        // A clip track for a part the drawing lacks is skipped, not refused: a drawing edited
        // live must not stop the world. Said once, after the description is accepted.
        let specs_missing: Vec<(String, String)> = (specs.iter())
            .filter_map(|s| Some((s, s.clip.as_ref()?)))
            .flat_map(|(s, clip)| {
                let missing = clip.parts().filter(|p| !s.drawing.has_part(p));
                missing.map(|p| (s.id.clone(), p.to_string()))
            })
            .collect();
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
            for c in spec.collider.iter().chain(&spec.sensor) {
                c.check()
                    .map_err(|why| WorldError::Collider(spec.id.clone(), why))?;
            }
            let current = self
                .by_id
                .get(&spec.id)
                .and_then(|&e| self.ecs.get::<Appearance>(e));
            let rest = match current {
                Some(a) if Arc::ptr_eq(&a.drawing, &spec.drawing) => a.rest.clone(),
                _ => Arc::new(spec.drawing.pose(&HashMap::new())?),
            };
            all_rests.push(rest);
        }
        for spec in &specs {
            let Some(a) = &spec.attach else { continue };
            let bad = |why: String| Err(WorldError::Attach(spec.id.clone(), why));
            let Some(carrier) = specs.iter().find(|s| s.id == a.to) else {
                return bad(format!("attached to {:?}, which the world does not describe", a.to));
            };
            if carrier.id == spec.id {
                return bad("attached to itself".into());
            }
            if carrier.attach.is_some() {
                return bad(format!("attached to {:?}, which is itself attached", a.to));
            }
            if !carrier.drawing.has_part(&a.part) {
                return bad(format!("{:?} has no part {:?}", a.to, a.part));
            }
            if a.turn && spec.flip {
                return bad("turns with its carrier, which mirrors it too: drop flip".into());
            }
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
        let mut carried = Vec::new();
        for (spec, rest) in specs.into_iter().zip(all_rests) {
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
                    self.ecs.spawn((transform, name)).id()
                }
            };
            // Let go this frame: it leaves with its carrier's velocity.
            let thrown = self.ecs.get::<Attached>(entity).map(|a| {
                let v = self.ecs.get::<Velocity>(a.to);
                v.map_or((0.0, 0.0), |v| (v.0, v.1))
            });
            let mut e = self.ecs.entity_mut(entity);
            let moves = spec.controller.is_some();
            // Carried is off the floor: no body until it is put down, then one where it was let go.
            let aloft = spec.attach.is_some();
            let collider = spec.collider.filter(|_| !aloft);
            let sensor = spec.sensor.filter(|_| !aloft);
            let kept = e.get::<Solid>().is_some_and(|s| {
                s.collider == collider && s.sensor == sensor && s.moves == moves
            });
            if !kept {
                if let Some(old) = e.take::<Solid>() {
                    self.physics.remove(old.body);
                }
                if collider.is_some() || sensor.is_some() {
                    let t = e.get::<Transform>().expect("every entity has a Transform");
                    let kind = match (moves, thrown, &collider) {
                        (true, _, _) => Body::Moved,
                        (false, Some((vx, vy)), Some(_)) => Body::Thrown(vx, vy),
                        _ => Body::Fixed,
                    };
                    let sliding = matches!(kind, Body::Thrown(..));
                    let (solid, zone) = (collider.as_ref(), sensor.as_ref());
                    let owner = entity.to_bits();
                    let body = self.physics.add(solid, zone, (t.x, t.y), kind, owner);
                    e.insert(Solid {
                        collider,
                        sensor,
                        moves,
                        body,
                        sliding,
                    });
                }
            }
            e.insert(Appearance {
                drawing: spec.drawing,
                flip: spec.flip,
                rest,
            });
            match spec.controller {
                Some(controller) => e.insert(controller),
                None => e.remove::<(Controller, Heading)>(),
            };
            e.remove::<Attached>();
            if let Some(a) = spec.attach {
                carried.push((entity, a));
            }
            self.animate(entity, spec.clip);
            next.insert(spec.id, entity);
            order.push(entity);
        }
        for (entity, a) in carried {
            let (to, at) = (next[&a.to], Point::new(a.at.0, a.at.1));
            let pivot = Point::new(a.pivot.0, a.pivot.1);
            let turned = a.turn.then_some(Affine::IDENTITY);
            let attached = Attached { to, part: a.part, at, pivot, turned };
            self.ecs.entity_mut(entity).insert(attached);
        }
        for (_, gone) in self.by_id.drain() {
            if let Some(s) = self.ecs.get::<Solid>(gone) {
                self.physics.remove(s.body);
            }
            self.ecs.despawn(gone);
        }
        (self.by_id, self.order) = (next, order);
        self.note_skipped(&specs_missing);
        self.follow();
        Ok(())
    }

    /// Let-go entities go where Rapier slid them, until they settle.
    fn slide_thrown(&mut self) {
        for &entity in &self.order {
            let Some(s) = self.ecs.get::<Solid>(entity).filter(|s| s.sliding) else {
                continue;
            };
            let (cx, cy) = s.collider.as_ref().expect("only a solid slides").centre();
            let ((x, y), body) = (self.physics.centre_of(s.body), s.body);
            let mut t = self.ecs.get_mut::<Transform>(entity).expect("every entity has one");
            (t.x, t.y) = (x - cx, y - cy);
            if self.physics.settle(body) {
                self.ecs.get_mut::<Solid>(entity).expect("checked above").sliding = false;
            }
        }
    }

    /// Reports what came into or left a sensor since the last tick. A pair whose entity is gone
    /// goes quietly: Lua removed it, so Lua knows.
    fn sense(&mut self) {
        let now: HashSet<(Entity, Entity)> = (self.physics.overlaps().into_iter())
            .map(|(zone, who)| (Entity::from_bits(zone), Entity::from_bits(who)))
            .collect();
        let name = |e: Entity| self.ecs.get::<Name>(e).map(|n| n.0.clone());
        let mut changes: Vec<_> = (now.difference(&self.inside).map(|&p| (p, true)))
            .chain(self.inside.difference(&now).map(|&p| (p, false)))
            .filter_map(|((zone, who), entered)| Some((name(zone)?, name(who)?, entered)))
            .collect();
        changes.sort(); // a set has no order; Lua should see the same one every run
        for (id, who, entered) in changes {
            self.events.push(match entered {
                true => WorldEvent::Enter { id, who },
                false => WorldEvent::Exit { id, who },
            });
        }
        self.inside = now;
    }

    fn note_skipped(&mut self, missing: &[(String, String)]) {
        let now: HashSet<_> = missing.iter().cloned().collect();
        for (id, part) in missing.iter().filter(|k| !self.skipped.contains(*k)) {
            let why = format!("its clip moves {part:?}, which its drawing lacks — skipped");
            self.notes.push(format!("entity {id:?}: {why}"));
        }
        self.skipped = now;
    }

    /// Notes for the app's console since the last drain: not errors, the world kept going.
    pub fn drain_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    /// Carried entities take their place from their carrier's part, as posed now.
    fn follow(&mut self) {
        for &entity in &self.order {
            let p = {
                let Some(a) = self.ecs.get::<Attached>(entity) else {
                    continue;
                };
                let e = self.ecs.entity(a.to);
                let look = e.get::<Appearance>().expect("every entity has an Appearance");
                let poses = match e.get::<Animator>() {
                    Some(anim) => present(anim.clip.sample(self.clock - anim.started), look),
                    None => HashMap::new(),
                };
                let part = look.drawing.part_at(&a.part, &poses).expect("checked at reconcile");
                let p = self.place(a.to) * part * a.at;
                let own = self.ecs.get::<Appearance>(entity).expect("every entity has one");
                let t = self.ecs.get::<Transform>(entity).expect("every entity has one");
                // Turned, the pivot is the centre the part's rotation and scale act about.
                let turned = a.turned.map(|_| {
                    let to_pivot = Affine::translate(a.at.to_vec2()) * Affine::scale(t.scale);
                    self.place(a.to) * part * to_pivot * Affine::translate(-a.pivot.to_vec2())
                });
                // Upright under a flipped carrier, the entity is mirrored too (its own `flip`),
                // so its pivot sits that far from its box's right edge instead.
                let w = own.drawing.size().0;
                let px = if look.flip { w - a.pivot.x } else { a.pivot.x };
                match turned {
                    Some(placed) => (placed * Point::ZERO, Some(placed)),
                    None => (Point::new(p.x - px * t.scale, p.y - a.pivot.y * t.scale), None),
                }
            };
            let (p, turned) = p;
            let mut t = self.ecs.get_mut::<Transform>(entity).expect("every entity has one");
            (t.x, t.y) = (p.x, p.y);
            if turned.is_some() {
                self.ecs.get_mut::<Attached>(entity).expect("just read").turned = turned;
            }
        }
    }

    /// Where an entity's drawing box goes: its transform, mirrored within the box if flipped.
    fn place(&self, entity: Entity) -> Affine {
        let e = self.ecs.entity(entity);
        if let Some(placed) = e.get::<Attached>().and_then(|a| a.turned) {
            return placed;
        }
        let t = e.get::<Transform>().expect("every entity has a Transform");
        let look = e.get::<Appearance>().expect("every entity has an Appearance");
        match look.flip {
            true => t.affine() * Affine::new([-1.0, 0.0, 0.0, 1.0, look.drawing.size().0, 0.0]),
            false => t.affine(),
        }
    }

    /// The bottom of the entity's box; a carried entity stands on its carrier's feet, just in
    /// front of it.
    fn feet(&self, entity: Entity) -> (f64, u8) {
        let e = self.ecs.entity(entity);
        if let Some(a) = e.get::<Attached>() {
            return (self.feet(a.to).0, 1);
        }
        let t = e.get::<Transform>().expect("every entity has a Transform");
        let look = e.get::<Appearance>().expect("every entity has an Appearance");
        (t.y + look.drawing.size().1 * t.scale, 0)
    }

    /// Each entity is an instance named by its id, so a hit on the world reports which entity.
    pub fn frame(&self, width: f64, height: f64) -> Result<Frame, WorldError> {
        let mut items = self
            .order
            .iter()
            .map(|&entity| {
                let e = self.ecs.entity(entity);
                let (Some(name), Some(look)) = (e.get::<Name>(), e.get::<Appearance>()) else {
                    unreachable!("reconcile gives every entity both");
                };
                let posed = match e.get::<Animator>() {
                    Some(a) => {
                        let poses = present(a.clip.sample(self.clock - a.started), look);
                        Arc::new(look.drawing.pose(&poses)?)
                    }
                    None => look.rest.clone(),
                };
                let item = Item::instance(self.place(entity), posed)?;
                Ok((self.feet(entity), item.with_id(name.0.as_str())))
            })
            .collect::<Result<Vec<_>, WorldError>>()?;
        if self.stacking == Order::Feet {
            // Stable, so ties keep list order.
            items.sort_by(|(a, _), (b, _)| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        }
        let items = items.into_iter().map(|(_, item)| item).collect();
        // Clipped: a head or hand spilling past the room's edge is not drawn outside it.
        Ok(Frame::new(width, height, None, items)?.clipped())
    }

    /// One frame: `elapsed` is the runtime's frame clock, which clips play against; `dt` is the
    /// step controllers move by (the runtime caps it after a stall).
    pub fn tick(&mut self, elapsed: f64, dt: f64) {
        self.clock = elapsed;
        self.physics.step(dt);
        self.slide_thrown();
        self.sense();
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
                // A solid entity goes only as far as the others let it.
                let wanted = (dx / length * step, dy / length * step);
                let (mx, my) = match e.get::<Solid>() {
                    Some(s) if s.collider.is_some() => self.physics.slide(s.body, wanted, dt),
                    Some(s) => {
                        self.physics.shift(s.body, wanted);
                        wanted
                    }
                    None => wanted,
                };
                let mut t = e
                    .get_mut::<Transform>()
                    .expect("every entity has a Transform");
                (t.x, t.y) = (t.x + mx, t.y + my);
                e.insert(match dt > 0.0 {
                    true => Velocity(mx / dt, my / dt),
                    false => Velocity::default(),
                });
            } else {
                e.insert(Velocity::default());
            }
        }
        self.follow();
        for &entity in &self.order {
            let e = self.ecs.entity(entity);
            let ended = e
                .get::<Animator>()
                .is_some_and(|a| !a.ended && a.clip.done(elapsed - a.started));
            if ended {
                let id = e.get::<Name>().expect("every entity has a Name").0.clone();
                self.ecs.get_mut::<Animator>(entity).expect("checked above").ended = true;
                self.events.push(WorldEvent::ClipEnd(id));
            }
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
        let sliding = |&e: &Entity| self.ecs.get::<Solid>(e).is_some_and(|s| s.sliding);
        self.ticking || self.order.iter().any(sliding)
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

    /// Switching clip handles restarts from the current clock; the same handle continues.
    fn animate(&mut self, entity: Entity, clip: Option<Arc<Clip>>) {
        let mut e = self.ecs.entity_mut(entity);
        match clip {
            None => {
                e.remove::<Animator>();
            }
            Some(clip) if !e.get::<Animator>().is_some_and(|a| Arc::ptr_eq(&a.clip, &clip)) => {
                e.insert(Animator {
                    clip,
                    started: self.clock,
                    ended: false,
                });
            }
            Some(_) => {}
        }
    }

    pub fn transform(&self, id: &str) -> Option<Transform> {
        self.ecs.get::<Transform>(*self.by_id.get(id)?).copied()
    }
}

/// A clip's poses for only the parts this drawing has; the rest were noted at reconcile.
fn present<'a>(mut poses: HashMap<&'a str, Pose>, look: &Appearance) -> HashMap<&'a str, Pose> {
    poses.retain(|part, _| look.drawing.has_part(part));
    poses
}

#[cfg(test)]
mod tests;
