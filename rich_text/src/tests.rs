//! Most tests inspect the [`LayoutJob`] directly (pure, no fonts) so they assert the
//! mark→format contract without shaping. One shaping test guards the bold-fallback invariant.

use egui::{Color32, FontFamily};

use super::{fingerprint, galley, layout_job, Marks, Run, Style, Theme};

fn style() -> Style {
    Style::new(14.0, Color32::from_rgb(0xe6, 0xe6, 0xea))
}

fn theme() -> Theme {
    Theme {
        code_color: Color32::from_rgb(0x9c, 0xc0, 0xff),
        code_bg: Color32::from_rgb(0x22, 0x26, 0x2e),
        link_color: Color32::from_rgb(0x4c, 0x8b, 0xf5),
        link_underline: Color32::from_rgb(0x2a, 0x4a, 0x80),
        strike_color: Color32::from_rgb(0x80, 0x80, 0x80),
    }
}

fn job(runs: &[Run]) -> egui::text::LayoutJob {
    layout_job(runs, style(), theme(), FontFamily::Proportional, 400.0)
}

#[test]
fn explicit_run_color_overrides_theme() {
    // tree-sitter / math feed an explicit colour — it must win even on a `code` run.
    let runs = vec![Run { text: "x".into(), marks: Marks::new().flag("code"), color: Some(Color32::RED) }];
    assert_eq!(job(&runs).sections[0].format.color, Color32::RED);
}

#[test]
fn code_mark_uses_theme_when_no_explicit_color() {
    let runs = vec![Run { text: "x".into(), marks: Marks::new().flag("code"), color: None }];
    let j = job(&runs);
    assert_eq!(j.sections[0].format.color, theme().code_color);
    assert_eq!(j.sections[0].format.background, theme().code_bg);
}

#[test]
fn unknown_marks_are_ignored() {
    let runs = vec![Run { text: "x".into(), marks: Marks::new().flag("wobble"), color: None }];
    // unknown key → base colour, type never changed to add "wobble"
    assert_eq!(job(&runs).sections[0].format.color, style().color);
}

#[test]
fn link_underlines_and_strike_strikes() {
    let runs = vec![Run {
        text: "x".into(),
        marks: Marks::new().with("link", "https://x").flag("strike"),
        color: None,
    }];
    let fmt = job(&runs).sections[0].format.clone();
    assert_ne!(fmt.underline.width, 0.0, "link underlines");
    assert_ne!(fmt.strikethrough.width, 0.0, "strike strikes");
    assert_eq!(fmt.color, theme().link_color);
}

// --- Block-level Style: the base every run inherits before its own marks -------------

#[test]
fn block_italic_and_line_metrics_flow_to_every_run() {
    // A quote block: base italic + a line height + tracking apply to a plain (unmarked) run.
    let style = Style { italic: true, line_height: Some(40.0), letter_spacing: 2.0, ..style() };
    let job = layout_job(&[Run::plain("q")], style, theme(), FontFamily::Proportional, 400.0);
    let fmt = &job.sections[0].format;
    assert!(fmt.italics, "block italic applies without a per-run mark");
    assert_eq!(fmt.line_height, Some(40.0));
    assert_eq!(fmt.extra_letter_spacing, 2.0);
}

#[test]
fn mono_base_sets_monospace_family() {
    // A code block: the base family is monospace even though the run carries no `code` mark.
    let style = Style { mono: true, ..style() };
    let job = layout_job(&[Run::plain("fn")], style, theme(), FontFamily::Proportional, 400.0);
    assert_eq!(job.sections[0].format.font_id.family, FontFamily::Monospace);
}

#[test]
fn block_strike_is_faint_but_run_strike_takes_run_colour() {
    // Done to-do: block-level strike, drawn in the theme's (faint) strike colour.
    let done = Style { strike: true, ..style() };
    let j = layout_job(&[Run::plain("x")], done, theme(), FontFamily::Proportional, 400.0);
    let f = &j.sections[0].format;
    assert_ne!(f.strikethrough.width, 0.0, "block strike strikes");
    assert_eq!(f.strikethrough.color, theme().strike_color, "done-to-do strike is faint");

    // Inline ~~strike~~: strikes in the run's own colour (here an explicit red), not the theme's.
    let run = Run { text: "x".into(), marks: Marks::new().flag("strike"), color: Some(Color32::RED) };
    let j = layout_job(&[run], style(), theme(), FontFamily::Proportional, 400.0);
    let f = &j.sections[0].format;
    assert_ne!(f.strikethrough.width, 0.0, "inline strike strikes");
    assert_eq!(f.strikethrough.color, Color32::RED, "inline strike matches the run's text colour");
}

#[test]
fn fingerprint_changes_with_block_style() {
    // The same runs under a different block Style must reshape (the cache can't reuse a stale
    // galley) — e.g. promoting a paragraph to a heading (bold) or marking a to-do done (strike).
    let runs = vec![Run::plain("hello")];
    let plain = style();
    for changed in [
        Style { bold: true, ..plain },
        Style { italic: true, ..plain },
        Style { strike: true, ..plain },
        Style { mono: true, ..plain },
        Style { line_height: Some(30.0), ..plain },
        Style { letter_spacing: 1.0, ..plain },
    ] {
        assert_ne!(
            fingerprint(&runs, plain, theme(), 400.0),
            fingerprint(&runs, changed, theme(), 400.0),
            "a block-Style change must change the fingerprint",
        );
    }
}

#[test]
fn fingerprint_changes_with_marks_not_only_length() {
    // Same text+length, different marks → different fingerprint (the doc_editor lesson).
    let plain = vec![Run::plain("hello")];
    let bold = vec![Run { text: "hello".into(), marks: Marks::new().flag("bold"), color: None }];
    assert_ne!(
        fingerprint(&plain, style(), theme(), 400.0),
        fingerprint(&bold, style(), theme(), 400.0),
    );
}

#[test]
fn bold_without_registered_font_falls_back_not_panics() {
    // A context with no `install_fonts` must still shape a bold run (fallback to Proportional),
    // not panic on an unbound Name family — this is what keeps font-less tests alive.
    let ctx = egui::Context::default();
    let runs = vec![Run::plain("normal "), Run { text: "bold".into(), marks: Marks::new().flag("bold"), color: None }];

    let mut size = None;
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        size = Some(galley(ui.ctx(), &runs, style(), theme(), 400.0).size());
    });
    assert!(size.expect("ran a frame").x > 0.0, "bold run shaped despite no bold face");
}
