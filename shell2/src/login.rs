//! The login screen: the device's identities as a vertical list, described with the runtime's
//! element vocab. `view` returns the tree; `update` handles selection. No raw scene/layout/input
//! code here — that all lives in `runtime`. (Port of sthalam's `screens/accounts.rs`.)

use std::time::Instant;

use crate::signup::SignupForm;
use crate::space::SpaceScreen;
use crate::{Msg, Screen, theme};
use runtime::{El, EventLoopProxy, MONO_FAMILY, PIXEL_FAMILY, col, custom, row, text, text_input};
use vault::{AccountInfo, Vault};
use vello::Scene;
use vello::kurbo::{Affine, Arc, Rect};
use vello::peniko::{Color, Fill};

#[derive(Clone)]
pub enum LoginMsg {
    Select(usize),
    Passphrase(String),
    Login,
    Done(Result<(), String>),
    AddAccount,
}

const PANEL_W: f32 = 460.0;

pub struct LoginScreen {
    accounts: Vec<AccountInfo>,
    selected: usize,
    passphrase: String,
    error: Option<String>,
    pending: Option<Instant>,
}

impl LoginScreen {
    pub fn update(
        &mut self,
        msg: LoginMsg,
        vault: &mut Vault,
        proxy: &EventLoopProxy<Msg>,
    ) -> Option<Screen> {
        match msg {
            LoginMsg::Select(i) => {
                self.selected = i;
                None
            }
            LoginMsg::Passphrase(pw) => {
                self.passphrase = pw;
                None
            }
            LoginMsg::Login => {
                if self.pending.is_some() || self.passphrase.is_empty() {
                    return None;
                }
                self.error = None;
                self.pending = Some(Instant::now());
                let selected_did = self.accounts[self.selected].did.clone();
                let mut vault = vault.clone();
                let passphrase = self.passphrase.clone();
                let proxy = proxy.clone();
                std::thread::spawn(move || {
                    let result = vault
                        .login(&selected_did, &passphrase)
                        .map_err(|e| e.to_string());
                    let _ = proxy.send_event(Msg::Login(LoginMsg::Done(result)));
                });
                None
            }
            LoginMsg::AddAccount => Some(Screen::Signup(SignupForm::default())),
            LoginMsg::Done(r) => {
                self.pending = None;
                match r {
                    Ok(()) => Some(Screen::Spaces(SpaceScreen::new(vault))),
                    Err(e) => {
                        self.error = Some(e);
                        None
                    }
                }
            }
        }
    }

    pub fn view(&self) -> El<Msg> {
        // The centered panel: wordmark, account rows, then the quiet links.
        let mut accounts: Vec<El<Msg>> = Vec::new();
        for (i, a) in self.accounts.iter().enumerate() {
            let selected = if self.selected == i { true } else { false };
            accounts.push(self.account_row(i, a, selected));
        }
        let acc_list = col()
            .id("accounts")
            .scroll_y()
            .h(360.0)
            .gap(8.0)
            .children(accounts);
        let panel = col().w(PANEL_W).gap(8.0).child(wordmark()).child(acc_list);
        let passphrase_label = row()
            .gap(4.0)
            .child(text("Passphrase.").color(theme::fg_3()))
            .child(text(&self.accounts[self.selected].label).color(theme::fg_2()));
        let passphrase_input = text_input(&self.passphrase, "passphrase", |m| {
            Msg::Login(LoginMsg::Passphrase(m))
        })
        .h(40.0)
        .px(12.0)
        .autofocus()
        .fill(theme::bg_1())
        .color(theme::fg_1())
        .radius(6.0)
        .stroke(1.0, theme::bd_2())
        .on_enter(Msg::Login(LoginMsg::Login))
        .hover_stroke(1.0, theme::bd_3());

        let mut login_button = row()
            .h(44.0)
            .center()
            .id("login")
            .child(text("Login").font_size(15.0).color(theme::fg_1()))
            .radius(6.0)
            .fill(theme::accent());
        login_button = if let Some(since) = self.pending {
            let secs = since.elapsed().as_secs_f64();
            let spinner: El<Msg> = custom(move |scene, _t, rect, t| {
                let angle = secs * std::f64::consts::TAU;
                let arc = Arc::new(rect.center(), (8.0, 8.0), angle, 4.8, 0.0);
                scene.stroke(
                    &vello::kurbo::Stroke::new(2.0),
                    t,
                    theme::fg_1(),
                    None,
                    &arc,
                );
            })
            .size(20.0, 20.0)
            .repaint();
            login_button.child(spinner).fill(theme::accent_press())
        } else {
            login_button
                .hover_fill(theme::accent_hover())
                .on_click(Msg::Login(LoginMsg::Login))
                .press_fill(theme::accent_press())
        };
        let passphrase_col = col()
            .gap(8.0)
            .w(PANEL_W)
            .child(passphrase_label)
            .child(passphrase_input)
            .child(login_button);
        let mut error_panel: El<Msg> = row().center().h(44.0).pad(20.0);
        if let Some(error) = &self.error {
            error_panel = error_panel
                .child(text(error).font_size(13.0))
                .fill(theme::error());
        }
        let add_account = row()
            .h(32.0)
            .px(12.0)
            .radius(6.0)
            .align_center()
            .id("add_account")
            .hover_fill(theme::bg_2())
            .press_fill(theme::bg_3())
            .tint(120.0)
            .on_click(Msg::Login(LoginMsg::AddAccount))
            .child(
                text("+ Add another account")
                    .font_size(13.0)
                    .color(theme::fg_3()),
            );

        // Root fills the window and centers the panel.
        col()
            .full()
            .center()
            .child(panel)
            .child(passphrase_col)
            .child(add_account)
            .child(error_panel)
    }

    pub fn new(vault: &Vault) -> Self {
        let (accounts, error) = match vault.accounts() {
            Ok(accounts) => (accounts, None),
            Err(e) => (Vec::new(), Some(e.to_string())),
        };
        Self {
            accounts,
            error,
            selected: 0,
            passphrase: String::new(),
            pending: None,
        }
    }

    /// One account row: identicon, label + short DID, a spacer, and the chevron. Selected → accent wash
    /// + accent border; others a hairline that turns accent on hover.
    fn account_row(&self, i: usize, a: &AccountInfo, selected: bool) -> El<Msg> {
        let edge = if selected {
            theme::accent()
        } else {
            theme::bd_1()
        };

        let r = row()
            .h(64.0)
            .px(16.0)
            .gap(16.0)
            .align_center()
            .stroke(1.0, edge)
            .hover_stroke(1.0, theme::accent())
            .on_click(Msg::Login(LoginMsg::Select(i)))
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
            .child(col().grow()); // spacer pushes the chevron to the right edge
        if selected {
            return r.fill(theme::accent_bg());
        }
        r
    }
}

/// The "sthalam" wordmark in the VT323 pixel face, drawn twice for the offset accent shadow.
fn wordmark() -> El<Msg> {
    custom(|scene, text, rect, t| {
        let o = 2.0;
        let (w, _) = text.measure("sthalam", PIXEL_FAMILY, 56.0, None);
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
fn identicon(did: &str) -> El<Msg> {
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
