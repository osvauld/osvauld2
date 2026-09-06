use runtime::{El, col, row, text};

use crate::{
    Msg, Screen,
    signup::{SignupForm, SignupMsg},
    theme,
};

pub struct Mnemonic {
    pub words: String,
    pub accepted: bool,
}

#[derive(Clone)]
pub enum MnemonicMsg {
    ToggleSaved,
    Continue,
}

impl Mnemonic {
    pub fn view(&self) -> El<Msg> {
        let mut i = 1;
        let mut row_fields = row().gap(24.0);
        let words: Vec<&str> = self.words.split_whitespace().collect();

        for (c, chunk) in words.chunks(4).enumerate() {
            let mut col_fields = col().gap(8.0);
            for word in chunk {
                let placed_word = row()
                    .gap(4.0)
                    .pad(16.0)
                    .align_center()
                    .stroke(1.0, theme::bd_2())
                    .radius(6.0)
                    .fill(theme::bg_1())
                    .child(
                        text(format!("{:02}. ", i))
                            .color(theme::fg_4())
                            .font_size(14.0)
                            .w(24.0),
                    )
                    .child(text(*word).color(theme::fg_1()).font_size(14.0));
                col_fields = col_fields.child(placed_word);
                i += 1;
            }
            row_fields = row_fields.child(col_fields);
        }

        let recover_phrase = row().child(row_fields);
        let mut continue_button = col()
            .pad(20.0)
            .radius(6.0)
            .child(text("Continue").no_wrap().color(theme::fg_1()))
            .fill(theme::fg_4());
        if self.accepted {
            continue_button = continue_button
                .fill(theme::accent())
                .on_click(Msg::Mnemonic(MnemonicMsg::Continue))
        };
        col()
            .align_center()
            .center()
            .id("recovery_panel")
            .gap(16.0)
            .child(
                text("Your 24-word recovery phrase. Write it down — it's the only way back in.")
                    .color(theme::fg_3()),
            )
            .full()
            .child(recover_phrase)
            .child(checkbox(
                self.accepted,
                "I've saved this phrase",
                Msg::Mnemonic(MnemonicMsg::ToggleSaved),
            ))
            .child(continue_button)
    }

    pub fn update(&mut self, msg: MnemonicMsg) -> Option<Screen> {
        match msg {
            MnemonicMsg::ToggleSaved => {
                self.accepted = !self.accepted;
                None
            }
            MnemonicMsg::Continue => Some(Screen::Signup(SignupForm::default())),
        }
    }
}

fn checkbox(checked: bool, label: &str, msg: Msg) -> El<Msg> {
    let mut mark = col().size(18.0, 18.0).radius(4.0).center();
    mark = if checked {
        mark.fill(theme::accent())
            .child(text("✓").font_size(12.0).no_wrap().color(theme::fg_1()))
    } else {
        mark.stroke(1.0, theme::bd_2())
            .hover_stroke(1.0, theme::bd_3())
    };
    row()
        .gap(8.0)
        .align_center()
        .on_click(msg)
        .child(mark)
        .child(text(label).font_size(13.0).color(theme::fg_2()))
}
