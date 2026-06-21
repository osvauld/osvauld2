//! Paint laid-out boxes onto an `egui::Ui` — the whole draw step.
//!
//! [`crate::layout`] already did the thinking (geometry, wrapping, colour-neutral galleys), so
//! this is a flat back-to-front loop: per box pick the `Look` for the pointer state, fill the
//! background, apply a default feedback tint only when the app declared no state styles, and
//! stamp the recoloured galley (plus selection + caret for the focused editor).

use egui::epaint::Shadow;
use egui::{pos2, Color32, CornerRadius, Stroke, StrokeKind};
use text_edit::TextField;

use crate::layout::Placed;
use crate::node::{ChartKind, ChartSpec, Corners};

/// The selection highlight (the accent at low alpha) painted behind a focused editor's glyphs.
const SELECTION: Color32 = Color32::from_rgba_premultiplied(0x2b, 0x44, 0x73, 0x80);

/// Per-corner radii in egui's u8 form.
fn corner_radius(c: Corners) -> CornerRadius {
    let r = |v: f32| v.clamp(0.0, 255.0) as u8;
    CornerRadius { nw: r(c.tl), ne: r(c.tr), sw: r(c.bl), se: r(c.br) }
}

/// Live pointer state for click feedback: cursor position in app-local coordinates (`None` when
/// not over the cell) and whether the primary button is held.
pub(crate) struct Pointer {
    pub hover: Option<egui::Pos2>,
    pub pressed: bool,
}

/// Paint every box in order (parents first, so children land on top). `focus` is the focused
/// editor's id and retained caret/selection, if any — drawn on the box whose `editor` id matches.
pub(crate) fn paint(ui: &egui::Ui, placed: &[Placed], pointer: &Pointer, focus: Option<(&str, &TextField)>) {
    for node in placed {
        // Clip to the scroll region so scrolled-out content doesn't bleed; non-scrolled boxes
        // carry the cell rect (a no-op clip).
        let painter = ui.painter().with_clip_rect(node.clip);
        let over = pointer.hover.is_some_and(|p| node.rect.contains(p));
        let clickable = node.on_click.is_some();

        // pressed (clickable) → active, hovered → hover, else resting — each falling back to the
        // resting look when the app didn't define it.
        let look = if over && pointer.pressed && clickable {
            node.active.unwrap_or(node.base)
        } else if over {
            node.hover.unwrap_or(node.base)
        } else {
            node.base
        };

        let radius = corner_radius(look.corner_radius);
        let alpha = look.opacity;

        // Shadow → fill → border, back-to-front like CSS paints a box.
        if let Some(s) = look.shadow {
            let shadow = Shadow {
                offset: [s.offset[0].round() as i8, s.offset[1].round() as i8],
                blur: s.blur.clamp(0.0, 255.0) as u8,
                spread: s.spread.clamp(0.0, 255.0) as u8,
                color: s.color.gamma_multiply(alpha),
            };
            painter.add(shadow.as_shape(node.rect, radius));
        }
        if let Some(bg) = look.background {
            painter.rect_filled(node.rect, radius, bg.gamma_multiply(alpha));
        }
        if let Some(b) = look.border {
            painter.rect_stroke(
                node.rect,
                radius,
                Stroke::new(b.width, b.color.gamma_multiply(alpha)),
                StrokeKind::Inside,
            );
        }

        // Default feedback only for clickable boxes that declared no states of their own, so
        // plain buttons still respond without the app spelling out hover/active.
        if over && clickable && node.hover.is_none() && node.active.is_none() {
            let overlay =
                if pointer.pressed { Color32::from_white_alpha(42) } else { Color32::from_white_alpha(16) };
            painter.rect_filled(node.rect, radius, overlay);
        }

        // Editable affordances, derived from the text colour so they read on any theme: the
        // focused field gets a ring + faint fill, an unfocused one a hover wash + text cursor.
        let focused_here = focus.is_some_and(|(id, _)| node.editor.as_deref() == Some(id));
        if focused_here {
            let r = node.rect.expand(3.0);
            painter.rect_filled(r, 3.0, look.color.gamma_multiply(alpha * 0.05));
            painter.rect_stroke(
                r,
                3.0,
                Stroke::new(1.0, look.color.gamma_multiply(alpha * 0.45)),
                StrokeKind::Outside,
            );
        } else if over && node.editor.is_some() {
            painter.rect_filled(node.rect.expand(3.0), 3.0, Color32::from_white_alpha(8));
        }
        if over && node.editor.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }

        if let Some((origin, galley)) = &node.text {
            // This box's editor field, when it's the focused one.
            let field = focus.filter(|(id, _)| node.editor.as_deref() == Some(*id)).map(|(_, f)| f);
            let shift = origin.to_vec2();

            // Selection highlight goes *behind* the glyphs.
            if let Some(field) = field {
                for r in field.selection_rects(galley) {
                    painter.rect_filled(r.translate(shift), 0.0, SELECTION.gamma_multiply(alpha));
                }
            }
            // The galley is colour-neutral (`PLACEHOLDER`); the state's colour applies here.
            // Caveat: baked per-run colours ignore alpha — only neutral text dims for now.
            painter.galley(*origin, galley.clone(), look.color.gamma_multiply(alpha));
            // The caret goes on top of the glyphs.
            if let Some(field) = field {
                let c = field.caret_rect(galley).translate(shift);
                let x = c.center().x;
                painter.line_segment(
                    [pos2(x, c.top()), pos2(x, c.bottom())],
                    Stroke::new(1.5, look.color.gamma_multiply(alpha)),
                );
            }
        }
    }
}

