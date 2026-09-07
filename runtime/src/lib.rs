//! The GUI runtime: owns the winit window + event loop and the wgpu/vello GPU plumbing, lays out and
//! paints an `El` tree, and routes input. An application implements [`App`] (`view` + `update`, the
//! Elm/Iced shape) and calls [`run`]; everything GPU/winit/vello/layout is internal here.
//! `shell2` (Rust screens) and `app_host` (Lua apps) are its two front-ends — one node
//! vocabulary, same pipeline (docs/architecture.md).

mod anim;
mod drag;
mod editor;
mod el;
mod id;
mod layout;
mod paint;
mod render;
mod scroll;
mod state;
mod text;
use crate::anim::{Driver, Transition};
use crate::drag::{DropEvent, DropPhase};
use crate::editor::{Focus, KeepInView};
use crate::el::Binding;
use crate::id::Id;
use crate::state::Slot;
use editor::Field;
use scroll::*;
use std::collections::HashSet;
use std::ops::Fn;
use std::sync::Arc;
use std::time::Instant;
use vello::Scene;
use vello::kurbo::{Insets, Point, Rect};
use vello::peniko::Color;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
pub use winit::event_loop::{EventLoopClosed, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey::*};
use winit::window::{CursorIcon, Window, WindowId};

pub use drag::{DragEvent, DragPhase, Mods};
pub use el::{
    Anchor, El, Placement, PlacementAlign, PlacementSide, col, custom, rich, row, text, text_area,
    text_input,
};
pub use render::Render;
use state::Store;
pub use text::{MONO_FAMILY, PIXEL_FAMILY, Run, TextEngine, UI_FAMILY};
pub use vello;

const LINE_STEP: f32 = 30.0;
/// An application: a tree-of-elements `view` derived from state, plus an `update` that mutates state
/// in response to messages. The runtime calls `view` to paint and `update` when a click hits an
/// element carrying a message. `Msg: Clone` because a laid-out region owns its message.
pub trait App {
    type Msg: Clone + Send + 'static;

    /// Describe the whole screen as an element tree, given the current state.
    fn view(&self) -> El<Self::Msg>;

    /// Apply a message (e.g. from a click) to the state. The next frame re-derives `view`.
    fn update(&mut self, msg: Self::Msg);

    /// Canvas clear color — the page background. Default opaque black.
    fn clear(&self) -> Color {
        Color::from_rgba8(0, 0, 0, 0xFF)
    }
    fn reload(&mut self) {}
}

#[derive(Clone)]
enum Capture {
    App {
        id: Id,
        origin: (f32, f32),
        start: (f32, f32),
    },
    Thumb {
        thumb: Thumb,
        scroll: Scroll,
        press_point: (f32, f32),
    },
    Text {
        id: Id,
        rect: Rect,
        pad: Insets,
    },
    Pending {
        id: Id,
        origin: (f32, f32),
        start: (f32, f32),
        press: (f32, f32),
    },
}
impl Capture {
    fn thumb(&self) -> Option<&Thumb> {
        if let Capture::Thumb { thumb, .. } = self {
            Some(thumb)
        } else {
            None
        }
    }
    fn is_grab(&self) -> bool {
        matches!(self, Capture::App { .. } | Capture::Thumb { .. })
    }
}

/// Drives one `App`: holds the GPU `Render`, the live pointer (logical coords), and the last frame's
/// clickable regions for hit-testing.
struct Runner<A: App> {
    app: A,
    render: Option<Render>,
    pointer: Option<(f32, f32)>,
    text: TextEngine,
    focused: Focus,
    modifiers: ModifiersState,
    debug: bool,
    store: Store,
    drag: Option<Capture>,
    scene: Scene,
    start: Instant,
    last_frame: Option<f64>,
    hits: Hits<A::Msg>,
    pressed: Option<(Rect, A::Msg)>,
}

