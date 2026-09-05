//! Text engine: registers the fonts we ship and shapes strings into vello glyph runs via parley.
//! This is the reusable "string → positioned glyphs" service every screen/widget calls.

use std::ops::Range;
use std::sync::Arc;

use parley::fontique::Blob;
use parley::style::{FontFamily, FontStyle, FontWeight};
use parley::{
    Alignment, AlignmentOptions, FontContext, Layout, LayoutContext, PositionedLayoutItem,
    StyleProperty,
};
use vello::Scene;
use vello::kurbo::{Affine, Rect};
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

/// One styled span of a rich string: a byte range plus the face it is drawn in.
///
/// Runs are the **flattened** form — non-overlapping, in order, covering the string. A document's
/// marks are not: they nest and overlap, and flattening them into runs is the caller's job.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub range: Range<usize>,
    pub family: &'static str,
    pub size: f32,
    /// CSS numeric weight — 400 regular, 700 bold.
    pub weight: f32,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: Color,
}

impl Run {
    /// A plain span in the default UI face. The `with_*` setters build the variants.
    pub fn new(range: Range<usize>, size: f32, color: Color) -> Self {
        Self {
            range,
            family: UI_FAMILY,
            size,
            weight: 400.0,
            italic: false,
            underline: false,
            strike: false,
            color,
        }
    }
    pub fn bold(mut self) -> Self {
        self.weight = 700.0;
        self
    }
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }
    pub fn strike(mut self) -> Self {
        self.strike = true;
        self
    }
    pub fn font(mut self, family: &'static str) -> Self {
        self.family = family;
        self
    }
}

