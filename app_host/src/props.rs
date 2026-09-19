use std::rc::Rc;

use crate::{Ctx, DragArgs, Handlers, Key, LuaMsg, Shape, parse_color, register};
use mlua::{Table, Value};
use runtime::El;
use runtime::vello::peniko::Color;
use std::marker::PhantomData;

/// How a Lua value becomes a Rust value. Keyed on the *type*, not the prop — once `Color`
/// implements this, every builder taking a `Color` decodes for free, at any arity.
pub(crate) trait FromProp: Sized {
    fn from_prop(v: &Value) -> mlua::Result<Self>;
}

fn want(v: &Value, ty: &str) -> mlua::Error {
    mlua::Error::runtime(format!("expected {ty} got {}", v.type_name()))
}

/// Handlers, drag state, zoom and press springs are all keyed by element `id`; without one they
/// are unreachable.
fn missing_id<'a>(id: Option<&'a str>, prop: &str) -> mlua::Result<&'a str> {
    id.ok_or_else(|| mlua::Error::runtime(format!("{prop} needs an id")))
}

impl FromProp for f32 {
    fn from_prop(v: &Value) -> mlua::Result<Self> {
        v.as_f32()
            .or_else(|| v.as_integer().map(|i| i as f32))
            .ok_or_else(|| want(v, "a number"))
    }
}

impl FromProp for bool {
    fn from_prop(v: &Value) -> mlua::Result<Self> {
        v.as_boolean().ok_or_else(|| want(v, "a bool"))
    }
}

impl FromProp for String {
    fn from_prop(v: &Value) -> mlua::Result<Self> {
        v.as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| want(v, "a str"))
    }
}
impl FromProp for Color {
    fn from_prop(v: &Value) -> mlua::Result<Self> {
        parse_color(&String::from_prop(v)?)
    }
}
impl<A: FromProp, B: FromProp> FromProp for (A, B) {
    fn from_prop(v: &Value) -> mlua::Result<Self> {
        let t = v.as_table().ok_or_else(|| want(v, "a list"))?;
        Ok((
            A::from_prop(&t.get::<Value>(1)?)?,
            B::from_prop(&t.get::<Value>(2)?)?,
        ))
    }
}

pub(crate) struct DragCtx<'a, M> {
    pub handlers: &'a mut Handlers,
    pub id: Option<&'a str>,
    pub to_msg: Rc<dyn Fn(LuaMsg) -> M>,
}
impl<M> DragCtx<'_, M> {
    /// Register the prop's function under this element's id — the key its messages carry.
    fn register(&mut self, name: &'static str, v: &Value) -> mlua::Result<Key> {
        let id = missing_id(self.id, name)?;
        let f = v.as_function().ok_or_else(|| want(v, "function"))?;
        register(self.handlers, id, name, f.clone())
    }
}

/// Every prop erased to one signature, so arity stops being part of the type.
type Apply<M> = fn(El<M>, &Value) -> mlua::Result<El<M>>;

/// `prop!(pad, f32)` expands to `("pad", |el, v| Ok(El::pad(el, f32::from_prop(v)?)))`.
///
/// The Lua name is `stringify!`d off the builder ident, so the two can't drift —
/// 0 args is a boolean gate, 1 arg decodes the value directly, n args decode a positional list.
/// Arms are enumerated rather than counted; std does the same for its tuple impls.
macro_rules! prop {
    ($f:ident) => {
        (
            stringify!($f),
            (|el, v| Ok(if bool::from_prop(v)? { El::$f(el) } else { el })) as Apply<M>,
        )
    };
    ($f:ident, $t: ty) => {
        (
            stringify!($f),
            (|el, v| Ok(El::$f(el, <$t>::from_prop(v)?))) as Apply<M>,
        )
    };
    ($f:ident, $a:ty, $b:ty) => {
        (
            stringify!($f),
            (|el, v| {
                let t = v.as_table().ok_or_else(|| want(v, "a list"))?;
                Ok(El::$f(
                    el,
                    <$a>::from_prop(&t.get::<Value>(1)?)?,
                    <$b>::from_prop(&t.get::<Value>(2)?)?,
                ))
            }) as Apply<M>,
        )
    };

    ($f:ident, $a:ty, $b:ty, $c: ty) => {
        (
            stringify!($f),
            (|el, v| {
                let t = v.as_table().ok_or_else(|| want(v, "a list"))?;
                Ok(El::$f(
                    el,
                    <$a>::from_prop(&t.get::<Value>(1)?)?,
                    <$b>::from_prop(&t.get::<Value>(2)?)?,
                    <$c>::from_prop(&t.get::<Value>(3)?)?,
                ))
            }) as Apply<M>,
        )
    };
    ($f:ident, $a:ty, $b:ty, $c: ty, $d: ty) => {
        (
            stringify!($f),
            (|el, v| {
                let t = v.as_table().ok_or_else(|| want(v, "a list"))?;
                Ok(El::$f(
                    el,
                    <$a>::from_prop(&t.get::<Value>(1)?)?,
                    <$b>::from_prop(&t.get::<Value>(2)?)?,
                    <$c>::from_prop(&t.get::<Value>(3)?)?,
                    <$d>::from_prop(&t.get::<Value>(4)?)?,
                ))
            }) as Apply<M>,
        )
    };
}

