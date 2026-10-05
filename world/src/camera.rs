//! Which part of the world the box shows: a point at the view's centre, following an entity on
//! the fixed step, kept inside bounds. Plan: docs/design/camera.md.

/// The camera as described: `follow` an entity or look `at` a point, never both.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CameraSpec {
    pub follow: Option<String>,
    pub at: Option<(f64, f64)>,
    /// Per second: a step closes `1 − exp(−ease · STEP)` of the gap. None is exact.
    pub ease: Option<f64>,
    /// `(x, y, w, h)` the view stays inside.
    pub bounds: Option<(f64, f64, f64, f64)>,
}

/// A handler's command: look here, or follow that.
#[derive(Clone, Debug, PartialEq)]
pub enum Look {
    At((f64, f64)),
    Follow(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CameraInspection {
    /// The world point at the view's centre.
    pub at: (f64, f64),
    pub follow: Option<String>,
    /// Following an id the world does not have: the camera holds.
    pub lost: bool,
    pub ease: Option<f64>,
    pub bounds: Option<(f64, f64, f64, f64)>,
    pub view: (f64, f64),
    /// Entities that meet the view, so are posed and drawn; the rest are skipped.
    pub drawn: usize,
}

/// Closer than this, an easing camera is there: it stops asking for ticks.
const ARRIVED: f64 = 1e-3;

pub(crate) struct Camera {
    pub at: (f64, f64),
    pub follow: Option<String>,
    pub lost: bool,
    pub view: (f64, f64),
    /// The last description, so describing the same again does not undo a `set_camera`.
    pub described: CameraSpec,
}

impl CameraSpec {
    pub fn check(&self) -> Result<(), String> {
        let finite = |p: Option<(f64, f64)>| p.is_none_or(|(x, y)| x.is_finite() && y.is_finite());
        if self.follow.is_some() && self.at.is_some() {
            return Err("follow and at: a camera follows an entity or looks at a point, not both".into());
        }
        if !finite(self.at) {
            return Err("at must be finite numbers".into());
        }
        if !self.ease.is_none_or(|e| e.is_finite() && e > 0.0) {
            return Err("ease must be a finite number above zero, per second".into());
        }
        let good = |(x, y, w, h): (f64, f64, f64, f64)| {
            [x, y, w, h].iter().all(|n| n.is_finite()) && w > 0.0 && h > 0.0
        };
        if !self.bounds.is_none_or(good) {
            return Err("bounds must be { x, y, w, h }, finite, with w and h above zero".into());
        }
        Ok(())
    }
}

impl Camera {
    /// Kept inside the bounds; a map narrower than the view is centred on that axis.
    pub fn clamp(&self, (x, y): (f64, f64)) -> (f64, f64) {
        let Some((bx, by, bw, bh)) = self.described.bounds else {
            return (x, y);
        };
        let axis = |v: f64, start: f64, size: f64, view: f64| match size <= view {
            true => start + size / 2.0,
            false => v.clamp(start + view / 2.0, start + size - view / 2.0),
        };
        (axis(x, bx, bw, self.view.0), axis(y, by, bh, self.view.1))
    }

    /// One step towards `goal`, already clamped; snaps once within `ARRIVED`.
    pub fn approach(&mut self, goal: (f64, f64), dt: f64) {
        let share = self.described.ease.map_or(1.0, |e| 1.0 - (-e * dt).exp());
        let (dx, dy) = (goal.0 - self.at.0, goal.1 - self.at.1);
        self.at = match dx.abs().max(dy.abs()) * (1.0 - share) < ARRIVED {
            true => goal,
            false => (self.at.0 + dx * share, self.at.1 + dy * share),
        };
    }

    pub fn arrived(&self, goal: (f64, f64)) -> bool {
        (goal.0 - self.at.0).abs().max((goal.1 - self.at.1).abs()) < ARRIVED
    }

    pub fn to_world(&self, (x, y): (f64, f64)) -> (f64, f64) {
        (x + self.at.0 - self.view.0 / 2.0, y + self.at.1 - self.view.1 / 2.0)
    }

    pub fn to_screen(&self, (x, y): (f64, f64)) -> (f64, f64) {
        (x - self.at.0 + self.view.0 / 2.0, y - self.at.1 + self.view.1 / 2.0)
    }

    pub fn inspect(&self, drawn: usize) -> CameraInspection {
        CameraInspection {
            at: self.at,
            follow: self.follow.clone(),
            lost: self.lost,
            ease: self.described.ease,
            bounds: self.described.bounds,
            view: self.view,
            drawn,
        }
    }
}
