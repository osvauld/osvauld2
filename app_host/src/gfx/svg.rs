//! `gfx.svg(name)`: an SVG from the app's own folder, as a drawing module — the plain table
//! `gfx.drawing` takes — plus a report of what the drawing format could not carry
//! (docs/design/svg-drawings.md).
//!
//! usvg does the SVG (CSS, `<use>`, arcs, units, the viewBox); this file only maps its tree onto
//! parts and shapes. Nothing outside the app's `files` map is ever read: images resolve to
//! nothing, and the `text` feature (fonts) is off.

use std::collections::{HashMap, HashSet};

use mlua::{Error, Lua, Table, Value};
use usvg::tiny_skia_path::PathSegment;
use usvg::{Color, Group, Node, Paint, Transform};

use crate::modules::{self, AppFiles};

const MAX_BYTES: usize = 4 << 20;
const MAX_COMMANDS: usize = 200_000;

pub(super) fn install(lua: &Lua, gfx: &Table) -> mlua::Result<()> {
    gfx.set(
        "svg",
        lua.create_function(|lua, name: Value| {
            let Value::String(name) = name else {
                return Err(Error::runtime("gfx.svg needs a file name, like \"art/car.svg\""));
            };
            let name = name.to_str()?.to_string();
            let owner = format!("gfx.svg({name:?})");
            if !legal(&name) {
                return Err(Error::runtime(format!(
                    "{owner}: a name is a path inside this app ending in .svg, with no . or .. segments"
                )));
            }
            let text = {
                let files = lua
                    .app_data_ref::<AppFiles>()
                    .ok_or_else(|| Error::runtime(format!("{owner}: no app files here")))?;
                match modules::read(&files.0, &name) {
                    Some(text) => text,
                    None => {
                        let mut names: Vec<String> = files.0.get_map("files").keys()
                            .map(|k| k.to_string())
                            .filter(|k| k.ends_with(".svg"))
                            .collect();
                        names.sort();
                        return Err(Error::runtime(format!(
                            "{owner}: no {name} in this app. It has: {}",
                            if names.is_empty() { "no .svg files".to_string() } else { names.join(", ") }
                        )));
                    }
                }
            };
            let svg = convert(&text).map_err(|e| Error::runtime(format!("{owner}: {e}")))?;
            svg.to_lua(lua)
        })?,
    )
}

fn legal(name: &str) -> bool {
    name.ends_with(".svg")
        && !name.contains('\\')
        && name.split('/').all(|s| !s.is_empty() && s != "." && s != "..")
}

type P = (f64, f64);

