//! Typed coordinate spaces and the transformations between them.

use euclid::{Point2D, Rect, Size2D, Transform2D, Vector2D};
use kurbo::Affine;

pub enum ScreenSpace {}
pub enum ViewportSpace {}
pub enum ContentSpace {}
pub enum NodeSpace {}

pub type ScreenPoint = Point2D<f64, ScreenSpace>;
pub type ScreenVector = Vector2D<f64, ScreenSpace>;
pub type ViewportVector = Vector2D<f64, ViewportSpace>;
pub type ContentPoint = Point2D<f64, ContentSpace>;
pub type ContentVector = Vector2D<f64, ContentSpace>;
pub type NodePoint = Point2D<f64, NodeSpace>;
pub type ScreenSize = Size2D<f64, ScreenSpace>;
pub type ContentSize = Size2D<f64, ContentSpace>;
pub type ScreenRect = Rect<f64, ScreenSpace>;
pub type ContentRect = Rect<f64, ContentSpace>;
pub type ContentToScreen = Transform2D<f64, ContentSpace, ScreenSpace>;
pub type ScreenToContent = Transform2D<f64, ScreenSpace, ContentSpace>;

/// A camera from one viewport's stable content coordinates into the window.
#[derive(Clone, Copy, Debug)]
pub struct CameraMap {
    content_to_screen: ContentToScreen,
}

impl CameraMap {
    pub fn new(origin: ScreenPoint, pan: ViewportVector, scale: f64) -> Self {
        assert!(scale > 0.0 && scale.is_finite());
        Self {
            content_to_screen: Transform2D::new(
                scale,
                0.0,
                0.0,
                scale,
                origin.x + pan.x,
                origin.y + pan.y,
            ),
        }
    }

    pub fn content_to_screen(&self, point: ContentPoint) -> ScreenPoint {
        self.content_to_screen.transform_point(point)
    }

    pub fn screen_to_content(&self, point: ScreenPoint) -> ContentPoint {
        self.content_to_screen
            .inverse()
            .expect("a camera scale is non-zero")
            .transform_point(point)
    }

    pub fn screen_vector_to_content(&self, vector: ScreenVector) -> ContentVector {
        self.content_to_screen
            .inverse()
            .expect("a camera scale is non-zero")
            .transform_vector(vector)
    }

    pub fn to_affine(self) -> Affine {
        self.content_to_screen.to_untyped().into()
    }
}

#[cfg(test)]
mod tests;
