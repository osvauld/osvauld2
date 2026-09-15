use crate::coords::{
    ContentPoint, ContentRect, ContentSize, ContentSpace, NodePoint, ScreenPoint, ScreenRect,
    ScreenSize, ScreenSpace, ScreenToContent,
};
use euclid::{Transform2D, UnknownUnit};
use vello::kurbo::{Affine, Point, Rect};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Clip {
    pub rect: Rect,
    pub to_screen: Affine,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Geometry {
    pub content_rect: ContentRect,
    pub screen_rect: ScreenRect,
    pub visible_rect: Option<ScreenRect>,
    screen_to_content: ScreenToContent,
}

impl Geometry {
    pub fn resolve(
        content_rect: Rect,
        content_to_screen: Affine,
        clips: impl IntoIterator<Item = Clip>,
    ) -> Self {
        let typed_content = ContentRect::new(
            ContentPoint::new(content_rect.x0, content_rect.y0),
            ContentSize::new(content_rect.width(), content_rect.height()),
        );
        let screen_rect = as_screen_rect(content_to_screen.transform_rect_bbox(content_rect));
        let visible_rect = clips.into_iter().try_fold(screen_rect, |visible, clip| {
            visible.intersection(&as_screen_rect(
                clip.to_screen.transform_rect_bbox(clip.rect),
            ))
        });
        let inverse: Transform2D<f64, UnknownUnit, UnknownUnit> =
            content_to_screen.inverse().into();
        Self {
            content_rect: typed_content,
            screen_rect,
            visible_rect,
            screen_to_content: inverse
                .with_source::<ScreenSpace>()
                .with_destination::<ContentSpace>(),
        }
    }

    pub fn content_point(&self, screen: ScreenPoint) -> ContentPoint {
        self.screen_to_content.transform_point(screen)
    }

    pub fn node_point(&self, screen: ScreenPoint) -> NodePoint {
        let point = self.content_point(screen);
        NodePoint::new(
            point.x - self.content_rect.min_x(),
            point.y - self.content_rect.min_y(),
        )
    }

    pub fn content_rect_kurbo(&self) -> Rect {
        self.content_rect.to_untyped().into()
    }

    pub fn screen_rect_kurbo(&self) -> Rect {
        self.screen_rect.to_untyped().into()
    }

    pub fn visible_rect_kurbo(&self) -> Option<Rect> {
        self.visible_rect.map(|rect| rect.to_untyped().into())
    }

    pub fn contains(&self, screen: Point) -> bool {
        self.visible_rect
            .is_some_and(|rect| rect.contains(ScreenPoint::new(screen.x, screen.y)))
    }

    pub fn scale(&self) -> f32 {
        if self.content_rect.width() != 0.0 {
            (self.screen_rect.width() / self.content_rect.width()) as f32
        } else {
            1.0
        }
    }
}

fn as_screen_rect(rect: Rect) -> ScreenRect {
    ScreenRect::new(
        ScreenPoint::new(rect.x0, rect.y0),
        ScreenSize::new(rect.width(), rect.height()),
    )
}

#[cfg(test)]
mod tests;
