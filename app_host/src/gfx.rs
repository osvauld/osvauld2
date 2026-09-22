//! Lua declarations for immutable runtime Frame resources. This module validates and compiles
//! aggregate values; it never renders or retains VM callbacks.

use std::sync::Arc;

use glam::{Quat, Vec3};
use mlua::{Error, Lua, Table, UserData, Value};
use runtime::frame::{
    Brush, Extend, Frame, GradientStop, Item, MAX_GRADIENT_STOPS, MAX_PATH_COMMANDS,
    MAX_STROKE_DASHES, Path, StrokeCap, StrokeJoin, StrokeStyle,
};
use runtime::scene3d::{BuiltinMesh, Camera3d, Object3d, Scene3d, TextSurface};
use runtime::vello::kurbo::{Affine, PathEl, Point};
use runtime::vello::peniko::Fill;

#[derive(Clone)]
#[allow(dead_code)] // Frame compilation consumes the path handle in the next Lua slice.
pub(crate) struct LuaPath(pub Arc<Path>);
impl UserData for LuaPath {}

#[derive(Clone)]
pub(crate) struct LuaBrush(pub Arc<Brush>);
impl UserData for LuaBrush {}

#[derive(Clone)]
pub(crate) struct LuaFrame(pub Arc<Frame>);
impl UserData for LuaFrame {}

#[derive(Clone)]
pub(crate) struct LuaScene3d(pub Arc<Scene3d>);
impl UserData for LuaScene3d {}

