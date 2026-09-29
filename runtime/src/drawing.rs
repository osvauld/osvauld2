//! Drawings: an immutable, validated part tree over Frame items. List order is draw order and
//! `parent` is the hierarchy — deliberately independent, so a far arm can belong to the body yet
//! draw behind it. Paths and pivots are drawing-absolute. `pose` compiles to an ordinary Frame;
//! part ids are author strings with no meaning here.

use std::collections::HashMap;

use kurbo::{Affine, Point};

use crate::frame::{Frame, FrameError, Item};
use crate::id::Id;

pub const MAX_PARTS: usize = 256;

pub struct PartSpec {
    pub id: String,
    pub parent: Option<String>,
    pub pivot: Point,
    pub shapes: Vec<Item>,
}

/// A part's offset from rest: moved by `x, y`, rotated (degrees) and scaled about its pivot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub x: f64,
    pub y: f64,
    pub rot: f64,
    pub scale: f64,
}

impl Default for Pose {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            rot: 0.0,
            scale: 1.0,
        }
    }
}

#[derive(Debug)]
struct Part {
    id: Id,
    parent: Option<usize>,
    pivot: Point,
    shapes: Vec<Item>,
}

#[derive(Debug)]
pub struct Drawing {
    width: f64,
    height: f64,
    parts: Vec<Part>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum DrawingError {
    #[error("drawing has more than {MAX_PARTS} parts")]
    TooManyParts,
    #[error("part id must not be empty")]
    EmptyId,
    #[error("duplicate part id {0:?}")]
    DuplicateId(String),
    #[error("part {part:?} has unknown parent {parent:?}")]
    UnknownParent { part: String, parent: String },
    #[error("part {0:?} is its own ancestor")]
    Cycle(String),
    #[error("pose names unknown part {0:?}")]
    UnknownPart(String),
    #[error(transparent)]
    Frame(#[from] FrameError),
}

impl Drawing {
    pub fn new(width: f64, height: f64, specs: Vec<PartSpec>) -> Result<Self, DrawingError> {
        if specs.len() > MAX_PARTS {
            return Err(DrawingError::TooManyParts);
        }
        Frame::new(width, height, None, Vec::new())?; // the size rules are Frame's
        let mut index = HashMap::with_capacity(specs.len());
        for (i, spec) in specs.iter().enumerate() {
            if spec.id.is_empty() {
                return Err(DrawingError::EmptyId);
            }
            if index.insert(spec.id.as_str(), i).is_some() {
                return Err(DrawingError::DuplicateId(spec.id.clone()));
            }
        }
        let parents = specs
            .iter()
            .map(|spec| {
                spec.parent
                    .as_ref()
                    .map(|parent| {
                        index.get(parent.as_str()).copied().ok_or_else(|| {
                            DrawingError::UnknownParent {
                                part: spec.id.clone(),
                                parent: parent.clone(),
                            }
                        })
                    })
                    .transpose()
            })
            .collect::<Result<Vec<_>, _>>()?;
        // A chain longer than the part count has revisited a part.
        for (i, spec) in specs.iter().enumerate() {
            let (mut at, mut steps) = (parents[i], 0);
            while let Some(p) = at {
                steps += 1;
                if steps > specs.len() {
                    return Err(DrawingError::Cycle(spec.id.clone()));
                }
                at = parents[p];
            }
        }
        let parts = specs
            .into_iter()
            .zip(parents)
            .map(|(spec, parent)| Part {
                id: spec.id.into(),
                parent,
                pivot: spec.pivot,
                shapes: spec.shapes,
            })
            .collect();
        Ok(Self {
            width,
            height,
            parts,
        })
    }

    pub fn size(&self) -> (f64, f64) {
        (self.width, self.height)
    }

    pub fn has_part(&self, id: &str) -> bool {
        self.parts.iter().any(|p| &*p.id == id)
    }

    /// Parts with shapes become named groups, so a hit on the Frame reports the part id.
    pub fn pose(&self, overrides: &HashMap<&str, Pose>) -> Result<Frame, DrawingError> {
        if let Some(name) = overrides
            .keys()
            .find(|name| !self.parts.iter().any(|p| &*p.id == **name))
        {
            return Err(DrawingError::UnknownPart(name.to_string()));
        }
        let mut memo = vec![None; self.parts.len()];
        let items = (0..self.parts.len())
            .filter(|&i| !self.parts[i].shapes.is_empty())
            .map(|i| {
                let part = &self.parts[i];
                let world = self.world(i, overrides, &mut memo);
                Ok(Item::group(world, part.shapes.clone())?.with_id(part.id.clone()))
            })
            .collect::<Result<Vec<_>, DrawingError>>()?;
        Ok(Frame::new(self.width, self.height, None, items)?)
    }

    fn world(
        &self,
        i: usize,
        overrides: &HashMap<&str, Pose>,
        memo: &mut [Option<Affine>],
    ) -> Affine {
        if let Some(world) = memo[i] {
            return world;
        }
        let part = &self.parts[i];
        let pose = overrides.get(&*part.id).copied().unwrap_or_default();
        let pivot = part.pivot.to_vec2();
        let local = Affine::translate((pose.x, pose.y))
            * Affine::translate(pivot)
            * Affine::rotate(pose.rot.to_radians())
            * Affine::scale(pose.scale)
            * Affine::translate(-pivot);
        let world = match part.parent {
            Some(parent) => self.world(parent, overrides, memo) * local,
            None => local,
        };
        memo[i] = Some(world);
        world
    }
}

#[cfg(test)]
mod tests;
