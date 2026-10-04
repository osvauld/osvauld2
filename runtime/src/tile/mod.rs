//! A runtime with no window: one app's layout, hit-testing, paint, state and text, whose frame is
//! handed to whoever shows it (`docs/design/app-threads.md` step 2). The host paints that frame
//! with a [`crate::tile`] element and feeds back what [`crate::El::on_tile`] reports.
//!
//! Nothing here starts a thread. A `Tile` is built on the thread that drives it and never moves:
//! its `El` closures are not `Send`. What crosses is plain data — a [`TileInput`] in, a `Scene` out.

use std::sync::Arc;

use vello::Scene;
use winit::dpi::PhysicalPosition;
use winit::event::{Ime, KeyEvent, MouseScrollDelta};
use winit::keyboard::ModifiersState;

use crate::{App, ElRect, KeyInput, Runner, SceneView3d};

/// One frame of a tile, as its host shows it: the paint, its 3D viewport (drawn by its own GPU
/// pass, so not in the Scene), and where its elements are — the latter only when asked for,
/// since only a driven (offscreen) host reads it.
#[derive(Clone)]
pub struct TileFrame {
    pub scene: Arc<Scene>,
    pub rects: Arc<[ElRect]>,
    pub view3d: Option<SceneView3d>,
}

impl Default for TileFrame {
    fn default() -> Self {
        Self {
            scene: Arc::new(Scene::new()),
            rects: Arc::new([]),
            view3d: None,
        }
    }
}

/// A [`TileInput`], when it happened on the host's clock — which a tile on a virtual clock
/// adopts, so a driven gesture is timed the same on both sides — and the modifiers held.
#[derive(Clone, Debug)]
pub struct TileEvent {
    pub input: TileInput,
    pub at: f64,
    pub mods: ModifiersState,
}

/// What the host routes to a tile. Coordinates are logical points from the tile's top-left.
#[derive(Clone, Debug)]
pub enum TileInput {
    /// The pointer is over the tile, or outside it while a press that began inside is held.
    Move(f32, f32),
    Leave,
    Button(bool),
    RightClick,
    /// Logical pixels, already scaled.
    Wheel(f32, f32),
    Key(KeyEvent),
    /// A driver's synthetic key (`DriverOp::Keyboard`).
    GameKey(KeyInput),
    Ime(Ime),
    /// The tile lost the keyboard: held keys are cancelled.
    Blur,
}

pub struct Tile<A: App> {
    runner: Runner<A>,
}

impl<A: App> Tile<A> {
    /// `size` in logical points. Time is the OS clock.
    pub fn new(app: A, size: (f32, f32)) -> Self {
        let mut runner = Runner::new(app, Some(size));
        runner.virtual_clock = false;
        runner.redraw();
        Self { runner }
    }

    /// Time is only what [`Self::set_clock`] and input stamps say — for a driven host.
    pub fn with_virtual_clock(mut self) -> Self {
        self.runner.virtual_clock = true;
        self
    }

    /// Move a virtual clock to the host's. Never backwards.
    pub fn set_clock(&mut self, now: f64) {
        if self.runner.virtual_clock && now > self.runner.clock {
            self.runner.clock = now;
        }
    }

    pub fn resize(&mut self, size: (f32, f32)) {
        if self.runner.offscreen != Some(size) {
            self.runner.offscreen = Some(size);
            self.runner.redraw();
        }
    }

    pub fn input(&mut self, TileEvent { input, at, mods }: TileEvent) {
        self.set_clock(at);
        let r = &mut self.runner;
        r.modifiers = mods;
        match input {
            TileInput::Move(x, y) => r.on_cursor_moved(PhysicalPosition::new(x as f64, y as f64)),
            TileInput::Leave => r.on_cursor_left(),
            TileInput::Button(down) => r.button(down),
            TileInput::RightClick => r.right_click(),
            TileInput::Wheel(dx, dy) => r.on_wheel_moved(MouseScrollDelta::PixelDelta(
                PhysicalPosition::new(dx as f64, dy as f64),
            )),
            TileInput::Key(event) => r.handle_input(event),
            TileInput::GameKey(event) => r.on_game_key(event),
            TileInput::Ime(ime) => r.on_ime(ime),
            TileInput::Blur => r.cancel_keys(),
        }
    }

    /// A message from outside the tile's own input — a wake, a reply.
    pub fn update(&mut self, msg: A::Msg) {
        self.runner.app.update(msg);
        self.runner.redraw();
    }

    /// Owe a frame: state changed from outside the tile's own input.
    pub fn invalidate(&mut self) {
        self.runner.redraw();
    }

    /// Something changed since the last frame, or an animation is running.
    pub fn wants_frame(&self) -> bool {
        self.runner.wants_frame.get()
    }

    /// Lay out, hit-test and paint; the scene is the caller's to show.
    pub fn frame(&mut self) -> Scene {
        self.runner.frame();
        std::mem::take(&mut self.runner.scene)
    }

    /// The last frame's 3D viewport, in tile coordinates.
    pub fn view3d(&self) -> Option<SceneView3d> {
        self.runner.view3d.clone()
    }

    /// Where every reachable element is, as of the last frame — tile coordinates.
    pub fn rects(&self) -> Vec<ElRect> {
        self.runner.reachable()
    }

    pub fn app(&self) -> &A {
        &self.runner.app
    }

    pub fn app_mut(&mut self) -> &mut A {
        &mut self.runner.app
    }
}

#[cfg(test)]
mod tests;
