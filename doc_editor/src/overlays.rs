//! In-cell overlays for the `.doc` editor. For now: the slash command palette, painted on a
//! foreground layer anchored to the caret, clamping/flipping within the cell. Deliberately
//! paint-only + geometry: it exposes the rects (`menu_rect`, `item_rects`) the editor hit-tests
//! itself — no interactive widgets here, so the editor never loses focus to the overlay.

use egui::{pos2, vec2, Align2, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, Stroke, StrokeKind};

use crate::block;
use crate::model::BlockKind;
use crate::theme;

/// One insertable block kind, as shown in the palette.
#[derive(Clone, Copy)]
pub struct SlashItem {
    pub kind: BlockKind,
    pub group: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    /// The markdown shortcut shown on the right (a quiet reminder of the input rule).
    pub md: &'static str,
}

/// The items matching `query` (case-insensitive label substring), in catalog order. Derived
/// from [`block::SPECS`] — every kind carrying a `SlashSpec`, in table order.
pub fn filtered(query: &str) -> Vec<SlashItem> {
    let q = query.trim().to_lowercase();
    block::SPECS
        .iter()
        .filter_map(|s| {
            s.slash.as_ref().map(|sl| SlashItem {
                kind: s.kind,
                group: sl.group,
                label: sl.label,
                hint: sl.hint,
                md: sl.md,
            })
        })
        .filter(|it| q.is_empty() || it.label.to_lowercase().contains(&q))
        .collect()
}

// Layout metrics (px).
const W_WIDE: f32 = 320.0;
const W_NARROW: f32 = 260.0;
const HEADER_H: f32 = 34.0;
const GROUP_H: f32 = 22.0;
const ITEM_H: f32 = 40.0;
const FOOTER_H: f32 = 28.0;
const BODY_PAD: f32 = 4.0;
const EDGE: f32 = 8.0;

fn group_count(items: &[SlashItem]) -> usize {
    let mut n = 0;
    let mut last: Option<&str> = None;
    for it in items {
        if last != Some(it.group) {
            n += 1;
            last = Some(it.group);
        }
    }
    n
}

/// The menu's screen rect: sized to its content, anchored just below the caret, clamped
/// inside `cell` and flipped above the caret when there isn't room below.
pub fn menu_rect(items: &[SlashItem], cell: Rect, caret: Rect) -> Rect {
    let narrow = cell.width() < 480.0;
    let w = if narrow { W_NARROW } else { W_WIDE };
    let h = HEADER_H
        + BODY_PAD * 2.0
        + group_count(items) as f32 * GROUP_H
        + items.len() as f32 * ITEM_H
        + FOOTER_H;

    let x = caret.left().clamp(cell.left() + EDGE, (cell.right() - w - EDGE).max(cell.left() + EDGE));
    let below = caret.bottom() + 6.0;
    let y = if below + h <= cell.bottom() - EDGE {
        below
    } else {
        (caret.top() - 6.0 - h).max(cell.top() + EDGE)
    };
    Rect::from_min_size(pos2(x, y), vec2(w, h))
}

/// The clickable row rect of each item, in the same layout `render` draws. The editor
/// hit-tests the pointer against these.
pub fn item_rects(menu: Rect, items: &[SlashItem]) -> Vec<Rect> {
    let mut rects = Vec::with_capacity(items.len());
    let mut y = menu.top() + HEADER_H + BODY_PAD;
    let mut last: Option<&str> = None;
    for it in items {
        if last != Some(it.group) {
            y += GROUP_H;
            last = Some(it.group);
        }
        rects.push(Rect::from_min_size(pos2(menu.left() + BODY_PAD, y), vec2(menu.width() - BODY_PAD * 2.0, ITEM_H)));
        y += ITEM_H;
    }
    rects
}

