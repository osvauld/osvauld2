//! The M2/M3 spike screen: a card with the Malayalam greeting. Throwaway once login lands; kept as
//! a hand-placed reference for the smooth-AA + Indic-shaping checks.

use vello::kurbo::{Affine, RoundedRect, Stroke};
use vello::peniko::Fill;
use vello::Scene;

use super::{Redraw, Screen};
use crate::text::{TextEngine, MALAYALAM_FAMILY};
use crate::theme;

#[allow(dead_code)] // kept as the hand-placed AA + Indic-shaping reference screen
pub struct DemoScreen;

impl Screen for DemoScreen {
    fn build(
        &mut self,
        scene: &mut Scene,
        text: &mut TextEngine,
        t: Affine,
        _viewport: (f32, f32),
        _now: f64,
    ) -> Redraw {
        let card = RoundedRect::new(80.0, 80.0, 480.0, 360.0, 24.0);
        scene.fill(Fill::NonZero, t, theme::bg_3(), None, &card);
        scene.stroke(&Stroke::new(2.0), t, theme::accent(), None, &card);
        text.draw(
            scene,
            "നമസ്കാരം",
            MALAYALAM_FAMILY,
            44.0,
            t * Affine::translate((130.0, 230.0)),
            theme::fg_1(),
        );
        Redraw::Idle
    }
}
