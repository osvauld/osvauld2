//! The login screen — the device's identities as a vertical list (a port of sthalam's
//! `screens/accounts.rs`). Stage A is static layout + paint only: corner tags, the pixel wordmark,
//! account rows with deterministic identicons, and the quiet links. Backdrop (banyan + vignette)
//! and interaction (↑↓ select / click / ↩ unlock) arrive in later stages. Square corners and the
//! terminal-stamp framing are the design's signature.

use vello::kurbo::{Affine, Rect, Stroke};
use vello::peniko::{Color, Fill};
use vello::Scene;

use super::{Redraw, Screen};
use crate::text::{TextEngine, MONO_FAMILY, PIXEL_FAMILY, UI_FAMILY};
use crate::theme;

const PANEL_W: f32 = 460.0;
const ROW_H: f32 = 64.0;
const ROW_GAP: f32 = 8.0;
const PAD: f32 = 32.0; // corner-tag inset

/// One identity to pick. (Stage A holds fakes; Stage D pulls `vault::AccountInfo`.)
struct Account {
    did: String,
    label: String,
}

pub struct LoginScreen {
    accounts: Vec<Account>,
    selected: usize,
}

impl LoginScreen {
    pub fn new() -> Self {
        let acc = |label: &str, did: &str| Account { label: label.into(), did: did.into() };
        Self {
            accounts: vec![
                acc("test", "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK"),
                acc("test2", "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRkAJeSv9JJfA"),
                acc("test3", "did:key:z6MkfMqd5W4D1iZjVwq1uFvN3wz1XhfHrYdR6QybxKn8wYqs"),
                acc("test4", "did:key:z6MksdLPj7AAt3X5oLrz1FmNQ4ihTwq1Cz3vh6c2ydLeRtKa"),
            ],
            selected: 0,
        }
    }
}

impl Screen for LoginScreen {
    fn build(
        &mut self,
        scene: &mut Scene,
        text: &mut TextEngine,
        t: Affine,
        viewport: (f32, f32),
        _now: f64,
    ) -> Redraw {
        let (vw, vh) = viewport;

        // ── Corner tags: terminal stamps in all four corners (mono caps, dim). ──
        let tag = |scene: &mut Scene, text: &mut TextEngine, s: &str, x: f32, y: f32| {
            text.draw(scene, s, MONO_FAMILY, 10.5, t * Affine::translate((x as f64, y as f64)), theme::fg_4());
        };
        tag(scene, text, "OSVAULD · 01 · STHALAM", PAD, 16.0);
        right_tag(scene, text, t, "02 / login", vw - PAD, 16.0);
        right_tag(scene, text, t, "v0.1.0 · local", vw - PAD, vh - 28.0);
        tag(scene, text, "↑↓ SELECT  ·  ↩ UNLOCK", PAD, vh - 28.0);

        // ── Centered panel: wordmark + rows + links. ──
        let n = self.accounts.len();
        let panel_h = panel_height(n);
        let px = ((vw - PANEL_W) / 2.0).max(PAD);
        let mut y = ((vh - panel_h) / 2.0).max(16.0);

        // Wordmark "sthalam" — VT323 pixel face, drawn twice for the offset drop-shadow.
        let size: f32 = 56.0;
        let offset = (size / 24.0).round() as f64;
        let wm = t * Affine::translate((px as f64, y as f64));
        text.draw(scene, "sthalam", PIXEL_FAMILY, size, wm * Affine::translate((offset, offset)), theme::accent_press());
        text.draw(scene, "sthalam", PIXEL_FAMILY, size, wm, theme::accent());
        let (_, wm_h) = text.measure("sthalam", PIXEL_FAMILY, size);
        y += wm_h + 26.0;

        // Account rows.
        for (i, account) in self.accounts.iter().enumerate() {
            account_row(scene, text, t, px, y, account, i == self.selected);
            y += ROW_H + ROW_GAP;
        }

        // Quiet links, centered under the rows.
        y += 14.0;
        let links = "+ ADD ACCOUNT      ↺ RECOVER WITH PHRASE";
        let (lw, _) = text.measure(links, MONO_FAMILY, 11.0);
        text.draw(
            scene,
            links,
            MONO_FAMILY,
            11.0,
            t * Affine::translate(((px + (PANEL_W - lw) / 2.0) as f64, y as f64)),
            theme::fg_3(),
        );

        // Static screen: paint once, then idle. (Stage B's backdrop will return Animating.)
        Redraw::Idle
    }
}

