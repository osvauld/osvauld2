//! The GUI runtime: owns the winit window + event loop and the wgpu/vello GPU plumbing, lays out and
//! paints an `El` tree, and routes input. An application implements [`App`] (`view` + `update`, the
//! Elm/Iced shape) and calls [`run`]; everything GPU/winit/vello/layout is internal here.
//! `shell2` (Rust screens) and `app_host` (Lua apps) are its two front-ends — one node
//! vocabulary, same pipeline (docs/architecture.md).

mod anim;
pub mod coords;
mod drag;
mod editor;
mod el;
pub mod frame;
mod geometry;
mod headless;
mod hover;
mod id;
mod layout;
mod paint;
mod render;
mod scroll;
mod state;
mod text;
mod zoom;
use crate::anim::{Driver, Spring, Transition};
use crate::coords::{NodePoint, ScreenPoint};
use crate::drag::{DropEvent, DropPhase};
use crate::editor::{Focus, KeepInView};
use crate::el::{Binding, Click};
use crate::geometry::{Clip, Geometry};
use crate::hover::Hovered;
use crate::id::Id;
use crate::layout::PlacedKind;
use crate::state::Slot;
use crate::zoom::Zoom;
use editor::Field;
use scroll::*;
use std::collections::HashSet;
use std::ops::Fn;
use std::sync::Arc;
use std::time::Instant;
use vello::Scene;
use vello::kurbo::{Affine, Insets, Point, Rect};
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
    Action, Anchor, At, El, ElInfo, FrameTick, Placement, PlacementAlign, PlacementSide, col,
    custom, frame, rich, row, text, text_area, text_input,
};
pub use headless::Headless;
pub use hover::{HoverEvent, HoverPhase};
pub use render::{CapturedImage, Render};
use state::Store;
pub use text::{MONO_FAMILY, PIXEL_FAMILY, Run, TextEngine, UI_FAMILY};
pub use vello;

const LINE_STEP: f32 = 30.0;

fn pan_axes((x, y): (f32, f32), (ax, ay): (bool, bool)) -> (f32, f32) {
    (if ax { x } else { 0.0 }, if ay { y } else { 0.0 })
}

/// An application: a tree-of-elements `view` derived from state, plus an `update` that mutates state
/// in response to messages. The runtime calls `view` to paint and `update` when a click hits an
/// element carrying a message. `Msg: Clone` because a laid-out region owns its message.
/// One deferred screenshot. The app supplies only the completion mapping; Runner owns when and
/// how pixels are produced. A request is consumed once, on the next frame after `update`.
pub struct ScreenshotRequest<M> {
    /// Custom logical viewport and physical scale. Both `None` means the live window target.
    pub viewport: Option<(f32, f32)>,
    pub scale: Option<f32>,
    pub complete: Box<dyn FnOnce(Result<CapturedImage, String>) -> M>,
}

pub trait App {
    type Msg: Clone + Send + 'static;

    /// Describe the whole screen as an element tree, given the current state.
    fn view(&self) -> El<Self::Msg>;

    /// Apply a message (e.g. from a click) to the state. The next frame re-derives `view`.
    fn update(&mut self, msg: Self::Msg);

    /// Called once after the window and renderer exist and the event loop is active. Services
    /// that expose an [`EventLoopProxy`] externally must start here, never in the `run_with`
    /// builder where requests can arrive before `run_app` begins polling.
    fn ready(&mut self) {}

    /// Canvas clear color — the page background. Default opaque black.
    fn clear(&self) -> Color {
        Color::from_rgba8(0, 0, 0, 0xFF)
    }
    fn reload(&mut self) {}

    /// Take a pending screenshot request, if any. The default keeps non-automation apps unaware
    /// of capture; implementations must remove the request when returning it.
    fn take_screenshot(&mut self) -> Option<ScreenshotRequest<Self::Msg>> {
        None
    }
}

