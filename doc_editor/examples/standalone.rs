//! Run the `.doc` editor full-window for fast iteration on feel:
//! `cargo run -p doc_editor --example standalone`. Scaffolding, not the product.

use std::sync::Arc;

use doc_editor::{theme, BlockKind, Doc, DocEditor};
use eframe::egui::{self, FontFamily};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("osvauld · .doc")
            .with_inner_size([960.0, 760.0])
            .with_min_inner_size([480.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native(
        "doc_editor",
        options,
        Box::new(|cc| {
            setup(&cc.egui_ctx);
            Ok(Box::new(App { doc: sample_doc(), editor: DocEditor::new() }))
        }),
    )
}

struct App {
    doc: Doc,
    editor: DocEditor,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.editor.show(ui, &self.doc, false);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let c = theme::BG_PAGE;
        [c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0, 1.0]
    }
}

/// Install Inter + JetBrains Mono and paint the page on the near-black canvas.
fn setup(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.to_owned(), Arc::new(egui::FontData::from_static(bytes)));
    };
    add("inter", include_bytes!("../../sthalam/assets/fonts/Inter-Regular.ttf"));
    add("inter_sb", include_bytes!("../../sthalam/assets/fonts/Inter-SemiBold.ttf"));
    add("jbmono", include_bytes!("../../sthalam/assets/fonts/JetBrainsMono-Regular.ttf"));
    // Prepend ours so egui's bundled faces stay as glyph fallback.
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "inter".to_owned());
    fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "jbmono".to_owned());
    fonts
        .families
        .insert(FontFamily::Name(theme::BOLD_FAMILY.into()), vec!["inter_sb".to_owned(), "inter".to_owned()]);
    ctx.set_fonts(fonts);

    ctx.global_style_mut(|s| {
        s.visuals.panel_fill = theme::BG_PAGE;
        s.visuals.override_text_color = Some(theme::FG_1);
    });
}

/// A realistic starter document exercising the full block vocabulary.
fn sample_doc() -> Doc {
    let d = Doc::new();
    // Repurpose the seeded empty paragraph as the opening heading, then append the rest.
    let h1 = d.block_ids()[0];
    d.set_kind(h1, BlockKind::H1);
    d.insert_text(h1, 0, "Architecture · the .doc engine");

    let p1_text = "The editor is a ProseMirror-class engine over a Loro CRDT — an ordered tree of \
         blocks, each its own rich-text container. Click to place the caret; type to edit.";
    let p1 = d.create_block(1, BlockKind::Paragraph, p1_text);

    d.create_block(2, BlockKind::H2, "Document model");
    d.create_block(
        3,
        BlockKind::Paragraph,
        "Every block is a typed node, addressed by a stable id, never by index. Start a \
         line with a markdown prefix to change its kind:",
    );
    d.create_block(4, BlockKind::BulletList, "\"# \" / \"## \" / \"### \" → headings");
    d.create_block(5, BlockKind::BulletList, "\"- \" → bullet · \"1. \" → numbered · \"[] \" → to-do");
    let nested = d.create_block(6, BlockKind::BulletList, "Tab to nest, Shift-Tab to outdent");

    let t1 = d.create_block(7, BlockKind::Todo, "Block schema + transactions");
    d.set_done(t1, true);
    d.create_block(8, BlockKind::Todo, "Decorations: presence, drop lines, comment anchors");

    d.create_block(
        9,
        BlockKind::Quote,
        "Everything is attributed to people — DIDs — not accounts.",
    );

    d.create_block(10, BlockKind::H3, "Conflict resolution");
    let code = d.create_block(
        11,
        BlockKind::Code,
        "fn rebase(anchor: Pos, tx: &Transaction) -> Anchor {\n    tx.steps.iter().fold(anchor, |a, s| s.map(a))\n}",
    );
    d.set_lang(code, "rust");

    d.create_block(12, BlockKind::Divider, "");
    d.create_block(13, BlockKind::Paragraph, "");

    // Applied after the flat build so the top-level indices used above stay valid.
    d.indent(nested);

    mark_word(&d, p1, p1_text, "ProseMirror-class", "bold");
    mark_word(&d, p1, p1_text, "Loro", "code");
    mark_word(&d, p1, p1_text, "rich-text container", "italic");
    d
}

/// Apply mark `key` to the first occurrence of `word` in `text`.
fn mark_word(d: &Doc, id: loro::TreeID, text: &str, word: &str, key: &str) {
    if let Some(byte) = text.find(word) {
        let start = text[..byte].chars().count();
        let end = start + word.chars().count();
        d.mark(id, start, end, key);
    }
}