#[derive(Clone)]
pub(crate) struct LuaTextSurface(pub Arc<TextSurface>);
impl UserData for LuaTextSurface {}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let gfx = lua.create_table()?;
    gfx.set(
        "path",
        lua.create_function(|lua, commands: Table| {
            let len = positional_len(&commands, "path")?;
            if len > MAX_PATH_COMMANDS {
                return Err(Error::runtime(format!(
                    "path has {len} commands; maximum is {MAX_PATH_COMMANDS}"
                )));
            }
            let mut elements = Vec::with_capacity(len);
            for index in 1..=len {
                elements.push(command(commands.get(index)?, index)?);
            }
            let path = Path::new(elements).map_err(Error::external)?;
            lua.create_userdata(LuaPath(Arc::new(path)))
        })?,
    )?;
    gfx.set(
        "solid",
        lua.create_function(|lua, color: String| {
            let brush = Brush::solid(crate::parse_color(&color)?).map_err(Error::external)?;
            lua.create_userdata(LuaBrush(Arc::new(brush)))
        })?,
    )?;
    gfx.set(
        "linear_gradient",
        lua.create_function(|lua, spec: Table| {
            named_fields(&spec, "linear_gradient", &["from", "to", "stops", "extend"])?;
            let start = point(
                need(&spec, "linear_gradient", "from")?,
                "linear_gradient.from",
            )?;
            let end = point(need(&spec, "linear_gradient", "to")?, "linear_gradient.to")?;
            let stop_table: Table = need(&spec, "linear_gradient", "stops")?;
            let len = positional_len(&stop_table, "linear_gradient.stops")?;
            if len > MAX_GRADIENT_STOPS {
                return Err(Error::runtime(format!(
                    "linear_gradient has too many stops: {len}"
                )));
            }
            let mut stops = Vec::with_capacity(len);
            for index in 1..=len {
                let stop: Table = stop_table.get(index)?;
                if positional_len(&stop, &format!("linear_gradient stop {index}"))? != 2 {
                    return Err(Error::runtime(format!(
                        "linear_gradient stop {index} needs offset and color"
                    )));
                }
                stops.push(
                    GradientStop::new(stop.get(1)?, crate::parse_color(&stop.get::<String>(2)?)?)
                        .map_err(Error::external)?,
                );
            }
            let extend = match spec.get::<Option<String>>("extend")?.as_deref() {
                None | Some("pad") => Extend::Pad,
                Some("repeat") => Extend::Repeat,
                Some("reflect") => Extend::Reflect,
                Some(value) => {
                    return Err(Error::runtime(format!("unknown gradient extend {value:?}")));
                }
            };
            let brush = Brush::linear(start, end, stops, extend).map_err(Error::external)?;
            lua.create_userdata(LuaBrush(Arc::new(brush)))
        })?,
    )?;
    gfx.set(
        "text_surface",
        lua.create_function(|lua, spec: Table| {
            named_fields(
                &spec,
                "text_surface",
                &["text", "font_size", "color", "background"],
            )?;
            let color = |name: &str, default: &str| -> mlua::Result<[f32; 4]> {
                Ok(csscolorparser::parse(
                    &spec
                        .get::<Option<String>>(name)?
                        .unwrap_or_else(|| default.into()),
                )
                .map_err(Error::external)?
                .to_rgba8()
                .map(|v| v as f32 / 255.0))
            };
            lua.create_userdata(LuaTextSurface(Arc::new(TextSurface {
                text: need::<String>(&spec, "text_surface", "text")?.into(),
                font_size: spec.get::<Option<f32>>("font_size")?.unwrap_or(32.0),
                color: color("color", "#ffffff")?,
                background: color("background", "#171923")?,
            })))
        })?,
    )?;
    gfx.set(
        "scene3d",
        lua.create_function(|lua, spec: Table| {
            named_fields(&spec, "scene3d", &["camera", "objects"])?;
            let camera_spec: Table = need(&spec, "scene3d", "camera")?;
            named_fields(
                &camera_spec,
                "scene3d.camera",
                &["eye", "target", "up", "fov_y", "near", "far"],
            )?;
            let camera = Camera3d {
                eye: vec3(need(&camera_spec, "scene3d.camera", "eye")?, "camera.eye")?,
                target: vec3(
                    need(&camera_spec, "scene3d.camera", "target")?,
                    "camera.target",
                )?,
                up: camera_spec
                    .get::<Option<Table>>("up")?
                    .map(|v| vec3(v, "camera.up"))
                    .transpose()?
                    .unwrap_or(Vec3::Y),
                fov_y_radians: camera_spec
                    .get::<Option<f32>>("fov_y")?
                    .unwrap_or(45.0)
                    .to_radians(),
                near: camera_spec.get::<Option<f32>>("near")?.unwrap_or(0.1),
                far: camera_spec.get::<Option<f32>>("far")?.unwrap_or(100.0),
            };
            let objects_spec: Table = need(&spec, "scene3d", "objects")?;
            let len = positional_len(&objects_spec, "scene3d.objects")?;
            let mut objects = Vec::with_capacity(len);
            for index in 1..=len {
                objects.push(object3d(objects_spec.get(index)?, index)?);
            }
            let scene = Scene3d::new(camera, objects).map_err(Error::external)?;
            lua.create_userdata(LuaScene3d(scene))
        })?,
    )?;
    gfx.set(
        "frame",
        lua.create_function(|lua, spec: Table| {
            frame_fields(&spec)?;
            let items = items(&spec)?;
            let frame = Frame::new(
                need(&spec, "frame", "width")?,
                need(&spec, "frame", "height")?,
                spec.get("baseline")?,
                items,
            )
            .map_err(Error::external)?;
            lua.create_userdata(LuaFrame(Arc::new(frame)))
        })?,
    )?;
    lua.globals().set("gfx", gfx)?;
    lua.load(
        r#"
        function gfx.fill(t) t._gfx = "fill" return t end
        function gfx.stroke(t) t._gfx = "stroke" return t end
        function gfx.group(t) t._gfx = "group" return t end
        function gfx.instance(t) t._gfx = "instance" return t end
        "#,
    )
    .exec()
}