struct Hits<M> {
    click: Vec<(Rect, M)>,
    input: Vec<(Rect, Id, Insets)>,
    input_maps: Vec<(Id, Box<dyn Fn(String) -> M>)>,
    context: Vec<(Rect, Box<dyn Fn((f32, f32)) -> M>)>,
    drag: Vec<(Rect, Id, Box<dyn Fn(DragEvent) -> M>)>,
    drop: Vec<(Rect, Id, Box<dyn Fn(DropEvent) -> M>)>,
    scroll: Vec<ScrollHit>,
    enter: Vec<(Id, M)>,
    esc: Vec<(Id, M)>,
    bar: Vec<Thumb>,
}
// Hand-written: `derive(Default)` would demand `M: Default`, which no message type owes us.
impl<M> Default for Hits<M> {
    fn default() -> Self {
        Self {
            click: Vec::new(),
            input: Vec::new(),
            input_maps: Vec::new(),
            context: Vec::new(),
            drag: Vec::new(),
            drop: Vec::new(),
            scroll: Vec::new(),
            enter: Vec::new(),
            esc: Vec::new(),
            bar: Vec::new(),
        }
    }
}

impl<M> Hits<M> {
    pub fn clear(&mut self) {
        let Self {
            click,
            input,
            input_maps,
            context,
            drag,
            drop,
            scroll,
            enter,
            esc,
            bar,
        } = self;
        click.clear();
        input.clear();
        input_maps.clear();
        context.clear();
        drag.clear();
        drop.clear();
        scroll.clear();
        enter.clear();
        esc.clear();
        bar.clear();
    }
}

