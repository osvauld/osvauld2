use crate::id::Id;
use std::collections::HashMap;
use vello::kurbo::Rect;

const BAR_W: f64 = 8.0;
const BAR_GAP: f64 = 2.0;
const MIN_THUMB: f64 = 24.0;
#[derive(Clone)]
pub(crate) struct Thumb {
    pub rect: Rect,
    pub id: Id, //scroll context for drag
    pub axis: Axis,
    pub gain: f32, // inverse projection: cursor px -> offset px
    pub viewport: f32,
    pub content: f32,
}

pub(crate) fn axis_thumb(
    rect: Rect,
    id: &Id,
    axis: Axis,
    viewport: f32,
    content: f32,
    offset: f32,
) -> Option<Thumb> {
    if content <= viewport {
        return None;
    }
    let track = match axis {
        Axis::X => rect.width(),
        Axis::Y => rect.height(),
    };
    let len = ((viewport as f64 / content as f64) * track).max(MIN_THUMB);
    //how far the top of thumb can slide
    let travel = track - len;
    if travel < 1.0 {
        return None;
    }
    //offset range 0 to range
    let range = (content - viewport) as f64;
    let start = (offset as f64 / range * travel).clamp(0.0, travel); //forward projection
    let gain = (range / travel) as f32;
    let r = match axis {
        Axis::Y => {
            let x1 = rect.x1 - BAR_GAP;
            Rect::new(x1 - BAR_W, rect.y0 + start, x1, rect.y0 + start + len)
        }
        Axis::X => {
            let y1 = rect.y1 - BAR_GAP;
            Rect::new(rect.x0 + start, y1 - BAR_W, rect.x0 + start + len, y1)
        }
    };
    Some(Thumb {
        rect: r,
        id: id.clone(),
        axis,
        gain,
        viewport,
        content,
    })
}
#[derive(Clone, Copy, Default)]
pub(crate) struct Scroll {
    pub x: f32,
    pub y: f32,
}
#[derive(Clone, Copy, PartialEq)]
pub enum Axis {
    X,
    Y,
}
impl Scroll {
    pub fn get(&self, axis: Axis) -> f32 {
        match axis {
            Axis::X => self.x,
            Axis::Y => self.y,
        }
    }
    pub fn set(&mut self, axis: Axis, v: f32) {
        match axis {
            Axis::X => self.x = v,
            Axis::Y => self.y = v,
        };
    }
}

pub(crate) struct Scrolls {
    map: HashMap<Id, Scroll>,
}

impl Scrolls {
    pub fn new() -> Self {
        Scrolls {
            map: HashMap::new(),
        }
    }
    pub fn get(&self, id: &str) -> Scroll {
        self.map.get(id).copied().unwrap_or_default()
    }
    pub fn keep_in_view(
        &mut self,
        id: &Id,
        axis: Axis,
        near: f32,
        far: f32,
        inner: f32,
        content: f32,
    ) {
        let s = self.map.entry(id.clone()).or_default();
        let cur = s.get(axis);
        let mut final_scroll: f32 = cur;
        if near < final_scroll {
            final_scroll = near
        } else if far > final_scroll + inner {
            final_scroll = far - inner
        }
        final_scroll = final_scroll.clamp(0.0, (content - inner).max(0.0));
        s.set(axis, final_scroll);
    }
    pub fn by(&mut self, id: &Id, axis: Axis, delta: f32, inner: f32, content: f32) -> f32 {
        let s = self.map.entry(id.clone()).or_default();

        let cur = s.get(axis);
        let next = (cur + delta).clamp(0.0, (content - inner).max(0.0));

        s.set(axis, next);
        delta - (next - cur)
    }
}
