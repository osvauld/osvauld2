//! Per-frame **layout**: turn the block tree into positioned [`Placed`] rows with shaped
//! galleys, plus the geometry helpers (gutter / checkbox / caret rects, vertical hit-test).
//! All coordinates are relative to the column rect's top-left; callers add `rect.min`.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use egui::{pos2, text::{CCursor, LayoutJob}, vec2, FontFamily, FontId, Galley, Rect, Stroke, TextFormat, Ui};
use loro::TreeID;

use crate::model::{BlockKind, Doc, Run};
use crate::{block, theme};

use super::{CachedBlock, Placed};

/// Lay out every block: compute its row geometry (relative to the column top-left) and
/// its text galley. The returned `Vec` lines up index-for-index with `ids`. Shaped galleys
/// are reused from `cache` for blocks whose content/kind/state/width are unchanged.
pub(super) fn layout_all(
    doc: &Doc,
    ids: &[TreeID],
    ui: &Ui,
    width: f32,
    cache: &mut HashMap<TreeID, CachedBlock>,
) -> (Vec<Placed>, f32) {
    let mut placed = Vec::with_capacity(ids.len());
    let mut top = theme::PAD_TOP;
    let mut ol_counters: Vec<usize> = Vec::new(); // running ordinal per depth

    for &id in ids {
        let kind = doc.kind(id);
        // Structural depth: a block's indent is its position in the tree, capped so deep
        // nesting stops marching rightward off the page (the tree keeps the real depth).
        let depth = doc.depth(id).min(8);
        let done = doc.done(id);

        // Numbered-list ordinals: count a run at each depth; any other block (or a
        // shallower one) resets that depth and everything deeper.
        if ol_counters.len() <= depth {
            ol_counters.resize(depth + 1, 0);
        }
        let ordinal = if kind == BlockKind::NumberedList {
            ol_counters[depth] += 1;
            for c in ol_counters.iter_mut().skip(depth + 1) {
                *c = 0;
            }
            Some(ol_counters[depth])
        } else {
            for c in ol_counters.iter_mut().skip(depth) {
                *c = 0;
            }
            None
        };

        // The gutter (`+` / grip) is pinned to the page margin — the same x at every depth.
        // Nesting indents only the spine and the text column to its right.
        let gutter_left = theme::OUTER_LEFT;
        let spine_x = theme::OUTER_LEFT + theme::GUTTER + depth as f32 * theme::INDENT;
        let content_x = spine_x + theme::SPINE + theme::CONTENT_PAD;
        let content_right =
            (content_x + theme::MAX_CONTENT).min(width - theme::RIGHT_PAD).max(content_x + 160.0);

        let lead_w = block::spec(kind).lead_w;
        let text_x = content_x + lead_w;
        let text_right = if kind == BlockKind::Code { content_right - 14.0 } else { content_right };
        let wrap = (text_right - text_x).max(80.0);

        let st = block::block_style(kind);
        // Reuse the cached galley unless this block's *styled* content changed. The key is a
        // fingerprint of the runs (text + marks), not just length, so applying a mark (which
        // leaves the length unchanged) still invalidates. A hit skips the `LayoutJob` build +
        // shaping (the expensive part). Computing `runs` per frame is the documented cost to
        // revisit (switch to Loro's diff) once docs get large.
        let runs = doc.runs(id);
        let fp = fingerprint(&runs);
        let (galley, gh) = match cache.get(&id) {
            Some(c) if c.fp == fp && c.kind == kind && c.done == done && c.wrap == wrap => {
                (c.galley.clone(), c.height)
            }
            _ => {
                let g = layout_runs(ui, &runs, wrap, &st, done);
                let h = g.size().y;
                cache.insert(id, CachedBlock { fp, kind, done, wrap, galley: g.clone(), height: h });
                (g, h)
            }
        };

        let (content_top, row_top, row_bottom) = match kind {
            BlockKind::Divider => (top, top, top + 33.0),
            BlockKind::Code => {
                let content_top = top + 4.0 + 11.0; // outer margin + box padding
                (content_top, top, content_top + gh + 11.0 + 4.0)
            }
            _ => {
                let content_top = top + st.py;
                (content_top, top, content_top + gh + st.py)
            }
        };
        top = row_bottom;

        placed.push(Placed {
            id,
            kind,
            depth,
            done,
            ordinal,
            galley,
            gutter_left,
            spine_x,
            content_x,
            content_right,
            text_x,
            content_top,
            row_top,
            row_bottom,
        });
    }

    // Drop cache entries for blocks that no longer exist (deleted / never reused).
    let live: HashSet<TreeID> = ids.iter().copied().collect();
    cache.retain(|id, _| live.contains(id));

    (placed, top + theme::PAD_BOTTOM)
}

/// The block's base text format (font, colour, line height, tracking) before per-run marks.
fn base_format(st: &block::BlockStyle) -> TextFormat {
    TextFormat {
        font_id: st.font.clone(),
        color: st.color,
        line_height: Some(st.line_height),
        extra_letter_spacing: st.letter_spacing,
        italics: st.italics,
        ..Default::default()
    }
}

