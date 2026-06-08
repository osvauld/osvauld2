//! Per-frame layout: turn the block tree into positioned [`Placed`] rows with shaped galleys,
//! plus geometry helpers (gutter / checkbox / caret rects, hit-test). Coordinates are relative
//! to the column rect's top-left; callers add `rect.min`.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use egui::{pos2, text::CCursor, vec2, FontFamily, Galley, Rect, Ui};
use loro::TreeID;

use crate::model::{BlockKind, Doc};
use crate::{block, theme};

use super::{CachedBlock, Placed};

/// doc_editor's inline-mark palette handed to the shared `rich_text` renderer (code/link/strike
/// colours). Block-level colours — heading/quote ink, code-block syntax — ride on the runs and the
/// `Style`, not here.
const THEME: rich_text::Theme = rich_text::Theme {
    code_color: theme::ACCENT_SOFT,
    code_bg: theme::CODE_INLINE_BG,
    link_color: theme::ACCENT_HI,
    link_underline: theme::ACCENT_BG,
    strike_color: theme::FAINT,
};

/// Lay out every block into row geometry + galley; the returned `Vec` lines up with `ids`.
/// Galleys are reused from `cache` when content/kind/state/width are unchanged.
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
        // Cap indent depth so deep nesting stops marching off the page (tree keeps real depth).
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

        // The gutter (`+` / grip) is pinned to the page margin at every depth; nesting indents
        // only the spine and the text column to its right.
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
        let style = block_to_style(&st, done);
        // Reuse the cached galley unless styled content changed. The fingerprint covers the runs
        // (text + marks) AND the block-level `Style` — so promoting to a heading (bold) or
        // checking a to-do (strike) reshapes even though the text is untouched — plus a code
        // block's language tag, which drives highlighting from that same text. Computing `runs`
        // per frame is the cost to revisit (Loro diff) at scale.
        let runs = doc.runs(id);
        let fp = {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            rich_text::fingerprint(&runs, style, THEME, wrap).hash(&mut h);
            if kind == BlockKind::Code {
                doc.lang(id).hash(&mut h);
            }
            h.finish()
        };
        let (galley, gh) = match cache.get(&id) {
            Some(c) if c.fp == fp && c.kind == kind && c.done == done && c.wrap == wrap => {
                (c.galley.clone(), c.height)
            }
            _ => {
                // Both paths shape through the shared `rich_text::galley`. Code highlights from its
                // plain source (it carries no inline marks); prose shapes its marked runs.
                let g = if kind == BlockKind::Code {
                    let src: String = runs.iter().map(|r| r.text.as_str()).collect();
                    let cruns = code_runs(&src, doc.lang(id).as_deref());
                    rich_text::galley(ui.ctx(), &cruns, style, THEME, wrap)
                } else {
                    rich_text::galley(ui.ctx(), &runs, style, THEME, wrap)
                };
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

/// Map a block's [`block::BlockStyle`] (+ whether the row is struck — a *done* to-do) onto the
/// shared [`rich_text::Style`]: the block-level base every run inherits before its own marks.
/// `mono` rides on the base font family; `bold`/`italic`/`strike` apply to every run.
pub(super) fn block_to_style(st: &block::BlockStyle, struck: bool) -> rich_text::Style {
    rich_text::Style {
        size: st.font.size,
        color: st.color,
        mono: st.font.family == FontFamily::Monospace,
        bold: st.bold,
        italic: st.italics,
        strike: struck,
        line_height: Some(st.line_height),
        letter_spacing: st.letter_spacing,
    }
}

/// Shape one plain (un-marked) run via `rich_text` — the placeholder inside an empty block.
pub(super) fn plain_galley(ui: &Ui, text: &str, wrap: f32, st: &block::BlockStyle) -> Arc<Galley> {
    rich_text::galley(ui.ctx(), &[rich_text::Run::plain(text)], block_to_style(st, false), THEME, wrap)
}

/// A code block's source as coloured runs: a tree-sitter pass ([`code_highlight`]) maps each span
/// to a [`theme::code_color`]; no (or unsupported) language is one uncoloured run that inherits
/// the block's base ink. The explicit colours bake into the galley, so screen (`paint`), scene,
/// and PDF (`crate::pdf`) all pick them up for free. Code carries no inline marks, so this bypasses
/// the [`Doc::runs`] path entirely.
fn code_runs(src: &str, lang: Option<&str>) -> Vec<rich_text::Run> {
    let spans = if src.is_empty() { None } else { lang.and_then(|l| code_highlight::highlight(l, src)) };
    match spans {
        Some(spans) if !spans.is_empty() => spans
            .into_iter()
            .map(|span| rich_text::Run {
                text: src[span.range].to_string(),
                marks: rich_text::Marks::new(),
                color: Some(theme::code_color(span.kind)),
            })
            .collect(),
        // No highlighting (plain / unsupported / empty): one uncoloured run, also the caret row.
        _ => vec![rich_text::Run::plain(src)],
    }
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

/// The clickable language tag at a code block's top-right — both the hover/click hotspot and
/// the anchor the language dropdown drops from. Spans the top strip of the code box (above the
/// first code line, which starts ~15px down), so clicking it never lands on the code text.
pub(super) fn lang_tag_rect(p: &Placed, rect: Rect) -> Rect {
    const TAG_W: f32 = 104.0;
    let right = rect.left() + p.content_right - 1.0;
    let top = rect.top() + p.row_top + 3.0;
    Rect::from_min_max(pos2(right - TAG_W, top), pos2(right, top + 15.0))
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
