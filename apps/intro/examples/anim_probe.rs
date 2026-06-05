//! Animation probe — isolates WHY motion looks jittery, independent of the app.
//!
//!   cargo run -p intro-app --example anim_probe --release
//!
//! It draws nothing but a few moving primitives that differ in the variables that
//! matter — size, line thickness, and speed — over a steady, vsync'd loop, and
//! prints frame timing. Watch each and tell me which look smooth vs jittery:
//!
//!   A  big filled circle, FAST orbit   (baseline — should be smoothest)
//!   B  small 4px square,  FAST orbit   (size test, fast)
//!   C  thin 1px line,     FAST sweep   (thin-line AA test, fast)
//!   E  small 4px square,  SLOW orbit   (matches the backdrop's slow speed)
//!
//! If A/B/C are smooth but E jitters → it's SLOW sub-pixel motion (the backdrop
//! moves too slowly; egui's AA "crawls"). If C jitters but A doesn't → thin lines.
//! If even A jitters → it's environmental (compositor/egui), not the scene.

use std::time::Instant;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke};

const ACCENT: Color32 = Color32::from_rgb(0x8A, 0x86, 0xE5);
const MINT: Color32 = Color32::from_rgb(0x57, 0xD9, 0x9A);
const BG: Color32 = Color32::from_rgb(0x0A, 0x0B, 0x10);

#[derive(Default)]
struct Probe {
    last: Option<Instant>,
    window_start: Option<Instant>,
    frames: u32,
    worst_ms: f32,
}

impl eframe::App for Probe {
    fn ui(&mut self, ui: &mut egui::Ui, _f: &mut eframe::Frame) {
        let now = Instant::now();
        if let Some(p) = self.last {
            self.frames += 1;
            self.worst_ms = self.worst_ms.max((now - p).as_secs_f32() * 1000.0);
        }
        self.last = Some(now);
        let ws = *self.window_start.get_or_insert(now);
        if (now - ws).as_secs_f32() >= 1.0 {
            eprintln!("fps ≈ {:>3}   worst frame {:>5.1} ms", self.frames, self.worst_ms);
            self.frames = 0;
            self.worst_ms = 0.0;
            self.window_start = Some(now);
        }

        ui.ctx().request_repaint();
        let t = ui.input(|i| i.time) as f32;
        let r = ui.max_rect();
        let c = r.center();
        let painter = ui.painter();
        painter.rect_filled(r, 0.0, BG);

        let rad = r.width().min(r.height()) * 0.28;
        let fast = t * 0.8; // ~8s orbit — ~1.5 px/frame at this radius
        let slow = t * 0.35; // matches the backdrop — fraction of a px/frame
        let label = |painter: &egui::Painter, p: Pos2, s: &str| {
            painter.text(p + egui::vec2(0.0, -18.0), Align2::CENTER_CENTER, s, FontId::proportional(13.0), Color32::WHITE);
        };

        // A — big circle, fast
        let a = Pos2::new(c.x + rad * fast.cos(), c.y + rad * fast.sin());
        painter.circle_filled(a, 26.0, ACCENT);
        label(painter, a, "A");

        // B — small 4px square, fast
        let b = Pos2::new(c.x + rad * 0.55 * (fast + 2.1).cos(), c.y + rad * 0.55 * (fast + 2.1).sin());
        painter.rect_filled(Rect::from_center_size(b, egui::vec2(4.0, 4.0)), 0.0, ACCENT);
        label(painter, b, "B");

        // C — thin 1px line, fast sweep
        let xc = c.x + rad * (fast * 0.7).sin();
        painter.line_segment([Pos2::new(xc, r.top() + 30.0), Pos2::new(xc, r.bottom() - 30.0)], Stroke::new(1.0, ACCENT));
        painter.text(Pos2::new(xc, r.top() + 18.0), Align2::CENTER_CENTER, "C", FontId::proportional(13.0), Color32::WHITE);

        // E — small 4px square, SLOW (backdrop-like)
        let e = Pos2::new(c.x + rad * 0.85 * slow.cos(), c.y + rad * 0.85 * (slow * 1.1).sin());
        painter.rect_filled(Rect::from_center_size(e, egui::vec2(4.0, 4.0)), 0.0, MINT);
        label(painter, e, "E");
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("anim probe")
            .with_inner_size([720.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native("anim-probe", options, Box::new(|_cc| Ok(Box::<Probe>::default())))
}
