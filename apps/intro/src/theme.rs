//! The app's own theming — lives *in* the app (these are self-contained,
//! uploadable wasm apps, so each carries its look baked in; no shared crate, no
//! host channel). The house dark/lavender/square palette + fonts, ported from
//! `sthalam::theme` so the app reads like the rest of osvauld.

use std::collections::BTreeMap;
use std::sync::Arc;

use osvauld_app::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke};

// ── Surfaces ────────────────────────────────────────────────────────────────
pub const BG: Color32 = Color32::from_rgb(0x0A, 0x0B, 0x10);
pub const SURFACE: Color32 = Color32::from_rgb(0x14, 0x15, 0x1C);
pub const RAISED: Color32 = Color32::from_rgb(0x1C, 0x1D, 0x27);

// ── Ink ─────────────────────────────────────────────────────────────────────
pub const FG: Color32 = Color32::from_rgb(0xF5, 0xF5, 0xF7);
pub const FG_2: Color32 = Color32::from_rgb(0xB6, 0xB7, 0xC3);
pub const MUTED: Color32 = Color32::from_rgb(0x7F, 0x81, 0x92);

// ── Lines & accent (single purple family) ────────────────────────────────────
pub const BORDER: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 31); // ~.12
pub const ACCENT: Color32 = Color32::from_rgb(0x8A, 0x86, 0xE5);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0xA0, 0x9D, 0xEE);
pub const ACCENT_PRESS: Color32 = Color32::from_rgb(0x6E, 0x6A, 0xD0);
pub const VERIFIED: Color32 = Color32::from_rgb(0x7E, 0xE7, 0x87);

/// The VT323 pixel face — the wordmark.
pub fn pixel() -> FontFamily {
    FontFamily::Name("pixel".into())
}

/// JetBrains Mono SemiBold — buttons / CTA.
pub fn mono_sb() -> FontFamily {
    FontFamily::Name("mono_sb".into())
}

/// Install the look on this app's egui context. Call once (set_fonts is dear).
pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    install_style(ctx);
    // egui pixel-snaps rects + line-segments to the grid by default (for crisp
    // static UI), which makes *moving* ones — the animated backdrop's dots and
    // links — step a whole pixel at a time = judder. Turn that off so they glide.
    // Text stays snapped (it never moves and crispness matters); the only cost is
    // static 1px borders being a touch softer.
    ctx.tessellation_options_mut(|t| {
        t.round_rects_to_pixels = false;
        t.round_line_segments_to_pixels = false;
    });
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

    // Prepend ours so egui's bundled faces stay as glyph fallback (✓, ▸, …).
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "inter".to_owned());
    fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "jbmono".to_owned());
    fonts.families.insert(FontFamily::Name("pixel".into()), vec!["vt323".to_owned()]);

    let mut sb = vec!["jbmono_sb".to_owned()];
    sb.extend(fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default());
    fonts.families.insert(FontFamily::Name("mono_sb".into()), sb);

    ctx.set_fonts(fonts);
}

fn install_style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = SURFACE;
    v.override_text_color = Some(FG);
    v.extreme_bg_color = RAISED; // TextEdit background
    v.hyperlink_color = ACCENT;
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(0x8A, 0x86, 0xE5, 64);
    v.selection.stroke = Stroke::new(1.0, ACCENT);

    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.inactive.bg_fill = RAISED;
    v.widgets.inactive.weak_bg_fill = RAISED;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.active.bg_fill = ACCENT;
    v.widgets.active.weak_bg_fill = ACCENT;
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);

    // Square corners are the signature.
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(0);
    }

    ctx.global_style_mut(|style| {
        style.visuals = v;
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(14.0, 9.0);
        style.text_styles = text_styles();
    });
}

fn text_styles() -> BTreeMap<egui::TextStyle, FontId> {
    use egui::TextStyle::*;
    [
        (Heading, FontId::new(24.0, FontFamily::Proportional)),
        (Body, FontId::new(15.5, FontFamily::Proportional)),
        (Button, FontId::new(14.0, FontFamily::Proportional)),
        (Small, FontId::new(12.0, FontFamily::Monospace)),
        (Monospace, FontId::new(13.0, FontFamily::Monospace)),
    ]
    .into()
}
