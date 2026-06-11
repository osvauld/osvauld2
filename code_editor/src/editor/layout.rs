use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use block_doc::{BlockDoc, BlockId};
use code_highlight::{HlKind, Span};
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontFamily, FontId, Galley, Ui};

use super::Theme;

pub(super) const FONT_SIZE: f32 = 12.5;

pub(super) fn font() -> FontId {
    FontId::new(FONT_SIZE, FontFamily::Monospace)
}

pub(super) struct CachedHl {
    text_hash: u64,
    spans: Vec<Span>,
}

pub(super) struct Layout {
    pub galley: Arc<Galley>,
    /// Each block's char range in the combined source — resolved by the caret/selection mapping.
    pub blocks: Vec<(BlockId, Range<usize>)>,
}

pub(super) fn layout(
    ui: &Ui,
    doc: &BlockDoc,
    theme: &Theme,
    hl: &mut HashMap<BlockId, CachedHl>,
) -> Layout {
    let ids = doc.block_ids();
    let font = font();
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY; // code never wraps; it scrolls horizontally

    let mut blocks = Vec::with_capacity(ids.len());
    let mut base = 0usize;

    for &id in &ids {
        let text = doc.text(id);
        let spans = cached_spans(hl, id, &text);
        for span in &spans {
            let fmt = TextFormat {
                font_id: font.clone(),
                color: code_color(span.kind, theme),
                ..Default::default()
            };
            job.append(&text[span.range.clone()], 0.0, fmt);
        }
        let start = base;
        base += text.chars().count();
        blocks.push((id, start..base));
    }

    let live: HashSet<BlockId> = ids.iter().copied().collect();
    hl.retain(|id, _| live.contains(id));

    let galley = ui.ctx().fonts_mut(|f| f.layout_job(job));
    Layout { galley, blocks }
}

fn cached_spans(hl: &mut HashMap<BlockId, CachedHl>, id: BlockId, text: &str) -> Vec<Span> {
    let h = hash_text(text);
    if let Some(c) = hl.get(&id) {
        if c.text_hash == h {
            return c.spans.clone();
        }
    }
    let spans = code_highlight::highlight("lua", text)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| vec![Span { range: 0..text.len(), kind: HlKind::Text }]);
    hl.insert(id, CachedHl { text_hash: h, spans: spans.clone() });
    spans
}

fn hash_text(text: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

pub(super) fn code_color(kind: HlKind, theme: &Theme) -> Color32 {
    use HlKind::*;
    match kind {
        Keyword => Color32::from_rgb(0xC4, 0xA7, 0xF7),
        Function => Color32::from_rgb(0x82, 0xAA, 0xFF),
        Type => Color32::from_rgb(0x7F, 0xD1, 0xC0),
        Constant | Attribute => Color32::from_rgb(0xFF, 0xCB, 0x6B),
        Number => Color32::from_rgb(0xFF, 0x9E, 0x64),
        String => Color32::from_rgb(0x9E, 0xCE, 0x6A),
        Comment => Color32::from_rgb(0x6B, 0x6D, 0x7E),
        Property => Color32::from_rgb(0x89, 0xDD, 0xFF),
        Operator => Color32::from_rgb(0xC0, 0xCA, 0xF5),
        Tag | Escape => Color32::from_rgb(0xF7, 0x76, 0x8E),
        Punctuation => theme.punct,
        Variable | Text => theme.fg,
    }
}
