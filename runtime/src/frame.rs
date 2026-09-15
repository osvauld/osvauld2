//! Immutable, bounded visual data in Frame-local coordinates.
//! Geometry places this payload; app front-ends only construct it through validated values.

use std::sync::Arc;

use kurbo::{Affine, BezPath, Cap, Join, PathEl, Point, Rect, Shape, Stroke};
use peniko::{
    Brush as PenikoBrush, Color, ColorStop as PenikoColorStop, Extend as PenikoExtend, Fill,
    Gradient,
};
use vello::Scene;

pub const MAX_PATH_COMMANDS: usize = 65_536;
pub const MAX_FRAME_COORDINATE: f64 = 10_000_000.0;
pub const MAX_FRAME_DEPTH: usize = 32;
pub const MAX_FRAME_ITEMS: usize = 4_096;
pub const MAX_GRADIENT_STOPS: usize = 64;
pub const MAX_STROKE_DASHES: usize = 64;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PathError {
    #[error("path has {count} commands; maximum is {MAX_PATH_COMMANDS}")]
    TooManyCommands { count: usize },
    #[error("path command {index} ({command}) needs an open subpath")]
    InvalidSequence { index: usize, command: &'static str },
    #[error("path command {index} has a non-finite or out-of-range coordinate")]
    InvalidCoordinate { index: usize },
}

/// A validated Bézier path, reusable by multiple future Frame items.
#[derive(Clone, Debug)]
pub struct Path {
    bezier: BezPath,
    bounds: Rect,
}

impl Path {
    pub fn new(elements: Vec<PathEl>) -> Result<Self, PathError> {
        if elements.len() > MAX_PATH_COMMANDS {
            return Err(PathError::TooManyCommands {
                count: elements.len(),
            });
        }
        let mut open = false;
        for (index, element) in elements.iter().enumerate() {
            let (command, points): (&str, &[Point]) = match element {
                PathEl::MoveTo(p) => ("move", std::slice::from_ref(p)),
                PathEl::LineTo(p) => ("line", std::slice::from_ref(p)),
                PathEl::QuadTo(a, b) => ("quad", &[*a, *b]),
                PathEl::CurveTo(a, b, c) => ("cubic", &[*a, *b, *c]),
                PathEl::ClosePath => ("close", &[]),
            };
            if !points.iter().all(valid_point) {
                return Err(PathError::InvalidCoordinate { index });
            }
            if command == "move" {
                open = true;
            } else if !open {
                return Err(PathError::InvalidSequence { index, command });
            } else if command == "close" {
                open = false;
            }
        }
        let bezier = BezPath::from_vec(elements);
        let bounds = bezier.bounding_box();
        Ok(Self { bezier, bounds })
    }

    pub fn bezier(&self) -> &BezPath {
        &self.bezier
    }

    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    pub fn command_count(&self) -> usize {
        self.bezier.elements().len()
    }
}

fn valid_point(point: &Point) -> bool {
    point.x.is_finite()
        && point.y.is_finite()
        && point.x.abs() <= MAX_FRAME_COORDINATE
        && point.y.abs() <= MAX_FRAME_COORDINATE
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameStats {
    pub expanded_items: usize,
    pub expanded_path_commands: usize,
    pub depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Extend {
    Pad,
    Repeat,
    Reflect,
}

#[derive(Clone, Copy, Debug)]
pub struct GradientStop {
    offset: f32,
    color: Color,
}

impl GradientStop {
    pub fn new(offset: f32, color: Color) -> Result<Self, FrameError> {
        if !(0.0..=1.0).contains(&offset) || !offset.is_finite() {
            return Err(FrameError::InvalidGradientStop);
        }
        valid_color(color)?;
        Ok(Self { offset, color })
    }
}

#[derive(Clone, Debug)]
pub struct Brush(PenikoBrush);

impl Brush {
    pub fn solid(color: Color) -> Result<Self, FrameError> {
        valid_color(color)?;
        Ok(Self(color.into()))
    }

    pub fn linear(
        start: Point,
        end: Point,
        stops: Vec<GradientStop>,
        extend: Extend,
    ) -> Result<Self, FrameError> {
        if !valid_point(&start) || !valid_point(&end) || start == end {
            return Err(FrameError::InvalidGradientGeometry);
        }
        if stops.len() < 2
            || stops.len() > MAX_GRADIENT_STOPS
            || stops.windows(2).any(|pair| pair[0].offset > pair[1].offset)
        {
            return Err(FrameError::InvalidGradientStops);
        }
        let extend = match extend {
            Extend::Pad => PenikoExtend::Pad,
            Extend::Repeat => PenikoExtend::Repeat,
            Extend::Reflect => PenikoExtend::Reflect,
        };
        let stops: Vec<PenikoColorStop> = stops
            .iter()
            .map(|stop| (stop.offset, stop.color).into())
            .collect();
        let gradient = Gradient::new_linear(start, end)
            .with_extend(extend)
            .with_stops(stops.as_slice());
        Ok(Self(gradient.into()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StrokeCap {
    Butt,
    Square,
    Round,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StrokeJoin {
    Bevel,
    Miter,
    Round,
}

#[derive(Clone, Debug)]
pub struct StrokeStyle(Stroke);

impl StrokeStyle {
    pub fn new(
        width: f64,
        cap: StrokeCap,
        join: StrokeJoin,
        miter_limit: f64,
        dashes: Vec<f64>,
        dash_offset: f64,
    ) -> Result<Self, FrameError> {
        if !width.is_finite() || width <= 0.0 || width > MAX_FRAME_COORDINATE {
            return Err(FrameError::InvalidStrokeWidth);
        }
        if !miter_limit.is_finite() || !(1.0..=MAX_FRAME_COORDINATE).contains(&miter_limit) {
            return Err(FrameError::InvalidMiterLimit);
        }
        if dashes.len() > MAX_STROKE_DASHES {
            return Err(FrameError::TooManyStrokeDashes);
        }
        let valid_dashes = dashes.iter().all(|v| valid_extent(*v))
            && (dashes.is_empty() || dashes.iter().any(|v| *v > 0.0));
        if !valid_dashes || !dash_offset.is_finite() || dash_offset.abs() > MAX_FRAME_COORDINATE {
            return Err(FrameError::InvalidDashPattern);
        }
        let cap = match cap {
            StrokeCap::Butt => Cap::Butt,
            StrokeCap::Square => Cap::Square,
            StrokeCap::Round => Cap::Round,
        };
        let join = match join {
            StrokeJoin::Bevel => Join::Bevel,
            StrokeJoin::Miter => Join::Miter,
            StrokeJoin::Round => Join::Round,
        };
        Ok(Self(
            Stroke::new(width)
                .with_caps(cap)
                .with_join(join)
                .with_miter_limit(miter_limit)
                .with_dashes(dash_offset, dashes),
        ))
    }
}

#[derive(Clone, Debug)]
enum ItemKind {
    Fill {
        path: Arc<Path>,
        brush: Arc<Brush>,
        rule: Fill,
    },
    Stroke {
        path: Arc<Path>,
        brush: Arc<Brush>,
        style: StrokeStyle,
    },
    Group {
        transform: Affine,
        items: Vec<Item>,
    },
    Instance {
        transform: Affine,
        frame: Arc<Frame>,
    },
}

#[derive(Clone, Debug)]
pub struct Item(ItemKind);

impl Item {
    pub fn fill(path: Arc<Path>, brush: Arc<Brush>, rule: Fill) -> Self {
        Self(ItemKind::Fill { path, brush, rule })
    }

    pub fn stroke(path: Arc<Path>, brush: Arc<Brush>, style: StrokeStyle) -> Self {
        Self(ItemKind::Stroke { path, brush, style })
    }

    pub fn group(transform: Affine, items: Vec<Item>) -> Result<Self, FrameError> {
        valid_transform(transform)?;
        Ok(Self(ItemKind::Group { transform, items }))
    }

    pub fn instance(transform: Affine, frame: Arc<Frame>) -> Result<Self, FrameError> {
        valid_transform(transform)?;
        Ok(Self(ItemKind::Instance { transform, frame }))
    }
}

#[derive(Clone, Debug)]
pub struct Frame {
    width: f64,
    height: f64,
    baseline: Option<f64>,
    items: Vec<Item>,
    stats: FrameStats,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum FrameError {
    #[error("Frame dimensions must be finite and between zero and {MAX_FRAME_COORDINATE}")]
    InvalidSize,
    #[error("Frame baseline must be finite and within its height")]
    InvalidBaseline,
    #[error("Frame transform is non-finite or out of range")]
    InvalidTransform,
    #[error("Frame color contains a non-finite component")]
    InvalidColor,
    #[error("gradient stop must have a finite offset between zero and one")]
    InvalidGradientStop,
    #[error("gradient needs between 2 and {MAX_GRADIENT_STOPS} ordered stops")]
    InvalidGradientStops,
    #[error("gradient geometry must contain two distinct, finite, bounded points")]
    InvalidGradientGeometry,
    #[error("stroke width must be finite, positive, and bounded")]
    InvalidStrokeWidth,
    #[error("stroke miter limit must be finite and between one and the coordinate limit")]
    InvalidMiterLimit,
    #[error("stroke dash pattern or offset is invalid")]
    InvalidDashPattern,
    #[error("stroke has more than {MAX_STROKE_DASHES} dash entries")]
    TooManyStrokeDashes,
    #[error("Frame nesting depth exceeds {MAX_FRAME_DEPTH}")]
    TooDeep,
    #[error("Frame expands to more than {MAX_FRAME_ITEMS} items")]
    TooManyItems,
    #[error("Frame expands to more than {MAX_PATH_COMMANDS} path commands")]
    TooManyPathCommands,
}

impl Frame {
    pub fn new(
        width: f64,
        height: f64,
        baseline: Option<f64>,
        items: Vec<Item>,
    ) -> Result<Self, FrameError> {
        if !valid_extent(width) || !valid_extent(height) {
            return Err(FrameError::InvalidSize);
        }
        if baseline.is_some_and(|b| !b.is_finite() || b < 0.0 || b > height) {
            return Err(FrameError::InvalidBaseline);
        }
        let stats = item_stats(&items, 0)?;
        Ok(Self {
            width,
            height,
            baseline,
            items,
            stats,
        })
    }

    pub fn size(&self) -> (f64, f64) {
        (self.width, self.height)
    }
    pub fn baseline(&self) -> Option<f64> {
        self.baseline
    }
    pub fn stats(&self) -> FrameStats {
        self.stats
    }
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub(crate) fn draw(&self, scene: &mut Scene, transform: Affine, alpha: f32) {
        draw_items(scene, &self.items, transform, alpha);
    }
}

fn draw_items(scene: &mut Scene, items: &[Item], transform: Affine, alpha: f32) {
    for item in items {
        match &item.0 {
            ItemKind::Fill { path, brush, rule } => {
                let faded;
                let brush = if alpha == 1.0 {
                    &brush.0
                } else {
                    faded = brush.0.clone().multiply_alpha(alpha);
                    &faded
                };
                scene.fill(*rule, transform, brush, None, path.bezier());
            }
            ItemKind::Stroke { path, brush, style } => {
                let faded;
                let brush = if alpha == 1.0 {
                    &brush.0
                } else {
                    faded = brush.0.clone().multiply_alpha(alpha);
                    &faded
                };
                scene.stroke(&style.0, transform, brush, None, path.bezier());
            }
            ItemKind::Group {
                transform: local,
                items,
            } => {
                draw_items(scene, items, transform * *local, alpha);
            }
            ItemKind::Instance {
                transform: local,
                frame,
            } => {
                draw_items(scene, frame.items(), transform * *local, alpha);
            }
        }
    }
}

fn item_stats(items: &[Item], depth: usize) -> Result<FrameStats, FrameError> {
    if depth > MAX_FRAME_DEPTH {
        return Err(FrameError::TooDeep);
    }
    let mut stats = FrameStats {
        expanded_items: items.len(),
        depth,
        ..FrameStats::default()
    };
    for item in items {
        let child = match &item.0 {
            ItemKind::Fill { path, .. } | ItemKind::Stroke { path, .. } => FrameStats {
                expanded_path_commands: path.command_count(),
                ..FrameStats::default()
            },
            ItemKind::Group { items, .. } => item_stats(items, depth + 1)?,
            ItemKind::Instance { frame, .. } => FrameStats {
                depth: depth + 1 + frame.stats.depth,
                ..frame.stats
            },
        };
        stats.expanded_items = stats
            .expanded_items
            .checked_add(child.expanded_items)
            .ok_or(FrameError::TooManyItems)?;
        stats.expanded_path_commands = stats
            .expanded_path_commands
            .checked_add(child.expanded_path_commands)
            .ok_or(FrameError::TooManyPathCommands)?;
        stats.depth = stats.depth.max(child.depth);
        if stats.expanded_items > MAX_FRAME_ITEMS {
            return Err(FrameError::TooManyItems);
        }
        if stats.expanded_path_commands > MAX_PATH_COMMANDS {
            return Err(FrameError::TooManyPathCommands);
        }
        if stats.depth > MAX_FRAME_DEPTH {
            return Err(FrameError::TooDeep);
        }
    }
    Ok(stats)
}

fn valid_extent(value: f64) -> bool {
    value.is_finite() && (0.0..=MAX_FRAME_COORDINATE).contains(&value)
}

fn valid_transform(transform: Affine) -> Result<(), FrameError> {
    transform
        .as_coeffs()
        .iter()
        .all(|v| v.is_finite() && v.abs() <= MAX_FRAME_COORDINATE)
        .then_some(())
        .ok_or(FrameError::InvalidTransform)
}

fn valid_color(color: Color) -> Result<(), FrameError> {
    color
        .components
        .iter()
        .all(|v| v.is_finite())
        .then_some(())
        .ok_or(FrameError::InvalidColor)
}

#[cfg(test)]
mod tests;
