use runtime::{
    App, El, EventLoopProxy, MONO_FAMILY, PIXEL_FAMILY, col, custom, row, text, text_area,
    text_input,
};
use std::time::Instant;
use vault::Vault;
use vello::{
    kurbo::{self, Affine},
    peniko::color,
};

use crate::{Msg, Screen, mnemonic::Mnemonic, theme};

pub struct SignupForm {
    pub label: String,
    pub pass: String,
    pub error: Option<String>,
    pub pending: Option<Instant>,
}

impl Default for SignupForm {
    fn default() -> Self {
        SignupForm {
            label: String::new(),
            pass: String::new(),
            error: None,
            pending: None,
        }
    }
}

#[derive(Clone)]
pub enum SignupMsg {
    Label(String),
    Pass(String),
    Submit,
    Done(Result<String, String>),
}

const PANEL_W: f32 = 360.0;

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
            None,
        );
        text.draw(
            scene,
            "sthalam",
            PIXEL_FAMILY,
            56.0,
            t * Affine::translate((rect.x0 + offset, rect.y0)),
            theme::accent(),
            None,
        );
    })
    .h(52.0)
}

impl SignupForm {
    pub fn view(&self) -> El<Msg> {
        let mut panel_children = Vec::new();
        let label = text("Username")
            .font_size(13.0)
            .no_wrap()
            .color(theme::fg_3());
        let label_input = text_input(&self.label, "label", |s| Msg::Signup(SignupMsg::Label(s)))
            .h(40.0)
            .w(PANEL_W)
            .px(12.0)
            .fill(theme::bg_1())
            .color(theme::fg_1())
            .radius(6.0)
            .stroke(1.0, theme::bd_2())
            .hover_stroke(1.0, theme::bd_3());

        let label_el = col().gap(4.0).child(label).child(label_input);

        let pass = text("Password")
            .font_size(13.0)
            .no_wrap()
            .color(theme::fg_3());
        let password_input =
            text_input(&self.pass, "password", |s| Msg::Signup(SignupMsg::Pass(s)))
                .h(40.0)
                .w(PANEL_W)
                .color(theme::fg_1())
                .px(12.0)
                .fill(theme::bg_1())
                .stroke(1.0, theme::bd_2())
                .radius(6.0)
                .hover_stroke(1.0, theme::bd_3());
        let password_el = col().gap(4.0).child(pass).child(password_input);
        let mut submit_button = row()
            .center()
            .gap(8.0)
            .child(text("Create Identity").no_wrap())
            .fill(theme::accent())
            .press_fill(theme::accent_press())
            .h(44.0)
            .mt(8.0)
            .radius(6.0);
        submit_button = if let Some(since) = self.pending {
            let secs = since.elapsed().as_secs_f64();
            let spinner: El<Msg> = custom(move |scene, _t, rect, t| {
                let angle = secs * std::f64::consts::TAU;
                let arc = kurbo::Arc::new(rect.center(), (8.0, 8.0), angle, 4.8, 0.0);
                scene.stroke(&kurbo::Stroke::new(2.0), t, theme::fg_1(), None, &arc);
            })
            .size(20.0, 20.0)
            .repaint();
            submit_button.child(spinner).fill(theme::accent_press())
        } else {
            submit_button
                .child(col().size(20.0, 20.0))
                .id("submit")
                .hover_fill(theme::accent_hover())
                .tint(120.0)
                .on_click(Msg::Signup(SignupMsg::Submit))
        };

        let mut error_panel: El<Msg> = row().center().h(44.0);
        if let Some(e) = &self.error {
            error_panel = error_panel
                .child(text(e).font_size(13.0))
                .fill(theme::error());
        }
        panel_children = vec![
            wordmark(),
            label_el,
            password_el,
            submit_button,
            error_panel,
        ];
        let panel = col().gap(16.0).children(panel_children);

        col().full().child(panel).center()
    }
    pub fn update(
        &mut self,
        msg: SignupMsg,
        vault: &mut Vault,
        proxy: &EventLoopProxy<Msg>,
    ) -> Option<Screen> {
        match msg {
            SignupMsg::Label(l) => {
                self.label = l;
                None
            }
            SignupMsg::Pass(s) => {
                self.pass = s;
                None
            }
            SignupMsg::Submit => {
                if self.pending.is_some() {
                    return None;
                }
                self.error = None;
                if !self.validate_submit() {
                    self.error = Some(String::from("validation failed"));
                    return None;
                }

                let (label, password) = (self.label.clone(), self.pass.clone());
                let mut vault = vault.clone();
                let proxy = proxy.clone();
                self.pending = Some(Instant::now());
                std::thread::spawn(move || {
                    let result = vault
                        .signup(&label, &password)
                        .map(|(_dud, m)| m.to_string())
                        .map_err(|e| e.to_string());
                    let _ = proxy.send_event(Msg::Signup(SignupMsg::Done(result)));
                });
                None
            }
            SignupMsg::Done(result) => match result {
                Ok(s) => {
                    self.pending = None;
                    Some(Screen::Mnemonic(Mnemonic {
                        words: s,
                        accepted: false,
                    }))
                }
                Err(e) => {
                    self.pending = None;
                    self.error = Some(e);
                    None
                }
            },
        }
    }

    fn validate_submit(&self) -> bool {
        if !self.pass.is_empty() && !self.label.trim().is_empty() {
            return true;
        }
        false
    }
}
