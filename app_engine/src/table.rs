//! The table grid renderer: a [`TableSpec`] + queried [`Row`]s in, a [`Node`] subtree out.
//! The data layer (schema, row projection, query, row identity) lives in `table_core`,
//! re-exported here; this module owns only the Node-vocabulary rendering, so the grid rides
//! the existing Node→Taffy→paint spine — scroll, hit-testing, and PDF export come for free.
//! All chrome colours derive from the host text colour with alpha, so the grid reads on a
//! dark app and a light page alike.

use std::ops::Range;

use egui::Color32;
use rich_text::Run;
pub(crate) use table_core::{
    apply, coerce, format_date, legal_options, parse_date, parse_decimal, read_rows, read_schema,
    CellValue, ColKind, Column, Row, TableSpec,
};

use crate::node::{Align, Edges, Node, Position, Resize, ScrollSpec, Val};

/// What the host wired a cell to: an `on_click` handler index (check flip), an editor binding id
/// (text/number/date edit-in-place), a select combobox, or nothing (read-only).
pub(crate) enum CellBind {
    None,
    Click(u32),
    Edit(String),
    /// A select cell: always an editor (click focuses, typing filters); `popup` is `Some` while
    /// focused — the candidate dropdown plus the live filter query shown in the field.
    Select { id: String, popup: Option<SelectPopup> },
}

/// The focused select cell's dropdown: the typed filter, the candidates as (label, commit
/// handler), and a no-op handler so clicks on popup chrome never reach what's underneath.
pub(crate) struct SelectPopup {
    pub query: String,
    pub options: Vec<(String, u32)>,
    pub guard: u32,
}

/// Build the grid subtree: a pinned header row, a hairline, then a scrollable body of banded
/// data rows. The outer node scrolls x (wide fixed columns move header + body together); the
/// body scrolls y when the app constrains the table's height, header staying put — so a table
/// is a data grid out of the box. `region` keys the retained offsets (the list name).
/// `color`/`font_size` come from the table node's resolved style; the caller overlays the outer
/// style afterwards. `bind(row, column)` wires a live cell — a check-flip handler, an editor
/// binding, or a select combobox ([`CellBind::None`] = read-only) — keeping this module free
/// of mlua.
pub(crate) fn grid(
    spec: &TableSpec,
    rows: &[Row],
    heights: &[f32],
    first: usize,
    lead: f32,
    trail: f32,
    region: &str,
    color: Color32,
    font_size: f32,
    bind: &mut dyn FnMut(&Row, &Column) -> CellBind,
) -> Node {
    let tint = |a: u8| Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), a);

    // Rows are at least as wide as every column's width floor (fixed width, else its kind's flex
    // minimum), so a too-narrow host overflows the grid's x-scroll region horizontally instead of
    // crushing flex columns to slivers.
    let fixed: f32 = spec.columns.iter().map(|c| c.width.unwrap_or(flex_min(c.kind))).sum();
    let span = |n: Node| {
        let mut n = n.width(Val::Pct(100.0));
        n.style.min_width = Val::Px(fixed);
        n
    };

    let header_font = (font_size - 2.0).max(10.0);
    let header = span(Node::row()).children(
        spec.columns
            .iter()
            .map(|c| {
                let mut b =
                    cell_box(c, Node::text(c.label.clone()).font(header_font).color(tint(150)));
                // Drag this cell's right edge to resize the column.
                b.resize = Some(Resize::Col { table: region.to_string(), key: c.key.clone() });
                b
            })
            .collect(),
    );
    let divider = span(Node::col()).height(Val::Px(1.0)).bg(tint(48));

    // The body is virtualized: `rows` is only the visible window, `first` its absolute start
    // index, and `lead`/`trail` are spacer heights reserving the off-screen rows above and below
    // so the scroll extent (and thumb) stays full-size. Each row is pinned to the uniform `row_h`
    // the window was computed against, so the rendered extent matches the reserved extent exactly
    // (no scroll drift). Banding keys off the absolute index, so it's stable as you scroll.
    let mut body = span(Node::col());
    body.scroll = Some(ScrollSpec::y(format!("__tbody:{region}")));
    let mut kids: Vec<Node> = Vec::with_capacity(rows.len() + 2);
    if lead > 0.0 {
        let mut s = span(Node::col());
        s.style.height = Val::Px(lead);
        kids.push(s);
    }
    for (i, row) in rows.iter().enumerate() {
        let mut node = span(Node::row());
        // Drag a row's bottom edge to resize that row (kept by id, correct under sort).
        node.resize = Some(Resize::Row { table: region.to_string(), row: row.id.clone() });
        node.style.height = Val::Px(heights[i]);
        node.style.flex_shrink = 0.0;
        if (first + i) % 2 == 1 {
            node = node.bg(tint(10)).radius(4.0);
        }
        node = node.children(
            spec.columns.iter().map(|c| cell(c, row, color, font_size, &tint, bind)).collect(),
        );
        kids.push(node);
    }
    if trail > 0.0 {
        let mut s = span(Node::col());
        s.style.height = Val::Px(trail);
        kids.push(s);
    }
    body.children = kids;

    let mut outer = Node::col().width(Val::Pct(100.0)).children(vec![header, divider, body]);
    outer.scroll = Some(ScrollSpec::x(format!("__table:{region}")));
    outer
}

