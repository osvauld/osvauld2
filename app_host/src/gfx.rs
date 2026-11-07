//! Lua declarations for immutable runtime Frame resources. This module validates and compiles
//! aggregate values; it never renders or retains VM callbacks.

pub(crate) mod glb;
mod image;
mod mesh;
mod texture;
pub(crate) mod world3d;

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use glam::{Quat, Vec3};
use mlua::{Error, Lua, Table, UserData, UserDataMethods, Value};
use runtime::drawing::{Drawing, PartSpec, Pose};
use runtime::frame::{
    Brush, Extend, Frame, GradientStop, Item, MAX_GRADIENT_STOPS, MAX_PATH_COMMANDS,
    MAX_STROKE_DASHES, Path, StrokeCap, StrokeJoin, StrokeStyle,
};
use runtime::scene3d::{Camera3d, Object3d, Scene3d, TextSurface, mesh::MeshData};
use runtime::vello::kurbo::{Affine, PathEl, Point, Rect};
use runtime::vello::peniko::{Color, Fill};
use world::clip::{Clip, Easing, Key, Prop, Track};

#[derive(Clone)]
#[allow(dead_code)] // Frame compilation consumes the path handle in the next Lua slice.
pub(crate) struct LuaPath(pub Arc<Path>);
impl UserData for LuaPath {}

#[derive(Clone)]
pub(crate) struct LuaBrush(pub Arc<Brush>);
impl UserData for LuaBrush {}

#[derive(Clone)]
pub(crate) struct LuaFrame(pub Arc<Frame>);
impl UserData for LuaFrame {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("width", |_, f| Ok(f.0.size().0));
        fields.add_field_method_get("height", |_, f| Ok(f.0.size().1));
    }
}

#[derive(Clone)]
pub(crate) struct LuaDrawing(pub Arc<Drawing>);
impl UserData for LuaDrawing {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("pose", |lua, this, overrides: Option<Table>| {
            let mut poses = Vec::new();
            let overrides = overrides.unwrap_or(lua.create_table()?);
            for pair in overrides.pairs::<String, Table>() {
                let (part, spec) = pair?;
                named_fields(&spec, "pose", &["x", "y", "rot", "scale"])?;
                let rest = Pose::default();
                poses.push((
                    part,
                    Pose {
                        x: spec.get::<Option<f64>>("x")?.unwrap_or(rest.x),
                        y: spec.get::<Option<f64>>("y")?.unwrap_or(rest.y),
                        rot: spec.get::<Option<f64>>("rot")?.unwrap_or(rest.rot),
                        scale: spec.get::<Option<f64>>("scale")?.unwrap_or(rest.scale),
                    },
                ));
            }
            let poses: HashMap<&str, Pose> = poses.iter().map(|(k, p)| (k.as_str(), *p)).collect();
            let frame = this.0.pose(&poses).map_err(Error::external)?;
            lua.create_userdata(LuaFrame(Arc::new(frame)))
        });
    }
}

#[derive(Clone)]
pub(crate) struct LuaClip(pub Arc<Clip>);
impl UserData for LuaClip {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("length", |_, c| Ok(c.0.length()));
        fields.add_field_method_get("loop", |_, c| Ok(c.0.looped()));
    }

    /// `clip:at(t)`: each tracked part's pose `t` seconds in, as the world samples it — for a
    /// frame that animates without a world, through `drawing:pose`.
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("at", |lua, this, t: f64| {
            let poses = lua.create_table()?;
            for (part, p) in this.0.sample(t) {
                let pose = lua.create_table()?;
                pose.set("x", p.x)?;
                pose.set("y", p.y)?;
                pose.set("rot", p.rot)?;
                pose.set("scale", p.scale)?;
                poses.set(part, pose)?;
            }
            Ok(poses)
        });
    }
}

#[derive(Clone)]
pub(crate) struct LuaScene3d(pub Arc<Scene3d>);
impl UserData for LuaScene3d {}

#[derive(Clone)]
pub(crate) struct LuaTextSurface(pub Arc<TextSurface>);
impl UserData for LuaTextSurface {}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let gfx = lua.create_table()?;
    mesh::install(lua, &gfx)?;
    texture::install(lua, &gfx)?;
    glb::install(lua, &gfx)?;
    gfx.set(
        "path",
        lua.create_function(|lua, commands: Table| {
            lua.create_userdata(LuaPath(Arc::new(path(&commands, "path")?)))
        })?,
    )?;
    gfx.set(
        "drawing",
        lua.create_function(|lua, (spec, options): (Table, Option<Table>)| {
            lua.create_userdata(LuaDrawing(Arc::new(drawing(&spec, options.as_ref())?)))
        })?,
    )?;
    gfx.set(
        "clip",
        lua.create_function(|lua, spec: Table| {
            lua.create_userdata(LuaClip(Arc::new(clip(&spec)?)))
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
            let brush = gradient("linear", &spec, &Vars::default(), "linear_gradient")?;
            lua.create_userdata(LuaBrush(Arc::new(brush)))
        })?,
    )?;
    for kind in ["radial", "sweep"] {
        let name = format!("{kind}_gradient");
        let owner = name.clone();
        let make = lua.create_function(move |lua, spec: Table| {
            let brush = gradient(kind, &spec, &Vars::default(), &owner)?;
            lua.create_userdata(LuaBrush(Arc::new(brush)))
        })?;
        gfx.set(name, make)?;
    }
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
            let camera = camera3d(&camera_spec)?;
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
        function gfx.shadow(t) t._gfx = "shadow" return t end
        "#,
    )
    .exec()
}

fn camera3d(spec: &Table) -> mlua::Result<Camera3d> {
    named_fields(spec, "scene3d.camera", &["eye", "target", "up", "fov_y", "near", "far"])?;
    Ok(Camera3d {
        eye: vec3(need(spec, "scene3d.camera", "eye")?, "camera.eye")?,
        target: vec3(need(spec, "scene3d.camera", "target")?, "camera.target")?,
        up: spec.get::<Option<Table>>("up")?.map(|v| vec3(v, "camera.up"))
            .transpose()?.unwrap_or(Vec3::Y),
        fov_y_radians: spec.get::<Option<f32>>("fov_y")?.unwrap_or(45.0).to_radians(),
        near: spec.get::<Option<f32>>("near")?.unwrap_or(0.1),
        far: spec.get::<Option<f32>>("far")?.unwrap_or(100.0),
    })
}

