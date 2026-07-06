//! Text engine: registers the fonts we ship and shapes strings into vello glyph runs via parley.
//! This is the reusable "string → positioned glyphs" service every screen/widget calls.

use std::sync::Arc;

use parley::fontique::Blob;
use parley::style::FontFamily;
use parley::{
    Alignment, AlignmentOptions, FontContext, Layout, LayoutContext, PositionedLayoutItem,
    StyleProperty,
};
use vello::kurbo::Affine;
use vello::peniko::{Color, Fill};
use vello::Scene;

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

    fn build_layout(&mut self, text: &str, family: &str, size: f32) -> Layout<[u8; 4]> {
        let mut builder = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(resolve_family(family)));
        builder.push_default(StyleProperty::FontSize(size));
        let mut layout = builder.build(text);
        layout.break_all_lines(None);
        layout
    }

    pub fn measure(&mut self, text: &str, family: &str, size: f32) -> (f32, f32) {
        let layout = self.build_layout(text, family, size);
        (layout.width(), layout.height())
    }

    pub fn draw(
        &mut self,
        scene: &mut Scene,
        text: &str,
        family: &str,
        size: f32,
        transform: Affine,
        brush: Color,
    ) {
        let mut layout = self.build_layout(text, family, size);
        layout.align(Alignment::Start, AlignmentOptions::default());
        self.draw_layout(scene, &layout, transform, brush)
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
