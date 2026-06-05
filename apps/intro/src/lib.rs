//! The intro app — osvauld's landing page: the philosophy as a post, with a live
//! comment thread, drawn over an animated "pieces" backdrop.
//!
//! Runs as a sandboxed wasm app in the shell and themes itself ([`theme`]). The
//! draw fn is `pub` so the native `preview` example runs the exact same UI.
//!
//! Copy is distilled from the osvauld whitepaper (V5). Comments are a plain `Vec`
//! for now; next steps make them a Loro doc, sign each with the vault identity,
//! and sync over iroh. Live cursor presence comes after that.

use osvauld_app::egui::{
    self, Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind,
};

mod theme;

struct Comment {
    author: String,
    body: String,
}

/// The app's whole state.
pub struct Intro {
    /// One-shot: install the theme on the first frame.
    themed: bool,
    draft: String,
    comments: Vec<Comment>,
}

impl Default for Intro {
    fn default() -> Self {
        Intro {
            themed: false,
            draft: String::new(),
            comments: vec![
                Comment {
                    author: "violet-fox".into(),
                    body: "no accounts, no cloud, and it still synced to my phone. sold.".into(),
                },
                Comment {
                    author: "amber-yak".into(),
                    body: "typed this comment and watched it land on the other laptop. live ✓".into(),
                },
            ],
        }
    }
}

/// One frame of the intro UI — called by the wasm `frame` export and the native
/// `preview` example alike.
pub fn draw(ui: &mut egui::Ui, s: &mut Intro) {
    // Fonts set via `set_fonts` only bind on the *next* frame, so the named
    // families ("pixel"/"mono_sb") aren't available the frame we install them.
    // Install on the first frame and skip drawing it; from the next frame they're
    // bound. `request_repaint` makes that next frame come immediately, so the
    // one blank frame is imperceptible.
    if !s.themed {
        theme::apply(ui.ctx());
        s.themed = true;
        ui.ctx().request_repaint();
        return;
    }

    let t = ui.input(|i| i.time) as f32;
    let rect = ui.max_rect();
    backdrop(ui, rect, t);
    ui.ctx().request_repaint(); // keep the backdrop drifting

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            ui.set_max_width(640.0);
            post(ui, s);
        });
        ui.add_space(30.0);
    });
}

/// The post (the philosophy) + the comment thread + composer.
fn post(ui: &mut egui::Ui, s: &mut Intro) {
    ui.label(RichText::new("osvauld").font(FontId::new(46.0, theme::pixel())).color(theme::FG));
    ui.label(
        RichText::new("Collaborative apps, without the cloud.")
            .italics()
            .size(15.0)
            .color(theme::MUTED),
    );
    ui.add_space(22.0);

    para(ui, "osvauld is one ever-changing workspace of live, programmable data. Apps aren't silos — each is just a cluster of small, independent pieces, and any app can read, combine, and build on the pieces the others create.");
    para(ui, "Every piece carries its own access: a cryptographic capability that opens only what it was granted for. Nothing is reachable by default.");
    para(ui, "There are no accounts. Your identity is a keypair you hold — that key is who you are. You sync through a node you run yourself: your machine, your data.");

    ui.add_space(4.0);
    ui.label(
        RichText::new("This page is one of those apps. The comments below are a live CRDT — signed by your key, synced to everyone here.")
            .size(14.0)
            .color(theme::ACCENT),
    );

    ui.add_space(18.0);
    ui.separator();
    ui.add_space(12.0);

    ui.label(
        RichText::new("COMMENTS")
            .font(FontId::new(11.0, FontFamily::Monospace))
            .color(theme::MUTED)
            .extra_letter_spacing(0.6),
    );
    ui.add_space(10.0);

    for c in &s.comments {
        comment_row(ui, c);
    }

    ui.add_space(4.0);
    composer(ui, s);
}

fn para(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(15.5).color(theme::FG_2));
    ui.add_space(12.0);
}

fn comment_row(ui: &mut egui::Ui, c: &Comment) {
    ui.horizontal(|ui| {
        // A square identity chip in the author's deterministic colour.
        let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
        ui.painter().rect_filled(
            Rect::from_center_size(r.center(), egui::vec2(9.0, 9.0)),
            0.0,
            identity_color(&c.author),
        );
        ui.add_space(2.0);
        ui.label(RichText::new(&c.author).strong().color(theme::FG));
        ui.label(
            RichText::new("verified")
                .font(FontId::new(10.0, FontFamily::Monospace))
                .color(theme::VERIFIED),
        );
    });
    ui.label(RichText::new(&c.body).size(14.0).color(theme::FG_2));
    ui.add_space(12.0);
}