fn object3d(spec: Table, index: usize) -> mlua::Result<Object3d> {
    named_fields(
        &spec,
        &format!("scene3d object {index}"),
        &[
            "id", "mesh", "position", "rotation", "scale", "color", "surface",
        ],
    )?;
    let mesh = match spec.get::<Option<String>>("mesh")?.as_deref() {
        None | Some("cube") => BuiltinMesh::Cube,
        Some(mesh) => return Err(Error::runtime(format!("unknown 3D mesh {mesh:?}"))),
    };
    let rotation = spec
        .get::<Option<Table>>("rotation")?
        .map(|v| quat(v, "object.rotation"))
        .transpose()?
        .unwrap_or(Quat::IDENTITY);
    let color = csscolorparser::parse(&need::<String>(&spec, "scene3d object", "color")?)
        .map_err(Error::external)?
        .to_rgba8()
        .map(|v| v as f32 / 255.0);
    Ok(Object3d {
        id: need::<String>(&spec, "scene3d object", "id")?.into(),
        mesh,
        position: spec
            .get::<Option<Table>>("position")?
            .map(|v| vec3(v, "object.position"))
            .transpose()?
            .unwrap_or(Vec3::ZERO),
        rotation,
        scale: spec
            .get::<Option<Table>>("scale")?
            .map(|v| vec3(v, "object.scale"))
            .transpose()?
            .unwrap_or(Vec3::ONE),
        color,
        surface: spec
            .get::<Option<mlua::AnyUserData>>("surface")?
            .map(|surface| {
                surface
                    .borrow::<LuaTextSurface>()
                    .map(|surface| surface.0.clone())
            })
            .transpose()?,
    })
}

fn vec3(table: Table, owner: &str) -> mlua::Result<Vec3> {
    if positional_len(&table, owner)? != 3 {
        return Err(Error::runtime(format!("{owner} needs x, y and z")));
    }
    Ok(Vec3::new(table.get(1)?, table.get(2)?, table.get(3)?))
}

fn quat(table: Table, owner: &str) -> mlua::Result<Quat> {
    if positional_len(&table, owner)? != 4 {
        return Err(Error::runtime(format!("{owner} needs x, y, z and w")));
    }
    Ok(Quat::from_xyzw(
        table.get(1)?,
        table.get(2)?,
        table.get(3)?,
        table.get(4)?,
    ))
}

fn command(command: Table, index: usize) -> mlua::Result<PathEl> {
    let len = positional_len(&command, &format!("path command {index}"))?;
    let name = command.get::<String>(1)?;
    let arity = match name.as_str() {
        "move" | "line" => 3,
        "quad" => 5,
        "cubic" => 7,
        "close" => 1,
        _ => {
            return Err(Error::runtime(format!(
                "path command {index}: unknown {name:?}"
            )));
        }
    };
    if len != arity {
        return Err(Error::runtime(format!(
            "path command {index} ({name}) needs {} values, got {}",
            arity - 1,
            len.saturating_sub(1)
        )));
    }
    let point = |x, y| -> mlua::Result<Point> { Ok(Point::new(command.get(x)?, command.get(y)?)) };
    Ok(match name.as_str() {
        "move" => PathEl::MoveTo(point(2, 3)?),
        "line" => PathEl::LineTo(point(2, 3)?),
        "quad" => PathEl::QuadTo(point(2, 3)?, point(4, 5)?),
        "cubic" => PathEl::CurveTo(point(2, 3)?, point(4, 5)?, point(6, 7)?),
        "close" => PathEl::ClosePath,
        _ => unreachable!(),
    })
}

fn items(table: &Table) -> mlua::Result<Vec<Item>> {
    (1..=table.raw_len())
        .map(|index| item(table.get(index)?, index))
        .collect()
}