fn object3d(spec: Table, index: usize) -> mlua::Result<Object3d> {
    named_fields(
        &spec,
        &format!("scene3d object {index}"),
        &[
            "id", "mesh", "position", "rotation", "scale", "color", "texture", "surface", "attach",
        ],
    )?;
    let mesh = match spec.get::<Value>("mesh")? {
        Value::Nil => MeshData::cube(),
        Value::String(name) if name.to_str()?.as_ref() == "cube" => MeshData::cube(),
        Value::UserData(handle) => handle.borrow::<mesh::LuaMesh>()
            .map_err(|_| Error::runtime("object.mesh must be a gfx.mesh"))?.0.clone(),
        _ => return Err(Error::runtime("object.mesh must be a gfx.mesh or 'cube'")),
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
        texture: match spec.get::<Value>("texture")? {
            Value::Nil => None,
            Value::UserData(handle) => Some(
                handle
                    .borrow::<texture::LuaTexture>()
                    .map_err(|_| Error::runtime("object.texture must be a gfx texture"))?
                    .0
                    .clone(),
            ),
            _ => return Err(Error::runtime("object.texture must be a gfx texture")),
        },
        surface: spec
            .get::<Option<mlua::AnyUserData>>("surface")?
            .map(|surface| {
                surface
                    .borrow::<LuaTextSurface>()
                    .map(|surface| surface.0.clone())
            })
            .transpose()?,
        attach: spec.get::<Option<String>>("attach")?.map(Into::into),
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

fn path(commands: &Table, owner: &str) -> mlua::Result<Path> {
    let len = positional_len(commands, owner)?;
    if len > MAX_PATH_COMMANDS {
        return Err(Error::runtime(format!(
            "{owner} has {len} commands; maximum is {MAX_PATH_COMMANDS}"
        )));
    }
    let mut elements = Vec::with_capacity(len);
    for index in 1..=len {
        elements.push(command(commands.get(index)?, index)?);
    }
    Path::new(elements).map_err(Error::external)
}

/// A drawing module is pure data — paths are command lists, not `gfx.path` handles — so the same
/// table can be rewritten by an editor or moved into a document without changing shape.
/// `options.vars` makes a variant: values for some of the drawing's own vars, by name.
fn drawing(spec: &Table, options: Option<&Table>) -> mlua::Result<Drawing> {
    named_fields(spec, "drawing", &["size", "parts", "vars"])?;
    let mut vars = match spec.get::<Option<Table>>("vars")? {
        Some(t) => Vars::read(&t, "drawing.vars")?,
        None => Vars::default(),
    };
    if let Some(options) = options {
        named_fields(options, "drawing options", &["vars"])?;
        if let Some(t) = options.get::<Option<Table>>("vars")? {
            vars.overridden_by(Vars::read(&t, "drawing options.vars")?, "drawing options.vars")?;
        }
    }
    let size = point(need(spec, "drawing", "size")?, "drawing.size")?;
    let parts: Table = need(spec, "drawing", "parts")?;
    let mut specs = Vec::new();
    for index in 1..=positional_len(&parts, "drawing.parts")? {
        let part: Table = parts.get(index)?;
        let owner = format!("drawing part {index}");
        named_fields(&part, &owner, &["id", "parent", "pivot", "shapes"])?;
        let shapes = match part.get::<Option<Table>>("shapes")? {
            None => Vec::new(),
            Some(list) => {
                let mut shapes = Vec::new();
                for i in 1..=positional_len(&list, &format!("{owner}.shapes"))? {
                    shapes.extend(shape(list.get(i)?, &vars, &format!("{owner} shape {i}"))?);
                }
                shapes
            }
        };
        specs.push(PartSpec {
            id: need(&part, &owner, "id")?,
            parent: part.get("parent")?,
            pivot: point(need(&part, &owner, "pivot")?, &format!("{owner}.pivot"))?,
            shapes,
        });
    }
    Drawing::new(size.x, size.y, specs).map_err(Error::external)
}

/// A drawing's `vars`: named colours and numbers its shapes use as `"@name"`, so one edit reaches
/// every shape that names it. Resolved once, as the drawing compiles; outside a drawing it's empty.
#[derive(Default)]
struct Vars(HashMap<String, Var>);

enum Var {
    Colour(Color),
    Number(f64),
}

impl Vars {
    fn read(t: &Table, at: &str) -> mlua::Result<Self> {
        let mut vars = HashMap::new();
        for pair in t.pairs::<String, Value>() {
            let (name, value) = pair?;
            let owner = format!("{at}.{name}");
            let var = match value {
                Value::String(c) => Var::Colour(
                    crate::parse_color(&c.to_str()?)
                        .map_err(|e| Error::runtime(format!("{owner}: {e}")))?,
                ),
                Value::Integer(n) => Var::Number(n as f64),
                Value::Number(n) => Var::Number(n),
                other => {
                    let what = other.type_name();
                    return Err(Error::runtime(format!(
                        "{owner} must be a colour or a number, got {what}"
                    )));
                }
            };
            vars.insert(name, var);
        }
        Ok(Self(vars))
    }

    /// A variant's values over the drawing's own. A variant may only change what the drawing
    /// declares, and keep its kind: a misspelt name or a colour for a width is an error.
    fn overridden_by(&mut self, over: Vars, at: &str) -> mlua::Result<()> {
        for (name, var) in over.0 {
            match (self.0.get(&name), &var) {
                (None, _) => {
                    return Err(Error::runtime(format!("{at}.{name}: the drawing has no var {name}")));
                }
                (Some(Var::Colour(_)), Var::Number(_)) => {
                    return Err(Error::runtime(format!("{at}.{name}: the drawing's is a colour")));
                }
                (Some(Var::Number(_)), Var::Colour(_)) => {
                    return Err(Error::runtime(format!("{at}.{name}: the drawing's is a number")));
                }
                _ => {
                    self.0.insert(name, var);
                }
            }
        }
        Ok(())
    }

    /// The var `"@name"` names, or `None` for a literal.
    fn named(&self, value: &Value, owner: &str) -> mlua::Result<Option<&Var>> {
        let Value::String(s) = value else {
            return Ok(None);
        };
        let Some(name) = s.to_str()?.strip_prefix('@').map(str::to_owned) else {
            return Ok(None);
        };
        let found = self.0.get(&name);
        found
            .map(Some)
            .ok_or_else(|| Error::runtime(format!("{owner}: no var @{name}")))
    }

    fn colour(&self, value: Value, owner: &str) -> mlua::Result<Color> {
        match (self.named(&value, owner)?, value) {
            (Some(Var::Colour(c)), _) => Ok(*c),
            (Some(Var::Number(_)), _) => Err(Error::runtime(format!("{owner}: that var is a number"))),
            (None, Value::String(c)) => crate::parse_color(&c.to_str()?),
            (None, other) => Err(Error::runtime(format!(
                "{owner} must be a colour, got {}",
                other.type_name()
            ))),
        }
    }

    fn number(&self, value: Value, owner: &str) -> mlua::Result<f64> {
        match (self.named(&value, owner)?, value) {
            (Some(Var::Number(n)), _) => Ok(*n),
            (Some(Var::Colour(_)), _) => Err(Error::runtime(format!("{owner}: that var is a colour"))),
            (None, Value::Integer(n)) => Ok(n as f64),
            (None, Value::Number(n)) => Ok(n),
            (None, other) => Err(Error::runtime(format!(
                "{owner} must be a number, got {}",
                other.type_name()
            ))),
        }
    }
}

/// One `ui.world` entity. `pos` is its spawn position only — the world owns placement after.
/// What `world(id)` returns: commands to a world at a moment, looked up by id at each call.
struct WorldHandle {
    id: String,
    worlds: crate::Worlds,
    viewing: Rc<Cell<bool>>,
}

impl WorldHandle {
    /// Runs a command on the world, refusing while `view` describes.
    fn command<R>(
        &self,
        owner: &str,
        f: impl FnOnce(&mut world::World2d) -> Result<R, world::WorldError>,
    ) -> mlua::Result<R> {
        if self.viewing.get() {
            return Err(Error::runtime(format!("{owner}: only in a handler; view describes")));
        }
        let mut worlds = self.worlds.borrow_mut();
        let world = (worlds.get_mut(&self.id))
            .ok_or_else(|| Error::runtime(format!("{owner}: there is no such world")))?;
        f(world).map_err(|e| Error::runtime(format!("world {:?}: {e}", self.id)))
    }

    /// Asks the world a question. Allowed in `view` too: asking changes nothing.
    fn ask<R>(
        &self,
        owner: &str,
        f: impl FnOnce(&world::World2d) -> Result<R, world::WorldError>,
    ) -> mlua::Result<(R, u64)> {
        let worlds = self.worlds.borrow();
        let world = (worlds.get(&self.id))
            .ok_or_else(|| Error::runtime(format!("{owner}: there is no such world")))?;
        let answer = f(world).map_err(|e| Error::runtime(format!("world {:?}: {e}", self.id)))?;
        Ok((answer, world.steps()))
    }
}

impl UserData for WorldHandle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // `rink:set("puck", { pos = { x, y }, velocity = { vx, vy }, spin = degrees_a_second })`;
        // any may be left out.
        methods.add_method("set", |_, this, (entity, fields): (String, Table)| {
            let owner = format!("world {:?}:set {entity:?}", this.id);
            named_fields(&fields, &owner, &["pos", "velocity", "spin"])?;
            let pair = |field| match maybe_table(&fields, &owner, field)? {
                Some(t) => point(t, &format!("{owner}.{field}")).map(|p| Some((p.x, p.y))),
                None => Ok(None),
            };
            let spin = maybe_number(&fields, &owner, "spin")?;
            let to = world::Set { pos: pair("pos")?, velocity: pair("velocity")?, spin };
            this.command(&owner, |w| w.set(&entity, to))
        });
        // `rink:push("ball", { vx, vy })` adds to a loose thing's velocity.
        methods.add_method("push", |_, this, (entity, by): (String, Value)| {
            let owner = format!("world {:?}:push {entity:?}", this.id);
            let Value::Table(by) = by else {
                return Err(Error::runtime(format!("{owner} must be {{ x, y }}, got {}", by.type_name())));
            };
            let by = point(by, &owner)?;
            this.command(&owner, |w| w.push(&entity, (by.x, by.y)))
        });
        // `rink:blast({ x, y }, { speed = 400, radius = 150 })`: how many loose things it threw.
        methods.add_method("blast", |_, this, (at, opts): (Table, Table)| {
            let owner = format!("world {:?}:blast", this.id);
            named_fields(&opts, &owner, &["speed", "radius"])?;
            let at = point(at, &format!("{owner} point"))?;
            let (speed, radius): (f64, f64) = (need(&opts, &owner, "speed")?, need(&opts, &owner, "radius")?);
            this.command(&owner, |w| w.blast((at.x, at.y), speed, radius))
        });
        // `rink:grab("puck", { x, y })` holds a loose thing and brings it to the point; call it
        // again as the pointer moves, `rink:release("puck")` to let go: a throw if it was moving.
        methods.add_method("grab", |_, this, (entity, to): (String, Value)| {
            let owner = format!("world {:?}:grab {entity:?}", this.id);
            let Value::Table(to) = to else {
                return Err(Error::runtime(format!("{owner} must be {{ x, y }}, got {}", to.type_name())));
            };
            let to = point(to, &owner)?;
            this.command(&owner, |w| w.grab(&entity, (to.x, to.y)))
        });
        methods.add_method("release", |_, this, entity: Value| {
            let owner = format!("world {:?}:release", this.id);
            let entity = entity_arg(entity, &owner)?;
            this.command(&owner, |w| w.release(&entity))
        });
        // `rink:after(1.5, "faceoff")`: `on_timer` gets `e.name` then.
        methods.add_method("after", |_, this, (secs, name): (Value, Value)| {
            let owner = format!("world {:?}:after", this.id);
            let secs = match secs {
                Value::Number(n) => n,
                Value::Integer(i) => i as f64,
                other => {
                    let what = other.type_name();
                    return Err(Error::runtime(format!("{owner}: seconds must be a number, got {what}")));
                }
            };
            let name = timer_name(name, &owner)?;
            this.command(&owner, |w| w.after(&name, secs))
        });
        methods.add_method("cancel", |_, this, name: Value| {
            let owner = format!("world {:?}:cancel", this.id);
            let name = timer_name(name, &owner)?;
            this.command(&owner, |w| Ok(w.cancel(&name)))
        });
        // `rink:ray({ x, y }, { x, y }, { skip = "guard" })`: nil, or
        // `{ id, at = { x, y }, normal = { x, y }, dist, tick }`.
        methods.add_method("ray", |lua, this, (from, to, opts): (Table, Table, Option<Table>)| {
            let owner = format!("world {:?}:ray", this.id);
            let (from, to) = (point(from, &format!("{owner} from"))?, point(to, &format!("{owner} to"))?);
            let skip = match &opts {
                Some(opts) => {
                    named_fields(opts, &owner, &["skip"])?;
                    opts.get::<Option<String>>("skip")?
                }
                None => None,
            };
            let (hit, tick) = this.ask(&owner, |w| w.ray((from.x, from.y), (to.x, to.y), skip.as_deref()))?;
            let Some(hit) = hit else { return Ok(Value::Nil) };
            let pair = |(x, y): (f64, f64)| lua.create_sequence_from([x, y]);
            let answer = lua.create_table()?;
            answer.set("id", hit.id)?;
            answer.set("at", pair(hit.at)?)?;
            answer.set("normal", pair(hit.normal)?)?;
            answer.set("dist", hit.dist)?;
            answer.set("tick", tick)?;
            Ok(Value::Table(answer))
        });
        // `map:set_camera({ at = { x, y } })` looks there and stops following;
        // `map:set_camera({ follow = "boss" })` follows, eased as described.
        methods.add_method("set_camera", |_, this, look: Table| {
            let owner = format!("world {:?}:set_camera", this.id);
            named_fields(&look, &owner, &["at", "follow"])?;
            let at = maybe_table(&look, &owner, "at")?;
            let follow = entity_id(&look, &owner, "follow")?;
            let look = match (at, follow) {
                (Some(p), None) => {
                    let p = point(p, &format!("{owner}.at"))?;
                    world::Look::At((p.x, p.y))
                }
                (None, Some(id)) => world::Look::Follow(id),
                _ => return Err(Error::runtime(format!("{owner}: set_camera needs at or follow, one of them"))),
            };
            this.command(&owner, |w| w.set_camera(look))
        });
        // `map:to_world({ x, y })`: a point in the box, as pointer events give it, in world units;
        // `map:to_screen` the other way — where to put a speech bubble over an entity.
        type Convert = fn(&world::World2d, (f64, f64)) -> (f64, f64);
        for (name, convert) in [("to_world", world::World2d::to_world as Convert), ("to_screen", world::World2d::to_screen)] {
            methods.add_method(name, move |lua, this, p: Table| {
                let owner = format!("world {:?}:{name}", this.id);
                let p = point(p, &owner)?;
                let ((x, y), _) = this.ask(&owner, |w| Ok(convert(w, (p.x, p.y))))?;
                lua.create_sequence_from([x, y])
            });
        }
        // `rink:at({ x, y })`: the ids there, sorted, as a list that also has `tick`.
        methods.add_method("at", |lua, this, at: Table| {
            let owner = format!("world {:?}:at", this.id);
            let at = point(at, &owner)?;
            let (ids, tick) = this.ask(&owner, |w| w.at((at.x, at.y)))?;
            let answer = lua.create_sequence_from(ids)?;
            answer.set("tick", tick)?;
            Ok(answer)
        });
    }
}

