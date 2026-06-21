//! PDF export for page-declaring apps. Layout runs once at the page rect (1 px = 1 pt) and the
//! same [`Placed`] list the screen paints is emitted through [`pdf_paint`] — paper matches glass
//! by construction. Only the resting look prints (hover/active/caret/selection are screen
//! chrome), colours go verbatim onto the white sheet (apps design in print colours), and box
//! shadows / corner radii don't print yet. Single page; overflow clips at the sheet edge.

use std::sync::Arc;

use egui::{pos2, vec2, FontFamily, Rect, Stroke};
use pdf_paint::printpdf::{Mm, Op, PaintMode, PdfDocument, PdfPage, PdfSaveOptions};
use pdf_paint::{over_white, rect_op, rgb, Emit, FontSet, PageMap};
pub use pdf_paint::FontBytes;

use crate::layout::{self, Placed};
use crate::{EngineApp, ViewSource};

const MM_PER_PT: f32 = 25.4 / 72.0;

impl EngineApp {
    /// Render the app to single-page PDF bytes at its declared page size. `fonts` are the TTFs
    /// the host registered with egui (regular / bold-family / mono), embedded so glyph positions
    /// match the shaped galleys.
    pub fn export_pdf(&mut self, fonts: FontBytes) -> Result<Vec<u8>, String> {
        let page = self.page().ok_or("app declares no page")?;
        let root = match &mut self.view {
            ViewSource::Static(node) => node.clone(),
            ViewSource::Script { script, .. } => {
                script.full_viewport(); // export every row, never the on-screen window
                script.view()?
            }
        };
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(page.width, page.height));
        let scroll = self.scroll.clone();

        // Lay out inside a throwaway pass on the engine's own context — export must not depend
        // on (or disturb) a host frame. The embedded faces are installed for shaping too, so
        // glyph advances match the PDF exactly. 1.0 ppp so galley metrics are in pt directly.
        install_fonts(&self.ctx, &fonts);
        let mut placed = Vec::new();
        self.ctx.set_pixels_per_point(1.0);
        let input = egui::RawInput { screen_rect: Some(rect), ..Default::default() };
        #[allow(deprecated)] // `Context::run` is the simplest headless driver
        let _ = self.ctx.run(input, |ctx| {
            placed = layout::layout(ctx, rect, &root, &scroll);
        });

        let mut pdf = PdfDocument::new("App");
        let fonts = FontSet::load(&mut pdf, fonts);
        let mut ops = Vec::new();
        // The white sheet (the page preview's surface).
        ops.push(Op::SetFillColor { col: rgb(1.0, 1.0, 1.0) });
        ops.push(rect_op(0.0, 0.0, page.width, page.height, PaintMode::Fill));
        let map = PageMap {
            page_h: page.height,
            offset_x: 0.0,
            offset_y: 0.0,
            src_left: 0.0,
            src_top: 0.0,
            scale: 1.0,
        };
        let mut emit = Emit { ops: &mut ops, map: &map, fonts: &fonts, ink: &over_white };
        for node in &placed {
            emit_node(&mut emit, node);
        }

        let pages = vec![PdfPage::new(Mm(page.width * MM_PER_PT), Mm(page.height * MM_PER_PT), ops)];
        let mut warn = Vec::new();
        Ok(pdf.with_pages(pages).save(&PdfSaveOptions::default(), &mut warn))
    }
}

/// Make the export faces egui's shaping faces: proportional / mono / the rich-text bold family
/// all resolve to the embedded TTFs. Shared with the screenshot path.
pub(crate) fn install_fonts(ctx: &egui::Context, fonts: &FontBytes) {
    let mut defs = egui::FontDefinitions::default();
    let mut add = |name: &str, bytes: &[u8]| {
        defs.font_data.insert(name.to_owned(), Arc::new(egui::FontData::from_owned(bytes.to_vec())));
    };
    add("export_sans", fonts.regular);
    add("export_sans_sb", fonts.bold);
    add("export_mono", fonts.mono);
    for (i, fb) in fonts.fallback.iter().enumerate() {
        add(&format!("export_fb{i}"), fb);
    }
    let prop = defs.families.entry(FontFamily::Proportional).or_default();
    prop.insert(0, "export_sans".to_owned());
    for i in 0..fonts.fallback.len() {
        prop.insert(1 + i, format!("export_fb{i}"));
    }
    defs.families.entry(FontFamily::Monospace).or_default().insert(0, "export_mono".to_owned());
    defs.families
        .entry(FontFamily::Name(rich_text::BOLD_FAMILY.into()))
        .or_default()
        .insert(0, "export_sans_sb".to_owned());
    ctx.set_fonts(defs);
}

/// One box, resting state: fill → border → recoloured text, the screen painter's order.
fn emit_node(e: &mut Emit, node: &Placed) {
    let look = node.base;
    let a = look.opacity;
    if let Some(bg) = look.background {
        e.rect(node.rect, Some(bg.gamma_multiply(a)), None);
    }
    if let Some(b) = look.border {
        e.rect(node.rect, None, Some(Stroke::new(b.width, b.color.gamma_multiply(a))));
    }
    if let Some((origin, galley)) = &node.text {
        e.galley(*origin, galley, look.color.gamma_multiply(a));
    }
}
