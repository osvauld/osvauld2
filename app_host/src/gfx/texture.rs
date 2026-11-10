//! `gfx.load_texture(path)` and the VM's texture budget, shared with `gfx.load_glb`.

use std::{
    cell::RefCell,
    sync::{Arc, Weak},
};

use mlua::{Error, Lua, Table, UserData};
use runtime::scene3d::texture::{TextureData, payload_bytes};

use super::glb::Reader;
use super::image;

pub(super) struct LuaTexture(pub Arc<TextureData>);
impl UserData for LuaTexture {}

const MAX_LIVE_TEXTURES: usize = 64;
const MAX_LIVE_BYTES: usize = 128 * 1024 * 1024;

/// Live native textures (mips included); weak, like `MeshBudget`.
#[derive(Default)]
pub(super) struct TextureBudget(RefCell<Vec<Weak<TextureData>>>);

impl TextureBudget {
    pub(super) fn of(lua: &Lua) -> mlua::Result<mlua::AppDataRef<'_, TextureBudget>> {
        lua.app_data_ref::<TextureBudget>()
            .ok_or_else(|| Error::runtime("gfx is not installed"))
    }

    /// Admit textures of `(width, height)` together, before decoding: all fit, or none.
    pub(super) fn admit(&self, sizes: &[(u32, u32)]) -> mlua::Result<()> {
        let mut live = self.0.borrow_mut();
        live.retain(|t| t.strong_count() > 0);
        let bytes: usize = live
            .iter()
            .filter_map(Weak::upgrade)
            .map(|t| t.payload_bytes())
            .sum();
        let more: usize = sizes.iter().map(|&(w, h)| payload_bytes(w, h)).sum();
        if live.len() + sizes.len() > MAX_LIVE_TEXTURES || bytes + more > MAX_LIVE_BYTES {
            return Err(Error::runtime(
                "textures exceed the VM's 64-texture or 128 MiB budget",
            ));
        }
        Ok(())
    }

    pub(super) fn register(&self, texture: &Arc<TextureData>) {
        self.0.borrow_mut().push(Arc::downgrade(texture));
    }
}

pub(super) fn install(lua: &Lua, gfx: &Table) -> mlua::Result<()> {
    lua.set_app_data(TextureBudget::default());
    gfx.set(
        "load_texture",
        lua.create_function(|lua, path: String| {
            let read = lua
                .app_data_ref::<Reader>()
                .map(|r| r.0.clone())
                .ok_or_else(|| Error::runtime("this host stores no binary files"))?;
            let bytes = read(&path).map_err(Error::runtime)?;
            let fail = |e: String| Error::runtime(format!("{path}: {e}"));
            let size = image::size(&bytes).map_err(fail)?;
            image::check(size.0, size.1).map_err(fail)?;
            TextureBudget::of(lua)?.admit(&[size])?;
            let img = image::decode(&bytes).map_err(fail)?;
            let texture = TextureData::new(img.width, img.height, img.rgba)
                .map_err(|e| fail(e.to_string()))?;
            TextureBudget::of(lua)?.register(&texture);
            lua.create_userdata(LuaTexture(texture))
        })?,
    )
}
