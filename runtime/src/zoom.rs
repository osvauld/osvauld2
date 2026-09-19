use crate::coords::{CameraMap, ScreenPoint, ViewportVector};
use vello::kurbo::{Point, Rect};

#[derive(Clone, Copy)]
pub(crate) struct Zoom {
    pub scale: f32,
    pub pan: (f32, f32),
}

impl Default for Zoom {
    fn default() -> Self {
        Self {
            scale: 1.0,
            pan: (0.0, 0.0),
        }
    }
}

impl Zoom {
    pub fn at(&mut self, rect: Rect, _content: (f32, f32), pointer: Point, lines: f32) {
        let old = self.scale;
        let next = (old * 1.1_f32.powf(lines)).clamp(0.4, 3.0);
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