/// `camera = { follow = id | at = { x, y }, ease = per_second, bounds = { x, y, w, h } }` on a
/// world; nil for none. Ranges are the world's to check (`CameraSpec::check`).
pub(crate) fn camera(node: &Table) -> mlua::Result<Option<world::CameraSpec>> {
    let Some(t) = maybe_table(node, "world", "camera")? else {
        return Ok(None);
    };
    let owner = "camera";
    named_fields(&t, owner, &["follow", "at", "ease", "bounds"])?;
    let at = match maybe_table(&t, owner, "at")? {
        Some(p) => point(p, "camera.at").map(|p| Some((p.x, p.y)))?,
        None => None,
    };
    let bounds = match maybe_table(&t, owner, "bounds")? {
        None => None,
        Some(b) if positional_len(&b, "camera.bounds")? == 4 => {
            Some((b.get(1)?, b.get(2)?, b.get(3)?, b.get(4)?))
        }
        Some(_) => return Err(Error::runtime("camera: bounds must be { x, y, w, h }")),
    };
    Ok(Some(world::CameraSpec {
        follow: entity_id(&t, owner, "follow")?,
        at,
        ease: maybe_number(&t, owner, "ease")?,
        bounds,
    }))
}

/// An optional field naming an entity.
fn entity_id(table: &Table, owner: &str, field: &str) -> mlua::Result<Option<String>> {
    match table.get::<Value>(field)? {
        Value::Nil => Ok(None),
        Value::String(s) => Ok(Some(s.to_str()?.to_string())),
        other => Err(Error::runtime(format!(
            "{owner}.{field} must be an entity id, got {}",
            other.type_name()
        ))),
    }
}

