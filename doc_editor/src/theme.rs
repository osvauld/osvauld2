//! Osvauld `.doc` editor design tokens, type scale, and canonical block-row geometry. Colour
//! values echo `sthalam::theme`, duplicated so the editor crate stands alone — once a shared
//! `ui`/`theme` crate exists, this should re-export from it instead.

use egui::Color32;

// ── Surfaces ──────────────────────────────────────────────────────────────────────
pub const BG_PAGE: Color32 = Color32::from_rgb(0x0A, 0x0B, 0x10); // doc surface
pub const BG_2: Color32 = Color32::from_rgb(0x14, 0x15, 0x1C); // raised — popovers/menus
pub const BG_3: Color32 = Color32::from_rgb(0x1C, 0x1D, 0x27); // elevated — chips, hover

// ── Ink ───────────────────────────────────────────────────────────────────────────
pub const FG_1: Color32 = Color32::from_rgb(0xF5, 0xF5, 0xF7); // primary text
pub const FG_2: Color32 = Color32::from_rgb(0xB6, 0xB7, 0xC3); // secondary — quote, code
pub const MUTED: Color32 = Color32::from_rgb(0x7F, 0x81, 0x92); // mono labels
pub const FAINT: Color32 = Color32::from_rgb(0x4D, 0x4E, 0x5C); // placeholders, idle gutter

// ── Hairlines (white over the dark page) ────────────────────────────────────────────
pub const HAIR: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 15); // .06
pub const BD: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 31); // .12
pub const BD_HI: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 56); // .22
pub const HOVER_BG: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 6); // .025
pub const CODE_BG: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 8); // .03
pub const GUTTER_HOVER: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 20); // .08

// ── Accent (single purple family) ───────────────────────────────────────────────────
pub const ACCENT: Color32 = Color32::from_rgb(0x8A, 0x86, 0xE5); // caret, spine-focus, rules
pub const ACCENT_HI: Color32 = Color32::from_rgb(0xA0, 0x9D, 0xEE); // link text
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(0xCB, 0xA6, 0xF7); // inline code text
pub const ACCENT_BG: Color32 = Color32::from_rgba_unmultiplied_const(0x8A, 0x86, 0xE5, 36); // .14
/// Background fill behind an inline-code mark (white over the dark page, ~.05).
pub const CODE_INLINE_BG: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 13);
pub const SEL_BG: Color32 = Color32::from_rgba_unmultiplied_const(0x8A, 0x86, 0xE5, 71); // .28
/// Active-branch indent guide — the deepest guide of the caret's branch.
pub const GUIDE_ACTIVE: Color32 = Color32::from_rgba_unmultiplied_const(0x8A, 0x86, 0xE5, 56); // .22

// ── Geometry (canonical px — the `G` block from the handoff) ─────────────────────────
/// Top padding before the first block.
pub const PAD_TOP: f32 = 28.0;
/// Left inset of a block's own box from the doc surface (`margin` + 8). The focus
/// type-tag lives in the `MARGIN` to the left of this.
pub const OUTER_LEFT: f32 = 36.0;
/// Width of the outer margin that holds the focus type-tag.
pub const MARGIN: f32 = 28.0;
/// The affordance gutter (`+` and `⋮⋮`), left of the spine.
pub const GUTTER: f32 = 40.0;
/// The persistent 1px spine at the text-column left.
pub const SPINE: f32 = 1.0;
/// Spine → text gap.
pub const CONTENT_PAD: f32 = 12.0;
/// Per nesting level.
pub const INDENT: f32 = 24.0;
/// Right inset of the text column.
pub const RIGHT_PAD: f32 = 28.0;
/// Bottom padding after the last block.
pub const PAD_BOTTOM: f32 = 40.0;
/// The longest a line runs before wrapping — a comfortable long-form reading measure.
pub const MAX_CONTENT: f32 = 760.0;

// ── Code syntax palette ─────────────────────────────────────────────────────────────
// Code-block highlight colours, tuned for the dark page; mapped from the engine-neutral
// `code_highlight::HlKind` so the highlighter stays palette-free. The PDF export runs these
// through its luminance-inverting `ink()`, so no separate print palette is needed.
pub const CODE_KW: Color32 = Color32::from_rgb(0xC4, 0xA7, 0xF7); // keyword — violet
pub const CODE_FN: Color32 = Color32::from_rgb(0x82, 0xAA, 0xFF); // function — blue
pub const CODE_TY: Color32 = Color32::from_rgb(0x7F, 0xD1, 0xC0); // type — teal
pub const CODE_STR: Color32 = Color32::from_rgb(0x9E, 0xCE, 0x6A); // string — green
pub const CODE_NUM: Color32 = Color32::from_rgb(0xFF, 0x9E, 0x64); // number — orange
pub const CODE_CONST: Color32 = Color32::from_rgb(0xFF, 0xCB, 0x6B); // constant — amber
pub const CODE_COMMENT: Color32 = Color32::from_rgb(0x6B, 0x6D, 0x7E); // comment — muted
pub const CODE_PROP: Color32 = Color32::from_rgb(0x89, 0xDD, 0xFF); // member — cyan
pub const CODE_OP: Color32 = Color32::from_rgb(0xC0, 0xCA, 0xF5); // operator — soft blue
pub const CODE_TAG: Color32 = Color32::from_rgb(0xF7, 0x76, 0x8E); // tag/escape — coral

/// Map a semantic highlight kind to its on-screen colour. `Variable`/`Text` stay the code
/// block's base ink (`FG_2`); `Punctuation` is muted so structure recedes behind tokens.
pub fn code_color(kind: code_highlight::HlKind) -> Color32 {
    use code_highlight::HlKind::*;
    match kind {
        Keyword => CODE_KW,
        Function => CODE_FN,
        Type => CODE_TY,
        Constant => CODE_CONST,
        Number => CODE_NUM,
        String => CODE_STR,
        Comment => CODE_COMMENT,
        Property => CODE_PROP,
        Operator => CODE_OP,
        Attribute => CODE_CONST,
        Tag => CODE_TAG,
        Escape => CODE_TAG,
        Variable | Text => FG_2,
        Punctuation => MUTED,
    }
}

// ── Fonts ─────────────────────────────────────────────────────────────────────────
/// The font-family name for bold runs and headings (egui can't synthesise weight — it needs a
/// real bold face). The consumer must register a bold proportional face under this exact name.
/// Re-exported from [`rich_text`] so the native editor and the Lua-app renderer share one source
/// of truth — the family names can't drift apart. The actual face is installed by
/// [`rich_text::install_fonts`]; the bold-or-fallback resolver is [`rich_text::bold_or_fallback`].
pub use rich_text::BOLD_FAMILY;

// Per-kind presentation (type scale, type-tags, placeholders) lives in `block.rs` as the
// `BlockSpec` table; this module is the raw design tokens it draws from.