/// Paint one `ui.chart` leaf with `egui_plot` into its laid-out `rect`, in a clipped child `Ui`.
/// `idx` keys the plot's persisted memory so multiple charts in a view don't collide. Static (no
/// pan/zoom). A categorical x-axis: bars/points/line sit at positions `0..n`, the labels show on the
/// ticks. Series colours cycle a small palette tinted to read on the host theme.
pub(crate) fn paint_chart(ui: &mut egui::Ui, idx: usize, spec: &ChartSpec, rect: egui::Rect) {
    use egui_plot::{Bar, BarChart, Legend, Line, Plot, Points};

    let labels = spec.x_labels.clone();
    let n = labels.len();
    let n_series = spec.series.len();

    // Clip to the chart rect AND the ambient viewport, so a scrolled chart can't paint over the tab
    // strip / neighbouring panels (egui_plot otherwise draws into our raw rect, ignoring the scroll
    // clip). Reserve a bottom band for our own slanted x-labels — egui_plot can't rotate axis text
    // (hardcoded angle 0, upstream TODO #162) so it just drops labels that would overlap.
    let viewport = rect.intersect(ui.clip_rect());
    let band = if n > 0 { 42.0 } else { 0.0 };
    let plot_rect = egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.max.y - band));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(plot_rect));
    child.set_clip_rect(plot_rect.intersect(viewport));

    // We draw the x-labels ourselves, so suppress egui_plot's (return empty), but keep one grid tick
    // per category via a custom spacer (its default picks only a couple of integer marks).
    let x_spacer = move |_g: egui_plot::GridInput| -> Vec<egui_plot::GridMark> {
        (0..n).map(|i| egui_plot::GridMark { value: i as f64, step_size: 1.0 }).collect()
    };
    let x_fmt = |_m: egui_plot::GridMark, _r: &std::ops::RangeInclusive<f64>| -> String { String::new() };
    // Hover readout: map the cursor's x back to its category label + the y value. egui_plot draws a
    // tooltip on line/scatter ONLY when a label_formatter is set (else hovering shows nothing).
    let lbls2 = labels.clone();
    let label_fmt = move |_name: &str, pt: &egui_plot::PlotPoint| -> String {
        let i = pt.x.round();
        let lbl = if i >= 0.0 && (i as usize) < lbls2.len() { lbls2[i as usize].as_str() } else { "" };
        format!("{lbl}\n{:.0}", pt.y)
    };

    let resp = Plot::new(("ui_chart", idx))
        .legend(Legend::default())
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_boxed_zoom(false)
        .include_y(0.0) // bars/lines share a 0 baseline
        .set_margin_fraction(egui::vec2(0.03, 0.12)) // headroom so top bars + tooltips aren't clipped
        .x_axis_formatter(x_fmt)
        .x_grid_spacer(x_spacer)
        .label_formatter(label_fmt)
        .show(&mut child, |p| {
            for (s, series) in spec.series.iter().enumerate() {
                let color = series_color(s);
                match spec.kind {
                    ChartKind::Line => {
                        let pts: Vec<[f64; 2]> =
                            series.values.iter().enumerate().map(|(i, &y)| [i as f64, y]).collect();
                        p.line(Line::new(series.name.clone(), pts).color(color).width(2.0));
                    }
                    ChartKind::Scatter => {
                        let pts: Vec<[f64; 2]> =
                            series.values.iter().enumerate().map(|(i, &y)| [i as f64, y]).collect();
                        p.points(Points::new(series.name.clone(), pts).color(color).radius(3.0));
                    }
                    ChartKind::Bar => {
                        // Cluster grouped series side by side within each x slot.
                        let w = 0.8 / n_series.max(1) as f64;
                        let off = (s as f64 - (n_series as f64 - 1.0) / 2.0) * w;
                        let bars: Vec<Bar> = series
                            .values
                            .iter()
                            .enumerate()
                            .map(|(i, &y)| Bar::new(i as f64 + off, y).width(w))
                            .collect();
                        p.bar_chart(BarChart::new(series.name.clone(), bars).color(color));
                    }
                }
            }
        });

    // Draw the x-labels ourselves so wide ones aren't dropped — but HORIZONTAL (rotated text in
    // epaint bypasses pixel-snapping and renders blurry). Avoid overlap by staggering adjacent labels
    // across two rows instead of slanting them.
    if n > 0 {
        let transform = resp.transform;
        let frame = *transform.frame();
        let color = ui.visuals().text_color();
        let font = egui::FontId::proportional(11.0);
        let painter = ui.painter().with_clip_rect(viewport);
        let row_h = 15.0;
        for (i, raw) in labels.iter().enumerate() {
            let text: String = if raw.chars().count() > 14 {
                format!("{}…", raw.chars().take(13).collect::<String>())
            } else {
                raw.clone()
            };
            let galley = painter.layout_no_wrap(text, font.clone(), color);
            let tick_x = transform.position_from_point(&egui_plot::PlotPoint::new(i as f64, 0.0)).x;
            let y = frame.bottom() + 4.0 + (i % 2) as f32 * row_h; // alternate rows → no overlap
            let pos = egui::pos2((tick_x - galley.size().x / 2.0).round(), y.round());
            painter.add(egui::epaint::TextShape::new(pos, galley, color));
        }
    }
}

/// A small categorical palette for chart series (legible on dark and light).
fn series_color(i: usize) -> Color32 {
    const PALETTE: [Color32; 6] = [
        Color32::from_rgb(0x4c, 0x8b, 0xf5),
        Color32::from_rgb(0x52, 0xc4, 0x1a),
        Color32::from_rgb(0xe5, 0xc0, 0x7b),
        Color32::from_rgb(0xe0, 0x6c, 0x75),
        Color32::from_rgb(0xc6, 0x78, 0xdd),
        Color32::from_rgb(0x56, 0xb6, 0xc2),
    ];
    PALETTE[i % PALETTE.len()]
}