impl<A: App> Runner<A> {
    fn frame(&mut self) {
        let Some(render) = self.render.as_ref() else {
            return;
        };
        let mut done_msgs = Vec::new();
        let now = self.start.elapsed().as_secs_f64();
        let dt = self.last_frame.map_or(0.0, |last| (now - last).min(0.1)) as f32;
        self.last_frame = Some(now);
        let viewport = render.viewport();
        let t = render.transform();
        let clear = self.app.clear();
        self.scene.reset();
        let app = &self.app;
        let pointer = self.pointer;
        let hits = &mut self.hits;
        let store = &mut self.store;
        let focused = &mut self.focused;
        let pressed = self.pressed.as_ref().map(|(rect, _)| rect.clone());
        let text = &mut self.text;
        let debug = self.debug;
        let mut needs_redraw = false;
        let mut any_in_flight = false;
        let mut placed = layout::solve(app.view(), text, viewport, store);
        let prev_inputs: HashSet<Id> = hits.input_maps.iter().map(|(id, _)| id.clone()).collect();
        hits.clear();
        for p in placed.iter_mut() {
            let hit_rect = match p.clip {
                Some(c) => c.intersect(p.rect),
                None => p.rect,
            };

            let over =
                pointer.is_some_and(|(px, py)| hit_rect.contains(Point::new(px as f64, py as f64)));
            if p.appearance.repaint {
                any_in_flight = true;
            }

            for (b, slot) in p.behaviour.bindings() {
                if let Some(id) = &p.id {
                    let (fl, landed) = Self::drive(b, slot, store, dt, over, id);
                    if fl {
                        any_in_flight = true;
                    }
                    if let Some(v) = landed
                        && let Some((at, m)) = &b.on_done
                        && *at == v
                    {
                        done_msgs.push(m.clone())
                    }
                }
            }

            if let Some(msg) = p.behaviour.on_click.take() {
                hits.click.push((hit_rect, msg));
            }
            if let Some((id, handler)) = p.behaviour.on_drag.take() {
                hits.drag.push((hit_rect, id, handler));
            }

            if let Some((id, handler)) = p.behaviour.on_drop.take() {
                hits.drop.push((hit_rect, id, handler));
            }
            if let Some(h) = p.behaviour.on_right_click.take() {
                hits.context.push((hit_rect, h));
            }

            if let Some(spec) = &mut p.behaviour.input
                && let Some(id) = &p.id
            {
                if let Some(m) = spec.map.take() {
                    hits.input_maps.push((id.clone(), m));
                }
                if let Some(ts) = &p.appearance.text {
                    editor::sync(
                        id,
                        &ts.text,
                        p.rect.width() as f32,
                        p.rect.height() as f32,
                        ts.family,
                        ts.size,
                        spec.multiline,
                        text,
                        p.pad,
                        store,
                    );
                }
                hits.input.push((hit_rect, id.clone(), p.pad));
                if spec.autofocus && !prev_inputs.contains(id) {
                    focused.set(id.clone());
                    let field = focused.focused_field(store);
                    if let Some(field) = field {
                        field.caret_to_end(text);
                    }

                    if let Some(scroll_parent) = &p.scroll_parent {
                        let scroll_hit = hits.scroll.iter().find(|s| s.id == *scroll_parent);
                        if let Some(scroll_hit) = scroll_hit {
                            let scroll = store.get_or::<Scroll>(&scroll_parent, Slot::Scroll);
                            let offset_y = scroll.y;
                            let near = p.rect.y0 - scroll_hit.rect.y0 + offset_y as f64;
                            let far = p.rect.y1 - scroll_hit.rect.y0 + offset_y as f64;
                            let view = KeepInView {
                                axis: Axis::Y,
                                near: near as f32,
                                far: far as f32,
                                inner: scroll_hit.inner.1,
                                content: scroll_hit.content.1,
                            };
                            scroll.keep_in_view(view);
                            needs_redraw = true;
                        }
                    }
                }
                if let Some(m) = spec.on_esc.take() {
                    hits.esc.push((id.clone(), m));
                }
                if let Some(m) = spec.on_enter.take() {
                    hits.enter.push((id.clone(), m));
                }
            }

            let iw = p.rect.width() as f32 - (p.pad.x0 + p.pad.x1) as f32;
            let ih = p.rect.height() as f32 - (p.pad.y0 + p.pad.y1) as f32;
            let scroll_vals: Option<(&Id, (f32, f32), (bool, bool))> =
                if p.behaviour.input.is_some()
                    && let Some(id) = &p.id
                {
                    let (cw, ch) = store
                        .get::<Field>(id, Slot::Editor)
                        .and_then(|f| f.layout_of())
                        .map(|l| (l.full_width(), l.height()))
                        .unwrap_or((0.0, 0.0));
                    Some((id, (cw, ch), (true, true)))
                } else if let Some(scroll) = &p.behaviour.scroll
                    && let Some(id) = &p.id
                {
                    Some((id, p.content_size, (scroll.x, scroll.y)))
                } else {
                    None
                };
            if let Some((id, content, (ax, ay))) = scroll_vals {
                hits.scroll.push(ScrollHit {
                    hit_rect,
                    rect: p.rect,
                    id: id.clone(),
                    content,
                    inner: (iw, ih),
                });
                let s = store.get_or::<Scroll>(id, Slot::Scroll);
                if ay {
                    if let Some(v) = axis_thumb(p.rect, id, Axis::Y, ih, content.1, s.y) {
                        hits.bar.push(v);
                    }
                }
                if ax {
                    if let Some(h) = axis_thumb(p.rect, id, Axis::X, iw, content.0, s.x) {
                        hits.bar.push(h);
                    }
                }
            }
        }
        store.sweep();
        focused.clear_if_gone(store);
        let dragging = self
            .drag
            .as_ref()
            .and_then(Capture::thumb)
            .map(|t| (&t.id, t.axis));
        paint::draw(
            &mut self.scene,
            &placed,
            text,
            t,
            pointer,
            store,
            &focused,
            pressed,
        );
        paint::scrollbars(&mut self.scene, &hits.bar, t, pointer, dragging);
        if debug {
            paint::debug_boxes(&mut self.scene, &placed, t, pointer, text, viewport);
        }
        self.render.as_mut().unwrap().present(clear, &self.scene);
        let dispatched = !done_msgs.is_empty();
        for m in done_msgs {
            self.app.update(m);
        }
        if any_in_flight || needs_redraw || dispatched {
            self.redraw();
        }
    }
    fn drive<M>(
        b: &Binding<M>,
        s: Slot,
        store: &mut Store,
        dt: f32,
        over: bool,
        id: &Id,
    ) -> (bool, Option<f32>) {
        let target = match b.driver {
            Driver::Hover => {
                if over {
                    1.0
                } else {
                    0.0
                }
            }
            Driver::Value(f) => f,
        };
        let tr = store.get_or_with(id, s, || Transition::new(target, b.duration));
        tr.target = target;
        let was = tr.in_flight();
        tr.tick(dt);
        (tr.in_flight(), (was && !tr.in_flight()).then_some(target))
    }

