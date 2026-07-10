//! Paint pass: draw a laid-out `Placed` list into the vello scene. Hover is *derived* here — a node
//! with `hover_*` set uses it when the pointer is inside its rect, else the base look. No stored
//! hover flags; it falls out of `pointer ∩ rect` each frame (and we only repaint on pointer moves).

use crate::id::Id;
use crate::layout::Placed;
use crate::scroll::Axis;
use crate::scroll::Scrolls;
use crate::scroll::Thumb;
use crate::text;
use crate::text::TextEngine;
use crate::Editors;
use crate::MONO_FAMILY;
use vello::kurbo::Line;
use vello::kurbo::{Affine, Insets, Point, Rect, RoundedRect, Stroke};
use vello::peniko::{Color, Fill};
use vello::Scene;

const SELECTION: Color = Color::from_rgba8(0x8A, 0x86, 0xE5, 0x66);
const THUMB: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0x59);
const THUMB_HOVER: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0x8C);
const THUMB_DRAG: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0xB3);
const DEBUG_HAIR: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0x28);
const DEBUG_BOX: Color = Color::from_rgba8(0x53, 0xB4, 0xFF, 0xFF);
const DEBUG_PAD: Color = Color::from_rgba8(0x7B, 0xE0, 0x8A, 0xFF);
const DEBUG_GUIDE: Color = Color::from_rgba8(0x53, 0xB4, 0xFF, 0x80);
const DEBUG_CHIP: Color = Color::from_rgba8(0x11, 0x11, 0x18, 0xF2);
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
        let mut clipping = false;
        if let Some(c) = p.clip {
            let intersected_rect = c.intersect(p.rect);
            if intersected_rect.width() <= 0.0 || intersected_rect.height() <= 0.0 {
                continue;
            }
            clipping = true;
            scene.push_clip_layer(Fill::NonZero, t, &c);
        }
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
                if let Some(layout) = editors.layout_of(&spec.id) {
                    let content = Rect::new(
                        p.rect.x0 + p.pad.x0,
                        p.rect.y0 + p.pad.y0,
                        p.rect.x1 - p.pad.x1,
                        p.rect.y1 - p.pad.y1,
                    );

                    scene.push_clip_layer(Fill::NonZero, t, &content);
                    let s = scrolls.get(&spec.id);
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
                    for (bb, _line) in editors.selection_geometry(&spec.id) {
                        let r = Rect::new(
                            p.rect.x0 + ox + bb.x0,
                            p.rect.y0 + oy + bb.y0,
                            p.rect.x0 + ox + bb.x1,
                            p.rect.y0 + oy + bb.y1,
                        );
                        scene.fill(Fill::NonZero, t, SELECTION, None, &r);
                    }
                    text.draw_layout(scene, layout, origin, ts.color);
                    if editors.is_focused(&spec.id) {
                        if let Some(bb) = editors.cursor_geometry(&spec.id, 1.5) {
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
                let (_, th) = text.measure(&ts.text, ts.family, ts.size);
                let (ox, oy) = content_offset(p.rect, p.pad, th, 0.0, 0.0, false);
                let origin = t * Affine::translate((p.rect.x0 + ox, p.rect.y0 + oy));
                text.draw(scene, &ts.text, ts.family, ts.size, origin, ts.color);
            }
        }
        if clipping {
            scene.pop_layer();
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
    dragging: Option<(&Id, Axis)>,
) {
    for b in bars {
        let active = dragging == Some((&b.id, b.axis));
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

pub fn debug_boxes<M>(
    scene: &mut Scene,
    placed: &[Placed<M>],
    t: Affine,
    pointer: Option<(f32, f32)>,
    text: &mut TextEngine,
    viewport: (f32, f32),
) {
    for p in placed {
        scene.stroke(&Stroke::new(1.0), t, DEBUG_HAIR, None, &p.rect);
    }
    let dashed = Stroke::new(1.0).with_dashes(0.0, [4.0, 4.0]);
    let Some((px, py)) = pointer else { return };
    let pt = Point::new(px as f64, py as f64);
    let Some(p) = placed.iter().rev().find(|p| p.rect.contains(pt)) else {
        return;
    };
    scene.stroke(&Stroke::new(1.0), t, DEBUG_BOX, None, &p.rect);
    scene.stroke(&Stroke::new(1.0), t, DEBUG_PAD, None, &p.rect.inset(-p.pad));
    scene.stroke(
        &dashed,
        t,
        DEBUG_GUIDE,
        None,
        &Line::new((p.rect.x0, 0.0), (p.rect.x0, viewport.1 as f64)),
    );

    scene.stroke(
        &dashed,
        t,
        DEBUG_GUIDE,
        None,
        &Line::new((p.rect.x1, 0.0), (p.rect.x1, viewport.1 as f64)),
    );

    scene.stroke(
        &dashed,
        t,
        DEBUG_GUIDE,
        None,
        &Line::new((0.0, p.rect.y1), (viewport.0 as f64, p.rect.y1)),
    );

    scene.stroke(
        &dashed,
        t,
        DEBUG_GUIDE,
        None,
        &Line::new((0.0, p.rect.y0), (viewport.0 as f64, p.rect.y0)),
    );
    scene.stroke(
        &dashed,
        t,
        DEBUG_GUIDE,
        None,
        &Line::new((0.0, p.rect.y1), (viewport.0 as f64, p.rect.y1)),
    );
    let label = format!("{:.0}x{:.0}", p.rect.width(), p.rect.height());
    let (w, h) = text.measure(&label, MONO_FAMILY, 11.0);
    let cx = p.rect.x0;
    let mut cy = p.rect.y0 - (h as f64 + 6.0);
    if cy < 0.0 {
        cy = p.rect.y0 + 4.0;
    }
    let plate = RoundedRect::new(cx, cy, cx + w as f64 + 12.0, cy + h as f64 + 6.0, 4.0);
    scene.fill(Fill::NonZero, t, DEBUG_CHIP, None, &plate);
    text.draw(
        scene,
        &label,
        MONO_FAMILY,
        11.0,
        t * Affine::translate((cx + 6.0, cy + 16.0)),
        DEBUG_BOX,
    );
}
