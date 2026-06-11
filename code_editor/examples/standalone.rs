//! Render the Lua code editor full-window for fast iteration on feel:
//! `cargo run -p code_editor --example standalone`. Scaffolding, not the product.
//!
//! Renders the combined-galley view, per-block syntax highlight, and the line-number gutter over a
//! `BlockDoc` seeded from sample Lua (the same `store::doc_from_source` path the shell uses). As of
//! P3b/P3c it is editable — click to place the caret, then type / select / backspace / delete.

use std::sync::Arc;

use code_editor::{store, BlockDoc, Editor, Theme};
use eframe::egui::{self, Color32, FontFamily};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("osvauld · .lua")
            .with_inner_size([960.0, 760.0])
            .with_min_inner_size([480.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native(
        "code_editor",
        options,
        Box::new(|cc| {
            setup(&cc.egui_ctx);
            Ok(Box::new(App { doc: store::doc_from_source(SAMPLE), editor: Editor::new() }))
        }),
    )
}

struct App {
    doc: BlockDoc,
    editor: Editor,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.editor.show(ui, &self.doc, &THEME);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let c = THEME.bg;
        [c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0, 1.0]
    }
}

/// The shell's code palette (mirrors `sthalam::theme` so the example looks like the real tab).
const THEME: Theme = Theme {
    bg: Color32::from_rgb(0x0A, 0x0B, 0x10),
    gutter_bg: Color32::from_rgb(0x0D, 0x0E, 0x13),
    gutter_fg: Color32::from_rgb(0x4D, 0x4E, 0x5C),
    fg: Color32::from_rgb(0xB6, 0xB7, 0xC3),
    punct: Color32::from_rgb(0x7F, 0x81, 0x92),
    rule: Color32::from_rgba_premultiplied(31, 31, 31, 31),
    selection: Color32::from_rgba_premultiplied(48, 46, 92, 110),
    caret: Color32::from_rgb(0x8A, 0x86, 0xE5),
};

/// Install JetBrains Mono as the monospace face (the editor is monospace-only) and paint the page.
fn setup(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "jbmono".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../../sthalam/assets/fonts/JetBrainsMono-Regular.ttf"
        ))),
    );
    // Prepend ours so egui's bundled face stays as glyph fallback.
    fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "jbmono".to_owned());
    ctx.set_fonts(fonts);
}

/// A representative app source: comments, a `local`, two functions, and the returned view — enough
/// to see every block kind and the highlight palette.
const SAMPLE: &str = "\
-- a small counter app
-- click the buttons to change the count

local count = 0

local function clamp(n)
  if n < 0 then return 0 end
  return n
end

function inc()
  count = clamp(count + 1)
end

function dec()
  count = clamp(count - 1)
end

return function()
  return ui.col{ style = { padding = 28, gap = 12 },
    ui.text{ tostring(count), style = { font = 32 } },
    ui.row{ style = { gap = 8 },
      ui.button{ \"-\", on_click = dec },
      ui.button{ \"+\", on_click = inc },
    },
  }
end
";
