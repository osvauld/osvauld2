//! Paint pass: draw a laid-out `Placed` list into the vello scene. Hover is *derived* here — a node
//! with `hover_*` set uses it when the pointer is inside its rect, else the base look. No stored
//! hover flags; it falls out of `pointer ∩ rect` each frame (and we only repaint on pointer moves).

use crate::layout::Placed;
use crate::scroll::Axis;
use crate::scroll::Scrolls;
use crate::scroll::Thumb;
use crate::text::TextEngine;
use crate::Editors;
use vello::kurbo::{Affine, Insets, Point, Rect, RoundedRect, Stroke};
use vello::peniko::{Color, Fill};
use vello::Scene;

const SELECTION: Color = Color::from_rgba8(0x8A, 0x86, 0xE5, 0x66);
const THUMB: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0x59);
const THUMB_HOVER: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0x8C);
const THUMB_DRAG: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0xB3);
/// Draw `placed` (in paint order) into `scene`. `t` maps logical points to physical pixels.
/// `pointer` is the cursor in logical points, if inside the window.
pub(crate) fn draw<M>(
    scene: &mut Scene,
    placed: &[Placed<M>],
    editors: &Editors,
    text: &mut TextEngine,
    t: Affine,
    pointer: Option<(f32, f32)>,
    scrolls: &mut Scrolls,
) {
    for p in placed {
        let over =
            pointer.is_some_and(|(px, py)| p.rect.contains(Point::new(px as f64, py as f64)));
        let (fill, stroke) = p.content.look.resolve(over);
        let shape = RoundedRect::from_rect(p.rect, p.content.look.radius);
        if let Some(c) = fill {
            scene.fill(Fill::NonZero, t, c, None, &shape);
        }
        if let Some(b) = stroke {
            scene.stroke(&Stroke::new(b.width), t, b.color, None, &shape);
        }
        if let Some(custom) = &p.content.custom {
            custom(scene, text, p.rect, t);
        }
        if let Some(ts) = &p.content.text {
            if let Some(spec) = &p.content.input {
                if let Some(layout) = editors.layout_of(spec.id) {
                    let content = Rect::new(
                        p.rect.x0 + p.pad.x0,
                        p.rect.y0 + p.pad.y0,
                        p.rect.x1 - p.pad.x1,
                        p.rect.y1 - p.pad.y1,
                    );

                    scene.push_clip_layer(Fill::NonZero, t, &content);
                    let s = scrolls.get(spec.id);
                    let (scroll_x, scroll_y) = (s.x, s.y);
                    let (ox, oy) = content_offset(
                        p.rect,
                        p.pad,
                        layout.height(),
                        scroll_x,
                        scroll_y,
                        spec.multiline,
                    );
                    let origin = t * Affine::translate((p.rect.x0 + ox, p.rect.y0 + oy));
                    for (bb, _line) in editors.selection_geometry(spec.id) {
                        let r = Rect::new(
                            p.rect.x0 + ox + bb.x0,
                            p.rect.y0 + oy + bb.y0,
                            p.rect.x0 + ox + bb.x1,
                            p.rect.y0 + oy + bb.y1,
                        );
                        scene.fill(Fill::NonZero, t, SELECTION, None, &r);
                    }
                    text.draw_layout(scene, layout, origin, ts.color);
                    if editors.is_focused(spec.id) {
                        if let Some(bb) = editors.cursor_geometry(spec.id, 1.5) {
                            let r = Rect::new(
                                p.rect.x0 + ox + bb.x0,
                                p.rect.y0 + oy + bb.y0,
                                p.rect.x0 + ox + bb.x1,
                                p.rect.y0 + oy + bb.y1,
                            );
                            scene.fill(Fill::NonZero, t, ts.color, None, &r);
                        }
                    }

                    scene.pop_layer();
                }
            } else {
                let origin = t * Affine::translate((p.rect.x0, p.rect.y0));
                text.draw(scene, &ts.text, ts.family, ts.size, origin, ts.color);
            }
        }
    }
}

pub(crate) fn content_offset(
    rect: Rect,
    pad: Insets,
    line_h: f32,
    scroll_x: f32,
    scroll_y: f32,
    multiline: bool,
) -> (f64, f64) {
    let ox = pad.x0 - scroll_x as f64;
    let oy = if !multiline {
        (rect.height() - line_h as f64) / 2.0
    } else {
        pad.y0 - scroll_y as f64
    };
    (ox, oy)
}

pub(crate) fn scrollbars(
    scene: &mut Scene,
    bars: &[Thumb],
    t: Affine,
    pointer: Option<(f32, f32)>,
    dragging: Option<(&'static str, Axis)>,
) {
    for b in bars {
        let active = dragging == Some((b.id, b.axis));
        let over = dragging.is_none()
            && pointer.is_some_and(|(px, py)| b.rect.contains(Point::new(px as f64, py as f64)));
        let color = if active {
            THUMB_DRAG
        } else if over {
            THUMB_HOVER
        } else {
            THUMB
        };
        let shape = RoundedRect::from_rect(b.rect, b.rect.width().min(b.rect.height()) / 2.0);
        scene.fill(Fill::NonZero, t, color, None, &shape);
    }
}
