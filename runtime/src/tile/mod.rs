//! A runtime with no window: one app's layout, hit-testing, paint, state and text, whose frame is
//! handed to whoever shows it (`docs/design/app-threads.md` step 2). The host paints that frame
//! with a [`crate::tile`] element and feeds back what [`crate::El::on_tile`] reports.
//!
//! Nothing here starts a thread. A `Tile` is built on the thread that drives it and never moves:
//! its `El` closures are not `Send`. What crosses is plain data — a [`TileInput`] in, a `Scene` out.

use vello::Scene;
use winit::dpi::PhysicalPosition;
use winit::event::{Ime, KeyEvent, MouseScrollDelta};
use winit::keyboard::ModifiersState;

use crate::{App, Runner};

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
    Ime(Ime),
    Modifiers(ModifiersState),
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

    pub fn resize(&mut self, size: (f32, f32)) {
        if self.runner.offscreen != Some(size) {
            self.runner.offscreen = Some(size);
            self.runner.redraw();
        }
    }

    pub fn input(&mut self, input: TileInput) {
        let r = &mut self.runner;
        match input {
            TileInput::Move(x, y) => r.on_cursor_moved(PhysicalPosition::new(x as f64, y as f64)),
            TileInput::Leave => r.on_cursor_left(),
            TileInput::Button(down) => r.button(down),
            TileInput::RightClick => r.right_click(),
            TileInput::Wheel(dx, dy) => r.on_wheel_moved(MouseScrollDelta::PixelDelta(
                PhysicalPosition::new(dx as f64, dy as f64),
            )),
            TileInput::Key(event) => r.handle_input(event),
            TileInput::Ime(ime) => r.on_ime(ime),
            TileInput::Modifiers(m) => r.modifiers = m,
            TileInput::Blur => r.cancel_keys(),
        }
    }

    /// A message from outside the tile's own input — a wake, a reply.
    pub fn update(&mut self, msg: A::Msg) {
        self.runner.app.update(msg);
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

    pub fn app(&self) -> &A {
        &self.runner.app
    }

    pub fn app_mut(&mut self) -> &mut A {
        &mut self.runner.app
    }
}

#[cfg(test)]
mod tests;
