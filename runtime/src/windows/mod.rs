//! Apps in their own OS windows (`docs/design/app-threads.md` step 10). The host names them in
//! [`App::windows`](crate::App::windows); each shows one host-supplied [`TileFrame`] filling the
//! window and sends its input back through [`App::window_event`](crate::App::window_event) — a
//! tile slot that is a whole window. The main window stays the Runner's own.
//!
//! Offscreen a window is only a size, reported once: nothing is created or drawn.

use std::sync::Arc;
use std::time::{Duration, Instant};

use vello::peniko::Color;
use vello::Scene;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

use crate::render::Render;
use crate::tile::{TileEvent, TileFrame, TileInput};

/// One window the host wants open, and what it shows now.
pub struct WindowFrame {
    pub key: String,
    pub title: String,
    pub frame: TileFrame,
}

/// What a window tells its host.
#[derive(Debug)]
pub enum WindowIn {
    /// In window coordinates, which are the shown frame's.
    Input(TileEvent),
    /// Logical points. Sent when the window opens, and on every resize.
    Resized((f32, f32)),
    /// The user closed it. It stays open until the host stops naming it.
    Closed,
}

struct Open {
    key: String,
    /// `None` offscreen.
    render: Option<Render>,
    id: Option<WindowId>,
    frame: TileFrame,
    /// The scene last presented, so only a new frame is drawn.
    drawn: Option<Arc<Scene>>,
}

#[derive(Default)]
pub(crate) struct Windows {
    open: Vec<Open>,
}

impl Windows {
    /// Open what `wanted` names and is not open, close what it no longer names, and ask a redraw
    /// where the frame changed. Returns what the host must hear: each new window's size.
    /// Windowed, a window opens only with an `event_loop`; without one it waits for the next call.
    pub(crate) fn sync(
        &mut self,
        wanted: Vec<WindowFrame>,
        event_loop: Option<&ActiveEventLoop>,
        offscreen: Option<(f32, f32)>,
        main: Option<&Render>,
    ) -> Vec<(String, WindowIn)> {
        let t = Instant::now();
        let before = self.open.len();
        self.open.retain(|o| wanted.iter().any(|w| w.key == o.key));
        slow("closing a window", t, before != self.open.len());
        let mut told = Vec::new();
        for w in wanted {
            if let Some(o) = self.open.iter_mut().find(|o| o.key == w.key) {
                if !o.drawn.as_ref().is_some_and(|d| Arc::ptr_eq(d, &w.frame.scene)) {
                    o.frame = w.frame;
                    if let Some(r) = &o.render {
                        r.request_redraw();
                    }
                }
                continue;
            }
            let (render, size) = match (offscreen, event_loop) {
                (Some(size), _) => (None, size),
                (None, Some(el)) => {
                    let t = Instant::now();
                    let attrs = Window::default_attributes().with_title(w.title.as_str());
                    let window = Arc::new(el.create_window(attrs).expect("create window"));
                    let render = match main {
                        Some(main) => main.beside(window),
                        None => pollster::block_on(Render::new(window)),
                    };
                    slow("opening a window", t, true);
                    render.set_ime_allowed(true);
                    render.request_redraw();
                    let size = render.viewport();
                    (Some(render), size)
                }
                (None, None) => continue,
            };
            told.push((w.key.clone(), WindowIn::Resized(size)));
            self.open.push(Open {
                key: w.key,
                id: render.as_ref().and_then(Render::window_id),
                render,
                frame: w.frame,
                drawn: None,
            });
        }
        told
    }

    pub(crate) fn owns(&self, id: WindowId) -> bool {
        self.open.iter().any(|o| o.id == Some(id))
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &str> {
        self.open.iter().map(|o| o.key.as_str())
    }

    /// Present the window's latest frame.
    pub(crate) fn paint(&mut self, id: WindowId, clear: Color) {
        let Some(o) = self.open.iter_mut().find(|o| o.id == Some(id)) else {
            return;
        };
        let Some(render) = o.render.as_mut() else {
            return;
        };
        let mut scene = Scene::new();
        scene.append(&o.frame.scene, Some(render.transform()));
        render.present(clear, &scene, o.frame.view3d.as_ref());
        o.drawn = Some(o.frame.scene.clone());
    }

    /// A window event for one of these windows, as its host hears it — `None` for what only the
    /// window handles. `at` and `mods` stamp input as the Runner stamps a tile's.
    pub(crate) fn event(
        &mut self,
        id: WindowId,
        event: WindowEvent,
        at: f64,
        mods: ModifiersState,
    ) -> Option<(String, WindowIn)> {
        let o = self.open.iter_mut().find(|o| o.id == Some(id))?;
        let render = o.render.as_mut()?;
        let input = match event {
            WindowEvent::CloseRequested => return Some((o.key.clone(), WindowIn::Closed)),
            WindowEvent::Resized(size) => {
                render.resize(size);
                render.request_redraw();
                return Some((o.key.clone(), WindowIn::Resized(render.viewport())));
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                render.set_scale(scale_factor);
                render.request_redraw();
                return Some((o.key.clone(), WindowIn::Resized(render.viewport())));
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = render.scale();
                TileInput::Move((position.x / scale) as f32, (position.y / scale) as f32)
            }
            WindowEvent::CursorLeft { .. } => TileInput::Leave,
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => TileInput::Button(state == ElementState::Pressed),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => TileInput::RightClick,
            WindowEvent::MouseWheel { delta, .. } => match delta {
                MouseScrollDelta::LineDelta(x, y) => {
                    TileInput::Wheel(x * crate::LINE_STEP, y * crate::LINE_STEP)
                }
                MouseScrollDelta::PixelDelta(p) => {
                    let scale = render.scale();
                    TileInput::Wheel((p.x / scale) as f32, (p.y / scale) as f32)
                }
            },
            WindowEvent::KeyboardInput { event, .. } => TileInput::Key(event),
            WindowEvent::Ime(ime) => TileInput::Ime(ime),
            WindowEvent::Focused(false) => TileInput::Blur,
            _ => return None,
        };
        Some((
            o.key.clone(),
            WindowIn::Input(TileEvent { input, at, mods }),
        ))
    }
}

/// Said only when slow: opening and closing are the costs a user waits on.
fn slow(what: &str, since: Instant, happened: bool) {
    let took = since.elapsed();
    if happened && took > Duration::from_millis(100) {
        eprintln!("runtime: {what} took {took:?}");
    }
}

#[cfg(test)]
mod tests;