#[derive(Clone)]
enum Capture {
    App {
        id: Id,
        geometry: Geometry,
        local_press: (f32, f32),
        screen_grab: (f32, f32),
        shape: Option<Grabbed>,
    },
    Thumb {
        thumb: Thumb,
        scroll: Scroll,
        press_point: (f32, f32),
    },
    Text {
        id: Id,
        geometry: Geometry,
        pad: Insets,
    },
    Zoom {
        id: Id,
        rect: Rect,
        content: (f32, f32),
        axes: (bool, bool),
        last: (f32, f32),
        /// Set while the press also armed a click inside this camera: no pan until it travels.
        click_at: Option<(f32, f32)>,
    },
    Pending {
        id: Id,
        geometry: Geometry,
        local_press: (f32, f32),
        screen_grab: (f32, f32),
        shape: Option<Grabbed>,
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
        matches!(
            self,
            Capture::App { .. } | Capture::Thumb { .. } | Capture::Zoom { .. }
        )
    }
}

/// Drives one `App`: holds the GPU `Render`, the live pointer (logical coords), and the last frame's
/// clickable regions for hit-testing.
struct Runner<A: App> {
    app: A,
    render: Option<Render>,
    /// Set only when there is no window: the viewport to lay out against, so `frame` still runs
    /// the whole layout/hit/paint path and fills `hits`. The scene it builds is thrown away.
    offscreen: Option<(f32, f32)>,
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
    pressed: Option<(Geometry, Option<Id>, Click<A::Msg>, Option<Shapes>)>,
    hovered: Hovered,
}

/// A frame an element draws, and the element-local origin it is drawn at — everything a pointer
/// event needs to name the shape beneath it. Only a `ui.frame` has one.
#[derive(Clone)]
struct Shapes {
    frame: Arc<crate::frame::Frame>,
    origin: (f64, f64),
}

impl Shapes {
    fn at(&self, local: NodePoint) -> Option<crate::frame::FrameHit> {
        self.frame
            .hit(Point::new(local.x - self.origin.0, local.y - self.origin.1))
    }
}

/// The shape a drag grabbed: what it was called, how to put a later point into its coordinates,
/// and where its frame sits inside the element. A gesture holds this from press to release.
#[derive(Clone)]
struct Grabbed {
    id: Id,
    into: vello::kurbo::Affine,
    origin: (f64, f64),
}

impl Grabbed {
    fn take(shapes: &Option<Shapes>, local: NodePoint) -> Option<Self> {
        let s = shapes.as_ref()?;
        let hit = s.at(local)?;
        Some(Self {
            id: hit.id,
            into: hit.into,
            origin: s.origin,
        })
    }

    /// The pointer in the grabbed shape's coordinates, wherever it has got to since.
    fn at(&self, local: NodePoint) -> (Id, (f32, f32)) {
        let p = self.into * Point::new(local.x - self.origin.0, local.y - self.origin.1);
        (self.id.clone(), (p.x as f32, p.y as f32))
    }
}

/// The shape under `local` for an element that draws a frame, and `None` for one that doesn't.
fn shape_at(shapes: &Option<Shapes>, local: NodePoint) -> Option<crate::frame::FrameHit> {
    shapes.as_ref().and_then(|s| s.at(local))
}

struct Hits<M> {
    /// The last field is the element's nearest zoomable ancestor.
    click: Vec<(Geometry, Option<Id>, Click<M>, Option<Id>, Option<Shapes>)>,
    input: Vec<(Geometry, Id, Insets)>,
    input_maps: Vec<(Id, Box<dyn Fn(String) -> M>)>,
    context: Vec<(Rect, Box<dyn Fn((f32, f32)) -> M>)>,
    drag: Vec<(Geometry, Id, Box<dyn Fn(DragEvent) -> M>, Option<Shapes>)>,
    drop: Vec<(Geometry, Id, Box<dyn Fn(DropEvent) -> M>)>,
    hover: Vec<(Geometry, Id, Box<dyn Fn(HoverEvent) -> M>, Option<Shapes>)>,
    scroll: Vec<ScrollHit>,
    enter: Vec<(Id, M)>,
    esc: Vec<(Id, M)>,
    bar: Vec<Thumb>,
    zoom: Vec<(Rect, Id, (f32, f32), (bool, bool))>,
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
            hover: Vec::new(),
            scroll: Vec::new(),
            enter: Vec::new(),
            esc: Vec::new(),
            bar: Vec::new(),
            zoom: Vec::new(),
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
            hover,
            scroll,
            enter,
            esc,
            bar,
            zoom,
        } = self;
        click.clear();
        input.clear();
        input_maps.clear();
        context.clear();
        drag.clear();
        drop.clear();
        hover.clear();
        scroll.clear();
        enter.clear();
        esc.clear();
        bar.clear();
        zoom.clear();
    }
}