/// Draw one account row at (x, y): identicon, label, short DID, chevron. Selected rows get the
/// accent wash, a 2px left bar, and an accent border; others a hairline.
fn account_row(
    scene: &mut Scene,
    text: &mut TextEngine,
    t: Affine,
    x: f32,
    y: f32,
    account: &Account,
    selected: bool,
) {
    let rect = Rect::new(x as f64, y as f64, (x + PANEL_W) as f64, (y + ROW_H) as f64);
    if selected {
        scene.fill(Fill::NonZero, t, theme::accent_bg(), None, &rect);
    }
    let edge = if selected { theme::accent() } else { theme::bd_1() };
    scene.stroke(&Stroke::new(1.0), t, edge, None, &rect);
    if selected {
        let bar = Rect::new(x as f64, y as f64, x as f64 + 2.0, (y + ROW_H) as f64);
        scene.fill(Fill::NonZero, t, theme::accent(), None, &bar);
    }

    let (icon, pad) = (40.0, 16.0);
    identicon(scene, t, x + pad, y + (ROW_H - icon) / 2.0, icon, &account.did);

    let tx = x + pad + icon + 16.0;
    let cy = y + ROW_H / 2.0;
    // Label (sans) above, short DID (mono) below the row's vertical centre.
    text.draw(scene, &account.label, UI_FAMILY, 15.0, t * Affine::translate((tx as f64, (cy - 18.0) as f64)), theme::fg_1());
    text.draw(scene, &short_did(&account.did), MONO_FAMILY, 11.0, t * Affine::translate((tx as f64, (cy + 2.0) as f64)), theme::fg_4());

    // Chevron, right-aligned.
    let chev = "▸";
    let (cw, _) = text.measure(chev, MONO_FAMILY, 14.0);
    let col = if selected { theme::accent() } else { theme::fg_4() };
    text.draw(scene, chev, MONO_FAMILY, 14.0, t * Affine::translate(((x + PANEL_W - pad - cw) as f64, (cy - 9.0) as f64)), col);
}

/// A deterministic pixel avatar: a 5×5 grid mirrored across the vertical axis, two-tone, seeded
/// from the DID (FNV-1a). Stable per identity, no assets. (Port of `components/identicon.rs`.)
fn identicon(scene: &mut Scene, t: Affine, x: f32, y: f32, size: f32, seed: &str) {
    let h = fnv1a(seed);
    let bg = Rect::new(x as f64, y as f64, (x + size) as f64, (y + size) as f64);
    scene.fill(Fill::NonZero, t, Color::from_rgba8(0xFF, 0xFF, 0xFF, 6), None, &bg);

    let color = TINTS[(h % TINTS.len() as u32) as usize];
    let cell = size / 5.0;
    for r in 0..5u32 {
        for c in 0..5u32 {
            // Only the left three columns carry bits; columns 3,4 mirror 1,0.
            let bit = r * 3 + if c < 3 { c } else { 4 - c };
            if (h >> bit) & 1 == 1 {
                let cx = x + c as f32 * cell;
                let cyy = y + r as f32 * cell;
                let cellr = Rect::new(cx as f64, cyy as f64, (cx + cell) as f64, (cyy + cell) as f64);
                scene.fill(Fill::NonZero, t, color, None, &cellr);
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

// Accent, accent-soft, then peer pastels — so accounts are distinguishable.
const TINTS: [Color; 6] = [
    Color::from_rgb8(0x8A, 0x86, 0xE5),
    Color::from_rgb8(0xCB, 0xA6, 0xF7),
    Color::from_rgb8(0xE8, 0xC3, 0xFF),
    Color::from_rgb8(0xB0, 0xE5, 0xFF),
    Color::from_rgb8(0xFF, 0xD4, 0x9A),
    Color::from_rgb8(0xC6, 0xFF, 0xD4),
];

/// did:key:z6Mk…r9 — DIDs are ASCII, so byte slicing is safe.
fn short_did(did: &str) -> String {
    if did.len() <= 20 {
        return did.to_string();
    }
    format!("{}…{}", &did[..16], &did[did.len() - 4..])
}

/// Draw a right-aligned mono tag whose *right* edge sits at `right`.
fn right_tag(scene: &mut Scene, text: &mut TextEngine, t: Affine, s: &str, right: f32, y: f32) {
    let (w, _) = text.measure(s, MONO_FAMILY, 10.5);
    text.draw(scene, s, MONO_FAMILY, 10.5, t * Affine::translate(((right - w) as f64, y as f64)), theme::fg_4());
}

/// Approximate stack height (wordmark + gaps + rows + links), used only to center vertically.
fn panel_height(n: usize) -> f32 {
    100.0 + n as f32 * (ROW_H + ROW_GAP)
}
