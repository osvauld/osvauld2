//! The login screen: the device's identities as a vertical list, described with the runtime's
//! element vocab. `view` returns the tree; `update` handles selection. No raw scene/layout/input
//! code here — that all lives in `runtime`. (Port of sthalam's `screens/accounts.rs`.)

use crate::theme;
use runtime::{col, custom, row, text, text_area, text_input, App, El, MONO_FAMILY, PIXEL_FAMILY};
use vello::kurbo::{Affine, Rect};
use vello::peniko::{Color, Fill};
use vello::Scene;

pub enum Event {
    SignUpRequested,
}
#[derive(Clone)]
pub enum LoginMsg {
    Select(usize),
    Passphrase(String),
    TextArea(String),
    Signup,
}

const PANEL_W: f32 = 460.0;

struct Account {
    did: String,
    label: String,
}

pub struct LoginScreen {
    accounts: Vec<Account>,
    selected: usize,
    passphrase: String,
    text_area: String,
    username: String,
}

impl LoginScreen {
    pub fn update(&mut self, msg: LoginMsg) -> Option<Event> {
        match msg {
            LoginMsg::Select(i) => {
                self.selected = i;
                None
            }
            LoginMsg::Passphrase(pw) => {
                self.passphrase = pw;
                None
            }
            LoginMsg::TextArea(ta) => {
                self.text_area = ta;
                None
            }
            LoginMsg::Signup => Some(Event::SignUpRequested),
        }
    }

    pub fn view(&self) -> El<LoginMsg> {
        // The centered panel: wordmark, account rows, then the quiet links.
        let mut kids: Vec<El<LoginMsg>> = vec![wordmark().mb(18.0)];
        let mut accounts: Vec<El<LoginMsg>> = Vec::new();
        for (i, a) in self.accounts.iter().enumerate() {
            accounts.push(account_row(i, a, i == self.selected));
        }
        kids.push(
            text_input(&self.passphrase, "passphrase", |s| LoginMsg::Passphrase(s))
                .h(40.0)
                .px(12.0)
                .font_size(15.0)
                .color(theme::fg_1())
                .stroke(1.0, theme::bd_1()),
        );
        kids.push(
            text_area(&self.text_area, "area", |t| LoginMsg::TextArea(t))
                .w(PANEL_W)
                .h(80.0)
                .px(12.0)
                .py(12.0)
                .font_size(15.0)
                .color(theme::fg_1())
                .stroke(1.0, theme::bd_1()),
        );
        kids.push(
            row()
                .h(44.0)
                .center()
                .radius(6.0)
                .fill(theme::accent())
                .hover_fill(theme::accent_press())
                .on_click(LoginMsg::Signup)
                .child(text("Sign Up").font_size(15.0).color(theme::fg_1())),
        );

        let acc_list = col()
            .scroll_y("accounts")
            .h(360.0)
            .gap(8.0)
            .children(accounts);
        let panel = col().w(PANEL_W).gap(8.0).child(acc_list).children(kids);

        // Root fills the window and centers the panel.
        col().full().center().child(panel)
    }

    pub fn new() -> Self {
        let acc = |label: &str, did: &str| Account {
            label: label.into(),
            did: did.into(),
        };
        Self {
            accounts: vec![
                acc(
                    "test",
                    "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
                ),
                acc(
                    "test2",
                    "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRkAJeSv9JJfA",
                ),
                acc(
                    "test3",
                    "did:key:z6MkfMqd5W4D1iZjVwq1uFvN3wz1XhfHrYdR6QybxKn8wYqs",
                ),
                acc(
                    "test4",
                    "did:key:z6MksdLPj7AAt3X5oLrz1FmNQ4ihTwq1Cz3vh6c2ydLeRtKa",
                ),
                acc(
                    "test",
                    "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
                ),
                acc(
                    "test2",
                    "did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRkAJeSv9JJfA",
                ),
                acc(
                    "test3",
                    "did:key:z6MkfMqd5W4D1iZjVwq1uFvN3wz1XhfHrYdR6QybxKn8wYqs",
                ),
                acc(
                    "test4",
                    "did:key:z6MksdLPj7AAt3X5oLrz1FmNQ4ihTwq1Cz3vh6c2ydLeRtKa",
                ),
                acc(
                    "test",
                    "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK",
                ),
            ],
            selected: 0,
            passphrase: "".to_string(),
            text_area: "".to_string(),
            username: "".to_string(),
        }
    }
}

