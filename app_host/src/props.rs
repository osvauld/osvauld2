use crate::{LuaMsg, parse_color};
use mlua::{Function, Table, Value};
use runtime::El;
use runtime::vello::peniko::Color;

type E = El<LuaMsg>;

/// A prop's Lua value type + the builder it forwards to. Each variant holds a plain fn pointer,
/// so builder methods are named directly — `El::pad` *is* a `fn(E, f32) -> E`.
///
/// Nothing here is tag-aware: any prop may appear on any node, exactly as CSS ignores `gap` on a
/// non-flex box. Tags differ only in construction and whether they take children.
pub(crate) enum Prop {
    F32(fn(E, f32) -> E),
    Color(fn(E, Color) -> E),
    Flag(fn(E) -> E),
    /// `{ width, "#rrggbb" }`
    Stroke(fn(E, f32, Color) -> E),
}

/// Applied in this order, so shorthands precede the longhands that override them: `full` before
/// `w`/`h`, `pad` before `px`/`py`. Lua table order is unspecified — this list is the only thing
/// making the result deterministic when both are set.
pub(crate) static PROPS: &[(&str, Prop)] = &[
    // box
    ("full", Prop::Flag(El::full)),
    ("w_full", Prop::Flag(El::w_full)),
    ("h_full", Prop::Flag(El::h_full)),
    ("w", Prop::F32(El::w)),
    ("h", Prop::F32(El::h)),
    ("grow", Prop::Flag(El::grow)),
    // spacing
    ("pad", Prop::F32(El::pad)),
    ("px", Prop::F32(El::px)),
    ("py", Prop::F32(El::py)),
    ("gap", Prop::F32(El::gap)),
    ("mt", Prop::F32(El::mt)),
    ("mb", Prop::F32(El::mb)),
    // alignment
    ("center", Prop::Flag(El::center)),
    ("align_center", Prop::Flag(El::align_center)),
    ("stretch", Prop::Flag(El::stretch)),
    // positioning
    ("absolute", Prop::Flag(El::absolute)),
    ("top", Prop::F32(El::top)),
    ("left", Prop::F32(El::left)),
    ("right", Prop::F32(El::right)),
    ("bottom", Prop::F32(El::bottom)),
    // paint
    ("fill", Prop::Color(El::fill)),
    ("color", Prop::Color(El::color)),
    ("radius", Prop::F32(El::radius)),
    ("stroke", Prop::Stroke(El::stroke)),
    ("opacity", Prop::F32(El::opacity)),
    ("font_size", Prop::F32(El::font_size)),
    // hover
    ("hover_fill", Prop::Color(El::hover_fill)),
    ("hover_stroke", Prop::Stroke(El::hover_stroke)),
    ("tint", Prop::F32(El::tint)),
    // animation
    ("fade_in", Prop::F32(El::fade_in)),
    // input
    ("autofocus", Prop::Flag(El::autofocus)),
];

/// Props whose value is a Lua function. Each is registered into `handlers` and reaches the app as
/// `LuaMsg::Call(idx)`; these builders all take a plain `M`, so one fn-pointer shape covers them.
pub(crate) static CALLBACKS: &[(&str, fn(E, LuaMsg) -> E)] = &[
    ("on_click", El::on_click),
    ("on_enter", El::on_enter),
    ("on_esc", El::on_esc),
    ("on_faded_out", El::on_faded_out),
];
pub(crate) static STRUCTURAL: &[&str] = &[
    "tag",
    "id",
    "value",
    "on_input",
    "on_drag",
    "on_drop",
    "on_right_click",
    "scroll",
];
fn as_num(v: &Value) -> Option<f32> {
    v.as_f32().or_else(|| v.as_integer().map(|i| i as f32))
}

pub(crate) fn apply(mut el: E, node: &Table, handlers: &mut Vec<Function>) -> mlua::Result<E> {
    let pairs = node.pairs::<Value, Value>();
    let mut found = Vec::new();
    for pair in pairs {
        let (k, v) = pair?;
        let Some(k) = k.as_str() else { continue };
        if let Some(i) = PROPS.iter().position(|(name, _)| *name == &*k) {
            found.push((i, v));
        } else if let Some((_, build)) = CALLBACKS.iter().find(|(h, _f)| *h == &*k) {
            let f = v
                .as_function()
                .ok_or_else(|| mlua::Error::runtime(format!("{k} expects a function")))?;
            let idx = handlers.len() as u32;
            handlers.push(f.clone());
            el = build(el, LuaMsg::Call(idx));
        } else if !STRUCTURAL.contains(&&*k) {
            return Err(mlua::Error::runtime(format!("unknown prop {k}")));
        }
    }
    // Registry order, not table order: `pad` must land before `px` when both are set.
    found.sort_unstable_by_key(|(i, _)| *i);

    for (i, v) in found {
        let (key, prop) = &PROPS[i];
        let want = |ty| mlua::Error::runtime(format!("{key} expects {ty}, got {}", v.type_name()));
        match prop {
            Prop::F32(f) => el = f(el, as_num(&v).ok_or_else(|| want("a number"))?),
            Prop::Color(f) => {
                let s = v.as_str().ok_or_else(|| want("a color string"))?;
                el = f(el, parse_color(&s)?);
            }
            Prop::Flag(f) => {
                if v.as_boolean().ok_or_else(|| want("a boolean"))? {
                    el = f(el);
                }
            }
            Prop::Stroke(f) => {
                let t = v.as_table().ok_or_else(|| want("{width, color}"))?;
                let w: f32 = t.get(1)?;
                let c: String = t.get(2)?;
                el = f(el, w, parse_color(&c)?);
            }
        }
    }
    Ok(el)
}