fn entity_arg(value: Value, owner: &str) -> mlua::Result<String> {
    match value {
        Value::String(s) => Ok(s.to_str()?.to_string()),
        other => Err(Error::runtime(format!("{owner} must be an entity id, got {}", other.type_name()))),
    }
}

fn timer_name(value: Value, owner: &str) -> mlua::Result<String> {
    match value {
        Value::String(s) => Ok(s.to_str()?.to_string()),
        other => Err(Error::runtime(format!(
            "{owner}: the timer's name must be a string, got {}",
            other.type_name()
        ))),
    }
}

pub(crate) fn install_world(lua: &Lua, worlds: crate::Worlds, viewing: Rc<Cell<bool>>) -> mlua::Result<()> {
    let world = lua.create_function(move |_, id: String| {
        Ok(WorldHandle { id, worlds: worlds.clone(), viewing: viewing.clone() })
    })?;
    lua.globals().set("world", world)
}

pub(crate) fn entity(spec: Table, index: usize) -> mlua::Result<world::EntitySpec> {
    let owner = format!("world entity {index}");
    let fields = [
        "id", "pos", "drawing", "clip", "controller", "flip", "attach", "collider", "sensor",
        "loose", "group", "blocks",
    ];
    named_fields(&spec, &owner, &fields)?;
    let pos = point(need(&spec, &owner, "pos")?, &format!("{owner}.pos"))?;
    let controller = match maybe_table(&spec, &owner, "controller")? {
        Some(c) => Some(controller(&c, &format!("{owner}.controller"))?),
        None => None,
    };
    // Checked by hand: mlua reads any value as a bool by truthiness, so `flip = "left"` would pass.
    let flip = match spec.get::<Value>("flip")? {
        Value::Nil => false,
        Value::Boolean(b) => b,
        other => {
            let what = other.type_name();
            return Err(Error::runtime(format!("{owner}.flip must be a boolean, got {what}")));
        }
    };
    Ok(world::EntitySpec {
        id: need(&spec, &owner, "id")?,
        pos: (pos.x, pos.y),
        drawing: need_gfx(&spec, &owner, "drawing", "a gfx.drawing", |d: &LuaDrawing| {
            d.0.clone()
        })?,
        clip: maybe_gfx(&spec, &owner, "clip", "a gfx.clip", |c: &LuaClip| c.0.clone())?,
        controller,
        flip,
        attach: match maybe_table(&spec, &owner, "attach")? {
            Some(a) => Some(attach(&a, &format!("{owner}.attach"))?),
            None => None,
        },
        collider: match maybe_table(&spec, &owner, "collider")? {
            Some(c) => Some(collider(&c, &format!("{owner}.collider"))?),
            None => None,
        },
        sensor: match maybe_table(&spec, &owner, "sensor")? {
            Some(c) => Some(collider(&c, &format!("{owner}.sensor"))?),
            None => None,
        },
        loose: loose(&spec, &owner)?,
        group: match spec.get::<Value>("group")? {
            Value::Nil => None,
            other => Some(name(other, &format!("{owner}.group"))?),
        },
        blocks: match maybe_table(&spec, &owner, "blocks")? {
            Some(list) => Some(list.sequence_values::<Value>().enumerate().map(|(i, v)| {
                name(v?, &format!("{owner}.blocks[{}]", i + 1))
            }).collect::<mlua::Result<_>>()?),
            None => None,
        },
    })
}