/// One account row: identicon, label + short DID, a spacer, and the chevron. Selected → accent wash
/// + accent border; others a hairline that turns accent on hover.
fn account_row(i: usize, a: &Account, selected: bool) -> El<LoginMsg> {
    let edge = if selected {
        theme::accent()
    } else {
        theme::bd_1()
    };
    let chevron_color = if selected {
        theme::accent()
    } else {
        theme::fg_4()
    };

    let mut r = row()
        .h(64.0)
        .px(16.0)
        .gap(16.0)
        .align_center()
        .stroke(1.0, edge)
        .hover_stroke(1.0, theme::accent())
        .on_click(LoginMsg::Select(i))
        .child(identicon(&a.did))
        .child(
            col()
                .child(text(&a.label).font_size(15.0).color(theme::fg_1()))
                .child(
                    text(short_did(&a.did))
                        .font(MONO_FAMILY)
                        .font_size(11.0)
                        .color(theme::fg_4()),
                ),
        )
        .child(col().grow()) // spacer pushes the chevron to the right edge
        .child(
            text("▸")
                .font(MONO_FAMILY)
                .font_size(14.0)
                .color(chevron_color),
        );
    if selected {
        r = r.fill(theme::accent_bg());
    }
    r
}

/// The "sthalam" wordmark in the VT323 pixel face, drawn twice for the offset accent shadow.
fn wordmark() -> El<LoginMsg> {
    custom(|scene, text, rect, t| {
        let o = 2.0;
        let (w, _) = text.measure("sthalam", PIXEL_FAMILY, 56.0);
        let offset = (rect.width() - w as f64) / 2.0;
        text.draw(
            scene,
            "sthalam",
            PIXEL_FAMILY,
            56.0,
            t * Affine::translate((rect.x0 + offset + o, rect.y0 + o)),
            theme::accent_press(),
        );
        text.draw(
            scene,
            "sthalam",
            PIXEL_FAMILY,
            56.0,
            t * Affine::translate((rect.x0 + offset, rect.y0)),
            theme::accent(),
        );
    })
    .h(52.0)
}

/// A deterministic 5×5 mirrored pixel avatar, seeded from the DID (FNV-1a), drawn into its rect.
fn identicon(did: &str) -> El<LoginMsg> {
    let seed = did.to_owned();
    custom(move |scene, _text, rect, t| draw_identicon(scene, rect, t, &seed)).size(40.0, 40.0)
}

fn draw_identicon(scene: &mut Scene, rect: Rect, t: Affine, seed: &str) {
    let h = fnv1a(seed);
    scene.fill(
        Fill::NonZero,
        t,
        Color::from_rgba8(0xFF, 0xFF, 0xFF, 6),
        None,
        &rect,
    );
    let color = TINTS[(h % TINTS.len() as u32) as usize];
    let cell = rect.width() / 5.0;
    for row in 0..5u32 {
        for c in 0..5u32 {
            // Left three columns carry bits; columns 3,4 mirror 1,0.
            let bit = row * 3 + if c < 3 { c } else { 4 - c };
            if (h >> bit) & 1 == 1 {
                let x = rect.x0 + c as f64 * cell;
                let y = rect.y0 + row as f64 * cell;
                scene.fill(
                    Fill::NonZero,
                    t,
                    color,
                    None,
                    &Rect::new(x, y, x + cell, y + cell),
                );
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
