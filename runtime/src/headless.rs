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

use crate::{App, DriverOp, ElRect, POINTER, Runner};

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
        self.runner.tick();
    }

    /// Move the clock on by hand, for a wait an app is supposed to notice — a debounce, a toast
    /// that dismisses itself. Nothing is drawn: follow it with `frame` to let the app act.
    ///
    /// The bridge's `Advance` op paints instead of making the caller do it. The difference is
    /// deliberate — in a Rust test the two steps are usually wanted apart, so the app can be
    /// inspected at the instant before it reacts.
    pub fn advance(&mut self, secs: f64) {
        self.runner.clock += secs;
    }

    /// Where every reachable element is — the same readback the bridge's `Rects` op answers with,
    /// and for the same reason: a coordinate guessed from the source is the one thing a layout
    /// test cannot check, because a wrong guess and a broken layout both look like "nothing
    /// happened". Paints first, since hit regions are built by painting.
    pub fn rects(&mut self) -> Vec<ElRect> {
        self.runner
            .run_driver(DriverOp::Rects)
            .expect("Headless is always offscreen")
            .rects
            .expect("the rects op reports rects")
    }

    /// Move the pointer, firing hover and — while a button is down — drag.
    pub fn move_to(&mut self, x: f32, y: f32) {
        self.frame();
        self.runner.clock += POINTER;
        self.runner
            .on_cursor_moved(PhysicalPosition::new(x as f64, y as f64));
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