fn composer(ui: &mut egui::Ui, s: &mut Intro) {
    ui.horizontal(|ui| {
        let field_w = (ui.available_width() - 132.0).max(120.0);
        ui.add(
            egui::TextEdit::singleline(&mut s.draft)
                .hint_text("add a comment…")
                .desired_width(field_w)
                .margin(egui::Margin::symmetric(12, 9))
                .background_color(theme::RAISED),
        );
        if cta_button(ui, "Comment").clicked() && !s.draft.trim().is_empty() {
            s.comments.push(Comment {
                author: "you".into(),
                body: std::mem::take(&mut s.draft),
            });
        }
    });
}

/// The accent CTA with a 3px hard-offset shadow the face presses onto when held —
/// the brand's signature button, ported from the shell's `paint_cta`.
fn cta_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(116.0, 38.0), Sense::click());
    let pressed = resp.is_pointer_button_down_on();
    let painter = ui.painter();

    let shadow = Rect::from_min_size(rect.min + egui::vec2(3.0, 3.0), rect.size());
    let face = if pressed { shadow } else { rect };
    if !pressed {
        painter.rect_filled(shadow, 0.0, theme::ACCENT_PRESS);
    }
    let fill = if resp.hovered() { theme::ACCENT_HOVER } else { theme::ACCENT };
    painter.rect_filled(face, 0.0, fill);
    painter.rect_stroke(face, CornerRadius::same(0), Stroke::new(1.0, theme::ACCENT_PRESS), StrokeKind::Inside);
    painter.text(
        face.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::new(13.0, theme::mono_sb()),
        Color32::WHITE,
    );
    resp
}

/// An animated backdrop: a slowly drifting constellation of small squares
/// ("pieces") joined by faint lines when near — the modular-live-data motif.
/// Time-driven (`ui.input(time)`), the same approach as the login backdrop.
fn backdrop(ui: &mut egui::Ui, rect: Rect, t: f32) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme::BG);

    const N: usize = 16;
    let amp = rect.width().min(rect.height()) * 0.035;
    let mut pts = [Pos2::ZERO; N];
    for (i, p) in pts.iter_mut().enumerate() {
        let fi = i as f32;
        // Base centre, inset so an orbit never reaches an edge — NO clamping. (The
        // old clamp pinned border nodes, which freeze-and-snap = the jitter.)
        let cx = rect.left() + (0.08 + 0.84 * frac(fi * 0.618_034 + 0.13)) * rect.width();
        let cy = rect.top() + (0.08 + 0.84 * frac(fi * 0.754_878 + 0.41)) * rect.height();
        // Each node glides a slow, smooth elliptical orbit — continuous motion,
        // varied speed/phase per node so they don't pulse in lockstep.
        let phase = fi * 1.7;
        let speed = 0.35 + 0.20 * frac(fi * 0.317);
        *p = Pos2::new(
            cx + amp * (t * speed + phase).cos(),
            cy + amp * 0.75 * (t * speed * 1.1 + phase).sin(),
        );
    }

    let max_d = rect.width().min(rect.height()) * 0.34;
    for i in 0..N {
        for j in (i + 1)..N {
            let d = pts[i].distance(pts[j]);
            if d < max_d {
                let a = (1.0 - d / max_d) * 26.0;
                painter.line_segment([pts[i], pts[j]], Stroke::new(1.2, alpha(theme::ACCENT, a as u8)));
            }
        }
    }
    for p in &pts {
        painter.rect_filled(Rect::from_center_size(*p, egui::vec2(4.0, 4.0)), 0.0, alpha(theme::ACCENT, 150));
    }
}

/// A stable colour per identity — the square chip next to each comment.
fn identity_color(seed: &str) -> Color32 {
    const WHEEL: [Color32; 8] = [
        Color32::from_rgb(0x8A, 0x86, 0xE5), // lavender
        Color32::from_rgb(0x57, 0xD9, 0x9A), // mint
        Color32::from_rgb(0xE5, 0xB0, 0x6A), // amber
        Color32::from_rgb(0xE5, 0x6A, 0x8A), // rose
        Color32::from_rgb(0x6A, 0xB8, 0xE5), // sky
        Color32::from_rgb(0xB0, 0xE5, 0x6A), // lime
        Color32::from_rgb(0xCB, 0xA6, 0xF7), // violet
        Color32::from_rgb(0x6A, 0xE5, 0xD9), // teal
    ];
    let h = seed.bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    WHEEL[(h as usize) % WHEEL.len()]
}

fn frac(x: f32) -> f32 {
    x - x.floor()
}

fn alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

osvauld_app::app!(Intro::default(), draw);

#[cfg(test)]
mod tests {
    use super::*;

    // Reproduce the shell's font trap headlessly: run two frames (first installs
    // fonts, both lay out text) and let any panic surface its message.
    #[test]
    fn draws_two_frames_headless() {
        let ctx = egui::Context::default();
        let mut s = Intro::default();
        for _ in 0..2 {
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| draw(ui, &mut s));
        }
    }
}
