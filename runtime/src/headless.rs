//! Drive an app without a window.
//!
//! This is not a second hit-test implementation and not a mock. It builds the same `Runner` the
//! event loop builds, minus the GPU, and calls the same methods winit's `window_event` calls —
//! the only substitution is where the pointer coordinate comes from. What it can't reach is
//! everything below that line: physical→logical scaling, real event ordering, and pixels.
//!
//! Every pointer method paints a frame first, because `hits` is filled by painting and in a
//! window you never receive an event against a frame that hasn't been drawn.

use winit::dpi::PhysicalPosition;
use winit::event::MouseScrollDelta;

use crate::{App, Runner};

/// One offscreen frame, at the 60Hz a window would run at.
const FRAME: f64 = 1.0 / 60.0;
/// How long after the frame a pointer event arrives — roughly a 120Hz mouse's report interval.
/// Without it every event in a gesture would share a timestamp and `dx / dt` would divide by zero.
const POINTER: f64 = 0.008;

pub struct Headless<A: App> {
    runner: Runner<A>,
}

impl<A: App> Headless<A> {
    /// `viewport` is in logical points, and offscreen the scale is 1.0 — so the numbers you pass
    /// to the pointer methods are the same ones an element's rect is measured in.
    pub fn new(app: A, viewport: (f32, f32)) -> Self {
        Self {
            runner: Runner::new(app, Some(viewport)),
        }
    }

    /// Lay out, collect hit regions, and build a scene that is then dropped, then move the clock
    /// on by one frame.
    pub fn frame(&mut self) {
        self.runner.frame();
        self.runner.clock += FRAME;
    }

    /// Move the clock on by hand, for a wait an app is supposed to notice — a debounce, a toast
    /// that dismisses itself. Nothing is drawn: follow it with `frame` to let the app act.
    pub fn advance(&mut self, secs: f64) {
        self.runner.clock += secs;
    }

    /// Move the pointer, firing hover and — while a button is down — drag.
    pub fn move_to(&mut self, x: f32, y: f32) {
        self.frame();
        self.runner.clock += POINTER;
        self.runner
            .on_cursor_moved(PhysicalPosition::new(x as f64, y as f64));
    }

    /// Deliver a logical-pixel wheel delta through the same eligibility path as a window event.
    pub fn wheel(&mut self, x: f32, y: f32, dx: f32, dy: f32) {
        self.move_to(x, y);
        self.runner
            .on_wheel_moved(MouseScrollDelta::PixelDelta(PhysicalPosition::new(
                dx as f64, dy as f64,
            )));
    }

    pub fn press(&mut self) {
        self.frame();
        self.runner.clock += POINTER;
        self.runner.click();
    }

    pub fn release(&mut self) {
        self.frame();
        self.runner.clock += POINTER;
        self.runner.on_cursor_release();
    }

    /// Press and release without travelling: a click, not a drag.
    pub fn click_at(&mut self, x: f32, y: f32) {
        self.move_to(x, y);
        self.press();
        self.release();
    }

    /// A press, `steps` moves, and a release. A drag only begins once the pointer has travelled
    /// past the runtime's slop, so a short drag with few steps fires nothing — which is the
    /// behaviour, not a limitation of this driver.
    pub fn drag(&mut self, from: (f32, f32), to: (f32, f32), steps: usize) {
        self.move_to(from.0, from.1);
        self.press();
        for i in 1..=steps.max(1) {
            let f = i as f32 / steps.max(1) as f32;
            self.move_to(from.0 + (to.0 - from.0) * f, from.1 + (to.1 - from.1) * f);
        }
        self.release();
    }

    pub fn app(&self) -> &A {
        &self.runner.app
    }

    pub fn app_mut(&mut self) -> &mut A {
        &mut self.runner.app
    }
}