pub(crate) struct Registry<M>(PhantomData<M>);
impl<M: 'static> Registry<M> {
    pub(crate) const BINDS: &'static [(&str, Bind<M>)] = &[
        // Element-local, zoom undone: the numbers the app placed the element's contents with.
        ("on_click", |el, v, cx| {
            let key = cx.register("on_click", v)?;
            let to_msg = cx.to_msg.clone();
            Ok(el.on_click_at(move |at| {
                to_msg(LuaMsg::CallAt(
                    key.clone(),
                    at.pos.0,
                    at.pos.1,
                    at.shape.clone().into(),
                ))
            }))
        }),
        ("on_frame", |el, v, cx| {
            let key = cx.register("on_frame", v)?;
            let to_msg = cx.to_msg.clone();
            Ok(el.on_frame(key.id.to_string(), move |event| {
                to_msg(LuaMsg::CallFrame(key.clone(), event.dt, event.elapsed))
            }))
        }),
        ("on_drag", |el, v, cx| {
            let key = cx.register("on_drag", v)?;
            let to_msg = cx.to_msg.clone();
            Ok(el.on_drag(key.id.to_string(), move |e| {
                to_msg(LuaMsg::CallDrag(
                    key.clone(),
                    DragArgs {
                        phase: e.phase.as_str(),
                        at: e.at,
                        delta: e.delta,
                        scale: e.scale,
                        origin: (e.pos.0 - e.grab.0, e.pos.1 - e.grab.1),
                        shape: Shape(e.shape.clone().map(|(id, at)| (id.to_string(), at.0, at.1))),
                    },
                ))
            }))
        }),
        ("on_drop", |el, v, cx| {
            let key = cx.register("on_drop", v)?;
            let to_msg = cx.to_msg.clone();
            Ok(el.on_drop(key.id.to_string(), move |e| {
                to_msg(LuaMsg::CallPhase(
                    key.clone(),
                    e.phase.as_str(),
                    e.pos.0 / e.size.0,
                    e.pos.1 / e.size.1,
                    Shape::default(),
                ))
            }))
        }),
        // Element-local, like on_click.
        ("on_hover", |el, v, cx| {
            let key = cx.register("on_hover", v)?;
            let to_msg = cx.to_msg.clone();
            Ok(el.on_hover(key.id.to_string(), move |e| {
                to_msg(LuaMsg::CallPhase(
                    key.clone(),
                    e.phase.as_str(),
                    e.pos.0,
                    e.pos.1,
                    e.shape.clone().into(),
                ))
            }))
        }),
    ];
    /// Applied in this order, so shorthands precede the longhands that override them: `full` before
    /// `w`/`h`, `size` before both, `pad` before `px`/`py`, `fade_in` before `fade` (it *is*
    /// `fade(1.0, ms)`). Lua table order is unspecified — this list is the only thing making the
    /// result deterministic when both are set.
    pub(crate) const PROPS: &'static [(&'static str, Apply<M>)] = &[
        // box
        prop!(full),
        prop!(w_full),
        prop!(h_full),
        prop!(size, f32, f32),
        prop!(w, f32),
        prop!(h, f32),
        prop!(no_shrink),
        prop!(min_w, f32),
        prop!(max_w, f32),
        prop!(min_h, f32),
        prop!(max_h, f32),
        // Not `prop!`: `grow` takes a bool *or* a number, and the macro's arms are keyed on one
        // type each. `grow = true` is the flag every app already writes; `grow = 2` is the share a
        // splitter between two elastic children has to write to both of them (§11). `false` means
        // 0.0 rather than "skip", so an override can switch growth off the way it switches it on.
        (
            "grow",
            (|el, v| {
                let n = match v {
                    Value::Boolean(b) => f32::from(*b),
                    _ => f32::from_prop(v)?,
                };
                Ok(El::grow_by(el, n))
            }) as Apply<M>,
        ),
        prop!(wrap),
        // spacing
        prop!(pad, f32),
        prop!(px, f32),
        prop!(py, f32),
        prop!(gap, f32),
        prop!(mt, f32),
        prop!(mb, f32),
        // alignment
        prop!(center),
        prop!(align_center),
        prop!(stretch),
        // positioning
        prop!(absolute),
        prop!(top, f32),
        prop!(left, f32),
        prop!(right, f32),
        prop!(bottom, f32),
        prop!(offset, (f32, f32)),
        // paint
        prop!(fill, Color),
        prop!(color, Color),
        prop!(radius, f32),
        prop!(stroke, f32, Color),
        prop!(stroke_dash, f32, Color, f32, f32),
        prop!(opacity, f32),
        prop!(font_size, f32),
        prop!(no_wrap),
        // hover
        prop!(hover_fill, Color),
        prop!(hover_stroke, f32, Color),
        prop!(tint, f32),
        prop!(press_fill, Color),
        prop!(press_stroke, f32, Color),
        prop!(press_scale, f32),
        // animation
        prop!(fade_in, f32),
        prop!(fade, f32, f32),
        prop!(slide_in, (f32, f32), f32),
        prop!(scale, f32),
        prop!(zoomable),
        prop!(zoom_x),
        // scroll — both need an `id`; `walk` enforces that, since `apply` can't see one
        prop!(scroll_x),
        prop!(scroll_y),
        // input
        prop!(autofocus),
    ];

    /// Props whose value is a Lua function taking no arguments. Each is registered into `handlers`
    /// and reaches the app as `LuaMsg::Call(key)`; these builders all take a plain `M`, so one
    /// fn-pointer shape covers them.
    pub(crate) const CALLBACKS: &'static [(&'static str, fn(El<M>, M) -> El<M>)] = &[
        ("on_enter", El::on_enter),
        ("on_esc", El::on_esc),
        ("on_faded_out", El::on_faded_out),
    ];
}
type Bind<M> = fn(El<M>, &Value, &mut DragCtx<M>) -> mlua::Result<El<M>>;

