//! Text engine: registers the fonts we ship and shapes strings into vello glyph runs via parley.
//! This is the reusable "string → positioned glyphs" service every screen/widget calls.

use std::sync::Arc;

use parley::fontique::Blob;
use parley::style::FontFamily;
use parley::{
    Alignment, AlignmentOptions, FontContext, Layout, LayoutContext, PositionedLayoutItem,
    StyleProperty,
};
use vello::Scene;
use vello::kurbo::Affine;
use vello::peniko::{Color, Fill};

/// The only faces we ship (OFL): non-standard fonts the OS can't be trusted to have, kept for
/// brand/mono determinism. Everything else — the UI sans and every script — comes from the OS.
const PIXEL_FONT: &[u8] = include_bytes!("../assets/VT323-Regular.ttf");
const MONO_FONT: &[u8] = include_bytes!("../assets/JetBrainsMono-Regular.ttf");
/// Default UI face: the OS's sans-serif. A CSS generic, resolved per machine — we ship no UI font.
pub const UI_FAMILY: &str = "sans-serif";
/// VT323 — the pixel face used for the "sthalam" wordmark.
pub const PIXEL_FAMILY: &str = "VT323";
/// JetBrains Mono — corner tags, short DIDs, quiet links.
pub const MONO_FAMILY: &str = "JetBrains Mono";

/// Resolve a family string the CSS way (as a `font-family` source): a generic keyword like
/// `sans-serif`/`monospace` resolves to the OS's generic face, anything else is a literal family
/// name (our shipped VT323/JetBrains, or an OS font). This is parley's own default mechanism — its
/// default family is `Source("sans-serif")`. `named()` would instead force a literal lookup and
/// never honor generics.
pub(crate) fn resolve_family(name: &str) -> FontFamily<'_> {
    FontFamily::from(name)
}

/// Owns parley's font collection (`font_cx`) + reusable shaping scratch (`layout_cx`). One per
/// render runtime, created once. Brush type is a throwaway `[u8; 4]` — parley's brush must be
/// `Default`, which `peniko::Color` isn't, and we set the real color at vello draw time anyway.
pub struct TextEngine {
    font_cx: FontContext,
    layout_cx: LayoutContext<[u8; 4]>,
}

impl TextEngine {
    pub fn new() -> Self {
        // System fonts on: the OS supplies the default sans and every script's fallback. We add
        // only the two brand/mono faces on top.
        let mut font_cx = FontContext::new();
        for font in [PIXEL_FONT, MONO_FONT] {
            font_cx.collection.register_fonts(
                Blob::new(Arc::new(font) as Arc<dyn AsRef<[u8]> + Send + Sync>),
                None,
            );
        }
        Self {
            font_cx,
            layout_cx: LayoutContext::new(),
        }
    }

    /// Lend parley's two contexts together — `PlainEditor::driver`/`refresh_layout` need both, and a
    /// single accessor avoids two simultaneous `&mut self` borrows at the call site.
    pub(crate) fn contexts(&mut self) -> (&mut FontContext, &mut LayoutContext<[u8; 4]>) {
        (&mut self.font_cx, &mut self.layout_cx)
    }

    /// `max_advance` is the line-break constraint: `None` never wraps, which is how every caller
    /// behaved before wrapping existed and is still what an unconstrained measurement means.
    fn build_layout(
        &mut self,
        text: &str,
        family: &str,
        size: f32,
        max_advance: Option<f32>,
    ) -> Layout<[u8; 4]> {
        let mut builder = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(resolve_family(family)));
        builder.push_default(StyleProperty::FontSize(size));
        let mut layout = builder.build(text);
        layout.break_all_lines(max_advance);
        layout
    }

    /// Shape `text` and report `(width, height)` after breaking lines at `max_width`.
    ///
    /// The width returned is what the text *used*, not what it was offered — at `Some(200.0)` a
    /// short string still measures short. That is the honest answer for a leaf's intrinsic size;
    /// filling the offered width is the parent's business, not the text's.
    ///
    /// Trailing whitespace is excluded (`width()`, not `full_width()`), which is both what CSS
    /// does at a line end and what this returned before the parameter existed.
    pub fn measure(
        &mut self,
        text: &str,
        family: &str,
        size: f32,
        max_width: Option<f32>,
    ) -> (f32, f32) {
        let layout = self.build_layout(text, family, size, max_width);
        (layout.width(), layout.height())
    }

    /// The narrowest and widest this string can be: `(min, max)`.
    ///
    /// Parley's own pair, and it lines up exactly with Taffy's `AvailableSpace::MinContent` /
    /// `MaxContent` — the two questions a flex container asks a leaf before it can decide how much
    /// room to give it. `min` takes every soft break, `max` takes none.
    pub fn content_widths(&mut self, text: &str, family: &str, size: f32) -> (f32, f32) {
        let w = self
            .build_layout(text, family, size, None)
            .calculate_content_widths();
        (w.min, w.max)
    }

    /// `max_width` must be the same constraint the layout pass measured with, or the glyphs will
    /// not match the box that was reserved for them: measuring at the parent's width and painting
    /// at `None` reserves a narrow, tall rect and then paints one long line straight out of it.
    pub fn draw(
        &mut self,
        scene: &mut Scene,
        text: &str,
        family: &str,
        size: f32,
        transform: Affine,
        brush: Color,
        max_width: Option<f32>,
    ) {
        let mut layout = self.build_layout(text, family, size, max_width);
        layout.align(Alignment::Start, AlignmentOptions::default());
        self.draw_layout(scene, &layout, transform, brush)
    }

    #[cfg(test)]
    fn lines(&mut self, text: &str, max_width: Option<f32>) -> usize {
        self.build_layout(text, UI_FAMILY, 16.0, max_width).len()
    }

    pub fn draw_layout(
        &self,
        scene: &mut Scene,
        layout: &Layout<[u8; 4]>,
        transform: Affine,
        brush: Color,
    ) {
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                scene
                    .draw_glyphs(run.font())
                    .font_size(run.font_size())
                    .normalized_coords(run.normalized_coords())
                    .transform(transform)
                    .brush(brush)
                    .draw(
                        Fill::NonZero,
                        glyph_run.positioned_glyphs().map(|g| vello::Glyph {
                            id: g.id,
                            x: g.x,
                            y: g.y,
                        }),
                    );
            }
        }
    }
}

