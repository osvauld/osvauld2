//! Visual check for the text measure hook: `cargo run -p runtime --example wrap`.
//!
//! No vault, no login, no Lua — just an `El` tree, because the thing under test is the layout
//! solve and nothing above it. **Resize the window**: the right-hand column is the live proof, and
//! before this change every panel here rendered as one long line running off its own box.
//!
//! What each panel pins, matching the tests in `layout.rs`:
//!   fixed    — wraps at 260 no matter how wide the window gets
//!   flexible — reflows continuously as the window resizes
//!   narrow   — the same string, taller, because the box is smaller
//!   padded   — the frame sits 20pt off the text on every side, counted once
//!   input    — designed sized: its box does not move when its value grows

use runtime::vello::peniko::Color;
use runtime::{App, El, Run, col, rich, row, text, text_input};

const PARA: &str = "The runtime was permanently at max-content: every string was shaped once, \
    with line breaking switched off, and the answer frozen into the style before Taffy ever ran. \
    A paragraph therefore could not know how wide its parent was, so it never wrapped.";

const INK: Color = Color::from_rgba8(0xE8, 0xE8, 0xF0, 0xFF);
const MUTED: Color = Color::from_rgba8(0x88, 0x88, 0x99, 0xFF);
const EDGE: Color = Color::from_rgba8(0x44, 0x44, 0x55, 0xFF);

struct Wrap;

/// A labelled box with a 1px frame, so the panel's edge is visible and overflow would be obvious.
fn panel<M: Clone>(label: &str, body: El<M>) -> El<M> {
    col()
        .gap(6.0)
        .child(
            text(label)
                .font_size(11.0)
                .font(runtime::MONO_FAMILY)
                .color(MUTED),
        )
        .child(body.stroke(1.0, EDGE).radius(4.0))
}

fn para<M: Clone>() -> El<M> {
    text(PARA).font_size(14.0).color(INK)
}

/// Every run feature at once, over one string. The point to look at is that it still wraps as a
/// single paragraph — the styles do not break it into pieces that lay out separately.
fn styled<M: Clone>() -> El<M> {
    const S: &str = "Bold and italic and struck and underlined and mono and red, \
        all wrapping as one paragraph rather than as six.";
    let at = |needle: &str| {
        let i = S.find(needle).expect("needle");
        i..i + needle.len()
    };
    rich(
        S,
        vec![
            Run::new(at("Bold"), 14.0, INK).bold(),
            Run::new(at("italic"), 14.0, INK).italic(),
            Run::new(at("struck"), 14.0, INK).strike(),
            Run::new(at("underlined"), 14.0, INK).underline(),
            Run::new(at("mono"), 14.0, INK).font(runtime::MONO_FAMILY),
            Run::new(at("red"), 14.0, Color::from_rgba8(0xF8, 0x5C, 0x5C, 0xFF)),
            Run::new(at("six"), 20.0, INK).bold(),
        ],
    )
    .color(INK)
    .font_size(14.0)
}

impl App for Wrap {
    type Msg = ();

    fn update(&mut self, _: ()) {}

    fn view(&self) -> El<()> {
        col()
            .full()
            .pad(24.0)
            .gap(20.0)
            .child(
                row()
                    .gap(20.0)
                    .child(panel("fixed 260", col().w(260.0).pad(12.0).child(para())))
                    .child(panel(
                        "flexible — resize the window",
                        col().grow().pad(12.0).child(para()),
                    )),
            )
            .child(
                row()
                    .gap(20.0)
                    .child(panel("narrow 160", col().w(160.0).pad(12.0).child(para())))
                    .child(panel(
                        "padding counted once",
                        col().w(260.0).pad(20.0).child(para()),
                    ))
                    .child(panel(
                        "rich runs — wraps as one paragraph",
                        col().w(300.0).pad(12.0).child(styled()),
                    ))
                    .child(panel(
                        "input: designed sized",
                        col()
                            .w(260.0)
                            .pad(12.0)
                            .gap(8.0)
                            .child(text_input(PARA, "long", |_| ()).h(28.0))
                            .child(text_input("hi", "short", |_| ()).h(28.0)),
                    )),
            )
    }
}

fn main() {
    runtime::run_with(|_| Wrap);
}
