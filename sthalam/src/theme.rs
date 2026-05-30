use std::collections::BTreeMap;
use std::sync::Arc;

use eframe::egui::{self, Color32, FontFamily, FontId};

// Sthalam design tokens, ported from the design system's sthalam-theme.css.
// Swap ACCENT to rebrand. Grouped as the CSS file groups them.

// Surfaces
pub const BG_PAGE: Color32 = Color32::from_rgb(0x0A, 0x0B, 0x10); // window canvas
pub const BG_1: Color32 = Color32::from_rgb(0x0D, 0x0E, 0x13); // fields
pub const BG_2: Color32 = Color32::from_rgb(0x14, 0x15, 0x1C); // raised / cards
pub const BG_3: Color32 = Color32::from_rgb(0x1C, 0x1D, 0x27); // elevated
pub const BG_4: Color32 = Color32::from_rgb(0x26, 0x27, 0x35); // interactive

// Foreground
pub const FG_1: Color32 = Color32::from_rgb(0xF5, 0xF5, 0xF7);
pub const FG_3: Color32 = Color32::from_rgb(0x7F, 0x81, 0x92);
pub const FG_4: Color32 = Color32::from_rgb(0x4D, 0x4E, 0x5C);

// Border (white at low alpha, as in the CSS)
pub const BD_1: Color32 = Color32::from_rgba_premultiplied(15, 15, 15, 15); // ~0.06, hairline
pub const BD_2: Color32 = Color32::from_rgba_premultiplied(31, 31, 31, 31); // ~0.12

// Accent (single purple family)
pub const ACCENT: Color32 = Color32::from_rgb(0x8A, 0x86, 0xE5);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0xA0, 0x9D, 0xEE);
pub const ACCENT_PRESS: Color32 = Color32::from_rgb(0x6E, 0x6A, 0xD0);
pub const ACCENT_BG: Color32 = Color32::from_rgba_premultiplied(19, 18, 32, 36); // ~0.14, selected row / chip

// Semantic
pub const OK: Color32 = Color32::from_rgb(0x7E, 0xE7, 0x87);
pub const WARN: Color32 = Color32::from_rgb(0xF5, 0xC0, 0x6A);
pub const WARN_BG: Color32 = Color32::from_rgba_premultiplied(30, 23, 13, 31); // ~0.12 amber
pub const ERR: Color32 = Color32::from_rgb(0xF4, 0x70, 0x68);
pub const ERR_BG: Color32 = Color32::from_rgba_premultiplied(29, 13, 12, 31); // ~0.12, error field ring

// Alias for the home screen, which still uses the placeholder card layout.
pub const CARD: Color32 = BG_2;

// VT323 pixel face — the wordmark.
pub fn pixel() -> FontFamily {
    FontFamily::Name("pixel".into())
}

// JetBrains Mono SemiBold — the call-to-action button.
pub fn mono_sb() -> FontFamily {
    FontFamily::Name("mono_sb".into())
}

pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    install_style(ctx);
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(egui::FontData::from_static(bytes)));
    };
    add("inter", include_bytes!("../assets/fonts/Inter-Regular.ttf"));
    add("jbmono", include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"));
    add("jbmono_sb", include_bytes!("../assets/fonts/JetBrainsMono-SemiBold.ttf"));
    add("vt323", include_bytes!("../assets/fonts/VT323-Regular.ttf"));

    // Prepend ours so egui's bundled faces stay as glyph fallback (✓, ▸, emoji).
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "inter".to_owned());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "jbmono".to_owned());
    fonts
        .families
        .insert(FontFamily::Name("pixel".into()), vec!["vt323".to_owned()]);

    // SemiBold first, then the full monospace chain (incl. egui's bundled
    // faces) so symbols like ▸ always resolve to *something*.
    let mut sb = vec!["jbmono_sb".to_owned()];
    sb.extend(fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default());
    fonts.families.insert(FontFamily::Name("mono_sb".into()), sb);

    ctx.set_fonts(fonts);
}

fn install_style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG_PAGE;
    v.window_fill = BG_2;
    v.window_stroke = egui::Stroke::new(1.0, BD_2);
    v.override_text_color = Some(FG_1);
    v.extreme_bg_color = BG_1; // TextEdit background
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(0x8A, 0x86, 0xE5, 64);
    v.selection.stroke = egui::Stroke::new(1.0, ACCENT);

    v.widgets.inactive.bg_fill = BG_3;
    v.widgets.inactive.weak_bg_fill = BG_3;
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BD_2);
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, FG_1);

    // Both hover and focus read as the accent border on fields/buttons.
    v.widgets.hovered.bg_fill = BG_4;
    v.widgets.hovered.weak_bg_fill = BG_4;
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, FG_1);

    v.widgets.active.bg_fill = ACCENT;
    v.widgets.active.weak_bg_fill = ACCENT;
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    v.widgets.active.fg_stroke = egui::Stroke::new(1.0, Color32::WHITE);

    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BD_2);
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, FG_1);

    // Square corners are the design's signature (matches VT323 + the pixel-offset button).
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = egui::CornerRadius::same(0);
    }

    ctx.global_style_mut(|style| {
        style.visuals = v;
        style.spacing.item_spacing = egui::vec2(10.0, 12.0);
        style.spacing.button_padding = egui::vec2(14.0, 9.0);
        style.spacing.interact_size.y = 38.0;
        style.text_styles = text_styles();
    });
}

fn text_styles() -> BTreeMap<egui::TextStyle, FontId> {
    use egui::TextStyle::*;
    [
        (Heading, FontId::new(28.0, FontFamily::Proportional)),
        (Body, FontId::new(15.0, FontFamily::Proportional)),
        (Button, FontId::new(14.0, FontFamily::Proportional)),
        (Small, FontId::new(12.0, FontFamily::Proportional)),
        (Monospace, FontId::new(13.0, FontFamily::Monospace)),
    ]
    .into()
}