/// A collision group's name: a string, nothing else.
fn name(value: Value, owner: &str) -> mlua::Result<String> {
    match value {
        Value::String(s) => Ok(s.to_str()?.to_string()),
        other => Err(Error::runtime(format!(
            "{owner} must be a group name (a string), got {}",
            other.type_name()
        ))),
    }
}

/// `table.field` as a number, if there; an integer is one too.
fn maybe_number(table: &Table, owner: &str, field: &str) -> mlua::Result<Option<f64>> {
    match table.get::<Value>(field)? {
        Value::Nil => Ok(None),
        Value::Integer(n) => Ok(Some(n as f64)),
        Value::Number(n) => Ok(Some(n)),
        other => {
            let what = other.type_name();
            Err(Error::runtime(format!("{owner}.{field} must be a number, got {what}")))
        }
    }
}

/// `loose = true` for the default stuff, or `{ bounce = 0..1, friction = n, grip = n, density = n }` with any
/// left to its default (no grip: it never turns); `false` or absent, it is not loose. The world
/// checks the ranges.
fn loose(spec: &Table, owner: &str) -> mlua::Result<Option<world::Material>> {
    let m = match spec.get::<Value>("loose")? {
        Value::Nil | Value::Boolean(false) => return Ok(None),
        Value::Boolean(true) => return Ok(Some(world::Material::default())),
        Value::Table(m) => m,
        other => {
            let what = other.type_name();
            return Err(Error::runtime(format!("{owner}.loose must be true or a table, got {what}")));
        }
    };
    let owner = format!("{owner}.loose");
    named_fields(&m, &owner, &["bounce", "friction", "grip", "density"])?;
    let d = world::Material::default();
    let number = |field: &str| maybe_number(&m, &owner, field);
    Ok(Some(world::Material {
        bounce: number("bounce")?.unwrap_or(d.bounce),
        friction: number("friction")?.unwrap_or(d.friction),
        grip: number("grip")?,
        density: number("density")?.unwrap_or(d.density),
    }))
}

/// `wells = { { at = { x, y }, radius = r, pull = a }, … }`: left out or empty, none.
pub(crate) fn wells(node: &Table, owner: &str) -> mlua::Result<Vec<world::Well>> {
    let list = match node.get::<Value>("wells")? {
        Value::Nil => return Ok(vec![]),
        Value::Table(t) => t,
        _ => return Err(Error::runtime(format!("{owner}: world wells must be a list"))),
    };
    let mut wells = Vec::new();
    for i in 1..=positional_len(&list, &format!("{owner}.wells"))? {
        let here = format!("{owner}: well {i}");
        let Value::Table(w) = list.get::<Value>(i)? else {
            return Err(Error::runtime(format!("{here} must be a table")));
        };
        named_fields(&w, &here, &["at", "radius", "pull"])?;
        let at = point(need(&w, &here, "at")?, &format!("{here}.at"))?;
        let (radius, pull) = (need(&w, &here, "radius")?, need(&w, &here, "pull")?);
        wells.push(world::Well { at: (at.x, at.y), radius, pull });
    }
    Ok(wells)
}

