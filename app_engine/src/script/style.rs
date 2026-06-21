//! The CSS-vocab style parser: an app's `style` table overlaid onto a [`Style`], plus the token
//! parsers for lengths, edges, corners, borders, shadows, and colours.

use egui::Color32;
use mlua::{Table, Value};

use crate::node::{Align, Border, BoxShadow, Corners, Direction, Edges, Position, Style, Val};

use super::{bool_field, color_field, field, lua_str, num, str_field};

/// Overlay an app's `style` table onto a base [`Style`]. Keys follow CSS names (snake_case),
/// with the original short forms kept as aliases. Unknown keys are ignored; missing keys keep
/// the base value.
pub(super) fn parse_style(style: Value, mut base: Style) -> Style {
    let Value::Table(t) = style else {
        return base;
    };
    // -- flex container -------------------------------------------------------
    if let Some(d) = str_field(&t, "flex_direction").or_else(|| str_field(&t, "direction")) {
        base.direction = if d == "row" { Direction::Row } else { Direction::Column };
    }
    if bool_field(&t, "flex_wrap") || str_field(&t, "flex_wrap").as_deref() == Some("wrap") {
        base.wrap = true;
    }
    if let Some(a) = align_field(&t, "justify_content").or_else(|| align_field(&t, "justify")) {
        base.justify_content = Some(a);
    }
    if let Some(a) = align_field(&t, "align_items") {
        base.align_items = Some(a);
    }
    if let Some(a) = align_field(&t, "align_self") {
        base.align_self = Some(a);
    }
    if let Some(v) = num(&t, "gap") {
        base.gap = v;
    }
    if let Some(v) = num(&t, "flex_grow").or_else(|| num(&t, "grow")) {
        base.flex_grow = v;
    }
    if let Some(v) = num(&t, "flex_shrink") {
        base.flex_shrink = v;
    }
    // -- box ------------------------------------------------------------------
    if let Some(v) = dim(&t, "width") {
        base.width = v;
    }
    if let Some(v) = dim(&t, "height") {
        base.height = v;
    }
    if let Some(v) = dim(&t, "min_width") {
        base.min_width = v;
    }
    if let Some(v) = dim(&t, "min_height") {
        base.min_height = v;
    }
    if let Some(v) = dim(&t, "max_width") {
        base.max_width = v;
    }
    if let Some(v) = dim(&t, "max_height") {
        base.max_height = v;
    }
    if let Some(e) = edges(&t, "padding") {
        base.padding = e;
    }
    if let Some(e) = edges(&t, "margin") {
        base.margin = e;
    }
    // -- position -------------------------------------------------------------
    if let Some(p) = str_field(&t, "position") {
        base.position = if p == "absolute" { Position::Absolute } else { Position::Relative };
    }
    if let Some(e) = edges(&t, "inset") {
        base.inset = e;
    }
    for (key, side) in [("top", 0), ("right", 1), ("bottom", 2), ("left", 3)] {
        if let Some(v) = dim(&t, key) {
            match side {
                0 => base.inset.top = v,
                1 => base.inset.right = v,
                2 => base.inset.bottom = v,
                _ => base.inset.left = v,
            }
        }
    }
    // -- paint ------------------------------------------------------------------
    if let Some(c) = color_field(&t, "background").or_else(|| color_field(&t, "bg")) {
        base.background = Some(c);
    }
    if let Some(c) = corners(&t) {
        base.corner_radius = c;
    }
    if let Some(b) = border_field(&t, "border") {
        base.border = Some(b);
    }
    if let Some(s) = shadow_field(&t, "box_shadow").or_else(|| shadow_field(&t, "shadow")) {
        base.shadow = Some(s);
    }
    if let Some(v) = num(&t, "opacity") {
        base.opacity = v.clamp(0.0, 1.0);
    }
    // -- text -------------------------------------------------------------------
    if let Some(c) = color_field(&t, "color") {
        base.color = c;
    }
    if let Some(v) = num(&t, "font_size").or_else(|| num(&t, "font")) {
        base.font_size = v;
    }
    base
}

/// One CSS alignment keyword (`-` and `_` both accepted, `flex-start`/`start` alike).
fn align_field(t: &Table, key: &str) -> Option<Align> {
    let s = str_field(t, key)?;
    match s.replace('_', "-").as_str() {
        "start" | "flex-start" => Some(Align::Start),
        "center" => Some(Align::Center),
        "end" | "flex-end" => Some(Align::End),
        "stretch" => Some(Align::Stretch),
        "baseline" => Some(Align::Baseline),
        "space-between" => Some(Align::SpaceBetween),
        "space-around" => Some(Align::SpaceAround),
        "space-evenly" => Some(Align::SpaceEvenly),
        _ => None,
    }
}