#[derive(Debug)]
struct Svg {
    size: P,
    parts: Vec<Part>,
    dropped: Vec<(&'static str, String)>,
}

#[derive(Debug)]
struct Part {
    id: String,
    parent: Option<String>,
    pivot: P,
    shapes: Vec<Shape>,
}

#[derive(Debug)]
struct Shape {
    path: Vec<Cmd>,
    fill: Option<Fill>,
    evenodd: bool,
    stroke: Option<Stroke>,
}

#[derive(Debug)]
enum Cmd {
    Move(P),
    Line(P),
    Quad(P, P),
    Cubic(P, P, P),
    Close,
}

#[derive(Debug)]
enum Fill {
    Solid(String),
    Linear { from: P, to: P, stops: Vec<(f64, String)>, extend: Option<&'static str> },
    Radial { center: P, radius: f64, focus: Option<(P, f64)>, stops: Vec<(f64, String)>, extend: Option<&'static str> },
}

#[derive(Debug)]
struct Stroke {
    width: f64,
    colour: String,
    cap: &'static str,
    join: &'static str,
    miter_limit: f64,
    dashes: Vec<f64>,
    dash_offset: f64,
}

/// What the XML says that usvg's tree no longer does: ids of `<use>` elements, `data-pivot`s, and
/// the elements it drops without a trace (text without fonts, images we never resolve).
#[derive(Default)]
struct Xml {
    uses: HashSet<String>,
    pivots: HashMap<String, String>,
    dropped: Vec<(&'static str, String)>,
}

fn scan(text: &str) -> Result<Xml, String> {
    let doc = usvg::roxmltree::Document::parse_with_options(
        text,
        usvg::roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() },
    )
    .map_err(|e| e.to_string())?;
    let mut xml = Xml::default();
    for node in doc.descendants().filter(|n| n.is_element()) {
        let id = node.attribute("id").unwrap_or("").to_string();
        let in_defs = node.ancestors().any(|a| a.tag_name().name() == "defs");
        match node.tag_name().name() {
            "use" if !id.is_empty() => {
                xml.uses.insert(id.clone());
            }
            "text" if !in_defs => xml.dropped.push(("text", id.clone())),
            "image" if !in_defs => xml.dropped.push(("image", id.clone())),
            _ => {}
        }
        if let (false, Some(p)) = (id.is_empty(), node.attribute("data-pivot")) {
            xml.pivots.insert(id, p.to_string());
        }
    }
    Ok(xml)
}

fn convert(text: &str) -> Result<Svg, String> {
    if text.len() > MAX_BYTES {
        return Err(format!("{} bytes; the most is 4 MiB", text.len()));
    }
    // An entity can expand to far more than the file holds; nothing in drawn art needs one.
    if text.contains("<!ENTITY") {
        return Err("declares an <!ENTITY; entities are not allowed".into());
    }
    let xml = scan(text)?;
    let options = usvg::Options {
        resources_dir: None,
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(text, &options).map_err(|e| e.to_string())?;
    let size = (tree.size().width() as f64, tree.size().height() as f64);
    let mut walk = Walk {
        xml: &xml,
        parts: Vec::new(),
        logical: vec![Logical { id: "root".into(), part: None, pivot: (size.0 / 2.0, size.1 / 2.0), next: 2 }],
        taken: HashSet::new(),
        dropped: Vec::new(),
        commands: 0,
    };
    walk.taken.insert("root".into());
    walk.group(tree.root(), 0, 1.0, false)?;
    let Walk { parts, dropped: found, .. } = walk;
    let mut dropped = xml.dropped;
    dropped.extend(found);
    Ok(Svg { size, parts, dropped })
}

/// A part as the SVG names it. Its shapes may land in several output parts (`body`, `body~2`) when
/// a child part is drawn between them; `part` is the one taking shapes now.
struct Logical {
    id: String,
    part: Option<usize>,
    pivot: P,
    next: u32,
}

struct Walk<'a> {
    xml: &'a Xml,
    parts: Vec<Part>,
    logical: Vec<Logical>,
    taken: HashSet<String>,
    dropped: Vec<(&'static str, String)>,
    commands: usize,
}

impl Walk<'_> {
    /// `owner` indexes `logical`: the part this group's shapes belong to.
    fn group(&mut self, g: &Group, owner: usize, opacity: f32, in_use: bool) -> Result<(), String> {
        let mut owner = owner;
        let mut in_use = in_use;
        let id = g.id();
        if !id.is_empty() && !in_use && self.taken.insert(id.to_string()) {
            let pivot = match self.xml.pivots.get(id) {
                Some(text) => pivot(text).ok_or_else(|| {
                    format!("group {id}: data-pivot must be two numbers \"x y\", got {text:?}")
                })?,
                None => {
                    let b = g.abs_bounding_box();
                    (((b.left() + b.right()) / 2.0) as f64, ((b.top() + b.bottom()) / 2.0) as f64)
                }
            };
            let parent = (owner != 0).then(|| self.logical[owner].id.clone());
            self.parts.push(Part { id: id.to_string(), parent, pivot, shapes: Vec::new() });
            self.logical.push(Logical { id: id.to_string(), part: Some(self.parts.len() - 1), pivot, next: 2 });
            owner = self.logical.len() - 1;
        }
        if !id.is_empty() && self.xml.uses.contains(id) {
            in_use = true; // the clone's own ids are copies; they fold into this part
        }
        let named = if id.is_empty() { self.logical[owner].id.clone() } else { id.to_string() };
        if !g.filters().is_empty() {
            self.dropped.push(("filter", named.clone()));
        }
        if g.mask().is_some() {
            self.dropped.push(("mask", named.clone()));
        }
        if g.clip_path().is_some() {
            self.dropped.push(("clip-path", named.clone()));
        }
        if g.blend_mode() != usvg::BlendMode::Normal {
            self.dropped.push(("blend", named.clone()));
        }
        let mut opacity = opacity;
        if g.opacity().get() < 1.0 {
            opacity *= g.opacity().get();
            if shapes_in(g) > 1 {
                self.dropped.push(("group-opacity", named));
            }
        }
        for child in g.children() {
            match child {
                Node::Group(inner) => self.group(inner, owner, opacity, in_use)?,
                Node::Path(path) => self.path(path, owner, opacity)?,
                // Images and text were listed from the XML; usvg resolves neither here.
                _ => {}
            }
        }
        Ok(())
    }

    fn path(&mut self, path: &usvg::Path, owner: usize, opacity: f32) -> Result<(), String> {
        if !path.is_visible() {
            return Ok(());
        }
        let ts = path.abs_transform();
        let id = path.id().to_string();
        let mut cmds = Vec::new();
        for seg in path.data().segments() {
            cmds.push(match seg {
                PathSegment::MoveTo(p) => Cmd::Move(map(ts, p.x, p.y)),
                PathSegment::LineTo(p) => Cmd::Line(map(ts, p.x, p.y)),
                PathSegment::QuadTo(a, p) => Cmd::Quad(map(ts, a.x, a.y), map(ts, p.x, p.y)),
                PathSegment::CubicTo(a, b, p) => {
                    Cmd::Cubic(map(ts, a.x, a.y), map(ts, b.x, b.y), map(ts, p.x, p.y))
                }
                PathSegment::Close => Cmd::Close,
            });
        }
        self.commands += cmds.len();
        if self.commands > MAX_COMMANDS {
            return Err(format!("more than {MAX_COMMANDS} path commands"));
        }
        let named = if id.is_empty() { self.logical[owner].id.clone() } else { id.clone() };
        let scale = (ts.sx * ts.sy - ts.kx * ts.ky).abs().sqrt() as f64;

        let mut evenodd = false;
        let fill = match path.fill() {
            None => None,
            Some(f) => {
                evenodd = f.rule() == usvg::FillRule::EvenOdd;
                let alpha = f.opacity().get() * opacity;
                match f.paint() {
                    Paint::Color(c) => Some(Fill::Solid(hex(*c, alpha))),
                    Paint::LinearGradient(lg) => {
                        let m = ts.pre_concat(lg.transform());
                        let (p1, p2) = linear(m, (lg.x1(), lg.y1()), (lg.x2(), lg.y2()));
                        Some(Fill::Linear {
                            from: p1,
                            to: p2,
                            stops: stops(lg.stops(), alpha),
                            extend: extend(lg.spread_method()),
                        })
                    }
                    Paint::RadialGradient(rg) => {
                        let m = ts.pre_concat(rg.transform());
                        if !conformal(m) {
                            self.dropped.push(("radial-skew", named.clone()));
                        }
                        let s = (m.sx * m.sy - m.kx * m.ky).abs().sqrt() as f64;
                        let center = map(m, rg.cx(), rg.cy());
                        let focus = map(m, rg.fx(), rg.fy());
                        let fr = rg.fr().get() as f64 * s;
                        let moved = (focus.0 - center.0).abs() > 1e-6 || (focus.1 - center.1).abs() > 1e-6;
                        Some(Fill::Radial {
                            center,
                            radius: rg.r().get() as f64 * s,
                            focus: (moved || fr > 0.0).then_some((focus, fr)),
                            stops: stops(rg.stops(), alpha),
                            extend: extend(rg.spread_method()),
                        })
                    }
                    Paint::Pattern(_) => {
                        self.dropped.push(("pattern", named.clone()));
                        None
                    }
                }
            }
        };
        let stroke = match path.stroke() {
            None => None,
            Some(s) => match s.paint() {
                Paint::Color(c) => Some(Stroke {
                    width: s.width().get() as f64 * scale,
                    colour: hex(*c, s.opacity().get() * opacity),
                    cap: match s.linecap() {
                        usvg::LineCap::Butt => "butt",
                        usvg::LineCap::Round => "round",
                        usvg::LineCap::Square => "square",
                    },
                    join: match s.linejoin() {
                        usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => "miter",
                        usvg::LineJoin::Round => "round",
                        usvg::LineJoin::Bevel => "bevel",
                    },
                    miter_limit: s.miterlimit().get() as f64,
                    dashes: s.dasharray().unwrap_or(&[]).iter().map(|d| *d as f64 * scale).collect(),
                    dash_offset: s.dashoffset() as f64 * scale,
                }),
                _ => {
                    self.dropped.push(("stroke-paint", named.clone()));
                    None
                }
            },
        };
        if fill.is_some() && stroke.is_some() && path.paint_order() == usvg::PaintOrder::StrokeAndFill {
            self.dropped.push(("paint-order", named));
        }
        if fill.is_none() && stroke.is_none() {
            return Ok(());
        }
        let part = self.receiving(owner);
        self.parts[part].shapes.push(Shape { path: cmds, fill, evenodd, stroke });
        Ok(())
    }

    /// The output part a shape of `owner` goes into: its current one while nothing has been
    /// drawn after it, else a fresh continuation (`id~2`, parent `id`) so draw order holds.
    fn receiving(&mut self, owner: usize) -> usize {
        let last = self.parts.len().checked_sub(1);
        let l = &self.logical[owner];
        if l.part.is_some() && l.part == last {
            return l.part.unwrap();
        }
        let (id, parent) = match l.part {
            None if owner == 0 => ("root".to_string(), None),
            _ => {
                let mut n = l.next;
                let mut id = format!("{}~{n}", l.id);
                while self.taken.contains(&id) {
                    n += 1;
                    id = format!("{}~{n}", l.id);
                }
                self.logical[owner].next = n + 1;
                (id, Some(self.logical[owner].id.clone()))
            }
        };
        self.taken.insert(id.clone());
        let pivot = self.logical[owner].pivot;
        self.parts.push(Part { id, parent, pivot, shapes: Vec::new() });
        self.logical[owner].part = Some(self.parts.len() - 1);
        self.parts.len() - 1
    }
}

fn shapes_in(g: &Group) -> usize {
    g.children()
        .iter()
        .map(|c| match c {
            Node::Group(inner) => shapes_in(inner),
            Node::Path(_) => 1,
            _ => 0,
        })
        .sum()
}

fn pivot(text: &str) -> Option<P> {
    let nums: Vec<f64> = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<f64>().ok().filter(|v| v.is_finite()))
        .collect::<Option<_>>()?;
    match nums[..] {
        [x, y] => Some((x, y)),
        _ => None,
    }
}

fn map(ts: Transform, x: f32, y: f32) -> P {
    (
        (ts.sx * x + ts.kx * y + ts.tx) as f64,
        (ts.ky * x + ts.sy * y + ts.ty) as f64,
    )
}

/// A linear gradient under an affine map, exactly: its value at q is
/// `dot(A⁻¹(q − M·p1), d) / |d|²`, which is a linear gradient from `M·p1` along `n = A⁻ᵀ·d`.
fn linear(m: Transform, p1: (f32, f32), p2: (f32, f32)) -> (P, P) {
    let from = map(m, p1.0, p1.1);
    let d = ((p2.0 - p1.0) as f64, (p2.1 - p1.1) as f64);
    let (a, b, c, e) = (m.sx as f64, m.ky as f64, m.kx as f64, m.sy as f64); // A = [[a, c], [b, e]]
    let det = a * e - b * c;
    if det.abs() < 1e-12 {
        let to = map(m, p2.0, p2.1);
        return (from, to);
    }
    // A⁻ᵀ = 1/det · [[e, −b], [−c, a]]
    let n = ((e * d.0 - b * d.1) / det, (-c * d.0 + a * d.1) / det);
    let dd = d.0 * d.0 + d.1 * d.1;
    let nn = n.0 * n.0 + n.1 * n.1;
    if nn < 1e-24 {
        return (from, from);
    }
    let k = dd / nn;
    (from, (from.0 + n.0 * k, from.1 + n.1 * k))
}

/// Rotation, uniform scale and reflection keep circles circles.
fn conformal(m: Transform) -> bool {
    let near = |a: f32, b: f32| (a - b).abs() <= 1e-4 * (1.0 + a.abs().max(b.abs()));
    (near(m.sx, m.sy) && near(m.kx, -m.ky)) || (near(m.sx, -m.sy) && near(m.kx, m.ky))
}

fn hex(c: Color, alpha: f32) -> String {
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", c.red, c.green, c.blue, a)
}

fn stops(stops: &[usvg::Stop], alpha: f32) -> Vec<(f64, String)> {
    stops
        .iter()
        .map(|s| (s.offset().get() as f64, hex(s.color(), s.opacity().get() * alpha)))
        .collect()
}

fn extend(m: usvg::SpreadMethod) -> Option<&'static str> {
    match m {
        usvg::SpreadMethod::Pad => None,
        usvg::SpreadMethod::Reflect => Some("reflect"),
        usvg::SpreadMethod::Repeat => Some("repeat"),
    }
}

impl Svg {
    fn to_lua(&self, lua: &Lua) -> mlua::Result<(Table, Table)> {
        let pair = |p: P| -> mlua::Result<Table> { lua.create_sequence_from([p.0, p.1]) };
        let spec = lua.create_table()?;
        spec.set("size", pair(self.size)?)?;
        let parts = lua.create_table()?;
        for part in &self.parts {
            let t = lua.create_table()?;
            t.set("id", part.id.as_str())?;
            if let Some(parent) = &part.parent {
                t.set("parent", parent.as_str())?;
            }
            t.set("pivot", pair(part.pivot)?)?;
            if !part.shapes.is_empty() {
                let shapes = lua.create_table()?;
                for shape in &part.shapes {
                    shapes.push(shape.to_lua(lua)?)?;
                }
                t.set("shapes", shapes)?;
            }
            parts.push(t)?;
        }
        spec.set("parts", parts)?;

        let report = lua.create_table()?;
        let dropped = lua.create_table()?;
        for (feature, id) in &self.dropped {
            let d = lua.create_table()?;
            d.set("feature", *feature)?;
            d.set("id", id.as_str())?;
            dropped.push(d)?;
        }
        report.set("dropped", dropped)?;
        Ok((spec, report))
    }
}

impl Shape {
    fn to_lua(&self, lua: &Lua) -> mlua::Result<Table> {
        let t = lua.create_table()?;
        let path = lua.create_table()?;
        for cmd in &self.path {
            let c = lua.create_table()?;
            let (name, pts): (&str, Vec<P>) = match *cmd {
                Cmd::Move(p) => ("move", vec![p]),
                Cmd::Line(p) => ("line", vec![p]),
                Cmd::Quad(a, p) => ("quad", vec![a, p]),
                Cmd::Cubic(a, b, p) => ("cubic", vec![a, b, p]),
                Cmd::Close => ("close", vec![]),
            };
            c.push(name)?;
            for p in pts {
                c.push(p.0)?;
                c.push(p.1)?;
            }
            path.push(c)?;
        }
        t.set("path", path)?;
        if let Some(fill) = &self.fill {
            t.set("fill", fill.to_lua(lua)?)?;
        }
        if self.evenodd && self.fill.is_some() {
            t.set("rule", "evenodd")?;
        }
        if let Some(s) = &self.stroke {
            let st = lua.create_table()?;
            st.push(s.width)?;
            st.push(s.colour.as_str())?;
            st.set("cap", s.cap)?;
            st.set("join", s.join)?;
            st.set("miter_limit", s.miter_limit)?;
            if !s.dashes.is_empty() {
                st.set("dashes", lua.create_sequence_from(s.dashes.iter().copied())?)?;
                st.set("dash_offset", s.dash_offset)?;
            }
            t.set("stroke", st)?;
        }
        Ok(t)
    }
}

impl Fill {
    fn to_lua(&self, lua: &Lua) -> mlua::Result<Value> {
        let pair = |p: P| -> mlua::Result<Table> { lua.create_sequence_from([p.0, p.1]) };
        let stops_t = |stops: &[(f64, String)]| -> mlua::Result<Table> {
            let t = lua.create_table()?;
            for (offset, colour) in stops {
                let s = lua.create_table()?;
                s.push(*offset)?;
                s.push(colour.as_str())?;
                t.push(s)?;
            }
            Ok(t)
        };
        Ok(match self {
            Fill::Solid(c) => Value::String(lua.create_string(c)?),
            Fill::Linear { from, to, stops, extend } => {
                let g = lua.create_table()?;
                g.set("from", pair(*from)?)?;
                g.set("to", pair(*to)?)?;
                g.set("stops", stops_t(stops)?)?;
                if let Some(e) = extend {
                    g.set("extend", *e)?;
                }
                let t = lua.create_table()?;
                t.set("linear", g)?;
                Value::Table(t)
            }
            Fill::Radial { center, radius, focus, stops, extend } => {
                let g = lua.create_table()?;
                g.set("center", pair(*center)?)?;
                g.set("radius", *radius)?;
                if let Some((f, r)) = focus {
                    g.set("focus", pair(*f)?)?;
                    g.set("focus_radius", *r)?;
                }
                g.set("stops", stops_t(stops)?)?;
                if let Some(e) = extend {
                    g.set("extend", *e)?;
                }
                let t = lua.create_table()?;
                t.set("radial", g)?;
                Value::Table(t)
            }
        })
    }
}
