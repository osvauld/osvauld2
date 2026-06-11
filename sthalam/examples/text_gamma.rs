//! A/B text rendering live: keys 1–6 switch the dark-mode coverage curve, H toggles font
//! hinting. Pick what looks smoothest on big-glyph curves ("v", "D"), bake it into theme.rs.

use eframe::egui::{self, epaint::AlphaFromCoverage, Color32, FontFamily, FontId, RichText};
use std::sync::Arc;

const CHOICES: [(&str, AlphaFromCoverage); 6] = [
    ("1  TwoCoverageMinusCoverageSq (egui dark default)", AlphaFromCoverage::TwoCoverageMinusCoverageSq),
    ("2  Gamma(0.5)", AlphaFromCoverage::Gamma(0.5)),
    ("3  Gamma(0.7)", AlphaFromCoverage::Gamma(0.7)),
    ("4  Gamma(0.85)", AlphaFromCoverage::Gamma(0.85)),
    ("5  Linear / Gamma(1.0)", AlphaFromCoverage::Linear),
    ("6  Gamma(1.2)", AlphaFromCoverage::Gamma(1.2)),
];

struct Demo {
    sel: usize,
    hinting: bool,
}

impl eframe::App for Demo {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.input(|i| {
            for (n, key) in [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4, egui::Key::Num5, egui::Key::Num6]
                .iter()
                .enumerate()
            {
                if i.key_pressed(*key) {
                    self.sel = n;
                }
            }
            if i.key_pressed(egui::Key::H) {
                self.hinting = !self.hinting;
            }
        });
        ctx.global_style_mut(|s| {
            s.visuals.panel_fill = Color32::from_rgb(0x0d, 0x10, 0x16);
            s.visuals.text_options.alpha_from_coverage = CHOICES[self.sel].1;
            s.visuals.text_options.font_hinting = self.hinting;
        });

        ui.add_space(24.0);
        ui.label(
            RichText::new(format!(
                "[{}]   hinting = {} (H toggles)   ppp = {}",
                CHOICES[self.sel].0,
                self.hinting,
                ctx.pixels_per_point()
            ))
            .color(Color32::from_rgb(0x8a, 0x91, 0xa0))
            .size(13.0),
        );
        ui.add_space(24.0);
        for size in [36.0, 24.0, 16.0, 13.0] {
            ui.label(
                RichText::new("Apps are Lua over CRDTs — vDavid Wove 0123")
                    .color(Color32::from_rgb(0xe8, 0xea, 0xf0))
                    .font(FontId::new(size, FontFamily::Proportional)),
            );
            ui.add_space(10.0);
        }
    }
}

fn main() -> eframe::Result {
    eframe::run_native(
        "text gamma",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "sans".into(),
                Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/NotoSans-Regular.ttf"))),
            );
            fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "sans".into());
            cc.egui_ctx.set_fonts(fonts);
            Ok(Box::new(Demo { sel: 0, hinting: true }))
        }),
    )
}