impl<A: App> Runner<A> {
    fn frame(&mut self) {
        // Windowed, the surface says how big and how sharp. Offscreen, we say, at 1.0 — which
        // makes physical and logical points the same number for anything driving it by hand.
        let (surface, surface_scale, surface_transform) = match (&self.render, self.offscreen) {
            (Some(r), _) => (r.viewport(), r.scale() as f32, r.transform()),
            (None, Some(v)) => (v, 1.0, Affine::IDENTITY),
            (None, None) => return,
        };
        let screenshot = self.app.take_screenshot();
        let mut done_msgs = Vec::new();
        let now = self.start.elapsed().as_secs_f64();
        let dt = self.last_frame.map_or(0.0, |last| (now - last).min(0.1)) as f32;
        self.last_frame = Some(now);
        let custom_capture = screenshot
            .as_ref()
            .is_some_and(|r| r.viewport.is_some() || r.scale.is_some());
        let viewport = screenshot
            .as_ref()
            .and_then(|r| r.viewport)
            .unwrap_or(surface);
        let capture_scale = screenshot
            .as_ref()
            .and_then(|r| r.scale)
            .unwrap_or(surface_scale);
        let t = if custom_capture {
            Affine::scale(capture_scale as f64)
        } else {
            surface_transform
        };
        let clear = self.app.clear();
        self.scene.reset();
        let app = &self.app;
        let pointer = self.pointer;
        let hits = &mut self.hits;
        let store = &mut self.store;
        let focused = &mut self.focused;
        let pressed = self
            .pressed
            .as_ref()
            .map(|(geometry, id, _, _)| (geometry.screen_rect_kurbo(), id.as_ref()));
        let text = &mut self.text;
        let debug = self.debug;
        let mut needs_redraw = false;
        let mut any_in_flight = false;
        let mut placed = layout::solve(app.view(), text, viewport, store);
        let prev_inputs: HashSet<Id> = hits.input_maps.iter().map(|(id, _)| id.clone()).collect();
        hits.clear();
        let mut clips = Vec::new();
        for p in placed.iter_mut() {
            match p.kind {
                PlacedKind::PushClip { rect, transform } => {
                    clips.push(Clip {
                        rect,
                        to_screen: transform,
                    });
                    continue;
                }
                PlacedKind::PopClip => {
                    clips.pop();
                    continue;
                }
                PlacedKind::Node => {}
            }
            let geometry = Geometry::resolve(p.rect, p.transform, clips.iter().copied());
            let visible = geometry.visible_rect_kurbo();
            let over =
                pointer.is_some_and(|(px, py)| geometry.contains(Point::new(px as f64, py as f64)));
            if p.appearance.repaint {
                any_in_flight = true;
            }
            if let Some(axes) = p.behaviour.zoom
                && let Some(id) = &p.id
            {
                store.get_or::<Zoom>(id, Slot::Zoom);
                if let Some(hit_rect) = visible {
                    hits.zoom.push((hit_rect, id.clone(), p.content_size, axes));
                }
            }

            if let Some((_b, _scale)) = &p.behaviour.press_scale
                && let Some(id) = &p.id
            {
                let pressed_id = pressed.and_then(|(_, id)| id);
                if Self::drive_spring(store, dt, pressed_id == Some(id), id) {
                    any_in_flight = true;
                }
            }

            for (b, slot) in p.behaviour.bindings() {
                if let Some(id) = &p.id {
                    let pressed_id = pressed.and_then(|(_, id)| id);
                    let (fl, landed) = Self::drive(b, slot, store, dt, over, pressed_id, id);
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

            if let Some((_id, map)) = p.behaviour.on_frame.take() {
                done_msgs.push(map(FrameTick { dt, elapsed: now }));
                any_in_flight = true;
            }

            // The visual is drawn at the content origin, so that is where its own coordinates
            // start — a padded frame element is offset from its own top-left.
            let shapes = p.appearance.frame.as_ref().map(|frame| Shapes {
                frame: frame.clone(),
                origin: (p.pad.x0, p.pad.y0),
            });
            if let Some(click) = p.behaviour.on_click.take()
                && geometry.visible_rect.is_some()
            {
                hits.click.push((
                    geometry,
                    p.id.clone(),
                    click,
                    p.zoom_parent.clone(),
                    shapes.clone(),
                ));
            }
            if let Some((id, handler)) = p.behaviour.on_drag.take()
                && visible.is_some()
            {
                hits.drag.push((geometry, id, handler, shapes.clone()));
            }

            if let Some((id, handler)) = p.behaviour.on_drop.take()
                && visible.is_some()
            {
                hits.drop.push((geometry, id, handler));
            }
            if let Some((id, handler)) = p.behaviour.on_hover.take()
                && visible.is_some()
            {
                hits.hover.push((geometry, id, handler, shapes));
            }
            if let Some(h) = p.behaviour.on_right_click.take()
                && let Some(hit_rect) = visible
            {
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
                if visible.is_some() {
                    hits.input.push((geometry, id.clone(), p.pad));
                }
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
            if let Some((id, content, (ax, ay))) = scroll_vals
                && let Some(hit_rect) = visible
            {
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
                        hits.bar.push(v.to_screen(p.transform, hit_rect));
                    }
                }
                if ax {
                    if let Some(h) = axis_thumb(p.rect, id, Axis::X, iw, content.0, s.x) {
                        hits.bar.push(h.to_screen(p.transform, hit_rect));
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
        let captured = if self.render.is_none() {
            None // nothing to present to, and a screenshot request just goes unanswered
        } else if custom_capture {
            // This frame ran the normal layout/hit/paint path against the requested viewport,
            // but its pixels never reach the surface. Temporary hit geometry must not accept
            // input before the normal restorative frame requested below.
            let result = self.render.as_mut().unwrap().capture_scene(
                clear,
                &self.scene,
                viewport,
                capture_scale,
            );
            self.hits.clear();
            Some(result)
        } else {
            let render = self.render.as_mut().unwrap();
            let presented = render.present(clear, &self.scene);
            screenshot.as_ref().map(|_| {
                if presented {
                    render.capture()
                } else {
                    // Surface loss must not turn a requested shot into the previous frame.
                    render.capture_scene(clear, &self.scene, viewport, capture_scale)
                }
            })
        };
        let mut dispatched = !done_msgs.is_empty();
        if let (Some(request), Some(image)) = (screenshot, captured) {
            self.app.update((request.complete)(image));
            dispatched = true;
        }
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
        pressed: Option<&Id>,
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
            Driver::Press => (pressed == Some(id)) as u8 as f32,
            Driver::Value(f) => f,
        };
        let tr = store.get_or_with(id, s, || Transition::new(target, b.duration));
        tr.target = target;
        let was = tr.in_flight();
        tr.tick(dt);
        (tr.in_flight(), (was && !tr.in_flight()).then_some(target))
    }

    fn drive_spring(store: &mut Store, dt: f32, pressed: bool, id: &Id) -> bool {
        let target = pressed as u8 as f32;
        let spring = store.get_or_with(id, Slot::PressScale, || Spring::new(target));
        spring.target = target;
        spring.tick(dt);
        spring.in_flight()
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
            .find(|thumb| thumb.hit_rect.contains(p))
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
        if let Some((geometry, id, _handler, shapes)) = self
            .hits
            .drag
            .iter()
            .rev()
            .find(|(geometry, _, _, _)| geometry.contains(p))
        {
            let content = geometry.content_point(ScreenPoint::new(p.x, p.y));
            self.drag = Some(Capture::Pending {
                id: id.clone(),
                geometry: *geometry,
                shape: Grabbed::take(shapes, geometry.node_point(ScreenPoint::new(p.x, p.y))),
                local_press: (content.x as f32, content.y as f32),
                screen_grab: (
                    px - geometry.screen_rect.min_x() as f32,
                    py - geometry.screen_rect.min_y() as f32,
                ),
                press: (px, py),
            });
            self.redraw();
        }
        let hit = self
            .hits
            .input
            .iter()
            .rev()
            .find(|(geometry, _, _)| geometry.contains(p));
        match hit {
            Some((geometry, id, pad)) => {
                self.focused.set(id.clone());
                let (lx, ly) = self.local_point(id, *geometry, *pad, px, py);
                let field = self.store.get_mut::<Field>(id, Slot::Editor);
                if let Some(field) = field {
                    field.click_at(lx, ly, &mut self.text);
                }
                self.drag = Some(Capture::Text {
                    id: id.clone(),
                    geometry: *geometry,
                    pad: *pad,
                });
            }
            None => {
                self.focused.blur();
            }
        }
        let clicked = self
            .hits
            .click
            .iter()
            .rev()
            .find(|(geometry, _, _, _, _)| geometry.contains(p));
        if let Some((geometry, id, click, _, shapes)) = clicked {
            self.pressed = Some((*geometry, id.clone(), click.clone(), shapes.clone()));
        }
        // A click inside a camera may still pan it. A button floating over the canvas is not
        // inside, and dragging off that button must not move the canvas.
        let pans = |camera: &Id| {
            clicked.is_none_or(|(_, id, _, parent, _)| {
                parent.as_ref() == Some(camera) || id.as_ref() == Some(camera)
            })
        };
        if self.drag.is_none()
            && let Some((rect, id, content, axes)) = self
                .hits
                .zoom
                .iter()
                .rev()
                .find(|(r, _, _, _)| r.contains(p))
            && pans(id)
        {
            self.drag = Some(Capture::Zoom {
                id: id.clone(),
                rect: *rect,
                content: *content,
                axes: *axes,
                last: (px, py),
                click_at: clicked.is_some().then_some((px, py)),
            });
        }

        self.redraw();
    }

    /// Fires enter/move/leave against the last frame's hover regions. `in_window` is false when the
    /// pointer has left, so everything leaves at its last position.
    fn hover(&mut self, (px, py): (f32, f32), in_window: bool) {
        let p = Point::new(px as f64, py as f64);
        let hover = &self.hits.hover;
        let phases = self.hovered.step(
            hover
                .iter()
                .map(|(g, id, _, _)| (id, in_window && g.contains(p))),
        );
        let msgs: Vec<_> = hover
            .iter()
            .zip(phases)
            .filter_map(|((geometry, _, handler, shapes), phase)| {
                let local = geometry.node_point(ScreenPoint::new(p.x, p.y));
                let pos = (local.x as f32, local.y as f32);
                Some(handler(HoverEvent {
                    phase: phase?,
                    pos,
                    shape: shape_at(shapes, local),
                }))
            })
            .collect();
        for m in msgs {
            self.app.update(m);
        }
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

    fn local_point(
        &self,
        id: &str,
        geometry: Geometry,
        pad: Insets,
        px: f32,
        py: f32,
    ) -> (f32, f32) {
        let point = geometry.node_point(ScreenPoint::new(px as f64, py as f64));
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

        let (ox, oy) = paint::content_offset(
            geometry.content_rect_kurbo(),
            pad,
            line_h,
            scroll.x,
            scroll.y,
            multiline,
        );
        let lx = (point.x - ox) as f32;
        let ly = (point.y - oy) as f32;
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
        geometry: Geometry,
        local_press: (f32, f32),
        screen_grab: (f32, f32),
        shape: Option<Grabbed>,
    ) {
        let pos = (px, py);
        let mods = self.mods();
        let content = geometry.content_point(ScreenPoint::new(px as f64, py as f64));
        let delta = (
            content.x as f32 - local_press.0,
            content.y as f32 - local_press.1,
        );
        let node = geometry.node_point(ScreenPoint::new(px as f64, py as f64));
        let event = DragEvent {
            at: (node.x as f32, node.y as f32),
            pos,
            delta,
            mods,
            grab: screen_grab,
            scale: geometry.scale(),
            phase: DragPhase::Move,
            shape: shape.as_ref().map(|g| g.at(node)),
        };
        if let Some((_, _, handler, _)) = self
            .hits
            .drag
            .iter()
            .find(|(_, id, _, _)| *id == handle_id.clone())
        {
            self.app.update(handler(event));
            self.redraw();
        }

        if let Some((geometry, _, handler)) = self
            .hits
            .drop
            .iter()
            .rev()
            .find(|(geometry, _, _)| geometry.contains(Point::new(px as f64, py as f64)))
        {
            let local = geometry.node_point(ScreenPoint::new(px as f64, py as f64));
            let drop_event = DropEvent {
                pos: (local.x as f32, local.y as f32),
                mods: self.mods(),
                dragged: handle_id,
                phase: DropPhase::Over,
                size: (
                    geometry.content_rect.width() as f32,
                    geometry.content_rect.height() as f32,
                ),
            };
            self.app.update(handler(drop_event));
        }
    }

    fn on_cursor_release(&mut self) {
        if let Some(cap) = self.drag.take() {
            match cap {
                Capture::App {
                    id,
                    geometry,
                    local_press,
                    screen_grab,
                    shape,
                } => {
                    if let Some((px, py)) = self.pointer {
                        let pos = (px, py);
                        let content =
                            geometry.content_point(ScreenPoint::new(px as f64, py as f64));
                        let delta = (
                            content.x as f32 - local_press.0,
                            content.y as f32 - local_press.1,
                        );

                        if let Some((target, _, handler)) = self
                            .hits
                            .drop
                            .iter()
                            .rev()
                            .find(|(g, _, _)| g.contains(Point::new(px as f64, py as f64)))
                        {
                            let local = target.node_point(ScreenPoint::new(px as f64, py as f64));
                            let drop_event = DropEvent {
                                pos: (local.x as f32, local.y as f32),
                                mods: self.mods(),
                                dragged: id.clone(),
                                phase: DropPhase::Release,
                                size: (
                                    target.content_rect.width() as f32,
                                    target.content_rect.height() as f32,
                                ),
                            };
                            self.app.update(handler(drop_event));
                        }
                        if let Some((_, _, handler, _)) =
                            self.hits.drag.iter().find(|(_, hid, _, _)| *hid == id)
                        {
                            let node = geometry.node_point(ScreenPoint::new(px as f64, py as f64));
                            let event = DragEvent {
                                at: (node.x as f32, node.y as f32),
                                pos,
                                delta,
                                mods: self.mods(),
                                grab: screen_grab,
                                scale: geometry.scale(),
                                phase: DragPhase::End,
                                shape: shape.as_ref().map(|g| g.at(node)),
                            };
                            self.app.update(handler(event));
                        }
                    }
                }
                // scrollbar + text selection have no "End" message — take() already cleared them
                Capture::Thumb { .. } | Capture::Text { .. } | Capture::Zoom { .. } => {}
                Capture::Pending { .. } => {}
            }
        }
        if let Some((geometry, _, click, shapes)) = self.pressed.take()
            && let Some((px, py)) = self.pointer
        {
            let point = Point::new(px as f64, py as f64);
            if geometry.contains(point) {
                let local = geometry.node_point(ScreenPoint::new(point.x, point.y));
                self.app.update(click.fire(At {
                    pos: (local.x as f32, local.y as f32),
                    shape: shape_at(&shapes, local),
                }));
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
                Capture::App {
                    id,
                    geometry,
                    local_press,
                    screen_grab,
                    shape,
                } => self.on_drag_move(
                    lx,
                    ly,
                    id.clone(),
                    *geometry,
                    *local_press,
                    *screen_grab,
                    shape.clone(),
                ),
                Capture::Text { id, geometry, pad } => {
                    let (lx, ly) = self.local_point(id, *geometry, *pad, lx, ly);
                    let text = &mut self.text;
                    let field = self.store.get_mut::<Field>(id, Slot::Editor);
                    if let Some(field) = field {
                        field.extend_to(lx, ly, text);
                    }
                }
                Capture::Zoom {
                    id,
                    rect,
                    content,
                    axes,
                    last,
                    click_at,
                } => {
                    // Same 5pt slop as a drag handle. `last` stays at the press, so no travel is lost.
                    let still_a_click = click_at
                        .is_some_and(|(cx, cy)| (lx - cx).abs() <= 5.0 && (ly - cy).abs() <= 5.0);
                    if !still_a_click {
                        if click_at.is_some() {
                            self.pressed = None;
                        }
                        let delta = pan_axes((lx - last.0, ly - last.1), *axes);
                        self.store
                            .get_or::<Zoom>(id, Slot::Zoom)
                            .pan_by(*rect, *content, delta);
                        self.drag = Some(Capture::Zoom {
                            id: id.clone(),
                            rect: *rect,
                            content: *content,
                            axes: *axes,
                            last: (lx, ly),
                            click_at: None,
                        });
                    }
                }
                Capture::Pending {
                    id,
                    geometry,
                    local_press,
                    screen_grab,
                    shape,
                    press,
                } => {
                    let dx = press.0 - lx;
                    let dy = press.1 - ly;
                    if dx.abs() > 5.0 || dy.abs() > 5.0 {
                        let press_at =
                            geometry.node_point(ScreenPoint::new(press.0 as f64, press.1 as f64));
                        let event = DragEvent {
                            // Where the press landed, not where the slop ended.
                            at: (press_at.x as f32, press_at.y as f32),
                            delta: (0.0, 0.0),
                            phase: DragPhase::Start,
                            pos: (lx, ly),
                            grab: *screen_grab,
                            scale: geometry.scale(),
                            mods: self.mods(),
                            shape: shape.as_ref().map(|g| g.at(press_at)),
                        };
                        self.drag = Some(Capture::App {
                            id: id.clone(),
                            geometry: *geometry,
                            local_press: *local_press,
                            screen_grab: *screen_grab,
                            shape: shape.clone(),
                        });
                        let handler = self
                            .hits
                            .drag
                            .iter()
                            .find(|(_, drag_id, _, _)| drag_id == id)
                            .map(|(_, _, handler, _)| handler);
                        if let Some(handler) = handler {
                            self.app.update(handler(event));
                            self.on_drag_move(
                                lx,
                                ly,
                                id.clone(),
                                *geometry,
                                *local_press,
                                *screen_grab,
                                shape.clone(),
                            );
                        }
                        self.pressed = None;
                    };
                }
            };
        }
        self.hover((lx, ly), true);

        let over_input = self
            .hits
            .input
            .iter()
            .any(|(geometry, _, _)| geometry.contains(p));
        // Repaint so hover follows the pointer (only while it's actually moving).
        if let Some(r) = &self.render {
            r.set_cursor(if self.drag.as_ref().is_some_and(Capture::is_grab) {
                CursorIcon::Grabbing
            } else if self.hits.drag.iter().any(|(r, _, _, _)| r.contains(p)) {
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
        let ctrl_zoom = self.modifiers.control_key();

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
        if ctrl_zoom {
            if let Some((rect, id, content, _axes)) = self
                .hits
                .zoom
                .iter()
                .rev()
                .find(|(r, _, _, _)| r.contains(p))
            {
                self.store.get_or::<Zoom>(id, Slot::Zoom).at(
                    *rect,
                    *content,
                    p,
                    -rem_v / LINE_STEP,
                );
                self.redraw();
            }
            return;
        }
        // The innermost scroller consumes first; only its remainder reaches the viewport camera.
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
        if let Some((rect, id, content, axes)) = self
            .hits
            .zoom
            .iter()
            .rev()
            .find(|(r, _, _, _)| r.contains(p))
        {
            self.store.get_or::<Zoom>(id, Slot::Zoom).pan_by(
                *rect,
                *content,
                pan_axes((-rem_h, -rem_v), *axes),
            );
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
        self.app.ready();
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
                if let Some(at) = self.pointer {
                    self.hover(at, false);
                }
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
    let mut runner = Runner::new(app, None);
    event_loop.run_app(&mut runner).expect("run app");
}

impl<A: App> Runner<A> {
    fn new(app: A, offscreen: Option<(f32, f32)>) -> Self {
        Self {
            app,
            render: None,
            offscreen,
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
            hovered: Hovered::default(),
        }
    }
}

#[cfg(test)]
mod tests;
