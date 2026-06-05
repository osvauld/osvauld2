//! Native preview of the intro app — runs the exact same egui UI as the wasm app,
//! in a desktop window, so you can see/iterate the look without the wasm host.
//!
//!   cargo run -p intro-app --example preview --release
//!
//! Prints a frame-timing line each second (fps + worst frame) to stderr so we can
//! tell apart "low/spiky frame rate" (a perf problem) from "steady rate but looks
//! jittery" (a pacing/motion problem). Dev harness only; the app ships as wasm.

use std::time::Instant;

use eframe::egui;
use intro_app::Intro;

struct Preview {
    state: Intro,
    last: Option<Instant>,
    window_start: Option<Instant>,
    frames: u32,
    worst_ms: f32,
}

impl Default for Preview {
    fn default() -> Self {
        Self { state: Intro::default(), last: None, window_start: None, frames: 0, worst_ms: 0.0 }
    }
}

impl eframe::App for Preview {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        if let Some(prev) = self.last {
            let dt_ms = (now - prev).as_secs_f32() * 1000.0;
            self.frames += 1;
            self.worst_ms = self.worst_ms.max(dt_ms);
        }
        self.last = Some(now);

        let ws = *self.window_start.get_or_insert(now);
        if (now - ws).as_secs_f32() >= 1.0 {
            eprintln!(
                "frames/s ≈ {:>3}   worst frame {:>5.1} ms   (16.7ms = 60fps, 6.9ms = 144fps)",
                self.frames, self.worst_ms
            );
            self.frames = 0;
            self.worst_ms = 0.0;
            self.window_start = Some(now);
        }

        intro_app::draw(ui, &mut self.state);
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("osvauld · intro (preview)")
            .with_inner_size([760.0, 720.0])
            .with_min_inner_size([420.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "intro-preview",
        options,
        Box::new(|_cc| Ok(Box::<Preview>::default())),
    )
}