fn item(spec: Table, index: usize) -> mlua::Result<Item> {
    let kind = spec.get::<String>("_gfx")?;
    match kind.as_str() {
        "fill" => {
            named_fields(&spec, "fill", &["_gfx", "id", "path", "brush", "rule"])?;
            let path = need_gfx(&spec, "fill", "path", "a gfx.path", |p: &LuaPath| {
                p.0.clone()
            })?;
            let brush = need_gfx(&spec, "fill", "brush", "a brush", |b: &LuaBrush| {
                b.0.clone()
            })?;
            let rule = match spec.get::<Option<String>>("rule")?.as_deref() {
                None | Some("nonzero") => Fill::NonZero,
                Some("evenodd") => Fill::EvenOdd,
                Some(value) => return Err(Error::runtime(format!("unknown fill rule {value:?}"))),
            };
            named(&spec, Item::fill(path, brush, rule))
        }
        "stroke" => {
            named_fields(
                &spec,
                "stroke",
                &[
                    "_gfx",
                    "id",
                    "path",
                    "brush",
                    "width",
                    "cap",
                    "join",
                    "miter_limit",
                    "dashes",
                    "dash_offset",
                ],
            )?;
            let path = need_gfx(&spec, "stroke", "path", "a gfx.path", |p: &LuaPath| {
                p.0.clone()
            })?;
            let brush = need_gfx(&spec, "stroke", "brush", "a brush", |b: &LuaBrush| {
                b.0.clone()
            })?;
            named(&spec, Item::stroke(path, brush, stroke_style(&spec)?))
        }
        "group" => {
            item_fields(&spec, "group", &["_gfx", "id", "transform"])?;
            let group = Item::group(transform(&spec)?, items(&spec)?).map_err(Error::external)?;
            named(&spec, group)
        }
        "instance" => {
            named_fields(&spec, "instance", &["_gfx", "id", "visual", "transform"])?;
            let frame = need_gfx(
                &spec,
                "instance",
                "visual",
                "a gfx.frame",
                |f: &LuaFrame| f.0.clone(),
            )?;
            let instance = Item::instance(transform(&spec)?, frame).map_err(Error::external)?;
            named(&spec, instance)
        }
        _ => Err(Error::runtime(format!(
            "Frame item {index}: unknown kind {kind:?}"
        ))),
    }
}

/// `id` is what a hit reports back. It is optional everywhere: an unnamed shape is paint, and
/// only named ones cost anything to hit-test.
fn named(spec: &Table, item: Item) -> mlua::Result<Item> {
    match spec.get::<Option<String>>("id")? {
        Some(id) if id.is_empty() => Err(Error::runtime("Frame item id must not be empty")),
        Some(id) => Ok(item.with_id(id)),
        None => Ok(item),
    }
}

fn stroke_style(spec: &Table) -> mlua::Result<StrokeStyle> {
    let cap = match spec.get::<Option<String>>("cap")?.as_deref() {
        None | Some("butt") => StrokeCap::Butt,
        Some("square") => StrokeCap::Square,
        Some("round") => StrokeCap::Round,
        Some(value) => return Err(Error::runtime(format!("unknown stroke cap {value:?}"))),
    };
    let join = match spec.get::<Option<String>>("join")?.as_deref() {
        None | Some("miter") => StrokeJoin::Miter,
        Some("bevel") => StrokeJoin::Bevel,
        Some("round") => StrokeJoin::Round,
        Some(value) => return Err(Error::runtime(format!("unknown stroke join {value:?}"))),
    };
    let dashes = match spec.get::<Option<Table>>("dashes")? {
        None => Vec::new(),
        Some(values) => {
            let len = positional_len(&values, "stroke.dashes")?;
            if len > MAX_STROKE_DASHES {
                return Err(Error::runtime(format!("stroke has too many dashes: {len}")));
            }
            (1..=len)
                .map(|index| values.get(index))
                .collect::<mlua::Result<Vec<f64>>>()?
        }
    };
    StrokeStyle::new(
        need(spec, "stroke", "width")?,
        cap,
        join,
        spec.get::<Option<f64>>("miter_limit")?.unwrap_or(4.0),
        dashes,
        spec.get::<Option<f64>>("dash_offset")?.unwrap_or(0.0),
    )
    .map_err(Error::external)
}

fn transform(spec: &Table) -> mlua::Result<Affine> {
    let Some(values) = spec.get::<Option<Table>>("transform")? else {
        return Ok(Affine::IDENTITY);
    };
    if positional_len(&values, "transform")? != 6 {
        return Err(Error::runtime("transform needs six coefficients"));
    }
    Ok(Affine::new([
        values.get(1)?,
        values.get(2)?,
        values.get(3)?,
        values.get(4)?,
        values.get(5)?,
        values.get(6)?,
    ]))
}