/// One length token: `auto`, `50%`, `24`, `24px`, `1.5rem` (1rem = 16px).
fn parse_val(s: &str) -> Option<Val> {
    let s = s.trim();
    if s == "auto" {
        return Some(Val::Auto);
    }
    if let Some(p) = s.strip_suffix('%') {
        return p.trim().parse().ok().map(Val::Pct);
    }
    if let Some(r) = s.strip_suffix("rem") {
        return r.trim().parse::<f32>().ok().map(|v| Val::Px(v * 16.0));
    }
    let s = s.strip_suffix("px").unwrap_or(s);
    s.trim().parse().ok().map(Val::Px)
}

/// A length: a number is pixels; strings go through [`parse_val`] (`"auto"`, `"50%"`, `"1.5rem"`).
fn dim(t: &Table, key: &str) -> Option<Val> {
    match field(t, key) {
        Value::Integer(i) => Some(Val::Px(i as f32)),
        Value::Number(n) => Some(Val::Px(n as f32)),
        Value::String(s) => parse_val(&lua_str(&s)),
        _ => None,
    }
}

/// Per-side values: a number applies to all sides; a string is the CSS 1/2/3/4-value shorthand
/// (`"10 20"` = vertical horizontal, …), each token a [`parse_val`] length.
fn edges(t: &Table, key: &str) -> Option<Edges> {
    match field(t, key) {
        Value::Integer(i) => Some(Edges::px(i as f32)),
        Value::Number(n) => Some(Edges::px(n as f32)),
        Value::String(s) => {
            let s = lua_str(&s);
            let v: Vec<Val> = s.split_whitespace().filter_map(parse_val).collect();
            match v.as_slice() {
                [a] => Some(Edges::all(*a)),
                [v, h] => Some(Edges { top: *v, bottom: *v, left: *h, right: *h }),
                [top, h, bottom] => {
                    Some(Edges { top: *top, bottom: *bottom, left: *h, right: *h })
                }
                [top, right, bottom, left] => {
                    Some(Edges { top: *top, right: *right, bottom: *bottom, left: *left })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// `border_radius` (alias `corner`): a number rounds all corners; a string is the CSS 4-value
/// form (`"12 12 0 0"`, top-left first, clockwise).
fn corners(t: &Table) -> Option<Corners> {
    let v = field(t, "border_radius");
    let v = if matches!(v, Value::Nil) { field(t, "corner") } else { v };
    match v {
        Value::Integer(i) => Some(Corners::same(i as f32)),
        Value::Number(n) => Some(Corners::same(n as f32)),
        Value::String(s) => {
            let s = lua_str(&s);
            let r: Vec<f32> = s
                .split_whitespace()
                .filter_map(|tok| parse_val(tok).and_then(|v| match v {
                    Val::Px(px) => Some(px),
                    _ => None,
                }))
                .collect();
            match r.as_slice() {
                [a] => Some(Corners::same(*a)),
                [tl, tr, br, bl] => Some(Corners { tl: *tl, tr: *tr, br: *br, bl: *bl }),
                _ => None,
            }
        }
        _ => None,
    }
}

/// CSS-ish `border`: `"1 #3a4151"` / `"2px solid #fff"` — first number is the width, first
/// parsable colour is the colour, `solid` is noise.
fn border_field(t: &Table, key: &str) -> Option<Border> {
    let s = str_field(t, key)?;
    let mut width = None;
    let mut color = None;
    for tok in s.split_whitespace() {
        if width.is_none() {
            if let Some(Val::Px(px)) = parse_val(tok) {
                width = Some(px);
                continue;
            }
        }
        if color.is_none() {
            if let Some(c) = parse_color(tok) {
                color = Some(c);
            }
        }
    }
    Some(Border { width: width?, color: color? })
}

/// CSS-ish `box_shadow`: `"0 4 12 #0008"` (offset-x offset-y blur [spread] colour).
fn shadow_field(t: &Table, key: &str) -> Option<BoxShadow> {
    let s = str_field(t, key)?;
    let mut nums = Vec::new();
    let mut color = None;
    for tok in s.split_whitespace() {
        match parse_val(tok) {
            Some(Val::Px(px)) if nums.len() < 4 => nums.push(px),
            _ => {
                if color.is_none() {
                    color = Some(parse_color(tok)?);
                }
            }
        }
    }
    if nums.len() < 2 {
        return None;
    }
    Some(BoxShadow {
        offset: [nums[0], nums[1]],
        blur: nums.get(2).copied().unwrap_or(0.0),
        spread: nums.get(3).copied().unwrap_or(0.0),
        color: color.unwrap_or(Color32::from_black_alpha(96)),
    })
}

/// Parse any CSS color (hex incl. `#rgb`/`#rgba`, `rgb()`, `hsl()`, `oklch()`, named) into a
/// colour, via `csscolorparser` (CSS Color Level 4).
pub(super) fn parse_color(s: &str) -> Option<Color32> {
    let [r, g, b, a] = csscolorparser::parse(s.trim()).ok()?.to_rgba8();
    Some(Color32::from_rgba_unmultiplied(r, g, b, a))
}
