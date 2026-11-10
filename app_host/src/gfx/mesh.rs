//! Strict Lua triangle resources; native payloads are bounded independently of Lua heap memory.

use std::{
    cell::RefCell,
    mem::size_of,
    sync::{Arc, Weak},
};

use mlua::{Error, Lua, Table, UserData, Value};
use runtime::scene3d::mesh::{MAX_INDICES, MAX_VERTICES, MeshData, Vertex};

use super::{named_fields, need, positional_len, vec3};

pub(super) struct LuaMesh(pub Arc<MeshData>);
impl UserData for LuaMesh {}

const MAX_LIVE_MESHES: usize = 128;
const MAX_LIVE_BYTES: usize = 64 * 1024 * 1024;

/// The VM's live native meshes, shared by every constructor (`gfx.mesh`, `gfx.load_glb`).
/// Weak: a mesh no handle or scene holds any more stops counting.
#[derive(Default)]
pub(super) struct MeshBudget(RefCell<Vec<Weak<MeshData>>>);

impl MeshBudget {
    pub(super) fn of(lua: &Lua) -> mlua::Result<mlua::AppDataRef<'_, MeshBudget>> {
        lua.app_data_ref::<MeshBudget>()
            .ok_or_else(|| Error::runtime("gfx is not installed"))
    }

    /// Admit meshes of `(vertices, indices)` together: all fit, or none are made.
    pub(super) fn admit(&self, sizes: &[(usize, usize)]) -> mlua::Result<()> {
        let mut live = self.0.borrow_mut();
        live.retain(|m| m.strong_count() > 0);
        let bytes: usize = live
            .iter()
            .filter_map(Weak::upgrade)
            .map(|m| m.payload_bytes())
            .sum();
        let more: usize = sizes
            .iter()
            .map(|(nv, ni)| nv * size_of::<Vertex>() + ni * size_of::<u32>())
            .sum();
        if live.len() + sizes.len() > MAX_LIVE_MESHES || bytes + more > MAX_LIVE_BYTES {
            return Err(Error::runtime(
                "mesh resources exceed the VM's 128-mesh or 64 MiB budget",
            ));
        }
        Ok(())
    }

    pub(super) fn register(&self, mesh: &Arc<MeshData>) {
        self.0.borrow_mut().push(Arc::downgrade(mesh));
    }
}

pub(super) fn install(lua: &Lua, gfx: &Table) -> mlua::Result<()> {
    lua.set_app_data(MeshBudget::default());
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
                    "mesh needs 1..262144 vertices and 1..524288 triangles",
                ));
            }
            MeshBudget::of(lua)?.admit(&[(nv, ni)])?;
            let mut v = Vec::with_capacity(nv);
            for i in 1..=nv {
                let vertex: Table = vertices.get(i)?;
                named_fields(&vertex, "mesh vertex", &["position", "normal", "uv"])?;
                let uv = match vertex.get::<Option<Table>>("uv")? {
                    None => [0.0; 2],
                    Some(uv) if positional_len(&uv, "mesh vertex.uv")? == 2 => {
                        [uv.get(1)?, uv.get(2)?]
                    }
                    Some(_) => return Err(Error::runtime("mesh vertex.uv needs u and v")),
                };
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
                    uv,
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
                ix.push((n as usize - 1) as u32);
            }
            let mesh = MeshData::new(v, ix).map_err(Error::external)?;
            MeshBudget::of(lua)?.register(&mesh);
            lua.create_userdata(LuaMesh(mesh))
        })?,
    )
}

#[cfg(test)]
mod tests;
