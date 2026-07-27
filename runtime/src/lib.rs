//! The GUI runtime: owns the winit window + event loop and the wgpu/vello GPU plumbing, lays out and
//! paints an `El` tree, and routes input. An application implements [`App`] (`view` + `update`, the
//! Elm/Iced shape) and calls [`run`]; everything GPU/winit/vello/layout is internal here. The
//! headless `app_engine` does not depend on this crate.

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
use crate::editor::{Focus, KeepInView};
use crate::el::Binding;
use crate::id::Id;
use editor::Field;
use scroll::*;
use std::collections::{HashMap, HashSet};
use std::ops::Fn;
use std::sync::Arc;
use std::time::Instant;
use vello::kurbo::{Insets, Point, Rect};
use vello::peniko::Color;
use vello::Scene;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey::*};
use winit::window::{CursorIcon, Window, WindowId};

pub use drag::{DragEvent, DragPhase, Mods};
pub use el::{
    col, custom, row, text, text_area, text_input, Anchor, El, Placement, PlacementAlign,
    PlacementSide,
};
pub use render::Render;
use state::Store;
pub use text::{TextEngine, MONO_FAMILY, PIXEL_FAMILY, UI_FAMILY};
pub use vello;

const LINE_STEP: f32 = 30.0;
/// An application: a tree-of-elements `view` derived from state, plus an `update` that mutates state
/// in response to messages. The runtime calls `view` to paint and `update` when a click hits an
/// element carrying a message. `Msg: Clone` because a laid-out region owns its message.
pub trait App {
    type Msg: Clone;

    /// Describe the whole screen as an element tree, given the current state.
    fn view(&self) -> El<Self::Msg>;

    /// Apply a message (e.g. from a click) to the state. The next frame re-derives `view`.
    fn update(&mut self, msg: Self::Msg);