/// The bold proportional family **if the consumer registered it**, else the default
/// proportional faces. egui panics on an unbound `FontFamily::Name`, so this lets an embedder
/// that hasn't bundled a bold face render un-bolded instead of crashing (and keeps headless
/// tests, which set up no fonts, working).
fn bold_family(ui: &Ui) -> FontFamily {
    let want = theme::bold_family();
    let registered = ui.ctx().fonts(|f| f.definitions().families.contains_key(&want));
    if registered {
        want
    } else {
        FontFamily::Proportional
    }
}

/// Lay out one block's *plain* text into a galley (used for placeholders — no marks).
pub(super) fn layout_run(ui: &Ui, text: &str, wrap: f32, st: &block::BlockStyle, struck: bool) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    let mut fmt = base_format(st);
    if st.bold {
        fmt.font_id = FontId::new(st.font.size, bold_family(ui));
    }
    if struck {
        fmt.strikethrough = Stroke::new(1.0, theme::FAINT);
    }
    job.append(text, 0.0, fmt);
    ui.ctx().fonts_mut(|f| f.layout_job(job))
}

/// Lay out a block's **styled runs** into one galley: each run appends with the base style
/// plus its inline marks — bold (a real bold face, or regular if none is bound), italic (egui
/// slant), strikethrough, inline code (mono + tinted + a soft fill), and link (accent +
/// underline). A bold *block* (heading) bolds every run. `struck` (a done to-do) strikes all.
pub(super) fn layout_runs(ui: &Ui, runs: &[Run], wrap: f32, st: &block::BlockStyle, struck: bool) -> Arc<Galley> {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    let bold_fam = bold_family(ui);
    let size = st.font.size;
    // An empty block still needs one (empty) section so the galley has a caret row.
    if runs.is_empty() {
        let mut fmt = base_format(st);
        if st.bold {
            fmt.font_id = FontId::new(size, bold_fam.clone());
        }
        job.append("", 0.0, fmt);
    }
    for run in runs {
        let mut fmt = base_format(st);
        if run.code {
            // Inline code wins the font: mono, slightly smaller, tinted, on a soft fill.
            fmt.font_id = FontId::new(size * 0.92, FontFamily::Monospace);
            fmt.color = theme::ACCENT_SOFT;
            fmt.background = theme::CODE_INLINE_BG;
        } else if st.bold || run.bold {
            fmt.font_id = FontId::new(size, bold_fam.clone());
        }
        if run.italic {
            fmt.italics = true;
        }
        if run.link.is_some() {
            fmt.color = theme::ACCENT_HI;
            fmt.underline = Stroke::new(1.0, theme::ACCENT_BG);
        }
        if run.strike || struck {
            let color = if struck { theme::FAINT } else { fmt.color };
            fmt.strikethrough = Stroke::new(1.0, color);
        }
        job.append(&run.text, 0.0, fmt);
    }
    ui.ctx().fonts_mut(|f| f.layout_job(job))
}

/// A cheap content+marks fingerprint for the galley cache (changes whenever any run's text
/// or marks change, even when the total length doesn't).
fn fingerprint(runs: &[Run]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    runs.hash(&mut h);
    h.finish()
}

// --- Geometry helpers (return absolute rects given the column rect) ----------------

fn gutter_pad_top(kind: BlockKind) -> f32 {
    block::block_style(kind).py.max(2.0) + if kind == BlockKind::H1 { 6.0 } else { 2.0 }
}

pub(super) fn plus_rect(p: &Placed, rect: Rect) -> Rect {
    let right = rect.left() + p.gutter_left + theme::GUTTER - 16.0;
    let top = rect.top() + p.row_top + gutter_pad_top(p.kind);
    Rect::from_min_size(pos2(right - 18.0, top), vec2(18.0, 20.0))
}

pub(super) fn grip_rect(p: &Placed, rect: Rect) -> Rect {
    let right = rect.left() + p.gutter_left + theme::GUTTER;
    let top = rect.top() + p.row_top + gutter_pad_top(p.kind);
    Rect::from_min_size(pos2(right - 16.0, top), vec2(16.0, 20.0))
}

pub(super) fn checkbox_rect(p: &Placed, rect: Rect) -> Rect {
    Rect::from_min_size(pos2(rect.left() + p.content_x, rect.top() + p.content_top + 3.0), vec2(16.0, 16.0))
}

/// The on-screen rect of a block's caret at offset 0 — the slash palette's anchor.
pub(super) fn caret_screen_rect(rect: Rect, p: &Placed) -> Rect {
    let cr = p.galley.pos_from_cursor(CCursor::new(0));
    let x = rect.left() + p.text_x;
    let y = rect.top() + p.content_top;
    Rect::from_min_max(pos2(x + cr.left(), y + cr.top()), pos2(x + cr.right(), y + cr.bottom()))
}

pub(super) fn block_at_y(placed: &[Placed], rect: Rect, y_abs: f32) -> Option<usize> {
    if placed.is_empty() {
        return None;
    }
    let y = y_abs - rect.top();
    let mut idx = 0;
    for (i, p) in placed.iter().enumerate() {
        if y >= p.row_top {
            idx = i;
        } else {
            break;
        }
    }
    Some(idx)
}
