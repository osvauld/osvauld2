//! Visual constants, ported from sthalam's design tokens (sthalam-theme.css → sthalam/src/theme.rs).
//! Functions (not consts) only because `Color::from_rgba8` isn't proven const here yet; swap to
//! `const` once confirmed. Borders are white-at-low-alpha, matching the CSS. Swap `accent` to rebrand.

use vello::peniko::Color;

// ── Surfaces ────────────────────────────────────────────────────────────
/// Page background — vello clears the target to this before drawing.
pub fn bg_page() -> Color {
    Color::from_rgba8(0x0A, 0x0B, 0x10, 0xFF)
}
/// Fields / inset.
#[allow(dead_code)] // Stage B (panel/fields)
pub fn bg_1() -> Color {
    Color::from_rgba8(0x0D, 0x0E, 0x13, 0xFF)
}
/// Raised / cards.
#[allow(dead_code)] // Stage B (panel)
pub fn bg_2() -> Color {
    Color::from_rgba8(0x14, 0x15, 0x1C, 0xFF)
}
/// Elevated.
#[allow(dead_code)] // used by demo.rs (reference screen)
pub fn bg_3() -> Color {
    Color::from_rgba8(0x1C, 0x1D, 0x27, 0xFF)
}

// ── Foreground ──────────────────────────────────────────────────────────
pub fn fg_1() -> Color {
    Color::from_rgba8(0xF5, 0xF5, 0xF7, 0xFF)
}
#[allow(dead_code)]
pub fn fg_2() -> Color {
    Color::from_rgba8(0xB6, 0xB7, 0xC3, 0xFF)
}
pub fn fg_3() -> Color {
    Color::from_rgba8(0x7F, 0x81, 0x92, 0xFF)
}
pub fn fg_4() -> Color {
    Color::from_rgba8(0x4D, 0x4E, 0x5C, 0xFF)
}

// ── Borders (white at low alpha, as in the CSS) ─────────────────────────
/// Hairline (~0.06).
pub fn bd_1() -> Color {
    Color::from_rgba8(0xFF, 0xFF, 0xFF, 0x0F)
}
/// ~0.12.
#[allow(dead_code)] // Stage B (panel border)
pub fn bd_2() -> Color {
    Color::from_rgba8(0xFF, 0xFF, 0xFF, 0x1F)
}
/// ~0.22 — border-hover; accent borders stay reserved for focus.
pub fn bd_3() -> Color {
    Color::from_rgba8(0xFF, 0xFF, 0xFF, 0x38)
}

// ── Accent (single purple family) ───────────────────────────────────────
pub fn accent() -> Color {
    Color::from_rgba8(0x8A, 0x86, 0xE5, 0xFF)
}
/// Hover — one step lighter than rest.
pub fn accent_hover() -> Color {
    Color::from_rgba8(0xA0, 0x9D, 0xEE, 0xFF)
}
/// Press — one step darker; also the wordmark's offset drop-shadow.
pub fn accent_press() -> Color {
    Color::from_rgba8(0x6E, 0x6A, 0xD0, 0xFF)
}
/// Selected row / chip fill (~0.14 over the page).
pub fn accent_bg() -> Color {
    Color::from_rgba8(0x8A, 0x86, 0xE5, 0x24)
}
// ── Status ──────────────────────────────────────
/// Errors / destructive actions
pub fn error() -> Color {
    Color::from_rgba8(0xE5, 0x48, 0x4D, 0xFF)
}
