//! Osvauld `.doc` editor design tokens, the document **type scale**, and the canonical
//! block-row **geometry**.
//!
//! These mirror the design handoff's `ED` palette, `KIND` type scale, and `G` geometry
//! (the dark, square, hairline block editor). The colour values also echo
//! `sthalam::theme`; they're duplicated here so the editor crate stands alone — once a
//! shared `ui`/`theme` crate exists, this module should re-export from it instead.

use egui::{Color32, FontFamily};

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

// ── Fonts ─────────────────────────────────────────────────────────────────────────
/// The font-family name the editor uses for **bold** runs and for headings (egui cannot
/// synthesise weight — it needs a real bold face). The *consumer* (the shell, the standalone
/// runner) must register a bold proportional face under this exact name; see each crate's
/// font setup. Kept as the single source so the names can't drift.
pub const BOLD_FAMILY: &str = "inter_sb";

/// The bold proportional family (see [`BOLD_FAMILY`]).
pub fn bold_family() -> FontFamily {
    FontFamily::Name(BOLD_FAMILY.into())
}

// Per-kind presentation (the type scale, type-tags, placeholders) now lives in `block.rs`
// as the `BlockSpec` table — the single source for how a kind appears. This module is the
// raw design tokens it draws from.
