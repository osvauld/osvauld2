//! `gfx.load_glb(path)`: a binary item file read as plain scene data — nodes (name, parent,
//! TRS, Blender custom properties from glTF `extras`), triangle meshes as ordinary `gfx.mesh`
//! handles, and materials with their base-colour textures as `gfx.texture` handles. Nothing
//! here knows what a node means; Lua decides that.
//!
//! GLB only, with embedded buffers: an exported file must stand alone, and an imported file
//! never reaches outside itself (no URIs, no scripts, no extensions that change geometry).
use std::rc::Rc;
use std::sync::Arc;

use glam::Vec3;
use mlua::{Error, Lua, Table, Value};
use runtime::scene3d::mesh::{MAX_INDICES, MAX_VERTICES, MeshData, Vertex, degenerate};
use runtime::scene3d::texture::{TextureData, payload_bytes};

use super::image::{self, Decoded};
use super::mesh::{LuaMesh, MeshBudget};
use super::texture::{LuaTexture, TextureBudget};

/// Reads one of the app's binary files by item path. Supplied by the host as VM app-data
/// (globals are frozen once the VM is sandboxed); Lua never sees the bytes.
pub type ReadFile = Rc<dyn Fn(&str) -> Result<Vec<u8>, String>>;
pub(crate) struct Reader(pub ReadFile);
/// The paths of the app's binary files, for an error that says what is there.
pub type ListFiles = Rc<dyn Fn() -> Vec<String>>;
pub(crate) struct Listing(pub ListFiles);

const MAX_NODES: usize = 4096;
const MAX_PRIMITIVES: usize = 128;
const MAX_EXTRAS_BYTES: usize = 16 * 1024;
const MAX_EXTRAS_DEPTH: usize = 16;
const MAX_LIMIT: f32 = 1_000_000.0;
/// Decoded base-colour images one file may carry (mips included), the VM's whole budget.
const MAX_IMAGE_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct Glb {
    pub nodes: Vec<Node>,
    /// One entry per glTF mesh: its triangle primitives (empty when every primitive was
    /// degenerate).
    pub meshes: Vec<Vec<Primitive>>,
    pub materials: Vec<Material>,
    /// Index-aligned with the file's images; only base-colour images are decoded.
    pub images: Vec<Option<Decoded>>,
    /// The default (or first) scene's root nodes.
    pub roots: Vec<usize>,
}

#[derive(Debug)]
pub(crate) struct Node {
    pub name: Option<String>,
    pub parent: Option<usize>,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub mesh: Option<usize>,
    pub extras: Option<serde_json::Value>,
}

#[derive(Debug)]
pub(crate) struct Primitive {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub material: Option<usize>,
}

#[derive(Debug)]
pub(crate) struct Material {
    pub name: Option<String>,
    /// Linear RGBA, as glTF stores it.
    pub base_color: [f32; 4],
    /// Image index of the base-colour texture.
    pub texture: Option<usize>,
    /// Which TEXCOORD_n set that texture reads.
    pub tex_coord: u32,
}

fn bad(why: impl std::fmt::Display) -> String {
    format!("glb: {why}")
}