/// What parley actually does with a width constraint.
///
/// These pin behaviour the measure hook is about to depend on, and they are deliberately
/// *relational* — no absolute pixel counts. The default family is the OS's sans-serif, so every
/// number here differs between machines; what cannot differ is that wrapping makes text narrower
/// and taller, and that it stops at the longest word.
#[cfg(test)]
mod tests {
    use super::*;

    /// A paragraph, chosen so no single word is long: every width below `max` has somewhere to
    /// break.
    const PARA: &str = "the quick brown fox jumps over the lazy dog and keeps on running";

    fn engine() -> TextEngine {
        TextEngine::new()
    }

    #[test]
    fn wrapping_trades_width_for_height() {
        let mut e = engine();
        let (uw, uh) = e.measure(PARA, UI_FAMILY, 16.0, None);
        let (ww, wh) = e.measure(PARA, UI_FAMILY, 16.0, Some(uw / 4.0));

        assert!(
            ww <= uw / 4.0,
            "wrapped past the constraint: {ww} > {}",
            uw / 4.0
        );
        assert!(wh > uh, "wrapping did not add height: {wh} vs {uh}");
        assert_eq!(e.lines(PARA, None), 1, "unconstrained text broke a line");
        assert!(e.lines(PARA, Some(uw / 4.0)) >= 4);
    }

    /// The pair Taffy asks for. `min` is the answer to `AvailableSpace::MinContent` and `max` to
    /// `MaxContent`, so `max` has to agree with an unconstrained measure — otherwise the measure
    /// hook would report two different sizes for the same question.
    #[test]
    fn content_widths_bracket_the_unconstrained_measure() {
        let mut e = engine();
        let (min, max) = e.content_widths(PARA, UI_FAMILY, 16.0);
        let (unconstrained, _) = e.measure(PARA, UI_FAMILY, 16.0, None);

        assert!(min > 0.0 && min < max, "min {min} max {max}");
        assert!(
            (max - unconstrained).abs() < 0.5,
            "max-content {max} disagrees with an unwrapped measure {unconstrained}"
        );
    }

    /// The one behaviour I could not read off the type signature: parley takes *soft* breaks only,
    /// so a constraint below the longest word is refused rather than breaking mid-word. That is
    /// what makes `min` a real floor — and it means the measure hook must expect a leaf to
    /// overflow rather than assume it always fits.
    #[test]
    fn a_constraint_below_min_content_does_not_break_a_word() {
        let mut e = engine();
        let (min, _) = e.content_widths(PARA, UI_FAMILY, 16.0);
        let (w, _) = e.measure(PARA, UI_FAMILY, 16.0, Some(1.0));

        assert!(
            (w - min).abs() < 0.5,
            "a 1px constraint gave {w}, not the longest word ({min})"
        );
    }

    /// Min-content is the tallest a string gets, and nothing exceeds it — but it is **not** one
    /// word per line. `min` is the width of the *longest* word, so two short neighbours still
    /// share a line when they both fit under it: this paragraph has 13 words and breaks into 12
    /// lines. Worth pinning, because "min-content means one word per line" is the obvious wrong
    /// intuition and it would make a height prediction off by a line.
    #[test]
    fn min_content_is_the_tallest_layout_but_not_one_word_per_line() {
        let mut e = engine();
        let (min, _) = e.content_widths(PARA, UI_FAMILY, 16.0);
        let words = PARA.split_whitespace().count();
        let at_min = e.lines(PARA, Some(min));

        assert!(
            at_min >= e.lines(PARA, Some(min * 2.0)),
            "a narrower constraint produced fewer lines"
        );
        assert!(
            at_min <= words,
            "{at_min} lines from {words} words — a word was broken"
        );
        let (w, h) = e.measure(PARA, UI_FAMILY, 16.0, Some(min));
        assert!(w <= min + 0.5, "a line ran past min-content: {w} > {min}");
        assert!(h >= e.measure(PARA, UI_FAMILY, 16.0, Some(min * 2.0)).1);
    }

    /// An empty string has to measure rather than panic: it is what an unfilled input holds, and
    /// the measure hook will be handed it on the very first frame.
    #[test]
    fn an_empty_string_measures_to_no_width() {
        let mut e = engine();
        let (w, h) = e.measure("", UI_FAMILY, 16.0, Some(100.0));
        assert_eq!(w, 0.0);
        assert!(h > 0.0, "an empty line still has a line height");
    }
}
