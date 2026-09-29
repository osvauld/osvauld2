//! Facing: which way an entity last moved, and what it shows that way. A top-down character is
//! drawn as views — front (down), back (up), side (drawn facing right) — and left mirrors the side
//! view unless it has its own. Anything a view leaves out falls back to the entity's own.

use std::cmp::Ordering;
use std::sync::Arc;

use bevy_ecs::prelude::Component;
use runtime::drawing::Drawing;

use crate::clip::Clip;

/// Kept on the entity across reconciles; stopping keeps the way it last moved.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dir {
    #[default]
    Down,
    Up,
    Left,
    Right,
}

impl Dir {
    pub(crate) const ALL: [Dir; 4] = [Dir::Down, Dir::Up, Dir::Left, Dir::Right];

    /// The way a move of `(dx, dy)` faces, y down. The larger component wins; on an exact
    /// diagonal the current facing stays if it is one of the two, else the horizontal one wins.
    pub(crate) fn of(dx: f64, dy: f64, current: Dir) -> Dir {
        let h = if dx > 0.0 { Dir::Right } else { Dir::Left };
        let v = if dy > 0.0 { Dir::Down } else { Dir::Up };
        match dx.abs().partial_cmp(&dy.abs()) {
            Some(Ordering::Greater) => h,
            Some(Ordering::Less) => v,
            _ if current == h || current == v => current,
            _ => h,
        }
    }
}

/// What an entity shows facing one way; a `None` falls back to the entity's own.
#[derive(Clone, Default)]
pub struct View {
    pub drawing: Option<Arc<Drawing>>,
    pub clip: Option<Arc<Clip>>,
    pub moving: Option<Arc<Clip>>,
}

#[derive(Clone, Default)]
pub struct Facings {
    pub down: Option<View>,
    pub up: Option<View>,
    pub side: Option<View>,
    pub left: Option<View>,
}

/// What one facing resolves to.
pub(crate) struct Shown<'a> {
    pub drawing: &'a Arc<Drawing>,
    pub clip: Option<&'a Arc<Clip>>,
    pub moving: Option<&'a Arc<Clip>>,
    pub mirrored: bool,
}

impl Facings {
    /// `drawing`, `clip` and `moving` are the entity's own — its controller's, for `moving`.
    pub(crate) fn shown<'a>(
        &'a self,
        dir: Dir,
        drawing: &'a Arc<Drawing>,
        clip: Option<&'a Arc<Clip>>,
        moving: Option<&'a Arc<Clip>>,
    ) -> Shown<'a> {
        let (view, mirrored) = match (dir, &self.left) {
            (Dir::Down, _) => (self.down.as_ref(), false),
            (Dir::Up, _) => (self.up.as_ref(), false),
            (Dir::Right, _) => (self.side.as_ref(), false),
            (Dir::Left, Some(left)) => (Some(left), false),
            (Dir::Left, None) => (self.side.as_ref(), self.side.is_some()),
        };
        Shown {
            drawing: view.and_then(|v| v.drawing.as_ref()).unwrap_or(drawing),
            clip: view.and_then(|v| v.clip.as_ref()).or(clip),
            moving: view.and_then(|v| v.moving.as_ref()).or(moving),
            mirrored,
        }
    }
}

#[cfg(test)]
mod tests;