/// Owns parley's font collection (`font_cx`) + reusable shaping scratch (`layout_cx`). One per
/// render runtime, created once. The brush is `[u8; 4]` because parley's brush must be `Default`
/// and `peniko::Color` isn't — but four bytes are an rgba, so a rich run's colour rides it through
/// shaping and `draw_layout` reads it back. A plain draw overrides it with one colour instead.
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

    /// Shape `text` with a style per byte range.
    ///
    /// One string with ranged properties, not one layout per run: line breaking has to see the
    /// whole paragraph. A bold word mid-sentence still wraps with its neighbours, and a run
    /// boundary is not a break opportunity — `un`+`bold`+`ed` stays one word.
    ///
    /// Defaults are pushed first so a byte no run covers still has a face rather than none.
    fn build_rich(
        &mut self,
        text: &str,
        runs: &[Run],
        max_advance: Option<f32>,
    ) -> Layout<[u8; 4]> {
        let mut builder = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(resolve_family(UI_FAMILY)));
        for r in runs {
            let at = r.range.clone();
            builder.push(
                StyleProperty::FontFamily(resolve_family(r.family)),
                at.clone(),
            );
            builder.push(StyleProperty::FontSize(r.size), at.clone());
            builder.push(
                StyleProperty::FontWeight(FontWeight::new(r.weight)),
                at.clone(),
            );
            let slant = if r.italic {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            };
            builder.push(StyleProperty::FontStyle(slant), at.clone());
            // The brush is `[u8; 4]` precisely so it can carry rgba this far; `draw_layout` reads
            // it back per run, which is the only way a string gets more than one colour.
            builder.push(
                StyleProperty::Brush(r.color.to_rgba8().to_u8_array()),
                at.clone(),
            );
            builder.push(StyleProperty::Underline(r.underline), at.clone());
            builder.push(StyleProperty::Strikethrough(r.strike), at);
        }
        let mut layout = builder.build(text);
        layout.break_all_lines(max_advance);
        layout.align(Alignment::Start, AlignmentOptions::default());
        layout
    }

    /// Shape a rich string and report `(width, height)` after breaking lines at `max_width`.
    pub fn measure_rich(&mut self, text: &str, runs: &[Run], max_width: Option<f32>) -> (f32, f32) {
        let layout = self.build_rich(text, runs, max_width);
        (layout.width(), layout.height())
    }

    /// Paint a rich string, each run in its own colour.
    pub fn draw_rich(
        &mut self,
        scene: &mut Scene,
        text: &str,
        runs: &[Run],
        transform: Affine,
        max_width: Option<f32>,
    ) {
        let layout = self.build_rich(text, runs, max_width);
        self.draw_layout(scene, &layout, transform, None);
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
        self.draw_layout(scene, &layout, transform, Some(brush))
    }

    #[cfg(test)]
    fn lines(&mut self, text: &str, max_width: Option<f32>) -> usize {
        self.build_layout(text, UI_FAMILY, 16.0, max_width).len()
    }

    /// `brush` paints every run one colour — `None` uses the colour each run was shaped with,
    /// which is how a rich string gets more than one. An override still wins for decorations, so
    /// a plain layout (whose runs were never given a brush) never paints a transparent underline.
    pub fn draw_layout(
        &self,
        scene: &mut Scene,
        layout: &Layout<[u8; 4]>,
        transform: Affine,
        brush: Option<Color>,
    ) {
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let style = glyph_run.style();
                let color = brush.unwrap_or_else(|| {
                    let [r, g, b, a] = style.brush;
                    Color::from_rgba8(r, g, b, a)
                });
                scene
                    .draw_glyphs(run.font())
                    .font_size(run.font_size())
                    .normalized_coords(run.normalized_coords())
                    .transform(transform)
                    .brush(color)
                    .draw(
                        Fill::NonZero,
                        glyph_run.positioned_glyphs().map(|g| vello::Glyph {
                            id: g.id,
                            x: g.x,
                            y: g.y,
                        }),
                    );

                // Decorations are metrics, not geometry: parley says *whether* a run is underlined
                // and leaves `offset`/`size` as `None` meaning "ask the run's font". Nothing draws
                // them for us, so they are rects. Per *run*, which is why a phrase crossing a font
                // fallback boundary can step: each font carries its own offset and thickness.
                let m = run.metrics();
                let deco = [
                    (&style.underline, m.underline_offset, m.underline_size),
                    (
                        &style.strikethrough,
                        m.strikethrough_offset,
                        m.strikethrough_size,
                    ),
                ];
                for (spec, fallback_offset, fallback_size) in deco {
                    let Some(d) = spec else { continue };
                    let offset = d.offset.unwrap_or(fallback_offset);
                    let size = d.size.unwrap_or(fallback_size);
                    let color = brush.unwrap_or_else(|| {
                        let [r, g, b, a] = d.brush;
                        Color::from_rgba8(r, g, b, a)
                    });
                    // `offset` is measured up from the baseline to the *top* of the rule.
                    let y = (glyph_run.baseline() - offset) as f64;
                    let x = glyph_run.offset() as f64;
                    let rect = Rect::new(x, y, x + glyph_run.advance() as f64, y + size as f64);
                    scene.fill(Fill::NonZero, transform, color, None, &rect);
                }
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

    // ── rich runs ──

    const WHITE: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0xFF);
    const RED: Color = Color::from_rgba8(0xFF, 0x00, 0x00, 0xFF);

    /// One run covering everything, so a rich layout can be compared against a plain one.
    fn uniform(text: &str, size: f32) -> Vec<Run> {
        vec![Run::new(0..text.len(), size, WHITE)]
    }

    /// `(baseline, font size)` for every glyph run on the first line.
    fn first_line_runs(e: &mut TextEngine, text: &str, runs: &[Run]) -> Vec<(f32, f32)> {
        let layout = e.build_rich(text, runs, None);
        let line = layout.lines().next().expect("no lines");
        line.items()
            .filter_map(|i| match i {
                PositionedLayoutItem::GlyphRun(g) => Some((g.baseline(), g.run().font_size())),
                _ => None,
            })
            .collect()
    }

    /// A rich layout with one uniform run has to agree with the plain path, or the two shaping
    /// routes have quietly diverged and every comparison below means nothing.
    #[test]
    fn one_uniform_run_measures_the_same_as_plain_text() {
        let mut e = engine();
        let (pw, ph) = e.measure(PARA, UI_FAMILY, 16.0, None);
        let (rw, rh) = e.measure_rich(PARA, &uniform(PARA, 16.0), None);

        assert!((rw - pw).abs() < 0.5, "width {rw} vs {pw}");
        assert!((rh - ph).abs() < 0.5, "height {rh} vs {ph}");
    }

    /// The line is as tall as its tallest run. This is the first thing rich text breaks that the
    /// plain leaf could assume away: one string no longer implies one line height.
    #[test]
    fn a_line_is_as_tall_as_its_largest_run() {
        let mut e = engine();
        let s = "small BIG";
        let mixed = vec![
            Run::new(0..6, 12.0, WHITE),
            Run::new(6..s.len(), 32.0, WHITE),
        ];

        let (w_mixed, h_mixed) = e.measure_rich(s, &mixed, None);
        let (w_small, h_small) = e.measure_rich(s, &uniform(s, 12.0), None);
        let (w_big, h_big) = e.measure_rich(s, &uniform(s, 32.0), None);

        // The width is what proves only *part* of the string got the larger size. Without it the
        // test cannot tell a mixed line from a uniformly large one, and a `push_default` in place
        // of a ranged `push` sails straight through — the last default silently wins everywhere.
        assert!(
            w_mixed > w_small && w_mixed < w_big,
            "width {w_mixed} is not between {w_small} and {w_big} — the runs are not mixed"
        );
        assert!(h_mixed > h_small, "mixed {h_mixed} vs all-small {h_small}");
        assert!(
            (h_mixed - h_big).abs() < 1.0,
            "mixed {h_mixed} vs all-big {h_big}"
        );
    }

    /// Runs on one line share one baseline — it is a property of the line, not of the run. Worth
    /// pinning because the obvious fear (every face sitting on its own baseline) would make mixed
    /// styling unusable, and because the known fallback *dip* is a different thing: it moves the
    /// whole line, which is what the next test measures.
    #[test]
    fn every_run_on_a_line_shares_one_baseline() {
        let mut e = engine();
        let s = "regular mono BIG";
        let runs = vec![
            Run::new(0..8, 16.0, WHITE),
            Run::new(8..13, 16.0, WHITE).font(MONO_FAMILY),
            Run::new(13..s.len(), 30.0, WHITE),
        ];

        let seen = first_line_runs(&mut e, s, &runs);
        assert!(seen.len() >= 2, "expected several runs, got {seen:?}");
        let first = seen[0].0;
        assert!(
            seen.iter().all(|(b, _)| (b - first).abs() < 0.01),
            "runs disagreed about the baseline: {seen:?}"
        );
    }

    /// A run boundary is not a line-break opportunity: `un`+`bold`+`ed` is still one word. If it
    /// were, styling a word would silently let it wrap mid-word, which no editor does.
    #[test]
    fn a_run_boundary_is_not_a_break_opportunity() {
        let mut e = engine();
        let s = "unbolded";
        let split = vec![
            Run::new(0..2, 16.0, WHITE),
            Run::new(2..6, 16.0, WHITE).bold(),
            Run::new(6..s.len(), 16.0, WHITE),
        ];

        let (w, _) = e.measure_rich(s, &split, Some(1.0));
        let (whole, _) = e.measure_rich(s, &split, None);
        assert!(
            (w - whole).abs() < 0.5,
            "a 1px constraint split the word: {w} vs {whole}"
        );
    }

    /// Colour rides the brush all the way into the built layout, which is what `draw_layout` reads
    /// back per run. Nothing else in this crate would notice if it were dropped.
    #[test]
    fn each_run_keeps_its_own_colour() {
        let mut e = engine();
        let s = "white red";
        let runs = vec![Run::new(0..6, 16.0, WHITE), Run::new(6..s.len(), 16.0, RED)];

        let layout = e.build_rich(s, &runs, None);
        let brushes: Vec<[u8; 4]> = layout
            .lines()
            .flat_map(|l| l.items())
            .filter_map(|i| match i {
                PositionedLayoutItem::GlyphRun(g) => Some(g.style().brush),
                _ => None,
            })
            .collect();

        assert!(
            brushes.contains(&[0xFF, 0xFF, 0xFF, 0xFF]) && brushes.contains(&[0xFF, 0, 0, 0xFF]),
            "expected both colours, got {brushes:?}"
        );
    }

    /// Family changes a line's height as surely as size does, and mixing takes the max.
    ///
    /// Measured on this machine at 16pt: the OS sans is 21.792 tall, JetBrains Mono 21.120,
    /// VT323 16.000. **So a paragraph whose second line happens to contain a mono word is 0.7pt
    /// taller than its first**, and the step moves as you edit. That is the residual of the
    /// fallback baseline problem, in the one place rich text puts it: prose that wants even
    /// leading has to push an explicit `LineHeight` rather than let the faces decide.
    ///
    /// Asserted as `mixed == max(a, b)` rather than against those numbers — and between the two
    /// faces we *ship*, so the OS's `sans-serif` cannot make it flaky. The width check is what
    /// stops the test passing when the family push is dropped entirely: ignoring it leaves one
    /// face, whose height already equals the max.
    #[test]
    fn a_lines_height_is_the_max_over_its_faces() {
        let mut e = engine();
        let s = "mono pixel";
        let all = |fam| vec![Run::new(0..s.len(), 16.0, WHITE).font(fam)];
        let mixed = vec![
            Run::new(0..5, 16.0, WHITE).font(MONO_FAMILY),
            Run::new(5..s.len(), 16.0, WHITE).font(PIXEL_FAMILY),
        ];

        let (w_mixed, h_mixed) = e.measure_rich(s, &mixed, None);
        let (w_mono, h_mono) = e.measure_rich(s, &all(MONO_FAMILY), None);
        let (w_pixel, h_pixel) = e.measure_rich(s, &all(PIXEL_FAMILY), None);

        assert!(
            (h_mono - h_pixel).abs() > 0.1,
            "the two faces are the same height ({h_mono}); this proves nothing"
        );
        assert!(
            w_mixed > w_pixel.min(w_mono) && w_mixed < w_pixel.max(w_mono),
            "width {w_mixed} is not between {w_pixel} and {w_mono} — one face was ignored"
        );
        assert!(
            (h_mixed - h_mono.max(h_pixel)).abs() < 0.01,
            "mixed {h_mixed} is not max({h_mono}, {h_pixel})"
        );
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