/// Draw the palette into `menu` (already positioned). `selected` is the highlighted flat
/// index. Pure paint — the caller owns hit-testing via [`item_rects`].
pub fn render(painter: &Painter, menu: Rect, items: &[SlashItem], selected: usize, query: &str) {
    painter.rect_filled(menu, CornerRadius::same(0), theme::BG_2);
    painter.rect_stroke(menu, CornerRadius::same(0), Stroke::new(1.0, theme::BD), StrokeKind::Inside);

    let mono = |s| FontId::new(s, FontFamily::Monospace);
    let ui_font = |s| FontId::new(s, FontFamily::Proportional);

    // ── Search header: "/" + query (or prompt) + caret + esc pill ──────────────────
    let header = Rect::from_min_size(menu.min, vec2(menu.width(), HEADER_H));
    painter.hline(menu.left()..=menu.right(), header.bottom(), Stroke::new(1.0, theme::HAIR));
    painter.text(pos2(header.left() + 11.0, header.center().y), Align2::LEFT_CENTER, "/", mono(13.0), theme::ACCENT);
    let (qtext, qcolor) = if query.is_empty() {
        ("Filter blocks…".to_owned(), theme::MUTED)
    } else {
        (query.to_owned(), theme::FG_1)
    };
    let g = painter.text(pos2(header.left() + 26.0, header.center().y), Align2::LEFT_CENTER, qtext, ui_font(13.0), qcolor);
    painter.vline(
        g.right() + 2.0,
        (header.center().y - 7.0)..=(header.center().y + 7.0),
        Stroke::new(2.0, theme::ACCENT),
    );
    kbd(painter, pos2(header.right() - 11.0, header.center().y), "esc");

    // ── Body: groups + items ────────────────────────────────────────────────────────
    let rects = item_rects(menu, items);
    let mut last_group: Option<&str> = None;
    for (i, (it, row)) in items.iter().zip(&rects).enumerate() {
        if last_group != Some(it.group) {
            painter.text(
                pos2(menu.left() + 13.0, row.top() - 6.0),
                Align2::LEFT_BOTTOM,
                it.group,
                mono(9.0),
                theme::FAINT,
            );
            last_group = Some(it.group);
        }

        let on = i == selected;
        if on {
            painter.rect_filled(*row, CornerRadius::same(0), theme::ACCENT_BG);
            painter.vline(row.left(), row.top()..=row.bottom(), Stroke::new(2.0, theme::ACCENT));
        }

        let icon = Rect::from_min_size(pos2(row.left() + 7.0, row.center().y - 13.0), vec2(26.0, 26.0));
        painter.rect_filled(icon, CornerRadius::same(0), theme::CODE_BG);
        painter.rect_stroke(icon, CornerRadius::same(0), Stroke::new(1.0, theme::HAIR), StrokeKind::Inside);
        painter.text(icon.center(), Align2::CENTER_CENTER, block::type_tag(it.kind), mono(11.0), theme::FG_2);

        let tx = icon.right() + 10.0;
        painter.text(pos2(tx, row.center().y - 7.0), Align2::LEFT_CENTER, it.label, ui_font(13.0), theme::FG_1);
        painter.text(pos2(tx, row.center().y + 8.0), Align2::LEFT_CENTER, it.hint, ui_font(11.0), theme::MUTED);

        if on {
            kbd(painter, pos2(row.right() - 8.0, row.center().y), "↵");
        } else if !it.md.is_empty() {
            painter.text(pos2(row.right() - 10.0, row.center().y), Align2::RIGHT_CENTER, it.md, mono(10.5), theme::FAINT);
        }
    }

    if items.is_empty() {
        painter.text(menu.center(), Align2::CENTER_CENTER, "no matching blocks", ui_font(12.0), theme::MUTED);
    }

    // ── Footer hints ─────────────────────────────────────────────────────────────────
    let footer = Rect::from_min_max(pos2(menu.left(), menu.bottom() - FOOTER_H), menu.max);
    painter.hline(menu.left()..=menu.right(), footer.top(), Stroke::new(1.0, theme::HAIR));
    painter.text(
        pos2(footer.left() + 11.0, footer.center().y),
        Align2::LEFT_CENTER,
        "↑↓ nav    ↵ insert    esc close",
        mono(10.0),
        theme::MUTED,
    );
}

// ── Code-block language dropdown ─────────────────────────────────────────────────────
// A compact picker dropped from a code block's language tag. Same dark/square/hairline look
// as the slash palette, but a fixed short list (Plain text + the bundled languages), so it's
// pointer-first (no search). Like the palette: paint-only here; the editor hit-tests the rects.

/// One row of the language picker.
pub struct LangRow {
    /// The token stored via `set_lang` (`"text"` = plain / no highlighting).
    pub token: &'static str,
    pub label: &'static str,
    /// Whether this is the block's current language (gets a ✓).
    pub current: bool,
}

