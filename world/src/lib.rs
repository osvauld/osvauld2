//! A retained 2D world: entities kept by author id across frames in a `bevy_ecs` World. Lua
//! describes; `reconcile` spawns, updates and despawns to match. `pos` is read only at spawn —
//! once an entity exists its placement belongs to the world. List order is draw order unless the
//! world sorts by feet (`Order::Feet`). Nothing
//! outside this crate sees ECS types. Held keys and motion live here too: Lua describes a
//! controller once, and `tick` moves the entity every frame without Lua. Moments — a press, a
//! change of direction — queue as `WorldEvent`s for Lua to decide on. Private 3D collider/solver
//! groundwork and retained 3D body IDs live here too; their host binding is not exposed yet.

pub mod clip;
mod physics;
mod physics3d;
pub mod world3d;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy_ecs::prelude::{Component, Entity, World};
use runtime::drawing::{Drawing, DrawingError, Pose};
use runtime::frame::{Frame, FrameError, Item};
use runtime::vello::kurbo::{Affine, Point};

use crate::clip::Clip;
use crate::physics::{Body, Physics};
pub use crate::physics::{Collider, Material, Shape};

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
    /// Moved by physics alone — pushed, bounced, slowed by the floor — by its collider's shape.
    /// It rests asleep, never fixed, so it can be pushed again.
    pub loose: Option<Material>,
    /// The collision group its collider is in, by name; without one, the common group.
    pub group: Option<String>,
    /// The groups its collider stops, by name — a line only paddles meet; without, it stops all.
    pub blocks: Option<Vec<String>>,
}

/// A change Lua makes at a moment, not by describing: an existing entity's description never
/// moves it. Unset fields are kept.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Set {
    /// The drawing box's top-left, as the description's `pos`.
    pub pos: Option<(f64, f64)>,
    /// Per second; only a loose thing's, since Rapier moves only those.
    pub velocity: Option<(f64, f64)>,
    /// Degrees a second; only a loose thing with grip turns.
    pub spin: Option<f64>,
}

/// Bit 0 is the common group, everything not named into another.
const COMMON: u32 = 1;

/// Units a second two things must meet at to be a hit: slower is settling, not striking.
const HIT: f64 = 1.0;

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

/// One entity as the world holds it now, for an agent or a test to read instead of probing pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityInspection {
    pub id: String,
    /// The drawing box's top-left, in world units.
    pub pos: (f64, f64),
    /// `"fixed"`, `"moved"` (a controller drives it), `"thrown"` (Rapier slides it until it
    /// settles), `"loose"` (Rapier moves it, always) or `"none"` (no collider or sensor, or
    /// carried).
    pub body: &'static str,
    /// Per second: Rapier's for a thrown body, the last tick's step for a controlled one.
    pub velocity: (f64, f64),
    /// Degrees turned, about the solid shape's centre, and degrees a second: only a loose thing
    /// with grip turns.
    pub rot: f64,
    pub spin: f64,
    /// `(carrier, part)`.
    pub attached: Option<(String, String)>,
    /// Ids of the zones it is inside, sorted.
    pub zones: Vec<String>,
    pub clip: Option<ClipInspection>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TimerInspection {
    pub name: String,
    /// Seconds until it fires.
    pub left: f64,
}

/// A world as an agent reads it: its entities and the timers still to fire.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldInspection {
    pub entities: Vec<EntityInspection>,
    pub timers: Vec<TimerInspection>,
    /// Steps taken: the frame the dump and every question's answer are as of.
    pub tick: u64,
    /// Seconds stalls have cost: frame time past `MAX_STEPS` steps, never run.
    pub dropped: f64,
}