/// `joints = { { id, kind = "pin" | "rope" | "spring", a, a_at, b, b_at, … }, … }`: what holds
/// things together, described again each view and kept while it is the same. Left out or empty,
/// none.
pub(crate) fn joints(node: &Table, owner: &str) -> mlua::Result<Vec<world::JointSpec>> {
    let list = match node.get::<Value>("joints")? {
        Value::Nil => return Ok(vec![]),
        Value::Table(t) => t,
        _ => return Err(Error::runtime(format!("{owner}: world joints must be a list"))),
    };
    let mut joints = Vec::new();
    for i in 1..=positional_len(&list, &format!("{owner}.joints"))? {
        let Value::Table(j) = list.get::<Value>(i)? else {
            return Err(Error::runtime(format!("{owner}: joint {i} must be a table")));
        };
        let id: Option<String> = j.get("id")?;
        let Some(id) = id else {
            return Err(Error::runtime(format!("{owner}: joint {i} needs id")));
        };
        let here = format!("{owner}: joint {id:?}");
        named_fields(&j, &here, &[
            "id", "kind", "a", "a_at", "b", "b_at", "limits", "length", "stiffness", "damping", "contacts",
        ])?;
        let kind: String = need(&j, &here, "kind")?;
        let has = |field: &str| j.get::<Value>(field).map(|v| !v.is_nil());
        let only = |field: &str, kinds: &str| -> mlua::Result<()> {
            Err(Error::runtime(format!("{here}: {field} is for {kinds}")))
                .or_else(|e| if has(field)? { Err(e) } else { Ok(()) })
        };
        let kind = match kind.as_str() {
            "pin" => {
                only("length", "rope and spring, not pin")?;
                only("stiffness", "spring, not pin")?;
                only("damping", "spring, not pin")?;
                let limits = match maybe_table(&j, &here, "limits")? {
                    Some(t) => {
                        if positional_len(&t, &format!("{here}.limits"))? != 2 {
                            return Err(Error::runtime(format!("{here}: limits needs low and high")));
                        }
                        Some((t.get(1)?, t.get(2)?))
                    }
                    None => None,
                };
                world::JointKind::Pin { limits }
            }
            "rope" => {
                only("limits", "pin, not rope")?;
                only("stiffness", "spring, not rope")?;
                only("damping", "spring, not rope")?;
                world::JointKind::Rope { length: need(&j, &here, "length")? }
            }
            "spring" => {
                only("limits", "pin, not spring")?;
                world::JointKind::Spring {
                    length: need(&j, &here, "length")?,
                    stiffness: need(&j, &here, "stiffness")?,
                    damping: need(&j, &here, "damping")?,
                }
            }
            other => {
                return Err(Error::runtime(format!(
                    "{here}: kind must be \"pin\", \"rope\" or \"spring\", got {other:?}"
                )));
            }
        };
        let a_at = point(need(&j, &here, "a_at")?, &format!("{here}.a_at"))?;
        let b_at = point(need(&j, &here, "b_at")?, &format!("{here}.b_at"))?;
        joints.push(world::JointSpec {
            kind,
            a: need(&j, &here, "a")?,
            a_at: (a_at.x, a_at.y),
            b: entity_id(&j, &here, "b")?,
            b_at: (b_at.x, b_at.y),
            contacts: j.get::<Option<bool>>("contacts")?.unwrap_or(false),
            id,
        });
    }
    Ok(joints)
}

/// `{ circle = r, at = {x, y} }` or `{ rect = {w, h}, at = {x, y} }`, in the entity's drawing
/// units; `at` (default the origin) is a circle's centre, a rect's top-left corner. A sensor is
/// the same shape.
fn collider(spec: &Table, owner: &str) -> mlua::Result<world::Collider> {
    named_fields(spec, owner, &["circle", "rect", "at"])?;
    let shape = match (spec.get::<Value>("circle")?, maybe_table(spec, owner, "rect")?) {
        (Value::Nil, Some(size)) => {
            let size = point(size, &format!("{owner}.rect"))?;
            world::Shape::Rect(size.x, size.y)
        }
        (Value::Nil, None) => return Err(Error::runtime(format!("{owner} needs circle or rect"))),
        (_, None) => world::Shape::Circle(need(spec, owner, "circle")?),
        (_, Some(_)) => {
            return Err(Error::runtime(format!("{owner} is a circle or a rect, not both")));
        }
    };
    let at = match maybe_table(spec, owner, "at")? {
        Some(at) => point(at, &format!("{owner}.at"))?,
        None => Point::ZERO,
    };
    Ok(world::Collider { shape, at: (at.x, at.y) })
}

/// `{ to = id, part = id, at = {x, y} }` — `at` is a point in the carrier's drawing at rest.
fn attach(spec: &Table, owner: &str) -> mlua::Result<world::Attach> {
    named_fields(spec, owner, &["to", "part", "at", "pivot", "turn"])?;
    let at = point(need(spec, owner, "at")?, &format!("{owner}.at"))?;
    let pivot = match maybe_table(spec, owner, "pivot")? {
        Some(p) => point(p, &format!("{owner}.pivot"))?,
        None => Point::ZERO,
    };
    let turn = match spec.get::<Value>("turn")? {
        Value::Nil => false,
        Value::Boolean(b) => b,
        other => {
            let what = other.type_name();
            return Err(Error::runtime(format!("{owner}.turn must be a boolean, got {what}")));
        }
    };
    let (to, part) = (need(spec, owner, "to")?, need(spec, owner, "part")?);
    Ok(world::Attach { to, part, at: (at.x, at.y), pivot: (pivot.x, pivot.y), turn })
}

/// An optional table field: absent is `None`, and anything but a table says which field it was.
fn maybe_table(spec: &Table, owner: &str, field: &str) -> mlua::Result<Option<Table>> {
    match spec.get::<Value>(field)? {
        Value::Nil => Ok(None),
        Value::Table(t) => Ok(Some(t)),
        other => Err(Error::runtime(format!(
            "{owner}.{field} must be a table, got {}",
            other.type_name()
        ))),
    }
}

