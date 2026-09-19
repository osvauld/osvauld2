//! Immutable, bounded visual data in Frame-local coordinates.
//! Geometry places this payload; app front-ends only construct it through validated values.

use std::sync::Arc;

use crate::id::Id;
use kurbo::{Affine, BezPath, Cap, Join, ParamCurveNearest, PathEl, Point, Rect, Shape, Stroke};
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

/// Sub-pixel is plenty to decide whether a pointer is on a line.
const HIT_ACCURACY: f64 = 0.1;

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
    /// How many items a hit test could name. Not the count of ids in the tree: an id'd container
    /// hides the ids inside it, because it answers as one shape.
    pub hittable: usize,
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
pub struct Item {
    kind: ItemKind,
    id: Option<Id>,
}

impl Item {
    pub fn fill(path: Arc<Path>, brush: Arc<Brush>, rule: Fill) -> Self {
        Self::new(ItemKind::Fill { path, brush, rule })
    }

    pub fn stroke(path: Arc<Path>, brush: Arc<Brush>, style: StrokeStyle) -> Self {
        Self::new(ItemKind::Stroke { path, brush, style })
    }

    pub fn group(transform: Affine, items: Vec<Item>) -> Result<Self, FrameError> {
        valid_transform(transform)?;
        Ok(Self::new(ItemKind::Group { transform, items }))
    }

    pub fn instance(transform: Affine, frame: Arc<Frame>) -> Result<Self, FrameError> {
        valid_transform(transform)?;
        Ok(Self::new(ItemKind::Instance { transform, frame }))
    }

    fn new(kind: ItemKind) -> Self {
        Self { kind, id: None }
    }

    /// Name this item, so a hit can report *which* shape it landed on. A named container answers
    /// for everything it holds and the names inside it stop being reachable — how an author picks
    /// the granularity they want: a whole dial, or each of its ticks.
    pub fn with_id(mut self, id: impl Into<Id>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn id(&self) -> Option<&Id> {
        self.id.as_ref()
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

    /// The topmost named shape under `p`, which is given in this frame's own coordinates.
    ///
    /// Unnamed shapes are paint: the pointer falls through them to whatever is beneath. A named
    /// container answers for everything it holds, so the point only has to reach some geometry
    /// inside it.
    pub fn hit(&self, p: Point) -> Option<FrameHit> {
        if self.stats.hittable == 0 {
            return None; // nothing in here can be named, so nothing in here is worth walking
        }
        find(&self.items, p, Affine::IDENTITY)
    }
}

/// Where a pointer landed inside a visual: which shape, and the point in *that shape's* own
/// coordinates, with the transform of every group and instance above it undone.
///
/// `into` is that undoing, kept so a later point can be put in the same space — which is what a
/// drag needs, since it keeps reporting in the shape it grabbed even after the pointer leaves it.
/// `local` is `into * p` for the point that produced the hit.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameHit {
    pub id: Id,
    pub local: Point,
    pub into: Affine,
}

/// Items paint in order and later ones cover earlier ones, so the hit walk runs backwards.
/// `into` is the frame-to-here transform accumulated on the way down.
fn find(items: &[Item], p: Point, into: Affine) -> Option<FrameHit> {
    for item in items.iter().rev() {
        let descend = match &item.kind {
            ItemKind::Group { transform, .. } | ItemKind::Instance { transform, .. } => {
                child(*transform, p, into)
            }
            _ => None,
        };
        let hit = match (&item.kind, &item.id, descend) {
            (kind, Some(id), None) if covers(kind, p) => Some(FrameHit {
                id: id.clone(),
                local: p,
                into,
            }),
            (ItemKind::Group { items, .. }, Some(id), Some((q, into))) if touches(items, q) => {
                Some(FrameHit {
                    id: id.clone(),
                    local: q,
                    into,
                })
            }
            (ItemKind::Group { items, .. }, None, Some((q, into))) => find(items, q, into),
            // Only the instance itself can be named — the names inside a reused visual aren't
            // reachable, so an anonymous one holds nothing worth descending for.
            (ItemKind::Instance { frame, .. }, Some(id), Some((q, into)))
                if touches(frame.items(), q) =>
            {
                Some(FrameHit {
                    id: id.clone(),
                    local: q,
                    into,
                })
            }
            _ => None,
        };
        if hit.is_some() {
            return hit;
        }
    }
    None
}

/// Any geometry at all, named or not — the question a named container asks of its contents.
fn touches(items: &[Item], p: Point) -> bool {
    items.iter().any(|item| match &item.kind {
        ItemKind::Group { transform, items } => {
            child_point(*transform, p).is_some_and(|q| touches(items, q))
        }
        ItemKind::Instance { transform, frame } => {
            child_point(*transform, p).is_some_and(|q| touches(frame.items(), q))
        }
        kind => covers(kind, p),
    })
}

/// A point in a child's own space. `None` when the transform collapses space: nothing drawn
/// through it is visible, so nothing through it is touchable either.
fn child_point(transform: Affine, p: Point) -> Option<Point> {
    (transform.determinant() != 0.0).then(|| transform.inverse() * p)
}

/// The same step, carrying the frame-to-child transform along with the point.
fn child(transform: Affine, p: Point, into: Affine) -> Option<(Point, Affine)> {
    let undo = (transform.determinant() != 0.0).then(|| transform.inverse())?;
    Some((undo * p, undo * into))
}

fn covers(kind: &ItemKind, p: Point) -> bool {
    match kind {
        ItemKind::Fill { path, rule, .. } => {
            path.bounds().contains(p)
                && match rule {
                    Fill::NonZero => path.bezier().winding(p) != 0,
                    Fill::EvenOdd => path.bezier().winding(p) % 2 != 0,
                }
        }
        // Within half a width of the line. Expanding the outline would be the exact answer, but
        // it costs an allocation per stroke on every frame build to serve one point per pointer
        // move. Caps and joins aren't modelled, and dashes aren't either: a dashed line is still
        // one line to the pointer.
        ItemKind::Stroke { path, style, .. } => {
            let reach = style.0.width / 2.0;
            path.bounds().inflate(reach, reach).contains(p)
                && path
                    .bezier()
                    .segments()
                    .any(|seg| seg.nearest(p, HIT_ACCURACY).distance_sq <= reach * reach)
        }
        _ => false,
    }
}

fn draw_items(scene: &mut Scene, items: &[Item], transform: Affine, alpha: f32) {
    for item in items {
        match &item.kind {
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
        let child = match &item.kind {
            ItemKind::Fill { path, .. } | ItemKind::Stroke { path, .. } => FrameStats {
                expanded_path_commands: path.command_count(),
                ..FrameStats::default()
            },
            ItemKind::Group { items, .. } => item_stats(items, depth + 1)?,
            // A reused visual keeps its own names to itself — fifty instances of one resource
            // would otherwise all answer to the same ones.
            ItemKind::Instance { frame, .. } => FrameStats {
                depth: depth + 1 + frame.stats.depth,
                hittable: 0,
                ..frame.stats
            },
        };
        stats.hittable += if item.id.is_some() { 1 } else { child.hittable };
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