    fn redraw(&self) {
        if let Some(render) = &self.render {
            render.request_redraw();
        }
    }
    /// Hit-test a press against the last frame's regions; topmost (last-painted) wins.
    fn click(&mut self) {
        let Some((px, py)) = self.pointer else { return };
        let p = vello::kurbo::Point::new(px as f64, py as f64);
        if let Some(thumb) = self
            .hits
            .bar
            .iter()
            .rev()
            .find(|thumb| thumb.rect.contains(p))
        {
            let bar = self
                .store
                .get::<Scroll>(&thumb.id, Slot::Scroll)
                .copied()
                .unwrap_or_default();
            self.drag = Some(Capture::Thumb {
                thumb: thumb.clone(),
                press_point: (px, py),
                scroll: bar,
            });
            self.redraw();
            return;
        }
        if let Some((rect, id, _handler)) =
            self.hits.drag.iter().rev().find(|(r, _, _)| r.contains(p))
        {
            let origin = (rect.x0 as f32, rect.y0 as f32);
            let start = (px - origin.0, py - origin.1);

            self.drag = Some(Capture::Pending {
                id: id.clone(),
                origin,
                start,
                press: (px, py),
            });
            self.redraw();
        }
        let hit = self.hits.input.iter().rev().find(|(r, _, _)| r.contains(p));
        match hit {
            Some((rect, id, pad)) => {
                self.focused.set(id.clone());
                let (lx, ly) = self.local_point(id, *rect, *pad, px, py);
                let field = self.store.get_mut::<Field>(id, Slot::Editor);
                if let Some(field) = field {
                    field.click_at(lx, ly, &mut self.text);
                }
                self.drag = Some(Capture::Text {
                    id: id.clone(),
                    rect: *rect,
                    pad: *pad,
                });
            }
            None => {
                self.focused.blur();
            }
        }
        if let Some((rect, msg)) = self.hits.click.iter().rev().find(|(r, _)| r.contains(p)) {
            let msg = msg.clone();
            self.pressed = Some((rect.clone(), msg));
        }

        self.redraw();
    }

    fn right_click(&mut self) {
        let Some((px, py)) = self.pointer else { return };
        let p = vello::kurbo::Point::new(px as f64, py as f64);
        if let Some((_, handler)) = self.hits.context.iter().rev().find(|(r, _)| r.contains(p)) {
            let msg = handler((px, py));
            self.app.update(msg);
            self.redraw();
        }
    }

    fn local_point(&self, id: &str, rect: Rect, pad: Insets, px: f32, py: f32) -> (f32, f32) {
        let field = self.store.get::<Field>(&Id::from(id), Slot::Editor);
        let line_h = field
            .and_then(|f| f.layout_of())
            .map(|l| l.height())
            .unwrap_or(0.0);
        let scroll = self
            .store
            .get::<Scroll>(&Id::from(id), Slot::Scroll)
            .copied()
            .unwrap_or_default();
        let multiline = self
            .store
            .get::<Field>(&Id::from(id), Slot::Editor)
            .map(|f| f.is_multiline())
            .unwrap_or(false);

        let (ox, oy) = paint::content_offset(rect, pad, line_h, scroll.x, scroll.y, multiline);
        let lx = (px as f64 - rect.x0 - ox) as f32;
        let ly = (py as f64 - rect.y0 - oy) as f32;
        (lx, ly)
    }

    fn mods(&self) -> Mods {
        Mods {
            shift: self.modifiers.shift_key(),
            ctrl: self.modifiers.control_key(),
            alt: self.modifiers.alt_key(),
            super_: self.modifiers.super_key(),
        }
    }

