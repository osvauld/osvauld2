//! Text engine: registers the fonts we ship and shapes strings into vello glyph runs via parley.
//! This is the reusable "string → positioned glyphs" service every screen/widget calls.

use std::sync::Arc;

use parley::fontique::Blob;
use parley::style::FontFamily;
use parley::{
    Alignment, AlignmentOptions, FontContext, LayoutContext, PositionedLayoutItem, StyleProperty,
};
use vello::kurbo::Affine;
use vello::peniko::{Color, Fill};
use vello::Scene;

/// Fonts embedded in the binary (OFL). We ship fonts — never trust the OS to have a given face,
/// and identical bytes everywhere = identical shaping across machines.
const UI_FONT: &[u8] = include_bytes!("../assets/NotoSans-Regular.ttf");
const MALAYALAM_FONT: &[u8] = include_bytes!("../assets/NotoSansMalayalam-Regular.ttf");
const PIXEL_FONT: &[u8] = include_bytes!("../assets/VT323-Regular.ttf");
const MONO_FONT: &[u8] = include_bytes!("../assets/JetBrainsMono-Regular.ttf");
/// Family names to select after registration (must match each font's internal name).
pub const UI_FAMILY: &str = "Noto Sans";
#[allow(dead_code)] // used by DemoScreen (the reference screen) and incoming Indic UI text
pub const MALAYALAM_FAMILY: &str = "Noto Sans Malayalam";
/// VT323 — the pixel face used for the "sthalam" wordmark.
pub const PIXEL_FAMILY: &str = "VT323";
/// JetBrains Mono — corner tags, short DIDs, quiet links.
pub const MONO_FAMILY: &str = "JetBrains Mono";

/// Owns parley's font collection (`font_cx`) + reusable shaping scratch (`layout_cx`). One per
/// render runtime, created once. Brush type is a throwaway `[u8; 4]` — parley's brush must be
/// `Default`, which `peniko::Color` isn't, and we set the real color at vello draw time anyway.
pub struct TextEngine {
    font_cx: FontContext,
    layout_cx: LayoutContext<[u8; 4]>,
}

impl TextEngine {
    pub fn new() -> Self {
        let mut font_cx = FontContext::new();
        for font in [UI_FONT, MALAYALAM_FONT, PIXEL_FONT, MONO_FONT] {
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

    /// Lay out a single line and return its (width, height) in logical units — for alignment math
    /// (right-aligning, centering) without committing glyphs to a scene.
    pub fn measure(&mut self, text: &str, family: &str, size: f32) -> (f32, f32) {
        let mut builder = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named(family)));
        builder.push_default(StyleProperty::FontSize(size));
        let mut layout = builder.build(text);
        layout.break_all_lines(None);
        (layout.width(), layout.height())
    }

    /// Shape `text` and emit its glyphs into `scene`, mapped by `transform` (its origin is the
    /// text's top-left) and painted `brush`. `transform` already folds in the global scale, so
    /// layout runs in logical units (scale 1.0).
    pub fn draw(
        &mut self,
        scene: &mut Scene,
        text: &str,
        family: &str,
        size: f32,
        transform: Affine,
        brush: Color,
    ) {
        let mut builder = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(FontFamily::named(family)));
        builder.push_default(StyleProperty::FontSize(size));
        let mut layout = builder.build(text);
        layout.break_all_lines(None);
        layout.align(Alignment::Start, AlignmentOptions::default());

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