/// An optional handle field: absent is `None`, anything else must be the right handle.
fn maybe_gfx<T: 'static, R>(
    spec: &Table,
    owner: &str,
    field: &str,
    what: &str,
    take: impl Fn(&T) -> R,
) -> mlua::Result<Option<R>> {
    match spec.get::<Value>(field)? {
        Value::Nil => Ok(None),
        _ => need_gfx(spec, owner, field, what, take).map(Some),
    }
}

/// Key codes are physical names (`"KeyW"`, `"ArrowLeft"`), the same as `on_key`'s `e.code`.
fn controller(spec: &Table, owner: &str) -> mlua::Result<world::Controller> {
    named_fields(spec, owner, &["speed", "axis_x", "axis_y"])?;
    let axis = |field: &str| -> mlua::Result<Option<world::Axis>> {
        let Some(axis) = spec.get::<Option<Table>>(field)? else {
            return Ok(None);
        };
        let owner = format!("{owner}.{field}");
        named_fields(&axis, &owner, &["neg", "pos"])?;
        let (neg, pos) = (need(&axis, &owner, "neg")?, need(&axis, &owner, "pos")?);
        Ok(Some(world::Axis { neg, pos }))
    };
    let (axis_x, axis_y) = (axis("axis_x")?, axis("axis_y")?);
    if axis_x.is_none() && axis_y.is_none() {
        return Err(Error::runtime(format!("{owner} needs axis_x or axis_y")));
    }
    let speed = need(spec, owner, "speed")?;
    Ok(world::Controller { speed, axis_x, axis_y })
}

/// `tracks = { part = { prop = { {time, value, easing?}, ... } } }`. Parts are sorted so a bad
/// clip reports the same error every run, whatever order Lua's table walk takes.
fn clip(spec: &Table) -> mlua::Result<Clip> {
    named_fields(spec, "clip", &["length", "loop", "tracks"])?;
    let by_part: Table = need(spec, "clip", "tracks")?;
    let mut parts = Vec::new();
    for pair in by_part.pairs::<String, Table>() {
        parts.push(pair.map_err(|e| Error::runtime(format!("clip.tracks: {e}")))?);
    }
    parts.sort_by(|a, b| a.0.cmp(&b.0));
    let mut tracks = Vec::new();
    for (part, props) in parts {
        let owner = format!("clip track {part}");
        named_fields(&props, &owner, &["x", "y", "rot", "scale"])?;
        let props_by_name = [("x", Prop::X), ("y", Prop::Y), ("rot", Prop::Rot), ("scale", Prop::Scale)];
        for (name, prop) in props_by_name {
            let Some(list) = props.get::<Option<Table>>(name)? else {
                continue;
            };
            let owner = format!("{owner}.{name}");
            let mut keys = Vec::new();
            for i in 1..=positional_len(&list, &owner)? {
                keys.push(key(list.get(i)?, &format!("{owner} key {i}"))?);
            }
            tracks.push(Track { part: part.clone(), prop, keys });
        }
    }
    let looped = spec.get::<Option<bool>>("loop")?.unwrap_or(false);
    Clip::new(need(spec, "clip", "length")?, looped, tracks).map_err(Error::external)
}

fn key(spec: Table, owner: &str) -> mlua::Result<Key> {
    let easing = match positional_len(&spec, owner)? {
        2 => Easing::Linear,
        3 => match spec.get::<String>(3)?.as_str() {
            "linear" => Easing::Linear,
            "in_out" => Easing::InOut,
            other => {
                return Err(Error::runtime(format!(
                    "{owner}: unknown easing {other:?}, expected linear or in_out"
                )));
            }
        },
        n => {
            return Err(Error::runtime(format!(
                "{owner} must be {{time, value, easing?}}, got {n} values"
            )));
        }
    };
    Ok(Key { time: spec.get(1)?, value: spec.get(2)?, easing })
}

/// The fields every gradient shares: `stops = { {offset, color}, … }` and `extend`.
fn stops_and_extend(
    spec: &Table,
    vars: &Vars,
    owner: &str,
) -> mlua::Result<(Vec<GradientStop>, Extend)> {
    let stop_table: Table = need(spec, owner, "stops")?;
    let len = positional_len(&stop_table, &format!("{owner}.stops"))?;
    if len > MAX_GRADIENT_STOPS {
        return Err(Error::runtime(format!("{owner} has too many stops: {len}")));
    }
    let mut stops = Vec::with_capacity(len);
    for index in 1..=len {
        let stop: Table = stop_table.get(index)?;
        if positional_len(&stop, &format!("{owner} stop {index}"))? != 2 {
            return Err(Error::runtime(format!(
                "{owner} stop {index} needs offset and color"
            )));
        }
        let color = vars.colour(stop.get(2)?, &format!("{owner} stop {index}"))?;
        stops.push(GradientStop::new(stop.get(1)?, color).map_err(Error::external)?);
    }
    let extend = match spec.get::<Option<String>>("extend")?.as_deref() {
        None | Some("pad") => Extend::Pad,
        Some("repeat") => Extend::Repeat,
        Some("reflect") => Extend::Reflect,
        Some(value) => return Err(Error::runtime(format!("unknown gradient extend {value:?}"))),
    };
    Ok((stops, extend))
}