/// The picker rows for a code block whose stored language is `current` (`None` = unset). The
/// "Plain text" row is current when the language is unset, `"text"`, or an unbundled token.
pub fn lang_rows(current: Option<&str>) -> Vec<LangRow> {
    let cur = current.map(|s| s.to_ascii_lowercase());
    let is_plain = match &cur {
        None => true,
        Some(s) => s == "text" || !code_highlight::supported(s),
    };
    let mut rows = vec![LangRow { token: "text", label: "Plain text", current: is_plain }];
    for l in code_highlight::LANGUAGES {
        rows.push(LangRow { token: l.token, label: l.label, current: cur.as_deref() == Some(l.token) });
    }
    rows
}

const LANG_W: f32 = 188.0;
const LANG_HEADER_H: f32 = 24.0;
const LANG_ITEM_H: f32 = 28.0;
const LANG_PAD: f32 = 4.0;

/// The dropdown's screen rect: right-aligned under the tag `anchor`, clamped inside `cell` and
/// flipped above the tag when there isn't room below.
pub fn lang_menu_rect(cell: Rect, anchor: Rect, n: usize) -> Rect {
    let w = LANG_W;
    let h = LANG_HEADER_H + LANG_PAD * 2.0 + n as f32 * LANG_ITEM_H;
    let x = (anchor.right() - w).clamp(cell.left() + EDGE, (cell.right() - w - EDGE).max(cell.left() + EDGE));
    let below = anchor.bottom() + 4.0;
    let y = if below + h <= cell.bottom() - EDGE { below } else { (anchor.top() - 4.0 - h).max(cell.top() + EDGE) };
    Rect::from_min_size(pos2(x, y), vec2(w, h))
}

/// The clickable row rects, in `render_lang`'s layout. The editor hit-tests against these.
pub fn lang_item_rects(menu: Rect, n: usize) -> Vec<Rect> {
    let mut rects = Vec::with_capacity(n);
    let mut y = menu.top() + LANG_HEADER_H + LANG_PAD;
    for _ in 0..n {
        rects.push(Rect::from_min_size(pos2(menu.left() + LANG_PAD, y), vec2(menu.width() - LANG_PAD * 2.0, LANG_ITEM_H)));
        y += LANG_ITEM_H;
    }
    rects
}

/// Draw the language dropdown. `selected` is the hover/keyboard-highlighted row.
pub fn render_lang(painter: &Painter, menu: Rect, rows: &[LangRow], selected: usize) {
    painter.rect_filled(menu, CornerRadius::same(0), theme::BG_2);
    painter.rect_stroke(menu, CornerRadius::same(0), Stroke::new(1.0, theme::BD), StrokeKind::Inside);

    let mono = |s| FontId::new(s, FontFamily::Monospace);
    let ui_font = |s| FontId::new(s, FontFamily::Proportional);

    let header = Rect::from_min_size(menu.min, vec2(menu.width(), LANG_HEADER_H));
    painter.hline(menu.left()..=menu.right(), header.bottom(), Stroke::new(1.0, theme::HAIR));
    painter.text(pos2(header.left() + 11.0, header.center().y), Align2::LEFT_CENTER, "LANGUAGE", mono(9.0), theme::FAINT);

    let rects = lang_item_rects(menu, rows.len());
    for (i, (row, r)) in rows.iter().zip(&rects).enumerate() {
        if i == selected {
            painter.rect_filled(*r, CornerRadius::same(0), theme::ACCENT_BG);
            painter.vline(r.left(), r.top()..=r.bottom(), Stroke::new(2.0, theme::ACCENT));
        }
        let color = if row.current { theme::FG_1 } else { theme::FG_2 };
        painter.text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, row.label, ui_font(13.0), color);
        if row.current {
            painter.text(pos2(r.right() - 11.0, r.center().y), Align2::RIGHT_CENTER, "✓", ui_font(12.0), theme::ACCENT);
        }
    }
}

/// A small keyboard pill, right-anchored at `right_center`.
fn kbd(painter: &Painter, right_center: Pos2, text: &str) {
    let font = FontId::new(10.0, FontFamily::Monospace);
    let galley = painter.layout_no_wrap(text.to_owned(), font, theme::FG_2);
    let pad = 4.0;
    let w = (galley.size().x + pad * 2.0).max(16.0);
    let h = 16.0;
    let r = Rect::from_min_size(pos2(right_center.x - w, right_center.y - h / 2.0), vec2(w, h));
    painter.rect_filled(r, CornerRadius::same(0), theme::GUTTER_HOVER);
    painter.rect_stroke(r, CornerRadius::same(0), Stroke::new(1.0, theme::BD), StrokeKind::Inside);
    painter.galley(pos2(r.center().x - galley.size().x / 2.0, r.center().y - galley.size().y / 2.0), galley, theme::FG_2);
}