pub(crate) static STRUCTURAL: &[&str] = &["tag", "id", "value", "on_input", "line"];

pub(crate) fn apply<M: 'static>(
    mut el: El<M>,
    node: &Table,
    context: &mut Ctx<M>,
    id: Option<&str>,
    consumed: &[&str],
) -> mlua::Result<El<M>> {
    let pairs = node.pairs::<Value, Value>();
    let mut found = Vec::new();
    for pair in pairs {
        let (k, v) = pair?;
        let Some(k) = k.as_str() else { continue };
        if let Some(i) = Registry::<M>::PROPS
            .iter()
            .position(|(name, _)| *name == &*k)
        {
            if k == "zoomable" || k == "zoom_x" {
                missing_id(id, &k)?;
            }
            if k == "press_scale" {
                missing_id(id, "press_scale")?;
                if !node.contains_key("on_click")? {
                    return Err(mlua::Error::runtime("press_scale needs on_click"));
                }
            }
            found.push((i, v));
        } else if let Some((name, build)) =
            Registry::<M>::CALLBACKS.iter().find(|(h, _f)| *h == &*k)
        {
            let key = DragCtx {
                handlers: context.handlers,
                id,
                to_msg: context.to_msg.clone(),
            }
            .register(name, &v)?;
            el = build(el, (context.to_msg)(LuaMsg::Call(key)));
        } else if let Some((_, bind)) = Registry::<M>::BINDS.iter().find(|(b, _v)| *b == &*k) {
            el = bind(
                el,
                &v,
                &mut DragCtx {
                    handlers: context.handlers,
                    id,
                    to_msg: context.to_msg.clone(),
                },
            )?;
        } else if !STRUCTURAL.contains(&&*k) && !consumed.contains(&&*k) {
            return Err(mlua::Error::runtime(format!("unknown prop {k}")));
        }
    }
    // Registry order, not table order: `pad` must land before `px` when both are set.
    found.sort_unstable_by_key(|(i, _)| *i);

    for (i, v) in found {
        let (key, f) = &Registry::<M>::PROPS[i];
        // `from_prop` only ever sees the value, so the prop name is reattached here.
        el = f(el, &v).map_err(|e| mlua::Error::runtime(format!("{key}: {e}")))?;
    }
    Ok(el)
}
