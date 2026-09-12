//! Paint pass: draw a laid-out `Placed` list into the vello scene. Hover is *derived* here — a node
//! with `hover_*` set uses it when the pointer is inside its rect, else the base look. No stored
//! hover flags; it falls out of `pointer ∩ rect` each frame (and we only repaint on pointer moves).

use crate::MONO_FAMILY;
use crate::anim::Transition;
use crate::editor::Field;
use crate::editor::Focus;
use crate::el::TextSpec;
use crate::id::Id;
use crate::layout::Placed;
use crate::scroll::Axis;
use crate::scroll::Scroll;
use crate::scroll::Thumb;
use crate::state::{Slot, Store};
use crate::text::TextEngine;
use vello::Scene;
use vello::kurbo::Line;
use vello::kurbo::{Affine, Insets, Point, Rect, RoundedRect, Stroke};
use vello::peniko::{Color, Fill};

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
    text: &mut TextEngine,
    t: Affine,
    pointer: Option<(f32, f32)>,
    store: &Store,
    focus: &Focus,
    pressed: Option<Rect>,
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
        let t_e = if let Some(spec) = &p.behaviour.tint
            && let Some(id) = &p.id
        {
            let progress = store
                .get::<Transition>(id, Slot::Tint)
                .map(|t| t.progress)
                .unwrap_or(0.0);
            spec.easing.apply(progress)
        } else {
            let t = if over { 1.0 } else { 0.0 };
            t
        };
        let (mut fill, mut stroke) = p.appearance.look.resolve_t(t_e);

        let pressed_over = over && pressed.is_some_and(|pr| pr == p.rect);
        if pressed_over {
            if let Some(filled) = p.appearance.look.press_fill {
                fill = Some(filled);
            }
            if let Some(press_stroke) = p.appearance.look.press_stroke {
                stroke = Some(press_stroke);
            }
        }

        let shape = RoundedRect::from_rect(p.rect, p.appearance.look.radius as f64);
        if let Some(mut c) = fill {
            c = c.multiply_alpha(p.alpha);
            scene.fill(Fill::NonZero, t, c, None, &shape);
        }
        if let Some(mut b) = stroke {
            b.color = b.color.multiply_alpha(p.alpha);
            let mut s = Stroke::new(b.width as f64);
            if let Some(d) = b.dash {
                s = s.with_dashes(0.0, d);
            }
            scene.stroke(&s, t, b.color, None, &shape);
        }
        if let Some(custom) = &p.appearance.custom {
            custom(scene, text, p.rect, t);
        }
        if let Some(ts) = &p.appearance.text {
            let text_color = ts.color.multiply_alpha(p.alpha);
            if let Some(spec) = &p.behaviour.input
                && let Some(id) = &p.id
            {
                let field_layout = store
                    .get::<Field>(id, Slot::Editor)
                    .and_then(|f| f.layout_of().map(|l| (f, l)));

                if let Some((field, layout)) = field_layout {
                    let content = Rect::new(
                        p.rect.x0 + p.pad.x0,
                        p.rect.y0 + p.pad.y0,
                        p.rect.x1 - p.pad.x1,
                        p.rect.y1 - p.pad.y1,
                    );

                    scene.push_clip_layer(Fill::NonZero, t, &content);
                    let s = store
                        .get::<Scroll>(id, Slot::Scroll)
                        .copied()
                        .unwrap_or_default();
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
                    for (bb, _line) in field.selection_geometry() {
                        let r = Rect::new(
                            p.rect.x0 + ox + bb.x0,
                            p.rect.y0 + oy + bb.y0,
                            p.rect.x0 + ox + bb.x1,
                            p.rect.y0 + oy + bb.y1,
                        );
                        scene.fill(Fill::NonZero, t, SELECTION, None, &r);
                    }
                    text.draw_layout(scene, layout, origin, Some(text_color));
                    if ts.text.is_empty()
                        && let Some(ph) = &spec.placeholder
                    {
                        // The same constraint `editor::sync` gives the real value, so a long
                        // placeholder wraps exactly where the text replacing it will.
                        text.draw(
                            scene,
                            ph,
                            ts.family,
                            ts.size,
                            origin,
                            text_color.multiply_alpha(0.4),
                            spec.multiline.then(|| content.width() as f32),
                        );
                    }
                    if focus.is_focused(id.clone()) {
                        if let Some(bb) = store
                            .get::<Field>(id, Slot::Editor)
                            .and_then(|f| f.cursor_geometry(1.5))
                        {
                            let r = Rect::new(
                                p.rect.x0 + ox + bb.x0,
                                p.rect.y0 + oy + bb.y0,
                                p.rect.x0 + ox + bb.x1,
                                p.rect.y0 + oy + bb.y1,
                            );
                            scene.fill(Fill::NonZero, t, text_color, None, &r);
                        }
                    }

                    scene.pop_layer();
                }
            } else {
                // Routed on `runs` exactly as `layout::measure_text` is, so a leaf is never
                // measured rich and painted plain — the two would disagree about its size.
                let w = wrap_width(ts, p.rect, p.pad);
                let (_, th) = measure_placed(text, ts, p.rect, p.pad);
                let (ox, oy) = content_offset(p.rect, p.pad, th, 0.0, 0.0, false);
                let origin = t * Affine::translate((p.rect.x0 + ox, p.rect.y0 + oy));
                if !ts.runs.is_empty() {
                    // `p.alpha` is folded in per run rather than handed down as one brush, since
                    // the whole point is that the runs carry their own colours.
                    text.draw_rich(scene, &ts.text, &ts.runs, origin, w, p.alpha);
                } else {
                    text.draw(scene, &ts.text, ts.family, ts.size, origin, text_color, w);
                }
            }
        }
        if clipping {
            scene.pop_layer();
        }
    }
}