/// The three gradients, from the fields of `gfx.linear_gradient` / `radial_gradient` /
/// `sweep_gradient` — also what a drawing's `fill = { linear | radial | sweep = … }` holds.
/// `owner` names it in errors.
fn gradient(kind: &str, spec: &Table, vars: &Vars, owner: &str) -> mlua::Result<Brush> {
    let at = |field: &str| -> mlua::Result<Point> {
        point(need(spec, owner, field)?, &format!("{owner}.{field}"))
    };
    let tail = ["stops", "extend"];
    let brush = match kind {
        "linear" => {
            named_fields(spec, owner, &[&["from", "to"][..], &tail].concat())?;
            let (stops, extend) = stops_and_extend(spec, vars, owner)?;
            Brush::linear(at("from")?, at("to")?, stops, extend)
        }
        // `focus` and `focus_radius` are the start circle: by default a point at the centre.
        "radial" => {
            let fields = [&["center", "radius", "focus", "focus_radius"][..], &tail].concat();
            named_fields(spec, owner, &fields)?;
            let (stops, extend) = stops_and_extend(spec, vars, owner)?;
            let center = at("center")?;
            let focus = match spec.get::<Option<Table>>("focus")? {
                Some(f) => point(f, &format!("{owner}.focus"))?,
                None => center,
            };
            let focus_radius = spec.get::<Option<f64>>("focus_radius")?.unwrap_or(0.0);
            let end = (center, need(spec, owner, "radius")?);
            Brush::radial((focus, focus_radius), end, stops, extend)
        }
        // Angles in degrees, like `rot` everywhere else; a full turn by default.
        "sweep" => {
            let fields = [&["center", "from_angle", "to_angle"][..], &tail].concat();
            named_fields(spec, owner, &fields)?;
            let (stops, extend) = stops_and_extend(spec, vars, owner)?;
            let from: f64 = spec.get::<Option<f64>>("from_angle")?.unwrap_or(0.0);
            let to: f64 = spec.get::<Option<f64>>("to_angle")?.unwrap_or(360.0);
            let (from, to) = (from.to_radians(), to.to_radians());
            Brush::sweep(at("center")?, from, to, stops, extend)
        }
        _ => unreachable!("callers pass a known kind"),
    };
    brush.map_err(|e| Error::runtime(format!("{owner}: {e}")))
}

/// A drawing's `fill`: a colour string, or one gradient as data — `{ linear = {…} }`,
/// `{ radial = {…} }` or `{ sweep = {…} }` — not a `gfx` handle, so a drawing stays pure data.
/// Gradient geometry is in the drawing's own space and moves with the part.
fn paint(value: Value, vars: &Vars, owner: &str) -> mlua::Result<Brush> {
    match value {
        Value::String(_) => Brush::solid(vars.colour(value, owner)?).map_err(Error::external),
        Value::Table(t) => {
            named_fields(&t, owner, &["linear", "radial", "sweep"])?;
            let mut kinds = ["linear", "radial", "sweep"].into_iter();
            let Some(kind) = kinds.find(|k| t.contains_key(*k).unwrap_or(false)) else {
                let wanted = "needs linear, radial or sweep";
                return Err(Error::runtime(format!("{owner} {wanted}")));
            };
            if t.pairs::<Value, Value>().count() > 1 {
                return Err(Error::runtime(format!("{owner} takes one gradient")));
            }
            gradient(kind, &need(&t, owner, kind)?, vars, &format!("{owner}.{kind}"))
        }
        other => Err(Error::runtime(format!(
            "{owner} must be a color or a gradient, got {}",
            other.type_name()
        ))),
    }
}

/// A soft shadow from `{ rect = {x, y, w, h}, radius?, blur, color }` — `gfx.shadow`'s fields,
/// also a drawing shape's `shadow`. Paint only: the pointer goes through it.
fn shadow(spec: &Table, vars: &Vars, owner: &str) -> mlua::Result<Item> {
    let r: Table = need(spec, owner, "rect")?;
    if positional_len(&r, &format!("{owner}.rect"))? != 4 {
        return Err(Error::runtime(format!("{owner}.rect needs x, y, w and h")));
    }
    let (x, y, w, h): (f64, f64, f64, f64) = (r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?);
    let radius = match spec.get::<Value>("radius")? {
        Value::Nil => 0.0,
        r => vars.number(r, &format!("{owner}.radius"))?,
    };
    let blur = vars.number(need(spec, owner, "blur")?, &format!("{owner}.blur"))?;
    let color = vars.colour(need(spec, owner, "color")?, &format!("{owner}.color"))?;
    Item::shadow(Rect::new(x, y, x + w, y + h), radius, blur, color)
        .map_err(|e| Error::runtime(format!("{owner}: {e}")))
}

/// Fill under stroke, sharing one path. Outlines join and cap round: drawn art, not diagrams.
fn shape(spec: Table, vars: &Vars, owner: &str) -> mlua::Result<Vec<Item>> {
    if let Some(s) = spec.get::<Option<Table>>("shadow")? {
        named_fields(&spec, owner, &["shadow"])?;
        let owner = format!("{owner}.shadow");
        named_fields(&s, &owner, &["rect", "radius", "blur", "color"])?;
        return Ok(vec![shadow(&s, vars, &owner)?]);
    }
    named_fields(&spec, owner, &["path", "fill", "stroke"])?;
    let path = Arc::new(path(
        &need(&spec, owner, "path")?,
        &format!("{owner}.path"),
    )?);
    let mut items = Vec::new();
    match spec.get::<Value>("fill")? {
        Value::Nil => {}
        fill => {
            let brush = paint(fill, vars, &format!("{owner}.fill"))?;
            items.push(Item::fill(path.clone(), Arc::new(brush), Fill::NonZero));
        }
    }
    if let Some(stroke) = spec.get::<Option<Table>>("stroke")? {
        if positional_len(&stroke, &format!("{owner}.stroke"))? != 2 {
            return Err(Error::runtime(format!(
                "{owner}.stroke needs width and color"
            )));
        }
        let ink = vars.colour(stroke.get(2)?, &format!("{owner}.stroke color"))?;
        let brush = Brush::solid(ink).map_err(Error::external)?;
        let style = StrokeStyle::new(
            vars.number(stroke.get(1)?, &format!("{owner}.stroke width"))?,
            StrokeCap::Round,
            StrokeJoin::Round,
            4.0,
            Vec::new(),
            0.0,
        )
        .map_err(Error::external)?;
        items.push(Item::stroke(path, Arc::new(brush), style));
    }
    if items.is_empty() {
        return Err(Error::runtime(format!("{owner} needs fill or stroke")));
    }
    Ok(items)
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
        "shadow" => {
            let fields = ["_gfx", "id", "rect", "radius", "blur", "color"];
            named_fields(&spec, "shadow", &fields)?;
            named(&spec, shadow(&spec, &Vars::default(), "shadow")?)
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