    fn notify_app_text(&mut self) {
        let focused_id = self.focused.get();
        if let Some(focused_id) = focused_id {
            let new_text = self
                .store
                .get::<Field>(focused_id, Slot::Editor)
                .map(|f| f.text_of());
            if let (Some(text), Some((_, map))) = (
                new_text,
                self.hits.input_maps.iter().find(|(k, _)| k == focused_id),
            ) {
                self.app.update(map(text.to_owned()));
            }
        }

        self.redraw();
    }
    fn drag_thumb(
        &mut self,
        lx: f32,
        ly: f32,
        thumb: &Thumb,
        scroll: &Scroll,
        (spx, spy): &(f32, f32),
    ) {
        if let Some(r) = &self.render {
            let desired = match thumb.axis {
                Axis::X => scroll.x + (lx - spx) * thumb.gain,
                Axis::Y => scroll.y + (ly - spy) * thumb.gain,
            };
            let cur = self.store.get_or::<Scroll>(&thumb.id, Slot::Scroll);
            let cur = cur.get(&thumb.axis);
            self.store.get_or::<Scroll>(&thumb.id, Slot::Scroll).by(
                thumb.axis,
                desired - cur,
                thumb.viewport,
                thumb.content,
            );
            r.request_redraw();
        }
    }
    fn on_drag_move(
        &mut self,
        px: f32,
        py: f32,
        handle_id: Id,
        origin: &(f32, f32),
        start: &(f32, f32),
    ) {
        let pos = (px, py);
        let mods = self.mods();
        let delta = (px - origin.0 - start.0, py - origin.1 - start.1);
        let event = DragEvent {
            pos,
            delta,
            mods,
            grab: *start,
            phase: DragPhase::Move,
        };
        if let Some((_, _, handler)) = self
            .hits
            .drag
            .iter()
            .find(|(_, id, _)| *id == handle_id.clone())
        {
            self.app.update(handler(event));
            self.redraw();
        }

        if let Some((rect, _, handler)) = self
            .hits
            .drop
            .iter()
            .rev()
            .find(|(r, _, _)| r.contains(Point::new(px as f64, py as f64)))
        {
            let drop_event = DropEvent {
                pos: (px - rect.x0 as f32, py - rect.y0 as f32),
                mods: self.mods(),
                dragged: handle_id,
                phase: DropPhase::Over,
                size: (rect.size().width as f32, rect.size().height as f32),
            };
            self.app.update(handler(drop_event));
        }
    }

    fn on_cursor_release(&mut self) {
        if let Some(cap) = self.drag.take() {
            match cap {
                Capture::App { id, origin, start } => {
                    if let Some((px, py)) = self.pointer {
                        let pos = (px, py);
                        let delta = (pos.0 - origin.0 - start.0, pos.1 - origin.1 - start.1);

                        if let Some((rect, _, handler)) = self
                            .hits
                            .drop
                            .iter()
                            .rev()
                            .find(|(r, _, _)| r.contains(Point::new(px as f64, py as f64)))
                        {
                            let drop_event = DropEvent {
                                pos: (px - rect.x0 as f32, py - rect.y0 as f32),
                                mods: self.mods(),
                                dragged: id.clone(),
                                phase: DropPhase::Release,
                                size: (rect.size().width as f32, rect.size().height as f32),
                            };
                            self.app.update(handler(drop_event));
                        }
                        if let Some((_, _, handler)) =
                            self.hits.drag.iter().find(|(_, hid, _)| *hid == id)
                        {
                            let event = DragEvent {
                                pos,
                                delta,
                                mods: self.mods(),
                                grab: start,
                                phase: DragPhase::End,
                            };
                            self.app.update(handler(event));
                        }
                    }
                }
                // scrollbar + text selection have no "End" message — take() already cleared them
                Capture::Thumb { .. } | Capture::Text { .. } => {}
                Capture::Pending { .. } => {}
            }
        }
        if let Some((rect, msg)) = self.pressed.take()
            && let Some((px, py)) = self.pointer
        {
            let point = Point::new(px as f64, py as f64);
            if rect.contains(point) {
                self.app.update(msg);
            }
        }
        self.redraw();
    }