/// The width a text leaf's glyphs must be shaped against: its content box, which is *nearly*
/// the constraint `layout::measure_text` was given. Shaping at a width nobody reserved space for
/// lets the glyphs leave the box, so this stays anchored to the rect — but it cannot use the rect
/// naked, for two reasons.
///
/// **Taffy rounds layout to whole pixels.** "+ new workspace" at 13pt measures 105.0010, so the
/// box it is given is 105 — one thousandth of a point short of the string that sized it. Re-shaped
/// against that, parley does the only thing it can and breaks the line, and a one-line label paints
/// as two inside a box reserved for one. It is a sub-pixel loss, so it lands on every label whose
/// natural width is not a whole number, at every window size. That is the bug this function had:
/// nothing about it was narrow-window-specific, which is exactly how it was reported.
///
/// [`ROUNDING_SLACK`] is the fix and one point is the right size for it: rounding can never take
/// more than a whole pixel, and no line break turns on a single point of width that was not already
/// going to be marginal.
///
/// **`no_wrap` has to reach here too.** It is a property of the leaf, not of the box, and a label
/// that refuses to fold in layout while folding in paint is the same drift in a new coat.
pub(crate) fn wrap_width(ts: &TextSpec, rect: Rect, pad: Insets) -> Option<f32> {
    if !ts.wrap {
        return None;
    }
    Some((rect.width() - pad.x0 - pad.x1).max(0.0) as f32 + ROUNDING_SLACK)
}

/// What Taffy's integer rounding can shave off a reserved box — see [`wrap_width`].
const ROUNDING_SLACK: f32 = 1.0;

/// Shape a placed text node exactly as [`draw`] will: same width, same plain/rich routing.
///
/// One function so paint and `layout::measure_text` cannot drift apart. Routing a rich leaf
/// through the plain path does not overflow — it renders *small*, inside a box sized for runs it
/// then ignored, which is why the test for this asserts the height back rather than a bound.
pub(crate) fn measure_placed(
    engine: &mut TextEngine,
    ts: &TextSpec,
    rect: Rect,
    pad: Insets,
) -> (f32, f32) {
    let w = wrap_width(ts, rect, pad);
    if ts.runs.is_empty() {
        engine.measure(&ts.text, ts.family, ts.size, w)
    } else {
        engine.measure_rich(&ts.text, &ts.runs, w)
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
    let (w, h) = text.measure(&label, MONO_FAMILY, 11.0, None);
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
        t * Affine::translate((cx + 6.0, cy + 6.0)),
        DEBUG_BOX,
        None,
    );
}