    /// Canvas clear color — the page background. Default opaque black.
    fn clear(&self) -> Color {
        Color::from_rgba8(0, 0, 0, 0xFF)
    }
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
    hits: Vec<(Rect, A::Msg)>,
    text: TextEngine,
    focused: Focus,
    input_hits: Vec<(Rect, Id, Insets)>,
    input_maps: Vec<(Id, Box<dyn Fn(String) -> A::Msg>)>,
    context_hits: Vec<(Rect, Box<dyn Fn((f32, f32)) -> A::Msg>)>,
    drag_hits: Vec<(Rect, Id, Box<dyn Fn(DragEvent) -> A::Msg>)>,
    scroll_hits: Vec<ScrollHit>,
    enter_msgs: Vec<(Id, A::Msg)>,
    esc_msgs: Vec<(Id, A::Msg)>,
    modifiers: ModifiersState,
    bar_hits: Vec<Thumb>,
    debug: bool,
    store: Store,
    drag: Option<Capture>,
    scene: Scene,
    start: Instant,
    last_frame: Option<f64>,
    exiting: HashMap<Id, A::Msg>, // element id which are existing; deliver msg when each hits zero
    exit_hits: Vec<(Rect, Id, A::Msg)>,
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
        let input_hits = &mut self.input_hits;
        let input_maps = &mut self.input_maps;
        let exit_hits = &mut self.exit_hits;
        let enter_msgs = &mut self.enter_msgs;
        let esc_msgs = &mut self.esc_msgs;
        let scroll_hits = &mut self.scroll_hits;
        let drag_hits = &mut self.drag_hits;
        let bar_hits = &mut self.bar_hits;
        let context_hits = &mut self.context_hits;
        let store = &mut self.store;
        let focused = &mut self.focused;
        let text = &mut self.text;
        let debug = self.debug;
        let mut needs_redraw = false;
        let mut any_in_flight = false;
        let mut placed = layout::solve(app.view(), text, viewport, store);
        let prev_inputs: HashSet<Id> = input_maps.iter().map(|(id, _)| id.clone()).collect();
        hits.clear();
        input_hits.clear();
        esc_msgs.clear();
        enter_msgs.clear();
        input_maps.clear();
        scroll_hits.clear();
        drag_hits.clear();
        bar_hits.clear();
        exit_hits.clear();
        context_hits.clear();
        for p in placed.iter_mut() {
            let hit_rect = match p.clip {
                Some(c) => c.intersect(p.rect),
                None => p.rect,
            };

            let over =
                pointer.is_some_and(|(px, py)| hit_rect.contains(Point::new(px as f64, py as f64)));
            let slide_binding = p.behaviour.slide.as_ref().map(|(b, _)| b);
            let tint_binding = p.behaviour.tint.as_ref();
            let fade_binding = p.behaviour.fade.as_ref();

            for b in [slide_binding, tint_binding, fade_binding] {
                if let Some(b) = b {
                    let is_exiting = self.exiting.contains_key(&b.id);
                    let (fl, landed) = Self::drive(b, store, dt, over, is_exiting);
                    if fl {
                        any_in_flight = true;
                    }
                    if landed {
                        if let Some(msg) = self.exiting.remove(&b.id) {
                            //exit finished deliver the
                            //held msg
                            done_msgs.push(msg)
                        } else if let Some(m) = &b.on_done {
                            done_msgs.push(m.clone())
                        }
                    }
                }
            }

            if let Some(msg) = p.behaviour.on_click.take() {
                if let Some(exit_id) = &p.behaviour.exit {
                    exit_hits.push((p.rect, exit_id.clone(), msg));
                } else {
                    hits.push((hit_rect, msg));
                }
            }
            if let Some((id, handler)) = p.behaviour.on_drag.take() {
                drag_hits.push((hit_rect, id, handler));
            }
            if let Some(h) = p.behaviour.on_right_click.take() {
                context_hits.push((hit_rect, h));
            }

            if let Some(spec) = &mut p.behaviour.input {
                if let Some(m) = spec.map.take() {
                    input_maps.push((spec.id.clone(), m));
                }
                if let Some(ts) = &p.appearance.text {
                    editor::sync(
                        &spec.id,
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
                input_hits.push((hit_rect, spec.id.clone(), p.pad));
                if spec.autofocus && !prev_inputs.contains(&spec.id) {
                    focused.set(spec.id.clone());
                    let field = focused.focused_field(store);
                    if let Some(field) = field {
                        field.caret_to_end(text);
                    }

                    if let Some(scroll_parent) = &p.scroll_parent {
                        let scroll_hit = scroll_hits.iter().find(|s| s.id == *scroll_parent);
                        if let Some(scroll_hit) = scroll_hit {
                            let scroll = store.get_or::<Scroll>(&scroll_parent);
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
                    esc_msgs.push((spec.id.clone(), m));
                }
                if let Some(m) = spec.on_enter.take() {
                    enter_msgs.push((spec.id.clone(), m));
                }
            }

            let iw = p.rect.width() as f32 - (p.pad.x0 + p.pad.x1) as f32;
            let ih = p.rect.height() as f32 - (p.pad.y0 + p.pad.y1) as f32;
            let scroll_vals: Option<(&Id, (f32, f32), (bool, bool))> =
                if let Some(input) = &p.behaviour.input {
                    let (cw, ch) = store
                        .get::<Field>(&Id::from(input.id.clone()))
                        .and_then(|f| f.layout_of())
                        .map(|l| (l.full_width(), l.height()))
                        .unwrap_or((0.0, 0.0));
                    Some((&input.id, (cw, ch), (true, true)))
                } else if let Some(scroll) = &p.behaviour.scroll {
                    Some((&scroll.id, p.content_size, (scroll.x, scroll.y)))
                } else {
                    None
                };
            if let Some((id, content, (ax, ay))) = scroll_vals {
                scroll_hits.push(ScrollHit {
                    hit_rect,
                    rect: p.rect,
                    id: id.clone(),
                    content,
                    inner: (iw, ih),
                });
                let s = store.get_or::<Scroll>(id);
                if ay {
                    if let Some(v) = axis_thumb(p.rect, id, Axis::Y, ih, content.1, s.y) {
                        bar_hits.push(v);
                    }
                }
                if ax {
                    if let Some(h) = axis_thumb(p.rect, id, Axis::X, iw, content.0, s.x) {
                        bar_hits.push(h);
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
        paint::draw(&mut self.scene, &placed, text, t, pointer, store, &focused);
        paint::scrollbars(&mut self.scene, bar_hits, t, pointer, dragging);
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
        store: &mut Store,
        dt: f32,
        over: bool,
        is_exiting: bool,
    ) -> (bool, bool) {
        let target = if is_exiting {
            0.0
        } else {
            match b.driver {
                Driver::Hover => {
                    if over {
                        1.0
                    } else {
                        0.0
                    }
                }
                Driver::Value(f) => f,
            }
        };
        let tr = store.get_or_with(&b.id, || Transition::new(target, b.duration));
        tr.target = target;
        let was = tr.in_flight();
        tr.tick(dt);
        (tr.in_flight(), was && !tr.in_flight())
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
            .bar_hits
            .iter()
            .rev()
            .find(|thumb| thumb.rect.contains(p))
        {
            let bar = self
                .store
                .get::<Scroll>(&thumb.id)
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
        if let Some((rect, id, handler)) =
            self.drag_hits.iter().rev().find(|(r, _, _)| r.contains(p))
        {
            let origin = (rect.x0 as f32, rect.y0 as f32);
            let start = (px - origin.0, py - origin.1);
            let event = DragEvent {
                phase: DragPhase::Start,
                pos: start,
                delta: (0.0, 0.0),
                mods: self.mods(),
            };

            self.drag = Some(Capture::App {
                id: id.clone(),
                origin,
                start,
            });

            self.app.update(handler(event));
            self.redraw();
            return;
        }
        let hit = self.input_hits.iter().rev().find(|(r, _, _)| r.contains(p));
        match hit {
            Some((rect, id, pad)) => {
                self.focused.set(id.clone());
                let (lx, ly) = self.local_point(id, *rect, *pad, px, py);
                let field = self.store.get_mut::<Field>(id);
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
        if let Some((_, msg)) = self.hits.iter().rev().find(|(r, _)| r.contains(p)) {
            let msg = msg.clone();
            self.app.update(msg);
        }
        if let Some((_, id, msg)) = self.exit_hits.iter().rev().find(|(r, _, _)| r.contains(p)) {
            self.exiting.insert(id.clone(), msg.clone());
        }

        self.redraw();
    }

    fn right_click(&mut self) {
        let Some((px, py)) = self.pointer else { return };
        let p = vello::kurbo::Point::new(px as f64, py as f64);
        if let Some((_, handler)) = self.context_hits.iter().rev().find(|(r, _)| r.contains(p)) {
            let msg = handler((px, py));
            self.app.update(msg);
            self.redraw();
        }
    }

    fn local_point(&self, id: &str, rect: Rect, pad: Insets, px: f32, py: f32) -> (f32, f32) {
        let field = self.store.get::<Field>(&Id::from(id));
        let line_h = field
            .and_then(|f| f.layout_of())
            .map(|l| l.height())
            .unwrap_or(0.0);
        let scroll = self
            .store
            .get::<Scroll>(&Id::from(id))
            .copied()
            .unwrap_or_default();
        let multiline = self
            .store
            .get::<Field>(&Id::from(id))
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
            let new_text = self.store.get::<Field>(focused_id).map(|f| f.text_of());
            if let (Some(text), Some((_, map))) = (
                new_text,
                self.input_maps.iter().find(|(k, _)| k == focused_id),
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
            let cur = self.store.get_or::<Scroll>(&thumb.id);
            let cur = cur.get(&thumb.axis);
            self.store.get_or::<Scroll>(&thumb.id).by(
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
        let pos = (px - origin.0, py - origin.1);
        let mods = self.mods();
        let delta = (pos.0 - start.0, pos.1 - start.1);
        let event = DragEvent {
            pos,
            delta,
            mods,
            phase: DragPhase::Move,
        };
        if let Some((_, _, handler)) = self
            .drag_hits
            .iter()
            .find(|(_, id, _)| *id == handle_id.clone())
        {
            self.app.update(handler(event));
            self.redraw();
        }
    }

    fn on_cursor_release(&mut self) {
        if let Some(cap) = self.drag.take() {
            match cap {
                Capture::App { id, origin, start } => {
                    if let Some((px, py)) = self.pointer {
                        let pos = (px - origin.0, py - origin.1);
                        let delta = (pos.0 - start.0, pos.1 - start.1);
                        let event = DragEvent {
                            pos,
                            delta,
                            mods: self.mods(),
                            phase: DragPhase::End,
                        };
                        if let Some((_, _, handler)) =
                            self.drag_hits.iter().find(|(_, hid, _)| *hid == id)
                        {
                            self.app.update(handler(event));
                        }
                    }
                }
                // scrollbar + text selection have no "End" message — take() already cleared them
                Capture::Thumb { .. } | Capture::Text { .. } => {}
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
                    let field = self.store.get_mut::<Field>(id);
                    if let Some(field) = field {
                        field.extend_to(lx, ly, text);
                    }
                }
            };
        }

        let over_input = self.input_hits.iter().any(|(r, _, _)| r.contains(p));
        // Repaint so hover follows the pointer (only while it's actually moving).
        if let Some(r) = &self.render {
            r.set_cursor(if self.drag.as_ref().is_some_and(Capture::is_grab) {
                CursorIcon::Grabbing
            } else if self.drag_hits.iter().any(|(r, _, _)| r.contains(p)) {
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
        for s in self.scroll_hits.iter().rev() {
            if !s.hit_rect.contains(p) {
                continue;
            }
            //chaining scroll
            if rem_h != 0.0 {
                rem_h =
                    self.store
                        .get_or::<Scroll>(&s.id)
                        .by(Axis::X, rem_h, s.inner.0, s.content.0);
            }
            if rem_v != 0.0 {
                rem_v =
                    self.store
                        .get_or::<Scroll>(&s.id)
                        .by(Axis::Y, rem_v, s.inner.1, s.content.1);
            }
            if rem_h.abs() < 0.5 && rem_v.abs() < 0.5 {
                break;
            }
        }
        self.redraw();
    }

    fn handle_input(&mut self, event: KeyEvent) {
        let pressed = event.state == ElementState::Pressed;
        let (mut enter_pressed, mut esc_pressed, mut f12_pressed) = (false, false, false);
        if event.logical_key == Key::Named(Enter) {
            enter_pressed = true;
        } else if event.logical_key == Key::Named(Escape) {
            esc_pressed = true;
        } else if event.logical_key == Key::Named(F12) {
            f12_pressed = true;
        }
        if pressed {
            if let Some(id) = self.focused.get() {
                if enter_pressed && !event.repeat {
                    if !self
                        .store
                        .get::<Field>(&id)
                        .map(|f| f.is_multiline())
                        .unwrap_or(false)
                    {
                        if let Some((_, m)) = self.enter_msgs.iter().find(|(k, _)| k == id) {
                            self.app.update(m.clone());
                            self.redraw();
                            return;
                        }
                    }
                } else if esc_pressed && !event.repeat {
                    if let Some((_, m)) = self.esc_msgs.iter().find(|(k, _)| k == id) {
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

impl<A: App> ApplicationHandler for Runner<A> {
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
}

/// Open a window and run the event loop, driving `app`. Blocks until the window closes.
pub fn run<A: App + 'static>(app: A) {
    env_logger::init();
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut runner = Runner {
        app,
        render: None,
        pointer: None,
        hits: Vec::new(),
        focused: Focus::new(),
        input_hits: Vec::new(),
        text: TextEngine::new(),
        input_maps: Vec::new(),
        drag_hits: Vec::new(),
        modifiers: ModifiersState::empty(),
        scroll_hits: Vec::new(),
        bar_hits: Vec::new(),
        context_hits: Vec::new(),
        enter_msgs: Vec::new(),
        esc_msgs: Vec::new(),
        debug: false,
        store: Store::new(),
        drag: None,
        scene: Scene::new(),
        start: Instant::now(),
        last_frame: None,
        exiting: HashMap::new(),
        exit_hits: Vec::new(),
    };
    event_loop.run_app(&mut runner).expect("run app");
}