    fn on_cursor_moved(&mut self, position: PhysicalPosition<f64>) {
        let scale = self.render.as_ref().map_or(1.0, |r| r.scale());
        let PhysicalPosition { x, y } = position;
        let lx = (x / scale) as f32;
        let ly = (y / scale) as f32;
        self.pointer = Some((lx, ly));
        let p = vello::kurbo::Point::new(lx as f64, ly as f64);
        let drag = self.drag.clone();
        //if its a scroll drag event return after scroll drag processed.
        if let Some(drag) = &drag {
            match drag {
                Capture::Thumb {
                    thumb,
                    scroll,
                    press_point,
                } => self.drag_thumb(lx, ly, thumb, scroll, press_point),
                Capture::App { id, origin, start } => {
                    self.on_drag_move(lx, ly, id.clone(), &origin, &start)
                }
                Capture::Text { id, rect, pad } => {
                    let (lx, ly) = self.local_point(id, *rect, *pad, lx, ly);
                    let text = &mut self.text;
                    let field = self.store.get_mut::<Field>(id, Slot::Editor);
                    if let Some(field) = field {
                        field.extend_to(lx, ly, text);
                    }
                }
                Capture::Pending {
                    id,
                    origin,
                    start,
                    press,
                } => {
                    let dx = press.0 - lx;
                    let dy = press.1 - ly;
                    if dx.abs() > 5.0 || dy.abs() > 5.0 {
                        self.drag = Some(Capture::App {
                            id: id.clone(),
                            origin: *origin,
                            start: *start,
                        });
                        let event = DragEvent {
                            delta: (0.0, 0.0),
                            phase: DragPhase::Start,
                            pos: (lx, ly),
                            grab: *start,
                            mods: self.mods(),
                        };
                        let handler = self
                            .hits
                            .drag
                            .iter()
                            .find(|(_, drag_id, _)| drag_id == id)
                            .map(|(_, _, handler)| handler);
                        if let Some(handler) = handler {
                            self.app.update(handler(event));
                            self.on_drag_move(lx, ly, id.clone(), origin, start);
                        }
                        self.pressed = None;
                    };
                }
            };
        }

        let over_input = self.hits.input.iter().any(|(r, _, _)| r.contains(p));
        // Repaint so hover follows the pointer (only while it's actually moving).
        if let Some(r) = &self.render {
            r.set_cursor(if self.drag.as_ref().is_some_and(Capture::is_grab) {
                CursorIcon::Grabbing
            } else if self.hits.drag.iter().any(|(r, _, _)| r.contains(p)) {
                CursorIcon::Grab
            } else if over_input {
                CursorIcon::Text
            } else {
                CursorIcon::Default
            });
            r.request_redraw();
        }
    }
    fn on_wheel_moved(&mut self, delta: MouseScrollDelta) {
        let Some((px, py)) = self.pointer else { return };
        let p = Point::new(px as f64, py as f64);
        let shift = self.modifiers.shift_key();

        //normalize to logical content-pixels
        let (dx, dy) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (x * LINE_STEP, y * LINE_STEP),
            MouseScrollDelta::PixelDelta(pos) => {
                let scale = self.render.as_ref().map_or(1.0, |r| r.scale());
                ((pos.x / scale) as f32, (pos.y / scale) as f32)
            }
        };
        let (dh, dv) = if shift { (-dy, 0.0) } else { (-dx, -dy) };
        let (mut rem_h, mut rem_v) = (dh, dv);
        //innermost scroll contenxt under the pointer wins
        for s in self.hits.scroll.iter().rev() {
            if !s.hit_rect.contains(p) {
                continue;
            }
            //chaining scroll
            if rem_h != 0.0 {
                rem_h = self.store.get_or::<Scroll>(&s.id, Slot::Scroll).by(
                    Axis::X,
                    rem_h,
                    s.inner.0,
                    s.content.0,
                );
            }
            if rem_v != 0.0 {
                rem_v = self.store.get_or::<Scroll>(&s.id, Slot::Scroll).by(
                    Axis::Y,
                    rem_v,
                    s.inner.1,
                    s.content.1,
                );
            }
            if rem_h.abs() < 0.5 && rem_v.abs() < 0.5 {
                break;
            }
        }
        self.redraw();
    }

    fn handle_input(&mut self, event: KeyEvent) {
        let pressed = event.state == ElementState::Pressed;
        let (mut enter_pressed, mut esc_pressed, mut f12_pressed, mut f5_pressed) =
            (false, false, false, false);
        if event.logical_key == Key::Named(Enter) {
            enter_pressed = true;
        } else if event.logical_key == Key::Named(Escape) {
            esc_pressed = true;
        } else if event.logical_key == Key::Named(F12) {
            f12_pressed = true;
        } else if event.logical_key == Key::Named(F5) {
            f5_pressed = true;
        }
        if pressed {
            if let Some(id) = self.focused.get() {
                if enter_pressed && !event.repeat {
                    if !self
                        .store
                        .get::<Field>(&id, Slot::Editor)
                        .map(|f| f.is_multiline())
                        .unwrap_or(false)
                    {
                        if let Some((_, m)) = self.hits.enter.iter().find(|(k, _)| k == id) {
                            self.app.update(m.clone());
                            self.redraw();
                            return;
                        }
                    }
                } else if esc_pressed && !event.repeat {
                    if let Some((_, m)) = self.hits.esc.iter().find(|(k, _)| k == id) {
                        self.app.update(m.clone());
                        self.redraw();
                        return;
                    }
                }
            }
            if f12_pressed {
                self.debug = !self.debug;
                self.redraw();
                return;
            }
            if f5_pressed {
                self.app.reload();
                self.redraw();
            }
        }
        if event.state != ElementState::Pressed {
            return;
        }

        let handled = self
            .focused
            .focused_field(&mut self.store)
            .map(|f| {
                f.on_key(
                    self.modifiers,
                    &mut self.text,
                    &event.logical_key,
                    &event.text,
                )
            })
            .is_some();
        if handled {
            self.notify_app_text();
        }
    }
}

