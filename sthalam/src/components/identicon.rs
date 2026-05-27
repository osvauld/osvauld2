// A deterministic pixel avatar for an account: a 5×5 grid mirrored across the vertical axis,
// two-tone, seeded from the DID (FNV-1a). No assets, stable per identity, and it reads as
// "computed, not chosen" — matching the VT323 wordmark. (Port of sthalam-login.jsx Identicon.)

use eframe::egui::{self, Color32, Pos2, Rect};

use crate::theme;

pub fn identicon(painter: &egui::Painter, top_left: Pos2, size: f32, seed: &str) {
    let h = fnv1a(seed);
    painter.rect_filled(Rect::from_min_size(top_left, egui::vec2(size, size)), 0.0, Color32::from_white_alpha(6));

    let color = TINTS[(h % TINTS.len() as u32) as usize];
    let cell = size / 5.0;
    for r in 0..5u32 {
        for c in 0..5u32 {
            // Only the left three columns carry bits; columns 3,4 mirror 1,0.
            let bit = r * 3 + if c < 3 { c } else { 4 - c };
            if (h >> bit) & 1 == 1 {
                let min = top_left + egui::vec2(c as f32 * cell, r as f32 * cell);
                painter.rect_filled(Rect::from_min_size(min, egui::vec2(cell, cell)), 0.0, color);
            }
        }
    }
}

fn fnv1a(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

// Accent, accent-soft, then the peer pastels from sthalam-theme.css — variety so accounts
// are distinguishable at a glance.
const TINTS: [Color32; 6] = [
    theme::ACCENT,
    Color32::from_rgb(0xCB, 0xA6, 0xF7),
    Color32::from_rgb(0xE8, 0xC3, 0xFF),
    Color32::from_rgb(0xB0, 0xE5, 0xFF),
    Color32::from_rgb(0xFF, 0xD4, 0x9A),
    Color32::from_rgb(0xC6, 0xFF, 0xD4),
];