pub(crate) fn parse(bytes: &[u8]) -> Result<Glb, String> {
    if bytes.get(..4) != Some(b"glTF") {
        return Err(bad("not a binary glTF (.glb) file"));
    }
    let gltf = gltf::Gltf::from_slice(bytes).map_err(bad)?;
    for buffer in gltf.buffers() {
        if let gltf::buffer::Source::Uri(_) = buffer.source() {
            return Err(bad("buffer refers to an external file; export as .glb with embedded data"));
        }
    }
    if let Some(ext) = gltf.extensions_required().next() {
        return Err(bad(format!("required extension {ext} is not supported")));
    }
    let blob = gltf.blob.as_deref();
    if gltf.nodes().len() > MAX_NODES {
        return Err(bad(format!("more than {MAX_NODES} nodes")));
    }

    let mut parent = vec![None; gltf.nodes().len()];
    for node in gltf.nodes() {
        for child in node.children() {
            if parent[child.index()].replace(node.index()).is_some() {
                return Err(bad(format!("node {} has two parents", child.index())));
            }
        }
    }
    for start in 0..parent.len() {
        let (mut at, mut steps) = (start, 0);
        while let Some(p) = parent[at] {
            at = p;
            steps += 1;
            if steps > parent.len() {
                return Err(bad("node hierarchy has a cycle"));
            }
        }
    }

    let nodes = gltf
        .nodes()
        .map(|node| {
            let (translation, rotation, scale) = node.transform().decomposed();
            let finite = translation
                .iter()
                .chain(&rotation)
                .chain(&scale)
                .all(|v| v.is_finite() && v.abs() <= MAX_LIMIT);
            if !finite {
                return Err(bad(format!("node {} has a non-finite transform", node.index())));
            }
            Ok(Node {
                name: node.name().map(str::to_string),
                parent: parent[node.index()],
                translation,
                rotation,
                scale,
                mesh: node.mesh().map(|m| m.index()),
                extras: extras(node.extras())?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let materials: Vec<Material> = gltf
        .materials()
        .map(|m| {
            let pbr = m.pbr_metallic_roughness();
            let base = pbr.base_color_texture();
            Material {
                name: m.name().map(str::to_string),
                base_color: pbr.base_color_factor(),
                texture: base.as_ref().map(|t| t.texture().source().index()),
                tex_coord: base.as_ref().map_or(0, |t| t.tex_coord()),
            }
        })
        .collect();
    let images = images(&gltf, blob, &materials)?;

    let mut count = 0;
    let meshes = gltf
        .meshes()
        .map(|mesh| {
            mesh.primitives()
                .filter_map(|p| {
                    count += 1;
                    if count > MAX_PRIMITIVES {
                        return Some(Err(bad(format!("more than {MAX_PRIMITIVES} primitives"))));
                    }
                    let set = p.material().index().map_or(0, |m| materials[m].tex_coord);
                    primitive(&p, blob, set).transpose()
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<Vec<_>, _>>()?;

    let roots = gltf
        .default_scene()
        .or_else(|| gltf.scenes().next())
        .map(|s| s.nodes().map(|n| n.index()).collect())
        .unwrap_or_else(|| (0..nodes.len()).filter(|&i| parent[i].is_none()).collect());

    Ok(Glb {
        nodes,
        meshes,
        materials,
        images,
        roots,
    })
}

/// Decodes the images base-colour textures use, refusing oversize ones from their headers.
fn images(
    gltf: &gltf::Gltf,
    blob: Option<&[u8]>,
    materials: &[Material],
) -> Result<Vec<Option<Decoded>>, String> {
    let mut out: Vec<Option<Decoded>> = gltf.images().map(|_| None).collect();
    let mut total = 0;
    for m in materials {
        let Some(index) = m.texture else { continue };
        if out[index].is_some() {
            continue;
        }
        let img = gltf.images().nth(index).expect("texture source is in range");
        let bytes = match img.source() {
            gltf::image::Source::View { view, .. } => {
                let start = view.offset();
                match (view.buffer().source(), blob) {
                    (gltf::buffer::Source::Bin, Some(b)) => b
                        .get(start..start + view.length())
                        .ok_or_else(|| bad(format!("image {index} is outside the buffer")))?,
                    _ => return Err(bad(format!("image {index} has no embedded data"))),
                }
            }
            gltf::image::Source::Uri { .. } => {
                return Err(bad(format!(
                    "image {index} refers to an external file; export as .glb with embedded data"
                )));
            }
        };
        let fail = |e: String| bad(format!("image {index}: {e}"));
        let (w, h) = image::size(bytes).map_err(fail)?;
        image::check(w, h).map_err(fail)?;
        total += payload_bytes(w, h);
        if total > MAX_IMAGE_BYTES {
            return Err(bad("textures over 128 MiB once decoded"));
        }
        out[index] = Some(image::decode(bytes).map_err(fail)?);
    }
    Ok(out)
}

/// `None` when nothing drawable is left after dropping zero-area triangles.
fn primitive(
    p: &gltf::Primitive,
    blob: Option<&[u8]>,
    uv_set: u32,
) -> Result<Option<Primitive>, String> {
    if p.mode() != gltf::mesh::Mode::Triangles {
        return Err(bad(format!("primitive mode {:?} is not triangles", p.mode())));
    }
    let reader = p.reader(|b| match b.source() {
        gltf::buffer::Source::Bin => blob,
        gltf::buffer::Source::Uri(_) => None,
    });
    let positions: Vec<[f32; 3]> = reader
        .read_positions()
        .ok_or_else(|| bad("primitive has no positions"))?
        .collect();
    if positions.len() > MAX_VERTICES {
        return Err(bad(format!("primitive over {MAX_VERTICES} vertices")));
    }
    if positions.iter().flatten().any(|v| !v.is_finite() || v.abs() > MAX_LIMIT) {
        return Err(bad("primitive has a non-finite or out-of-range position"));
    }
    let raw: Vec<u32> = match reader.read_indices() {
        Some(ix) => ix.into_u32().collect(),
        None => (0..positions.len() as u32).collect(),
    };
    if raw.len() % 3 != 0 || raw.iter().any(|&i| i as usize >= positions.len()) {
        return Err(bad("primitive has invalid indices"));
    }
    let at = |i: u32| Vec3::from_array(positions[i as usize]);
    let indices: Vec<u32> = raw
        .chunks_exact(3)
        .filter(|t| !degenerate(at(t[0]), at(t[1]), at(t[2])))
        .flatten()
        .copied()
        .collect();
    if indices.is_empty() {
        return Ok(None);
    }
    if indices.len() > MAX_INDICES {
        return Err(bad(format!("primitive over {} triangles", MAX_INDICES / 3)));
    }
    let normals = match reader.read_normals() {
        Some(n) => {
            let n: Vec<[f32; 3]> = n.collect();
            if n.len() != positions.len() {
                return Err(bad("primitive normals do not match its positions"));
            }
            n.into_iter()
                .map(Vec3::from_array)
                .map(|n| if n.is_finite() && n.length_squared() > 1e-12 { n } else { Vec3::Y })
                .collect()
        }
        None => smooth_normals(&positions, &indices),
    };
    let uvs: Vec<[f32; 2]> = match reader.read_tex_coords(uv_set) {
        Some(uv) => {
            let uv: Vec<[f32; 2]> = uv.into_f32().collect();
            if uv.len() != positions.len() {
                return Err(bad("primitive texture coordinates do not match its positions"));
            }
            if uv.iter().flatten().any(|v| !v.is_finite() || v.abs() > MAX_LIMIT) {
                return Err(bad("primitive has a non-finite texture coordinate"));
            }
            uv
        }
        None => vec![[0.0; 2]; positions.len()],
    };
    let vertices = positions
        .iter()
        .zip(normals)
        .zip(uvs)
        .map(|((p, n), uv)| Vertex {
            position: *p,
            normal: n.to_array(),
            uv,
        })
        .collect();
    Ok(Some(Primitive {
        vertices,
        indices,
        material: p.material().index(),
    }))
}

/// Area-weighted vertex normals for a primitive exported without them.
fn smooth_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<Vec3> {
    let mut acc = vec![Vec3::ZERO; positions.len()];
    for t in indices.chunks_exact(3) {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| Vec3::from_array(positions[i as usize]));
        let n = (b - a).cross(c - a);
        for &i in t {
            acc[i as usize] += n;
        }
    }
    acc.into_iter()
        .map(|n| n.try_normalize().unwrap_or(Vec3::Y))
        .collect()
}

fn extras(raw: &gltf::json::Extras) -> Result<Option<serde_json::Value>, String> {
    let Some(raw) = raw else { return Ok(None) };
    if raw.get().len() > MAX_EXTRAS_BYTES {
        return Err(bad(format!("node extras over {MAX_EXTRAS_BYTES} bytes")));
    }
    let value: serde_json::Value = serde_json::from_str(raw.get()).map_err(bad)?;
    fn depth(v: &serde_json::Value) -> usize {
        match v {
            serde_json::Value::Array(a) => 1 + a.iter().map(depth).max().unwrap_or(0),
            serde_json::Value::Object(o) => 1 + o.values().map(depth).max().unwrap_or(0),
            _ => 0,
        }
    }
    if depth(&value) > MAX_EXTRAS_DEPTH {
        return Err(bad("node extras nest too deeply"));
    }
    Ok(Some(value))
}

fn json_to_lua(lua: &Lua, v: &serde_json::Value) -> mlua::Result<Value> {
    Ok(match v {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(b) => Value::Boolean(*b),
        serde_json::Value::Number(n) => Value::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Value::String(lua.create_string(s)?),
        serde_json::Value::Array(a) => {
            Value::Table(lua.create_sequence_from(
                a.iter().map(|v| json_to_lua(lua, v)).collect::<mlua::Result<Vec<_>>>()?,
            )?)
        }
        serde_json::Value::Object(o) => {
            let t = lua.create_table()?;
            for (k, v) in o {
                t.set(k.as_str(), json_to_lua(lua, v)?)?;
            }
            Value::Table(t)
        }
    })
}

/// Linear → sRGB `#rrggbbaa`, the form `scene3d` object colours take.
fn css_color([r, g, b, a]: [f32; 4]) -> String {
    let srgb = |c: f32| {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
    };
    let byte = |c: f32| (c * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        byte(srgb(r)),
        byte(srgb(g)),
        byte(srgb(b)),
        byte(a.clamp(0.0, 1.0))
    )
}

fn seq(lua: &Lua, v: &[f32]) -> mlua::Result<Table> {
    lua.create_sequence_from(v.iter().copied())
}

fn to_lua(lua: &Lua, glb: Glb, path: &str) -> mlua::Result<Table> {
    let sizes: Vec<(usize, usize)> = glb
        .meshes
        .iter()
        .flatten()
        .map(|p| (p.vertices.len(), p.indices.len()))
        .collect();
    let budget = MeshBudget::of(lua)?;
    budget.admit(&sizes)?;
    let image_sizes: Vec<(u32, u32)> =
        glb.images.iter().flatten().map(|i| (i.width, i.height)).collect();
    let texture_budget = TextureBudget::of(lua)?;
    texture_budget.admit(&image_sizes)?;
    let textures: Vec<Option<Arc<TextureData>>> = glb
        .images
        .into_iter()
        .map(|i| {
            i.map(|i| TextureData::new(i.width, i.height, i.rgba))
                .transpose()
                .map_err(|e| Error::runtime(bad(e)))
        })
        .collect::<mlua::Result<_>>()?;
    type Bounds = ([f32; 3], [f32; 3]);
    let mut handles: Vec<Vec<(Arc<MeshData>, Option<usize>, Bounds)>> = Vec::new();
    for prims in glb.meshes {
        let mut out = Vec::new();
        for p in prims {
            let bounds = p.vertices.iter().fold(
                ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]),
                |(lo, hi), v| {
                    (
                        std::array::from_fn(|i| lo[i].min(v.position[i])),
                        std::array::from_fn(|i| hi[i].max(v.position[i])),
                    )
                },
            );
            let mesh = MeshData::new(p.vertices, p.indices).map_err(|e| Error::runtime(bad(e)))?;
            out.push((mesh, p.material, bounds));
        }
        handles.push(out);
    }
    // Only now, with every mesh valid: a failed file leaves the budget as it was.
    for (mesh, _, _) in handles.iter().flatten() {
        budget.register(mesh);
    }
    for texture in textures.iter().flatten() {
        texture_budget.register(texture);
    }
    drop((budget, texture_budget));
    // One handle per image, shared by every material that uses it.
    let texture_handles = textures
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            t.map(|t| lua.create_userdata(LuaTexture(t, format!("{path} image {}", i + 1).into())))
                .transpose()
        })
        .collect::<mlua::Result<Vec<_>>>()?;

    let model = lua.create_table()?;
    let meshes = lua.create_table()?;
    for prims in handles {
        let list = lua.create_table()?;
        for (mesh, material, (min, max)) in prims {
            let p = lua.create_table()?;
            p.set("mesh", lua.create_userdata(LuaMesh(mesh))?)?;
            p.set("material", material.map(|m| m + 1))?;
            p.set("min", seq(lua, &min)?)?;
            p.set("max", seq(lua, &max)?)?;
            list.push(p)?;
        }
        meshes.push(list)?;
    }
    let materials = lua.create_table()?;
    for m in &glb.materials {
        let t = lua.create_table()?;
        t.set("name", m.name.as_deref())?;
        t.set("color", css_color(m.base_color))?;
        if let Some(i) = m.texture {
            t.set("texture", texture_handles[i].clone())?;
        }
        materials.push(t)?;
    }
    let nodes = lua.create_table()?;
    for n in &glb.nodes {
        let t = lua.create_table()?;
        t.set("name", n.name.as_deref())?;
        t.set("parent", n.parent.map(|p| p + 1))?;
        t.set("translation", seq(lua, &n.translation)?)?;
        t.set("rotation", seq(lua, &n.rotation)?)?;
        t.set("scale", seq(lua, &n.scale)?)?;
        t.set("mesh", n.mesh.map(|m| m + 1))?;
        if let Some(extras) = &n.extras {
            t.set("extras", json_to_lua(lua, extras)?)?;
        }
        nodes.push(t)?;
    }
    model.set("nodes", nodes)?;
    model.set("meshes", meshes)?;
    model.set("materials", materials)?;
    model.set(
        "roots",
        lua.create_sequence_from(glb.roots.iter().map(|r| r + 1))?,
    )?;
    Ok(model)
}

/// Adds `gfx.load_glb`; it reads through whatever [`set_reader`] supplied.
pub(super) fn install(lua: &Lua, gfx: &Table) -> mlua::Result<()> {
    gfx.set(
        "load_glb",
        lua.create_function(move |lua, path: String| {
            let read = lua
                .app_data_ref::<Reader>()
                .map(|r| r.0.clone())
                .ok_or_else(|| Error::runtime("this host stores no binary files"))?;
            let bytes = read(&path).map_err(Error::runtime)?;
            let glb = parse(&bytes).map_err(|e| Error::runtime(format!("{path}: {e}")))?;
            to_lua(lua, glb, &path).map_err(|e| Error::runtime(format!("{path}: {e}")))
        })?,
    )
}

pub(crate) fn set_reader(lua: &Lua, read: ReadFile) {
    lua.set_app_data(Reader(read));
}

pub(crate) fn set_listing(lua: &Lua, list: ListFiles) {
    lua.set_app_data(Listing(list));
}

#[cfg(test)]
mod tests;