fn frame_fields(spec: &Table) -> mlua::Result<()> {
    item_fields(spec, "frame", &["width", "height", "baseline"])
}

fn item_fields(table: &Table, owner: &str, allowed: &[&str]) -> mlua::Result<()> {
    let len = table.raw_len();
    for pair in table.clone().pairs::<Value, Value>() {
        let (key, _) = pair?;
        match key {
            Value::Integer(i) if i > 0 && i as usize <= len => {}
            Value::String(key) if allowed.contains(&key.to_str()?.as_ref()) => {}
            Value::String(key) => {
                return Err(Error::runtime(format!(
                    "{owner}: unknown field {}",
                    key.to_str()?
                )));
            }
            // A hole in the array part: `{a, nil, b}` has length 1, so `b` is unreachable and
            // silently dropped. Saying "sparse" is the only way the author learns which it was.
            key => {
                return Err(Error::runtime(format!(
                    "{owner}: sparse or non-string field {}",
                    key.type_name()
                )));
            }
        }
    }
    Ok(())
}

/// A required field, named when it is missing. `spec.get` on its own reports only that it found a
/// nil, which leaves the author diffing their call against the reference to work out which of
/// four fields they left out — and two different mistakes produce identical text.
fn need<T: mlua::FromLua>(spec: &Table, owner: &str, field: &str) -> mlua::Result<T> {
    if spec.get::<Value>(field)?.is_nil() {
        return Err(Error::runtime(format!("{owner} needs {field}")));
    }
    spec.get(field)
        .map_err(|e| Error::runtime(format!("{owner}.{field}: {e}")))
}

/// The same, for a field holding a compiled handle — a `gfx.path`, `gfx.solid`, `gfx.frame`.
/// Passing the wrong one of those is a borrow failure deep in mlua otherwise, which reads as an
/// internal error rather than as "you passed a brush where a path goes".
fn need_gfx<T: 'static, R>(
    spec: &Table,
    owner: &str,
    field: &str,
    what: &str,
    take: impl Fn(&T) -> R,
) -> mlua::Result<R> {
    let held: Value = spec.get(field)?;
    let Value::UserData(held) = held else {
        return Err(Error::runtime(match held {
            Value::Nil => format!("{owner} needs {field}, {what}"),
            other => format!("{owner}.{field} must be {what}, got {}", other.type_name()),
        }));
    };
    let held = held
        .borrow::<T>()
        .map_err(|_| Error::runtime(format!("{owner}.{field} must be {what}")))?;
    Ok(take(&held))
}

fn point(table: Table, owner: &str) -> mlua::Result<Point> {
    if positional_len(&table, owner)? != 2 {
        return Err(Error::runtime(format!("{owner} needs x and y")));
    }
    Ok(Point::new(table.get(1)?, table.get(2)?))
}

fn named_fields(table: &Table, owner: &str, allowed: &[&str]) -> mlua::Result<()> {
    for pair in table.clone().pairs::<Value, Value>() {
        let (key, _) = pair?;
        let Value::String(key) = key else {
            return Err(Error::runtime(format!(
                "{owner}: positional fields are not allowed"
            )));
        };
        let key = key.to_str()?;
        if !allowed.contains(&key.as_ref()) {
            return Err(Error::runtime(format!("{owner}: unknown field {key}")));
        }
    }
    Ok(())
}

fn positional_len(table: &Table, owner: &str) -> mlua::Result<usize> {
    let len = table.raw_len();
    for pair in table.clone().pairs::<Value, Value>() {
        let (key, _) = pair?;
        if !matches!(key, Value::Integer(i) if i > 0 && i as usize <= len) {
            return Err(Error::runtime(format!(
                "{owner}: named or sparse fields are not allowed"
            )));
        }
    }
    Ok(len)
}