/// The visible row window for a `total`-row body of uniform height `row_h`, scrolled to `offset_y`
/// inside a `viewport_h`-tall viewport. Returns the `[first, last)` slice to build, plus the spacer
/// heights reserving the rows above and below it. A few `OVERSCAN` rows each side absorb fast-scroll
/// and the one-frame offset lag. A non-finite `viewport_h` (the default — headless / PDF / tests)
/// yields the whole range: virtualization is a live-view optimization, off unless a viewport is set.
pub(crate) fn row_window(
    total: usize,
    row_h: f32,
    offset_y: f32,
    viewport_h: f32,
) -> (Range<usize>, f32, f32) {
    if total == 0 || row_h <= 0.0 {
        return (0..total, 0.0, 0.0);
    }
    const OVERSCAN: usize = 4;
    // Float→int casts saturate, so an infinite/huge viewport collapses to the full range.
    let first = ((offset_y / row_h).floor() as usize).saturating_sub(OVERSCAN);
    let count = ((viewport_h / row_h).ceil() as usize).saturating_add(1 + 2 * OVERSCAN);
    let last = first.saturating_add(count).min(total);
    let lead = first as f32 * row_h;
    let trail = total.saturating_sub(last) as f32 * row_h;
    (first..last, lead, trail)
}

/// Variable-height counterpart of [`row_window`]: the visible window over rows of individual
/// `heights` (taken only when some rows were drag-resized — otherwise the uniform path stays O(1)).
/// Walks cumulative heights, so it's O(rows); a non-finite viewport yields the whole range.
pub(crate) fn row_window_variable(
    heights: &[f32],
    offset_y: f32,
    viewport_h: f32,
) -> (Range<usize>, f32, f32) {
    let total = heights.len();
    if total == 0 {
        return (0..0, 0.0, 0.0);
    }
    if !viewport_h.is_finite() {
        return (0..total, 0.0, 0.0);
    }
    const OVERSCAN: usize = 4;
    let bottom = offset_y + viewport_h;
    // First row whose bottom edge passes the viewport top; first row whose top is past its bottom.
    let mut cum = 0.0_f32;
    let mut first = total;
    let mut last = total;
    for (i, &h) in heights.iter().enumerate() {
        if first == total && cum + h > offset_y {
            first = i;
        }
        if cum >= bottom {
            last = i;
            break;
        }
        cum += h;
    }
    let first = first.min(total.saturating_sub(1)).saturating_sub(OVERSCAN);
    let last = last.saturating_add(OVERSCAN).min(total);
    let lead: f32 = heights[..first].iter().sum();
    let trail: f32 = heights[last..].iter().sum();
    (first..last, lead, trail)
}

/// One cell's content by column type, in its sized box. Live cells go through `bind`: a check's
/// whole box is the click target flipping its bool; a text/number/date cell becomes an editor
/// leaf filling the box, so clicking anywhere in the cell places the caret; a select cell is a
/// combobox — an editor whose dropdown floats beneath while focused.
fn cell(
    col: &Column,
    row: &Row,
    color: Color32,
    font: f32,
    tint: &impl Fn(u8) -> Color32,
    bind: &mut dyn FnMut(&Row, &Column) -> CellBind,
) -> Node {
    let v = row.cells.get(&col.key).cloned().unwrap_or(CellValue::Empty);
    let bound = bind(row, col);
    let content = match (&col.kind, &bound) {
        (ColKind::Text | ColKind::Number | ColKind::Decimal | ColKind::Date, CellBind::Edit(id)) => {
            let mut n = Node::editor(id.clone(), vec![Run::plain(v.display())]);
            n = n.font(font).color(color).width(Val::Pct(100.0));
            n
        }
        (ColKind::Text | ColKind::Number | ColKind::Decimal | ColKind::Date, _) => {
            Node::text(v.display()).font(font).color(color)
        }
        (ColKind::Check, _) => check(matches!(v, CellValue::Bool(true)), tint),
        (ColKind::Select, CellBind::Select { id, popup }) => {
            // The combobox field: shows the value at rest, the filter query while open.
            let shown = popup.as_ref().map_or_else(|| v.display(), |p| p.query.clone());
            let mut field = Node::editor(id.clone(), vec![Run::plain(shown)])
                .font(font)
                .color(color)
                .width(Val::Pct(100.0));
            field.style.padding =
                Edges { top: Val::Px(2.0), bottom: Val::Px(2.0), left: Val::Px(6.0), right: Val::Px(6.0) };
            field = field.bg(tint(14)).radius(6.0);
            match popup {
                Some(p) => Node::col()
                    .width(Val::Pct(100.0))
                    .children(vec![field, dropdown(p, color, font, tint)]),
                None => field,
            }
        }
        (ColKind::Select, _) if v == CellValue::Empty => Node::text(""),
        (ColKind::Select, _) => pill(v.display(), font, tint),
    };
    let mut b = cell_box(col, content);
    if let CellBind::Click(h) = bound {
        b.on_click = Some(h);
    }
    b
}

