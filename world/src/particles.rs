//! Particles: dust, spray, sparks, snow. An entity's `emit` streams small fading discs that Rust
//! spawns, moves and drops each fixed step, with no Lua per frame. They are paint only: they
//! touch no body and no pointer. Randomness is seeded from the world's step and the entity's
//! id, so the same run paints the same particles.

use std::sync::Arc;

use bevy_ecs::prelude::Component;
use runtime::frame::{Brush, FrameError, Item, Path};
use runtime::vello::kurbo::{Affine, PathEl, Point};
use runtime::vello::peniko::{Color, Fill};

/// Live particles per emitter, past which none spawn until some die.
pub const MAX_LIVE: usize = 1024;

/// An emitter as described.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Emit {
    /// Particles a second; 0 emits none (the live ones finish).
    pub rate: f64,
    /// Seconds each one lives, fading out over it.
    pub life: f64,
    /// Units a second it leaves at.
    pub speed: f64,
    /// Degrees, in the entity's frame: 0 is +x, 90 is +y (down).
    pub angle: f64,
    /// Degrees the direction is spread over, centred on `angle`.
    pub spread: f64,
    /// Radius, units, at birth; it grows by `grow` units a second (negative shrinks).
    pub size: f64,
    pub grow: f64,
    pub color: Color,
    /// Where they leave from, in the entity's drawing box.
    pub at: (f64, f64),
    /// Units a second squared, +y down: dust settles, sparks fall, smoke rises (negative).
    pub gravity: f64,
}

impl Emit {
    pub(crate) fn check(&self) -> Result<(), String> {
        let fine = |v: f64| v.is_finite();
        if !(fine(self.rate) && self.rate >= 0.0 && self.rate <= 10_000.0) {
            return Err(format!("emit rate must be from 0 to 10000 a second, got {}", self.rate));
        }
        if !(fine(self.life) && self.life > 0.0 && self.life <= 60.0) {
            return Err(format!("emit life must be above 0 and at most 60 seconds, got {}", self.life));
        }
        if !(fine(self.speed) && self.speed >= 0.0) {
            return Err(format!("emit speed must be 0 or more, got {}", self.speed));
        }
        if !(fine(self.spread) && (0.0..=360.0).contains(&self.spread)) {
            return Err(format!("emit spread must be from 0 to 360, got {}", self.spread));
        }
        if !(fine(self.size) && self.size > 0.0) {
            return Err(format!("emit size must be above 0, got {}", self.size));
        }
        let rest = [self.angle, self.grow, self.gravity, self.at.0, self.at.1];
        if !rest.into_iter().all(fine) {
            return Err("emit angle, grow, gravity and at must be finite".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Particle {
    pos: (f64, f64),
    velocity: (f64, f64),
    age: f64,
}

/// An entity's stream: what it emits now (None once `emit` is gone and the live ones finish),
/// what is alive, and the share of a particle owed from earlier steps.
#[derive(Component, Clone, Debug)]
pub(crate) struct Emitter {
    pub spec: Option<Emit>,
    /// The last spec, for how the live ones look after `emit` is gone.
    look: Emit,
    live: Vec<Particle>,
    owed: f64,
    seed: u64,
}

impl Emitter {
    pub fn new(spec: Emit, id: &str) -> Self {
        Self { spec: Some(spec), look: spec, live: Vec::new(), owed: 0.0, seed: fnv(id) }
    }

    pub fn describe(&mut self, spec: Option<Emit>) {
        if let Some(s) = spec {
            self.look = s;
        }
        self.spec = spec;
    }

    pub fn live(&self) -> usize {
        self.live.len()
    }

    /// Whether it still has anything to do: emitting, or particles still fading.
    pub fn busy(&self) -> bool {
        self.spec.is_some_and(|s| s.rate > 0.0) || !self.live.is_empty()
    }

    pub fn positions(&self) -> Vec<(f64, f64)> {
        self.live.iter().map(|p| p.pos).collect()
    }

    /// One fixed step: age and move the live ones, drop the dead, then spawn what `rate` owes
    /// at `place` (the entity's placement: drawing box to world). `step` seeds the randomness.
    pub fn advance(&mut self, dt: f64, place: Affine, step: u64) {
        let g = self.look.gravity;
        for p in &mut self.live {
            p.age += dt;
            p.velocity.1 += g * dt;
            p.pos = (p.pos.0 + p.velocity.0 * dt, p.pos.1 + p.velocity.1 * dt);
        }
        let life = self.look.life;
        self.live.retain(|p| p.age < life);
        let Some(spec) = self.spec else {
            self.owed = 0.0;
            return;
        };
        self.owed += spec.rate * dt;
        let due = self.owed.floor();
        self.owed -= due;
        let origin = place * Point::new(spec.at.0, spec.at.1);
        for k in 0..due as u64 {
            if self.live.len() >= MAX_LIVE {
                break;
            }
            let r = split(self.seed ^ step.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ k.wrapping_mul(0xD1B5_4A32_D192_ED03));
            let unit = |bits: u64| (bits >> 11) as f64 / (1u64 << 53) as f64;
            let angle = (spec.angle + (unit(r) - 0.5) * spec.spread).to_radians();
            let speed = spec.speed * (0.6 + 0.4 * unit(split(r)));
            // The direction turns with the entity, as its drawing does.
            let dir = place * Point::new(angle.cos(), angle.sin()) - place * Point::ZERO;
            let len = dir.hypot().max(1e-12);
            let velocity = (dir.x / len * speed, dir.y / len * speed);
            self.live.push(Particle { pos: (origin.x, origin.y), velocity, age: 0.0 });
        }
    }

    /// The live particles as paint, in world units, oldest first (the young on top).
    pub fn items(&self) -> Result<Vec<Item>, FrameError> {
        let s = self.look;
        let mut items = Vec::with_capacity(self.live.len());
        for p in &self.live {
            let left = 1.0 - p.age / s.life;
            let r = (s.size + s.grow * p.age).max(0.0);
            if r <= 0.0 || left <= 0.0 {
                continue;
            }
            let color = s.color.multiply_alpha(left as f32);
            let path = Path::new(disc(p.pos, r)).map_err(|_| FrameError::InvalidSize)?;
            items.push(Item::fill(Arc::new(path), Arc::new(Brush::solid(color)?), Fill::NonZero));
        }
        Ok(items)
    }
}

fn disc((cx, cy): (f64, f64), r: f64) -> Vec<PathEl> {
    let k = r * 0.5523;
    let p = |x: f64, y: f64| Point::new(cx + x, cy + y);
    vec![
        PathEl::MoveTo(p(0.0, -r)),
        PathEl::CurveTo(p(k, -r), p(r, -k), p(r, 0.0)),
        PathEl::CurveTo(p(r, k), p(k, r), p(0.0, r)),
        PathEl::CurveTo(p(-k, r), p(-r, k), p(-r, 0.0)),
        PathEl::CurveTo(p(-r, -k), p(-k, -r), p(0.0, -r)),
        PathEl::ClosePath,
    ]
}

/// FNV-1a: a stable hash of an id, the same on every machine and Rust version.
fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// SplitMix64's finaliser: one well-mixed word from another.
fn split(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
