//! A `Screen` is *what* to paint; `Render` is *how*. Each frame the runtime resets the scene and
//! asks the active screen to populate it. New screens are just new `Screen` implementors — this is
//! the in-the-small host-runtime / declared-view split.

use vello::kurbo::Affine;
use vello::Scene;

use crate::text::TextEngine;

mod demo;
mod login;
#[allow(unused_imports)] // kept as the hand-placed AA/Indic reference screen
pub use demo::DemoScreen;
pub use login::LoginScreen;

/// What a screen wants *after* this frame. The runtime is retained: it paints once and then sleeps
/// until an event, unless a screen asks to keep going. This is the one dial behind all animation.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Redraw {
    /// Static — sleep until the next input/resize. Costs zero CPU at idle.
    Idle,
    /// Animating — paint another frame as soon as possible.
    Animating,
}

pub trait Screen {
    /// Populate `scene` (already reset) with this screen's content. `t` maps logical units to
    /// physical pixels (scale × supersample). `viewport` is the window size in logical points — the
    /// canvas to lay out within. `now` is seconds since startup, the clock for time-driven motion.
    /// Returns whether the screen still needs frames (animating) or is settled (idle).
    fn build(
        &mut self,
        scene: &mut Scene,
        text: &mut TextEngine,
        t: Affine,
        viewport: (f32, f32),
        now: f64,
    ) -> Redraw;
}