/// The floating candidate list under a focused select cell. An opaque surface (derived from
/// the text colour's polarity, so it reads on dark and light themes) on the popup layer.
fn dropdown(p: &SelectPopup, color: Color32, font: f32, tint: &impl Fn(u8) -> Color32) -> Node {
    let dark_text = (color.r() as u16 + color.g() as u16 + color.b() as u16) < 384;
    let surface = if dark_text {
        Color32::from_rgb(0xfc, 0xfc, 0xfa)
    } else {
        Color32::from_rgb(0x20, 0x23, 0x28)
    };
    let mut pop = Node::col().padding(4.0).radius(8.0).bg(surface).border(1.0, tint(48));
    pop.popup = true;
    pop.on_click = Some(p.guard);
    pop.style.position = Position::Absolute;
    pop.style.inset = Edges {
        top: Val::Pct(100.0),
        left: Val::Px(0.0),
        right: Val::Auto,
        bottom: Val::Auto,
    };
    pop.style.min_width = Val::Px(140.0);
    pop.children = if p.options.is_empty() {
        vec![option_row(Node::text("no match").font(font).color(tint(110)), None)]
    } else {
        p.options
            .iter()
            .map(|(label, h)| {
                option_row(Node::text(label.clone()).font(font).color(color), Some(*h))
            })
            .collect()
    };
    pop
}

fn option_row(label: Node, on_click: Option<u32>) -> Node {
    let mut o = Node::row().width(Val::Pct(100.0)).radius(4.0).children(vec![label]);
    o.style.padding =
        Edges { top: Val::Px(5.0), bottom: Val::Px(5.0), left: Val::Px(8.0), right: Val::Px(8.0) };
    if on_click.is_some() {
        o.on_click = on_click;
        let mut hover = o.style.clone();
        hover.background = Some(Color32::from_rgba_unmultiplied(0x80, 0x80, 0x80, 48));
        o.hover = Some(hover);
    }
    o
}

/// The shared cell container: header and data cells use the same widths, so columns align.
/// A flex column is `width: 0` + `grow: 1` (flex-basis 0), so every row splits leftover space
/// identically.
fn cell_box(col: &Column, content: Node) -> Node {
    let mut b = Node::row().children(vec![content]);
    b.style.padding = Edges {
        top: Val::Px(6.0),
        bottom: Val::Px(6.0),
        left: Val::Px(10.0),
        right: Val::Px(10.0),
    };
    b.style.align_items = Some(Align::Center);
    match col.width {
        Some(w) => {
            b.style.width = Val::Px(w);
            b.style.flex_shrink = 0.0;
        }
        None => {
            b.style.width = Val::Px(0.0);
            b.style.flex_grow = 1.0;
            b.style.min_width = Val::Px(flex_min(col.kind));
        }
    }
    if matches!(col.kind, ColKind::Number | ColKind::Decimal) {
        b.style.justify_content = Some(Align::End);
    }
    b
}

/// A flex (auto-width) column's minimum width by kind: compact kinds stay narrow, free text gets
/// room. Their sum sets the row's min width, so a wide table overflows into x-scroll.
fn flex_min(kind: ColKind) -> f32 {
    match kind {
        ColKind::Check => 56.0,
        ColKind::Number | ColKind::Decimal => 90.0,
        ColKind::Date => 110.0,
        ColKind::Text | ColKind::Select => 140.0,
    }
}

/// A glyph-free checkbox: filled when true, outlined when false (no font dependency).
fn check(on: bool, tint: &impl Fn(u8) -> Color32) -> Node {
    let b = Node::col().width(Val::Px(14.0)).height(Val::Px(14.0)).radius(4.0);
    if on {
        b.bg(tint(200))
    } else {
        b.border(1.5, tint(90))
    }
}

fn pill(label: String, font: f32, tint: &impl Fn(u8) -> Color32) -> Node {
    let mut p = Node::text(label).font((font - 2.0).max(10.0)).color(tint(220)).bg(tint(26)).radius(99.0);
    p.style.padding =
        Edges { top: Val::Px(3.0), bottom: Val::Px(3.0), left: Val::Px(8.0), right: Val::Px(8.0) };
    p
}
