use crate::coords::{CameraMap, ScreenPoint, ViewportVector};
use vello::kurbo::{Point, Rect};

/// How far a camera zooms out and in unless its element says otherwise: enough for a chart, too
/// little for drawing a finger.
pub const DEFAULT_RANGE: (f32, f32) = (0.4, 3.0);

#[derive(Clone, Copy)]
pub(crate) struct Zoom {
    pub scale: f32,
    pub pan: (f32, f32),
    /// The element's `(min, max)` scale, refreshed every layout.
    pub range: (f32, f32),
    /// The scale `on_zoom` was last told, so it hears each change once.
    pub told: Option<f32>,
}

impl Default for Zoom {
    fn default() -> Self {
        Self {
            scale: 1.0,
            pan: (0.0, 0.0),
            range: DEFAULT_RANGE,
            told: None,
        }
    }
}

impl Zoom {
    pub fn at(&mut self, rect: Rect, _content: (f32, f32), pointer: Point, lines: f32) {
        let old = self.scale;
        // `max` then `min`, not `clamp`: a reversed range from a Rust caller must not panic.
        let next = (old * 1.1_f32.powf(lines)).max(self.range.0).min(self.range.1);
        if (next - old).abs() < f32::EPSILON {
            return;
        }
        let origin = ScreenPoint::new(rect.x0, rect.y0);
        let pointer = ScreenPoint::new(pointer.x, pointer.y);
        let camera = CameraMap::new(
            origin,
            ViewportVector::new(self.pan.0 as f64, self.pan.1 as f64),
            old as f64,
        );
        let content = camera.screen_to_content(pointer);
        self.scale = next;
        self.pan = (
            (pointer.x - origin.x - content.x * next as f64) as f32,
            (pointer.y - origin.y - content.y * next as f64) as f32,
        );
    }

    pub fn pan_by(&mut self, _rect: Rect, _content: (f32, f32), delta: (f32, f32)) {
        self.pan = (self.pan.0 + delta.0, self.pan.1 + delta.1);
    }
}

#[cfg(test)]
mod tests;