impl<A: App> ApplicationHandler<A::Msg> for Runner<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.render.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title("osvauld");
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let render = pollster::block_on(Render::new(window));
        render.request_redraw(); // paint the first frame; after that we only repaint on demand
        render.set_ime_allowed(true);
        self.render = Some(render);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(r) = self.render.as_mut() {
                    r.set_scale(scale_factor);
                    r.request_redraw();
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(r) = self.render.as_mut() {
                    r.resize(size);
                    r.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.on_cursor_moved(position);
                // Physical → logical: hit-testing and rects are all in logical points.
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer = None;
                self.redraw();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => self.click(),
            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state: ElementState::Pressed,
                ..
            } => self.right_click(),
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                self.on_cursor_release();
            }
            // Retained: paint on demand. `frame` re-requests only while the app is animating.
            WindowEvent::RedrawRequested => self.frame(),
            WindowEvent::KeyboardInput { event, .. } => {
                self.handle_input(event);
            }
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
            }
            WindowEvent::Ime(ime) => {
                match ime {
                    Ime::Enabled => {}
                    Ime::Preedit(s, cur) => {
                        if let Some(field) = self.focused.focused_field(&mut self.store) {
                            field.on_ime(&s, cur, &mut self.text);
                            self.redraw();
                        }
                    }
                    Ime::Commit(s) => {
                        if let Some(field) = self.focused.focused_field(&mut self.store) {
                            field.on_ime_commit(&s, &mut self.text);
                            self.notify_app_text();
                        }
                    }
                    Ime::Disabled => {
                        if let Some(field) = self.focused.focused_field(&mut self.store) {
                            field.on_ime_disabled(&mut self.text);
                        }
                    }
                };
            }
            WindowEvent::MouseWheel { delta, .. } => self.on_wheel_moved(delta),
            _ => {}
        }
    }
    fn user_event(&mut self, _: &ActiveEventLoop, msg: A::Msg) {
        self.app.update(msg);
        self.redraw();
    }
}

/// Open a window and run the event loop, driving `app`. Blocks until the window closes.
pub fn run<A: App + 'static>(app: A) {
    run_with(|_| app);
}

pub fn run_with<A: App + 'static>(build: impl FnOnce(EventLoopProxy<A::Msg>) -> A) {
    env_logger::init();

    let event_loop = EventLoop::<A::Msg>::with_user_event()
        .build()
        .expect("event loop");
    let app = build(event_loop.create_proxy());
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut runner = Runner {
        app,
        render: None,
        pointer: None,
        hits: Hits::default(),
        focused: Focus::new(),
        text: TextEngine::new(),
        modifiers: ModifiersState::empty(),
        debug: false,
        store: Store::new(),
        drag: None,
        scene: Scene::new(),
        start: Instant::now(),
        last_frame: None,
        pressed: None,
    };
    event_loop.run_app(&mut runner).expect("run app");
}