/// What a ray met first.
#[derive(Clone, Debug, PartialEq)]
pub struct RayHit {
    pub id: String,
    /// Where the line met its collider, in world units.
    pub at: (f64, f64),
    /// The surface's direction there, a unit vector pointing out of it.
    pub normal: (f64, f64),
    /// From the ray's start to `at`.
    pub dist: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipInspection {
    /// Seconds since this clip started playing on the entity.
    pub time: f64,
    pub length: f64,
    pub looped: bool,
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
    /// A timer Lua set with `after` came due.
    Timer(String),
    /// Something Rapier moves (loose or thrown) came into contact with `who`, closing at `speed`
    /// units a second along the contact normal. Each of two such things gets its own.
    Hit { id: String, who: String, speed: f64 },
}

/// The world moves in steps of this, whatever the frame rate: as the 3D world does.
pub const STEP: f64 = 1.0 / 120.0;
/// The most steps one frame runs; more time than this, after a stall, is dropped.
pub const MAX_STEPS: u32 = 8;

/// A moment Lua asked for. Counted from the world's next frame: a handler has no clock of its
/// own, and the world's is its last frame's — stale if it was idle.
struct Timer {
    name: String,
    secs: f64,
    /// On the frame clock, once that next frame has come.
    due: Option<f64>,
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
    /// The point in the drawing it turns about: a turning body's solid shape's centre.
    pub pivot: (f64, f64),
}

impl Transform {
    fn affine(&self) -> Affine {
        Affine::translate((self.x + self.pivot.0, self.y + self.pivot.1))
            * Affine::rotate(self.rot.to_radians())
            * Affine::translate((-self.pivot.0, -self.pivot.1))
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
    /// Moving under Rapier: let go and not yet settled, or loose and awake.
    sliding: bool,
    loose: Option<Material>,
    /// `(memberships, filter)` as Rapier bits.
    groups: (u32, u32),
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

/// A part of a world's description, in order: entities described this time, or a group's —
/// `None` when it is kept exactly as it was, because nothing it read has changed.
pub enum Part {
    Specs(Vec<EntitySpec>),
    Group(String, Option<Vec<EntitySpec>>),
}

/// What is kept of each part's entities, so a kept group needs none of its specs again. Plain
/// specs, wherever they stand in the description, are the `None` block.
#[derive(Default)]
struct Block {
    entities: Vec<Entity>,
    controlled: bool,
    ticking: bool,
    /// `(entity id, part)` clip tracks its drawings lack.
    missing: Vec<(String, String)>,
    /// `(rider id, carrier id)`: a kept rider still needs its carrier.
    rides: Vec<(String, String)>,
}

/// Which part an entity was described in.
#[derive(Component)]
struct InBlock(Option<String>);

#[derive(Default)]
pub struct World2d {
    ecs: World,
    clock: f64,
    /// Fixed steps taken since the world began.
    steps: u64,
    /// Frame time not yet a whole step, run on a later frame.
    carry: f64,
    /// Frame time beyond `MAX_STEPS` steps, after a stall: dropped, never replayed.
    dropped: f64,
    ticking: bool,
    controlled: bool,
    held: HashSet<String>,
    /// `(action, key code)`, as the world's `actions` name them.
    actions: Vec<(String, String)>,
    events: Vec<WorldEvent>,
    by_id: HashMap<String, Entity>,
    order: Vec<Entity>,
    blocks: HashMap<Option<String>, Block>,
    stacking: Order,
    physics: Physics,
    /// `(zone, who)` sensor overlaps as of the last tick, to report only changes.
    inside: HashSet<(Entity, Entity)>,
    /// Solid pairs in contact as of the last tick, to report only new ones.
    touching: HashSet<(Entity, Entity)>,
    /// `(entity id, part)` clip tracks skipped because the drawing lacks the part — noted once,
    /// and again only if the part comes back and goes missing anew.
    skipped: HashSet<(String, String)>,
    notes: Vec<String>,
    /// Collision group names in the order first seen: name `i` is bit `i + 1`. Only grows, so a
    /// name keeps its bit for the world's life.
    groups: Vec<String>,
    timers: Vec<Timer>,
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
    #[error("entity {0:?}: {1}")]
    Set(String, String),
    #[error("timer {0:?}: seconds must be a finite number, zero or more")]
    Timer(String),
    /// A group described wrongly.
    #[error("{0}")]
    Group(String),
    /// A question asked wrongly, as `"ray: why"`.
    #[error("{0}")]
    Ask(String),
    #[error(transparent)]
    Drawing(#[from] DrawingError),
    #[error(transparent)]
    Frame(#[from] FrameError),
}

impl World2d {
    /// The `(memberships, filter)` bits of `spec`'s collider, naming any new group.
    fn groups_of(&mut self, spec: &EntitySpec) -> Result<(u32, u32), String> {
        if (spec.group.is_some() || spec.blocks.is_some()) && spec.collider.is_none() {
            return Err("group and blocks need a collider: they say what it meets".into());
        }
        if spec.blocks.as_ref().is_some_and(|b| b.is_empty()) {
            return Err("blocks is empty: a collider that stops nothing is no collider".into());
        }
        let mut bit = |name: &String| {
            let i = match self.groups.iter().position(|g| g == name) {
                Some(i) => i,
                None if self.groups.len() < 31 => {
                    self.groups.push(name.clone());
                    self.groups.len() - 1
                }
                None => return Err(format!("too many collision groups: {name:?} would be the 32nd")),
            };
            Ok(2u32 << i)
        };
        let member = spec.group.as_ref().map_or(Ok(COMMON), &mut bit)?;
        let filter = match &spec.blocks {
            Some(names) => names.iter().map(&mut bit).try_fold(0, |all, b| Ok::<_, String>(all | b?))?,
            None => u32::MAX,
        };
        Ok((member, filter))
    }

    /// The whole description as one part: what an app without groups gives.
    pub fn reconcile(&mut self, specs: Vec<EntitySpec>) -> Result<(), WorldError> {
        self.reconcile_parts(vec![Part::Specs(specs)])
    }

    /// All-or-nothing: every check and every pose runs before the world is touched, so a bad
    /// description leaves the last good world in place. A kept group's entities are not looked
    /// at again: not checked, posed or diffed — that is what keeping it saves.
    pub fn reconcile_parts(&mut self, parts: Vec<Part>) -> Result<(), WorldError> {
        let parts: Vec<(Option<String>, Option<Vec<EntitySpec>>)> = (parts.into_iter())
            .map(|p| match p {
                Part::Specs(specs) => (None, Some(specs)),
                Part::Group(id, specs) => (Some(id), specs),
            })
            .collect();
        let mut named = HashSet::new();
        let mut kept = HashSet::new();
        for (key, specs) in &parts {
            let Some(id) = key else { continue };
            if !named.insert(id.as_str()) {
                return Err(WorldError::Group(format!("two groups share the id {id:?}")));
            }
            if specs.is_none() {
                if !self.blocks.contains_key(key) {
                    return Err(WorldError::Group(format!(
                        "group {id:?} is kept but was never described"
                    )));
                }
                kept.insert(id.clone());
            }
        }
        let specs: Vec<&EntitySpec> = (parts.iter()).filter_map(|(_, s)| s.as_ref()).flatten().collect();
        // A clip track for a part the drawing lacks is skipped, not refused: a drawing edited
        // live must not stop the world. Said once, after the description is accepted.
        let mut seen = HashSet::with_capacity(specs.len());
        let mut all_rests = Vec::with_capacity(specs.len());
        for spec in &specs {
            if spec.id.is_empty() {
                return Err(WorldError::EmptyId);
            }
            let in_kept = self.by_id.get(&spec.id).is_some_and(|&e| self.kept(e, &kept));
            if !seen.insert(spec.id.as_str()) || in_kept {
                return Err(WorldError::DuplicateId(spec.id.clone()));
            }
            if let Some(c) = &spec.controller
                && !(c.speed.is_finite() && c.speed >= 0.0)
            {
                return Err(WorldError::Speed(spec.id.clone()));
            }
            if let Some(m) = &spec.loose {
                let bad = |why: String| Err(WorldError::Collider(spec.id.clone(), why));
                if spec.collider.is_none() {
                    return bad("loose needs a collider: physics moves it by its shape".into());
                }
                if spec.controller.is_some() {
                    return bad("loose with a controller: one thing moves it, not two".into());
                }
                m.check().or_else(bad)?;
            }
            for c in spec.collider.iter().chain(&spec.sensor) {
                c.check()
                    .map_err(|why| WorldError::Collider(spec.id.clone(), why))?;
            }
            self.groups_of(spec)
                .map_err(|why| WorldError::Collider(spec.id.clone(), why))?;
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
        // A carrier described this time, or kept in a group.
        let kept_carrier = |id: &String| {
            let e = *self.by_id.get(id).filter(|&&e| self.kept(e, &kept))?;
            let look = self.ecs.get::<Appearance>(e).expect("every entity has one");
            Some((look.drawing.clone(), self.ecs.get::<Attached>(e).is_some()))
        };
        for spec in &specs {
            let Some(a) = &spec.attach else { continue };
            let bad = |why: String| Err(WorldError::Attach(spec.id.clone(), why));
            let (drawing, riding) = match specs.iter().find(|s| s.id == a.to) {
                Some(carrier) if carrier.id == spec.id => return bad("attached to itself".into()),
                Some(carrier) => (carrier.drawing.clone(), carrier.attach.is_some()),
                None => match kept_carrier(&a.to) {
                    Some(found) => found,
                    None => {
                        return bad(format!("attached to {:?}, which the world does not describe", a.to));
                    }
                },
            };
            if riding {
                return bad(format!("attached to {:?}, which is itself attached", a.to));
            }
            if !drawing.has_part(&a.part) {
                return bad(format!("{:?} has no part {:?}", a.to, a.part));
            }
            if a.turn && spec.flip {
                return bad("turns with its carrier, which mirrors it too: drop flip".into());
            }
        }
        // A kept rider still needs its carrier, wherever that is described now.
        for name in &kept {
            for (rider, carrier) in &self.blocks[&Some(name.clone())].rides {
                let bad = |why: String| Err(WorldError::Attach(rider.clone(), why));
                match specs.iter().find(|s| &s.id == carrier) {
                    Some(c) if c.attach.is_some() => {
                        return bad(format!("rides {carrier:?}, which is itself attached now"));
                    }
                    Some(_) => {}
                    None if kept_carrier(carrier).is_some() => {}
                    None => {
                        return bad(format!("rides {carrier:?}, which the world no longer describes"));
                    }
                }
            }
        }
        let fresh: HashSet<String> = specs.iter().map(|s| s.id.clone()).collect();
        drop(specs);

        let mut old = std::mem::take(&mut self.blocks);
        let mut blocks: HashMap<Option<String>, Block> = (kept.iter())
            .map(|name| (Some(name.clone()), old.remove(&Some(name.clone())).expect("checked above")))
            .collect();
        let mut rests = all_rests.into_iter();
        let mut order = Vec::with_capacity(self.order.len());
        let mut carried = Vec::new();
        for (key, specs) in parts {
            let Some(specs) = specs else {
                order.extend(blocks[&key].entities.iter().copied());
                continue;
            };
            let mut block = blocks.remove(&key).unwrap_or_default();
            for spec in specs {
                block.controlled |= spec.controller.is_some();
                block.ticking |= spec.clip.is_some() || spec.controller.is_some();
                if let Some(clip) = &spec.clip {
                    let missing = clip.parts().filter(|p| !spec.drawing.has_part(p));
                    block.missing.extend(missing.map(|p| (spec.id.clone(), p.to_string())));
                }
                if let Some(a) = &spec.attach {
                    block.rides.push((spec.id.clone(), a.to.clone()));
                }
                let rest = rests.next().expect("one rest per spec");
                let entity = self.put(spec, rest, &key, &mut carried);
                block.entities.push(entity);
                order.push(entity);
            }
            blocks.insert(key, block);
        }
        // What was described before and is not now goes: a whole group no longer described, or
        // an entity left out of a part described afresh.
        for gone in old.into_values().flat_map(|b| b.entities) {
            let name = self.ecs.get::<Name>(gone).expect("every entity has a Name").0.clone();
            if fresh.contains(&name) {
                continue; // described again, in another part
            }
            if let Some(s) = self.ecs.get::<Solid>(gone) {
                self.physics.remove(s.body);
            }
            self.by_id.remove(&name);
            self.ecs.despawn(gone);
        }
        for (entity, a) in carried {
            let (to, at) = (self.by_id[&a.to], Point::new(a.at.0, a.at.1));
            let pivot = Point::new(a.pivot.0, a.pivot.1);
            let turned = a.turn.then_some(Affine::IDENTITY);
            let attached = Attached { to, part: a.part, at, pivot, turned };
            self.ecs.entity_mut(entity).insert(attached);
        }
        self.controlled = blocks.values().any(|b| b.controlled);
        self.ticking = blocks.values().any(|b| b.ticking);
        if !self.wants_keys() {
            self.held.clear(); // key events stop arriving, so nothing can release them later
        }
        let mut missing: Vec<_> = blocks.values().flat_map(|b| b.missing.iter().cloned()).collect();
        missing.sort();
        (self.blocks, self.order) = (blocks, order);
        self.note_skipped(&missing);
        self.follow();
        Ok(())
    }

    /// Whether a group of this id was described and is still here to be kept.
    pub fn has_group(&self, id: &str) -> bool {
        self.blocks.contains_key(&Some(id.to_owned()))
    }

    /// Whether `entity` belongs to one of the `kept` groups.
    fn kept(&self, entity: Entity, kept: &HashSet<String>) -> bool {
        let block = self.ecs.get::<InBlock>(entity).and_then(|b| b.0.as_ref());
        block.is_some_and(|b| kept.contains(b))
    }

    /// Spawns `spec`'s entity, or brings the one of that id up to date: its body, look,
    /// controller and clip. Its carrier is resolved once every part is placed.
    fn put(
        &mut self,
        spec: EntitySpec,
        rest: Arc<Frame>,
        block: &Option<String>,
        carried: &mut Vec<(Entity, Attach)>,
    ) -> Entity {
        let entity = match self.by_id.get(&spec.id) {
            Some(&entity) => entity,
            None => {
                let (x, y) = spec.pos;
                let transform = Transform {
                    x,
                    y,
                    rot: 0.0,
                    scale: 1.0,
                    pivot: (0.0, 0.0),
                };
                let name = Name(spec.id.clone());
                self.ecs.spawn((transform, name)).id()
            }
        };
        // Let go this frame: it leaves with its carrier's velocity, from its carrier's body.
        let thrown = self.ecs.get::<Attached>(entity).map(|a| {
            let v = self.ecs.get::<Velocity>(a.to);
            v.map_or((0.0, 0.0), |v| (v.0, v.1))
        });
        let carrier = (self.ecs.get::<Attached>(entity))
            .and_then(|a| self.ecs.get::<Solid>(a.to))
            .map(|s| s.body);
        let groups = self.groups_of(&spec).expect("checked above");
        let mut e = self.ecs.entity_mut(entity);
        let moves = spec.controller.is_some();
        // Carried is off the floor: no body until it is put down, then one where it was let go.
        let aloft = spec.attach.is_some();
        let collider = spec.collider.filter(|_| !aloft);
        let sensor = spec.sensor.filter(|_| !aloft);
        let loose = spec.loose;
        let kept = e.get::<Solid>().is_some_and(|s| {
            (s.collider == collider && s.sensor == sensor && s.moves == moves)
                && (s.loose == loose && s.groups == groups)
        });
        if !kept {
            if let Some(old) = e.take::<Solid>() {
                self.physics.remove(old.body);
                // A new body starts upright.
                e.get_mut::<Transform>().expect("every entity has one").rot = 0.0;
            }
            if collider.is_some() || sensor.is_some() {
                let t = e.get::<Transform>().expect("every entity has a Transform");
                let kind = match (moves, thrown, &collider, loose) {
                    (true, ..) => Body::Moved,
                    (false, v, Some(_), Some(m)) => Body::Dynamic(v.unwrap_or_default(), m),
                    (false, Some(v), Some(_), None) => Body::Dynamic(v, Material::default()),
                    _ => Body::Fixed,
                };
                let dynamic = matches!(kind, Body::Dynamic(..));
                let dropped = thrown.is_some();
                let (solid, zone) = (collider.as_ref(), sensor.as_ref());
                let owner = entity.to_bits();
                let body = self.physics.add(solid, zone, (t.x, t.y), kind, owner, groups);
                // Loose from the start, it waits asleep; let go, it is on the move.
                if dynamic && !dropped {
                    self.physics.sleep_if_clear(body);
                }
                if let (true, Some(from), Some(c)) = (dynamic, carrier, solid) {
                    self.physics.bring_in(body, from);
                    let ((x, y), (cx, cy)) = (self.physics.centre_of(body), c.centre());
                    let mut t = e.get_mut::<Transform>().expect("every entity has one");
                    (t.x, t.y) = (x - cx, y - cy);
                }
                e.insert(Solid {
                    collider,
                    sensor,
                    moves,
                    body,
                    sliding: dynamic && !self.physics.asleep(body),
                    loose,
                    groups,
                });
            }
        }
        e.insert(Appearance {
            drawing: spec.drawing,
            flip: spec.flip,
            rest,
        });
        e.insert(InBlock(block.clone()));
        match spec.controller {
            Some(controller) => e.insert(controller),
            None => e.remove::<(Controller, Heading)>(),
        };
        e.remove::<Attached>();
        if let Some(a) = spec.attach {
            carried.push((entity, a));
        }
        self.animate(entity, spec.clip);
        self.by_id.insert(spec.id, entity);
        entity
    }

    /// Let-go entities go where Rapier slid them, until they settle.
    fn slide_thrown(&mut self) {
        for &entity in &self.order {
            let Some(s) = self.ecs.get::<Solid>(entity) else {
                continue;
            };
            let (body, loose) = (s.body, s.loose.is_some());
            // A loose body rests asleep and wakes when something touches it; only a moving one
            // needs following.
            if !s.sliding && !(loose && !self.physics.asleep(body)) {
                continue;
            }
            let (cx, cy) = s.collider.as_ref().expect("only a solid slides").centre();
            let (x, y) = self.physics.centre_of(body);
            let (rot, _) = self.physics.turn(body);
            let mut t = self.ecs.get_mut::<Transform>(entity).expect("every entity has one");
            (t.x, t.y, t.rot, t.pivot) = (x - cx, y - cy, rot, (cx, cy));
            // Let go, it turns fixed at rest; loose, it only sleeps, to be pushed again.
            let rest = if loose { self.physics.asleep(body) } else { self.physics.settle(body) };
            self.ecs.get_mut::<Solid>(entity).expect("checked above").sliding = !rest;
        }
    }

    /// Each solid entity's velocity now: Rapier's, or a controller's last step.
    fn velocities(&self) -> HashMap<Entity, (f64, f64)> {
        let velocity = |&entity: &Entity| {
            let e = self.ecs.entity(entity);
            let s = e.get::<Solid>().filter(|s| s.collider.is_some())?;
            Some((entity, match e.get::<Velocity>() {
                _ if s.sliding || s.loose.is_some() => self.physics.velocity(s.body),
                Some(v) => (v.0, v.1),
                None => (0.0, 0.0),
            }))
        };
        self.order.iter().filter_map(velocity).collect()
    }

    /// Reports the solid pairs that came into contact this tick, at the speed they closed at
    /// before it.
    fn hits(&mut self, before: &HashMap<Entity, (f64, f64)>) {
        let pair = |a: u64, b: u64| (Entity::from_bits(a.min(b)), Entity::from_bits(a.max(b)));
        let now: HashMap<_, _> = (self.physics.contacts().into_iter())
            .map(|(a, b, normal)| (pair(a, b), normal))
            .collect();
        let at = |e| before.get(&e).copied().unwrap_or_default();
        let moving = |e| self.ecs.get::<Solid>(e).is_some_and(|s| s.sliding || s.loose.is_some());
        let name = |e: Entity| self.ecs.get::<Name>(e).map(|n| n.0.clone());
        let mut hits = vec![];
        for (&(a, b), &(nx, ny)) in now.iter().filter(|(p, _)| !self.touching.contains(p)) {
            let ((ax, ay), (bx, by)) = (at(a), at(b));
            let speed = ((ax - bx) * nx + (ay - by) * ny).abs();
            for (me, other) in [(a, b), (b, a)].into_iter().filter(|&(me, _)| moving(me)) {
                if let (true, Some(id), Some(who)) = (speed >= HIT, name(me), name(other)) {
                    hits.push((id, who, speed));
                }
            }
        }
        hits.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1))); // a map has no order either
        self.events.extend(hits.into_iter().map(|(id, who, speed)| WorldEvent::Hit { id, who, speed }));
        self.touching = now.into_keys().collect();
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

    /// One frame: `elapsed` is the runtime's frame clock, which clips and timers go by; `dt` the
    /// time since the last frame, run as whole fixed steps. What is short of a step waits for the
    /// next frame; past `MAX_STEPS`, after a stall, the world runs slower rather than jump.
    pub fn tick(&mut self, elapsed: f64, dt: f64) {
        self.clock = elapsed;
        for t in self.timers.iter_mut().filter(|t| t.due.is_none()) {
            t.due = Some(elapsed + t.secs);
        }
        let available = self.carry + dt.max(0.0);
        let window = available.min(STEP * MAX_STEPS as f64);
        let steps = ((window / STEP + 1e-9).floor() as u32).min(MAX_STEPS);
        self.carry = (window - steps as f64 * STEP).max(0.0);
        self.dropped += available - window;
        for _ in 0..steps {
            self.step(STEP);
        }
        // A body taken away leaves its zones even on a frame too short for a step.
        self.sense();
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
        let (mut due, left) = (std::mem::take(&mut self.timers).into_iter())
            .partition::<Vec<_>, _>(|t| t.due.is_some_and(|d| d <= elapsed));
        self.timers = left;
        due.sort_by(|a, b| a.due.partial_cmp(&b.due).expect("finite").then(a.name.cmp(&b.name)));
        self.events.extend(due.into_iter().map(|t| WorldEvent::Timer(t.name)));
    }

    /// One fixed step: bodies, then zones and hits, then walkers.
    fn step(&mut self, dt: f64) {
        let before = self.velocities();
        self.physics.step(dt);
        self.steps += 1;
        self.slide_thrown();
        self.sense();
        self.hits(&before);
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
                e.insert(Velocity(mx / dt, my / dt));
            } else if length == 0.0 {
                e.insert(Velocity::default());
            }
        }
    }

    /// Fires `Timer(name)` once `secs` have passed; the same name again starts it over.
    pub fn after(&mut self, name: &str, secs: f64) -> Result<(), WorldError> {
        if !(secs.is_finite() && secs >= 0.0) {
            return Err(WorldError::Timer(name.to_string()));
        }
        self.cancel(name);
        let name = name.to_string();
        self.timers.push(Timer { name, secs, due: None });
        Ok(())
    }

    /// Drops the timer `name`, if there is one.
    pub fn cancel(&mut self, name: &str) {
        self.timers.retain(|t| t.name != name);
    }

    /// The timers still to fire, by name.
    pub fn timers(&self) -> Vec<TimerInspection> {
        let mut timers: Vec<_> = (self.timers.iter())
            .map(|t| TimerInspection {
                name: t.name.clone(),
                left: t.due.map_or(t.secs, |d| d - self.clock),
            })
            .collect();
        timers.sort_by(|a, b| a.name.cmp(&b.name));
        timers
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
        self.ticking || !self.timers.is_empty() || self.order.iter().any(sliding)
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

    /// Lua's command between ticks: puts an entity at `to.pos` and sets its velocity. A body
    /// moved into something is pushed out awake, like one spawned there.
    pub fn set(&mut self, id: &str, to: Set) -> Result<(), WorldError> {
        let err = |why: &str| WorldError::Set(id.to_string(), why.to_string());
        let entity = *self.by_id.get(id).ok_or_else(|| err("there is no such entity"))?;
        let finite = |p: Option<(f64, f64)>| p.is_none_or(|(x, y)| x.is_finite() && y.is_finite());
        if !finite(to.pos) || !finite(to.velocity) || !to.spin.is_none_or(f64::is_finite) {
            return Err(err("pos, velocity and spin must be finite numbers"));
        }
        let e = self.ecs.entity(entity);
        if e.contains::<Attached>() {
            return Err(err("it is carried: it goes where its carrier puts it"));
        }
        let solid = e.get::<Solid>().map(|s| {
            let anchor = s.collider.as_ref().or(s.sensor.as_ref()).expect("a body has a shape");
            (s.body, anchor.centre(), s.loose.is_some())
        });
        if to.velocity.is_some() && !solid.is_some_and(|(.., loose)| loose) {
            return Err(err("velocity needs a loose thing: Rapier moves only those"));
        }
        let turns = e.get::<Solid>().and_then(|s| s.loose).is_some_and(|m| m.grip.is_some());
        if to.spin.is_some() && !turns {
            return Err(err("spin needs a loose thing with grip: nothing else turns"));
        }
        if let Some((x, y)) = to.pos {
            let mut t = self.ecs.get_mut::<Transform>(entity).expect("every entity has one");
            (t.x, t.y) = (x, y);
        }
        if let Some((body, (cx, cy), loose)) = solid {
            let at = to.pos.map(|(x, y)| (x + cx, y + cy));
            self.physics.set(body, at, to.velocity, to.spin);
            // Awake now, so followed until it rests again.
            self.ecs.get_mut::<Solid>(entity).expect("checked above").sliding |= loose;
        }
        Ok(())
    }

    /// Lua's question: the first solid thing on the line from `from` to `to`, leaving out
    /// `skip` — a looker's own body. As of the last step: a `set` since shows at the next.
    pub fn ray(&self, from: (f64, f64), to: (f64, f64), skip: Option<&str>) -> Result<Option<RayHit>, WorldError> {
        let err = |why: &str| WorldError::Ask(format!("ray: {why}"));
        if ![from.0, from.1, to.0, to.1].iter().all(|v| v.is_finite()) {
            return Err(err("from and to must be finite numbers"));
        }
        if from == to {
            return Err(err("from and to are the same point: a ray needs a direction"));
        }
        let skip = match skip {
            Some(id) => Some(*self.by_id.get(id).ok_or_else(|| err("skip: there is no such entity"))?),
            None => None,
        };
        let skip = skip.and_then(|e| self.ecs.get::<Solid>(e)).map(|s| s.body);
        let name = |owner| self.ecs.get::<Name>(Entity::from_bits(owner)).map(|n| n.0.clone());
        Ok(self.physics.ray(from, to, skip).and_then(|(owner, at, normal, dist)| {
            Some(RayHit { id: name(owner)?, at, normal, dist })
        }))
    }

    /// Lua's question: the ids of everything whose collider or zone covers `point`, sorted.
    pub fn at(&self, point: (f64, f64)) -> Result<Vec<String>, WorldError> {
        if !(point.0.is_finite() && point.1.is_finite()) {
            return Err(WorldError::Ask("at: the point must be finite numbers".into()));
        }
        let name = |owner| self.ecs.get::<Name>(Entity::from_bits(owner)).map(|n| n.0.clone());
        let mut ids: Vec<String> = self.physics.at(point).into_iter().filter_map(name).collect();
        ids.sort();
        ids.dedup(); // a collider and a zone of one entity
        Ok(ids)
    }

    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// Seconds a stall cost: the world ran slower rather than jump.
    pub fn dropped(&self) -> f64 {
        self.dropped
    }

    pub fn transform(&self, id: &str) -> Option<Transform> {
        self.ecs.get::<Transform>(*self.by_id.get(id)?).copied()
    }
}

impl World2d {
    /// Every entity, in description order.
    pub fn inspect(&self) -> Vec<EntityInspection> {
        let name = |entity| self.ecs.get::<Name>(entity).expect("every entity has one").0.clone();
        (self.order.iter())
            .map(|&entity| {
                let e = self.ecs.entity(entity);
                let t = e.get::<Transform>().expect("every entity has a Transform");
                let solid = e.get::<Solid>();
                let body = match solid {
                    None => "none",
                    Some(s) if s.loose.is_some() => "loose",
                    Some(s) if s.sliding => "thrown",
                    Some(s) if s.moves => "moved",
                    Some(_) => "fixed",
                };
                let velocity = match (solid, e.get::<Velocity>()) {
                    (Some(s), _) if s.sliding || s.loose.is_some() => self.physics.velocity(s.body),
                    (_, Some(v)) => (v.0, v.1),
                    _ => (0.0, 0.0),
                };
                let spin = match solid {
                    Some(s) if s.loose.is_some() => self.physics.turn(s.body).1,
                    _ => 0.0,
                };
                let mut zones: Vec<String> = (self.inside.iter())
                    .filter(|(_, who)| *who == entity)
                    .map(|&(zone, _)| name(zone))
                    .collect();
                zones.sort();
                EntityInspection {
                    id: name(entity),
                    pos: (t.x, t.y),
                    body,
                    velocity,
                    rot: t.rot,
                    spin,
                    attached: e.get::<Attached>().map(|a| (name(a.to), a.part.clone())),
                    zones,
                    clip: e.get::<Animator>().map(|a| ClipInspection {
                        time: self.clock - a.started,
                        length: a.clip.length(),
                        looped: a.clip.looped(),
                    }),
                }
            })
            .collect()
    }
}

/// A clip's poses for only the parts this drawing has; the rest were noted at reconcile.
fn present<'a>(mut poses: HashMap<&'a str, Pose>, look: &Appearance) -> HashMap<&'a str, Pose> {
    poses.retain(|part, _| look.drawing.has_part(part));
    poses
}

#[cfg(test)]
mod tests;
