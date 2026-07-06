//! The GUI runtime: owns the winit window + event loop and the wgpu/vello GPU plumbing, lays out and
//! paints an `El` tree, and routes input. An application implements [`App`] (`view` + `update`, the
//! Elm/Iced shape) and calls [`run`]; everything GPU/winit/vello/layout is internal here. The
//! headless `app_engine` does not depend on this crate.

mod editor;
mod el;
mod layout;
mod paint;
mod render;
mod scroll;
mod text;

use editor::Editors;
use scroll::*;
use std::sync::Arc;
use vello::kurbo::{Insets, Point, Rect};
use vello::peniko::Color;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::{CursorIcon, Window, WindowId};

pub use el::{col, custom, row, text, text_area, text_input, El};
pub use render::Render;
pub use text::{TextEngine, MONO_FAMILY, PIXEL_FAMILY, UI_FAMILY};
/// Re-exported so screens can use vello drawing types without a direct dependency.
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

    /// Whether the screen needs continuous frames (animation). Default false → idle until input.
    fn animating(&self) -> bool {
        false
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
    editors: Editors,
    input_hits: Vec<(Rect, &'static str, Insets)>,
    input_maps: Vec<(&'static str, Box<dyn Fn(String) -> A::Msg>)>,
    scroll_hits: Vec<(Rect, &'static str, (f32, f32), (f32, f32))>,
    scrolls: Scrolls,
    drag: Option<(Rect, &'static str, Insets)>,
    scroll_drag: Option<(Thumb, (f32, f32), Scroll)>,
    modifiers: ModifiersState,
    bar_hits: Vec<Thumb>,
}

impl<A: App> Runner<A> {
    fn frame(&mut self) {
        if self.render.is_none() {
            return;
        }
        let clear = self.app.clear();
        let app = &self.app;
        let pointer = self.pointer;
        let hits = &mut self.hits;
        let input_hits = &mut self.input_hits;
        let input_maps = &mut self.input_maps;
        let scroll_hits = &mut self.scroll_hits;
        let scroll_drag = self.scroll_drag;
        let bar_hits = &mut self.bar_hits;
        let scrolls = &mut self.scrolls;
        let editors = &mut self.editors;
        let text = &mut self.text;
        let render = self.render.as_mut().expect("render present");
        render.paint(clear, text, |scene, text, t, viewport, _now| {
            let mut placed = layout::solve(app.view(), text, viewport);
            hits.clear();
            input_hits.clear();
            input_maps.clear();
            scroll_hits.clear();
            bar_hits.clear();
            for p in placed.iter_mut() {
                if let Some(msg) = p.content.on_click.take() {
                    hits.push((p.rect, msg));
                }

                if let Some(spec) = &mut p.content.input {
                    if let Some(m) = spec.map.take() {
                        input_maps.push((spec.id, m));
                    }
                    if let Some(ts) = &p.content.text {
                        editors.sync(
                            spec.id,
                            &ts.text,
                            p.rect.width() as f32,
                            p.rect.height() as f32,
                            ts.family,
                            ts.size,
                            spec.multiline,
                            text,
                            p.pad,
                            scrolls,
                        );
                        let (cw, ch) = editors
                            .layout_of(spec.id)
                            .map(|l| (l.full_width(), l.height()))
                            .unwrap_or((0.0, 0.0));
                        let iw = p.rect.width() as f32 - (p.pad.x0 + p.pad.x1) as f32;
                        let ih = p.rect.height() as f32 - (p.pad.y0 + p.pad.y1) as f32;
                        scroll_hits.push((p.rect, spec.id, (cw, ch), (iw, ih)));
                        let s = scrolls.get(spec.id);
                        if let Some(v) = axis_thumb(p.rect, spec.id, Axis::Y, ih, ch, s.y) {
                            bar_hits.push(v)
                        };
                        if let Some(h) = axis_thumb(p.rect, spec.id, Axis::X, iw, cw, s.x) {
                            bar_hits.push(h)
                        };
                    }
                    input_hits.push((p.rect, spec.id, p.pad));
                }
            }
            let dragging = self.scroll_drag.map(|(t, _, _)| (t.id, t.axis));
            paint::draw(scene, &placed, editors, text, t, pointer, scrolls);
            paint::scrollbars(scene, bar_hits, t, pointer, dragging);
        });
        if self.app.animating() {
            self.redraw();
        }
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
            let bar = self.scrolls.get(thumb.id);
            self.scroll_drag = Some((thumb.clone(), (px, py), bar));
            self.redraw();
            return;
        }
        let hit = self
            .input_hits
            .iter()
            .rev()
            .find(|(r, _, _)| r.contains(p))
            .map(|&(r, id, padding)| (r, id, padding));
        match hit {
            Some((rect, id, pad)) => {
                self.editors.focus(id);
                let (lx, ly) = self.local_point(id, rect, pad, px, py);
                self.editors.click_at(id, lx, ly, &mut self.text);
                self.drag = Some((rect, id, pad));
            }
            None => {
                self.editors.blur();
                self.drag = None
            }
        }
        if let Some((_, msg)) = self.hits.iter().rev().find(|(r, _)| r.contains(p)) {
            let msg = msg.clone();
            self.app.update(msg);
        }

        self.redraw();
    }

    fn local_point(&self, id: &str, rect: Rect, pad: Insets, px: f32, py: f32) -> (f32, f32) {
        let line_h = self
            .editors
            .layout_of(id)
            .map(|l| l.height())
            .unwrap_or(0.0);
        let scroll = self.scrolls.get(id);
        let multiline = self.editors.is_multiline(id);

        let (ox, oy) = paint::content_offset(rect, pad, line_h, scroll.x, scroll.y, multiline);
        let lx = (px as f64 - rect.x0 - ox) as f32;
        let ly = (py as f64 - rect.y0 - oy) as f32;
        (lx, ly)
    }

    fn notify_app_text(&mut self) {
        if let Some(id) = self.editors.focused_id() {
            let new_text = self.editors.text_of(id).map(|s| s.to_owned());
            if let (Some(text), Some((_, map))) =
                (new_text, self.input_maps.iter().find(|(k, _)| *k == id))
            {
                self.app.update(map(text));
            }
        }
        self.redraw();
    }
    fn drag_thumb(&mut self, lx: f32, ly: f32) -> bool {
        if let Some((thumb, (spx, spy), scroll)) = self.scroll_drag {
            if let Some(r) = &self.render {
                r.set_cursor(CursorIcon::Default);
                let desired = match thumb.axis {
                    Axis::X => scroll.x + (lx - spx) * thumb.gain,
                    Axis::Y => scroll.y + (ly - spy) * thumb.gain,
                };
                let cur = self.scrolls.get(thumb.id);
                let cur = cur.get(thumb.axis);
                self.scrolls.by(
                    thumb.id,
                    thumb.axis,
                    desired - cur,
                    thumb.viewport,
                    thumb.content,
                );
                r.request_redraw();
            }
            return true;
        }
        false
    }

    fn on_cursor_moved(&mut self, position: PhysicalPosition<f64>) {
        let scale = self.render.as_ref().map_or(1.0, |r| r.scale());
        let PhysicalPosition { x, y } = position;
        let lx = (x / scale) as f32;
        let ly = (y / scale) as f32;
        self.pointer = Some((lx, ly));
        let p = vello::kurbo::Point::new(lx as f64, ly as f64);
        //if its a scroll drag event return after scroll drag processed.
        if self.drag_thumb(lx, ly) {
            return;
        }
        let over_input = self.input_hits.iter().any(|(r, _, _)| r.contains(p));
        // Repaint so hover follows the pointer (only while it's actually moving).
        if let Some(r) = &self.render {
            r.set_cursor(if over_input {
                CursorIcon::Text
            } else {
                CursorIcon::Default
            });
            r.request_redraw();
        }
        if let Some((rect, id, pad)) = self.drag {
            let (lx, ly) = self.local_point(id, rect, pad, lx, ly);
            let text = &mut self.text;
            self.editors.extend_to(id, lx, ly, text);
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
        for &(r, id, content, inner) in self.scroll_hits.iter().rev() {
            if !r.contains(p) {
                continue;
            }
            //chaining scroll
            if rem_h != 0.0 {
                rem_h = self.scrolls.by(id, Axis::X, rem_h, inner.0, content.0);
            }
            if rem_v != 0.0 {
                rem_v = self.scrolls.by(id, Axis::Y, rem_v, inner.1, content.1);
            }
            if rem_h.abs() < 0.5 && rem_v.abs() < 0.5 {
                break;
            }
        }
        self.redraw();
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
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                self.drag = None;
                self.scroll_drag = None;
                self.redraw();
            }
            // Retained: paint on demand. `frame` re-requests only while the app is animating.
            WindowEvent::RedrawRequested => self.frame(),
            WindowEvent::KeyboardInput { event, .. } => {
                let edited = self.editors.on_key(&event, self.modifiers, &mut self.text);
                if edited {
                    self.notify_app_text();
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
            }
            WindowEvent::Ime(ime) => {
                match ime {
                    Ime::Enabled => {}
                    Ime::Preedit(s, cur) => {
                        self.editors.on_ime(&s, cur, &mut self.text);
                        self.redraw();
                    }
                    Ime::Commit(s) => {
                        self.editors.on_ime_commit(&s, &mut self.text);
                        self.notify_app_text();
                    }
                    Ime::Disabled => {
                        self.editors.on_ime_disabled(&mut self.text);
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
        editors: Editors::new(),
        input_hits: Vec::new(),
        text: TextEngine::new(),
        input_maps: Vec::new(),
        drag: None,
        modifiers: ModifiersState::empty(),
        scrolls: Scrolls::new(),
        scroll_hits: Vec::new(),
        scroll_drag: None,
        bar_hits: Vec::new(),
    };
    event_loop.run_app(&mut runner).expect("run app");
}
