//! Strict Lua triangle resources; native payloads are bounded independently of Lua heap memory.

use std::{
    cell::RefCell,
    sync::{Arc, Weak},
};

use mlua::{Error, Lua, Table, UserData, Value};
use runtime::scene3d::mesh::{MAX_INDICES, MAX_VERTICES, MeshData, Vertex};

use super::{named_fields, need, positional_len, vec3};

pub(super) struct LuaMesh(pub Arc<MeshData>);
impl UserData for LuaMesh {}

pub(super) fn install(lua: &Lua, gfx: &Table) -> mlua::Result<()> {
    let live = RefCell::new(Vec::<Weak<MeshData>>::new());
    gfx.set(
        "mesh",
        lua.create_function(move |lua, spec: Table| {
            named_fields(&spec, "mesh", &["vertices", "indices"])?;
            let vertices: Table = need(&spec, "mesh", "vertices")?;
            let indices: Table = need(&spec, "mesh", "indices")?;
            let nv = positional_len(&vertices, "mesh.vertices")?;
            let ni = positional_len(&indices, "mesh.indices")?;
            if nv == 0 || nv > MAX_VERTICES || ni == 0 || ni > MAX_INDICES || ni % 3 != 0 {
                return Err(Error::runtime(
                    "mesh needs 1..65536 vertices and 1..65536 triangles",
                ));
            }
            let mut live = live.borrow_mut();
            live.retain(|m| m.strong_count() > 0);
            let bytes: usize = live
                .iter()
                .filter_map(Weak::upgrade)
                .map(|m| m.payload_bytes())
                .sum();
            if live.len() >= 128 || bytes + nv * 24 + ni * 2 > 32 * 1024 * 1024 {
                return Err(Error::runtime(
                    "mesh resources exceed the VM's 128-mesh or 32 MiB budget",
                ));
            }
            let mut v = Vec::with_capacity(nv);
            for i in 1..=nv {
                let vertex: Table = vertices.get(i)?;
                named_fields(&vertex, "mesh vertex", &["position", "normal"])?;
                v.push(Vertex {
                    position: vec3(
                        need(&vertex, "mesh vertex", "position")?,
                        "mesh vertex.position",
                    )?
                    .to_array(),
                    normal: vec3(
                        need(&vertex, "mesh vertex", "normal")?,
                        "mesh vertex.normal",
                    )?
                    .to_array(),
                });
            }
            let mut ix = Vec::with_capacity(ni);
            for i in 1..=ni {
                let n = match indices.get::<Value>(i)? {
                    Value::Integer(n) => n as f64,
                    Value::Number(n) => n,
                    _ => return Err(Error::runtime(format!("mesh index {i} must be an integer"))),
                };
                if !(1.0..=nv as f64).contains(&n) || n.fract() != 0.0 {
                    return Err(Error::runtime(format!(
                        "mesh index {i} must be an integer within 1..{nv}"
                    )));
                }
                ix.push((n as usize - 1) as u16);
            }
            let mesh = MeshData::new(v, ix).map_err(Error::external)?;
            live.push(Arc::downgrade(&mesh));
            lua.create_userdata(LuaMesh(mesh))
        })?,
    )
}

#[cfg(test)]
mod tests;
