//! `gfx.load_texture(path)`, pictures (2D drawing shapes, `gfx.image`), and the VM's texture
//! budget they share with `gfx.load_glb`.

use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{Arc, Weak},
};

use mlua::{Error, Lua, Table, UserData};
use runtime::frame::Picture;
use runtime::scene3d::texture::{TextureData, payload_bytes};

use super::glb::{Listing, Reader};
use super::image;

/// A texture and the name the dump shows for it: its file, or its model's.
pub(super) struct LuaTexture(pub Arc<TextureData>, pub Arc<str>);
impl UserData for LuaTexture {}

const MAX_LIVE_TEXTURES: usize = 64;
const MAX_LIVE_BYTES: usize = 128 * 1024 * 1024;

/// Live native textures (mips included) and the regions copied out of them; weak, like
/// `MeshBudget`. Only textures count towards the 64.
#[derive(Default)]
pub(super) struct TextureBudget(RefCell<Vec<Live>>);

enum Live {
    Texture(Weak<TextureData>),
    Region(Weak<Picture>),
}

impl TextureBudget {
    pub(super) fn of(lua: &Lua) -> mlua::Result<mlua::AppDataRef<'_, TextureBudget>> {
        lua.app_data_ref::<TextureBudget>()
            .ok_or_else(|| Error::runtime("gfx is not installed"))
    }

    /// Textures and bytes held now.
    pub(super) fn live(&self) -> (usize, usize) {
        let mut live = self.0.borrow_mut();
        live.retain(|l| match l {
            Live::Texture(t) => t.strong_count() > 0,
            Live::Region(r) => r.strong_count() > 0,
        });
        live.iter().fold((0, 0), |(n, bytes), l| match l {
            Live::Texture(t) => (n + 1, bytes + t.upgrade().map_or(0, |t| t.payload_bytes())),
            Live::Region(r) => (n, bytes + r.upgrade().map_or(0, |r| r.copied_bytes())),
        })
    }

    /// Admit textures of `(width, height)` together, before decoding: all fit, or none.
    pub(super) fn admit(&self, sizes: &[(u32, u32)]) -> mlua::Result<()> {
        let more: usize = sizes.iter().map(|&(w, h)| payload_bytes(w, h)).sum();
        self.fits(sizes.len(), more).map_err(Error::runtime)
    }

    fn fits(&self, textures: usize, bytes: usize) -> Result<(), String> {
        let (n, held) = self.live();
        if n + textures > MAX_LIVE_TEXTURES || held + bytes > MAX_LIVE_BYTES {
            return Err(format!(
                "textures exceed the VM's 64-texture or 128 MiB budget (holding {n}, {} MiB)",
                held >> 20
            ));
        }
        Ok(())
    }

    pub(super) fn register(&self, texture: &Arc<TextureData>) {
        self.0.borrow_mut().push(Live::Texture(Arc::downgrade(texture)));
    }
}

/// What this VM has decoded for pictures, so a file named by forty shapes is read and decoded
/// once, and a region named twice is copied once. Weak: a picture nothing draws is let go.
/// Paths are safe keys because a VM reads one source revision's files.
#[derive(Default)]
struct Pictures {
    files: RefCell<HashMap<String, Weak<TextureData>>>,
    pictures: RefCell<HashMap<(u64, [u32; 4]), Weak<Picture>>>,
}

/// The texture at `path`, as `gfx.load_texture` reads it. Errors are for the caller to place.
fn read_texture(lua: &Lua, path: &str) -> Result<Arc<TextureData>, String> {
    let read = lua
        .app_data_ref::<Reader>()
        .map(|r| r.0.clone())
        .ok_or("this host stores no binary files")?;
    let bytes = read(path)?;
    let fail = |e: String| format!("{path}: {e}");
    let size = image::size(&bytes).map_err(fail)?;
    image::check(size.0, size.1).map_err(fail)?;
    let budget = TextureBudget::of(lua).map_err(|e| e.to_string())?;
    budget.fits(1, payload_bytes(size.0, size.1)).map_err(fail)?;
    let img = image::decode(&bytes).map_err(fail)?;
    let texture =
        TextureData::new(img.width, img.height, img.rgba).map_err(|e| fail(e.to_string()))?;
    budget.register(&texture);
    Ok(texture)
}

/// A drawing shape's `image`: the file's texture, decoded once per VM while something holds it.
/// A missing file lists the app's pictures.
pub(super) fn picture_file(lua: &Lua, path: &str) -> Result<Arc<TextureData>, String> {
    let pictures = lua.app_data_ref::<Pictures>().ok_or("gfx is not installed")?;
    if let Some(t) = pictures.files.borrow().get(path).and_then(Weak::upgrade) {
        return Ok(t);
    }
    if let Some(list) = lua.app_data_ref::<Listing>().map(|l| l.0.clone()) {
        let names = list();
        if !names.iter().any(|n| n == path) {
            let mut pics: Vec<&str> = names
                .iter()
                .map(String::as_str)
                .filter(|n| [".png", ".jpg", ".jpeg"].iter().any(|e| n.to_lowercase().ends_with(e)))
                .collect();
            pics.sort();
            let has = if pics.is_empty() { "no .png or .jpg files".into() } else { pics.join(", ") };
            return Err(format!("no {path} in this app. It has: {has}"));
        }
    }
    let texture = read_texture(lua, path)?;
    pictures.files.borrow_mut().insert(path.to_string(), Arc::downgrade(&texture));
    Ok(texture)
}

/// `src` of `texture` as a picture named `name`, kept for whoever names it next. All of it shares
/// the texture's pixels; a region is a copy, counted against the budget.
pub(super) fn picture(
    lua: &Lua,
    name: &Arc<str>,
    texture: &Arc<TextureData>,
    src: Option<[u32; 4]>,
) -> Result<Arc<Picture>, String> {
    let (w, h) = texture.size();
    let src = src.unwrap_or([0, 0, w, h]);
    let pictures = lua.app_data_ref::<Pictures>().ok_or("gfx is not installed")?;
    let key = (texture.resource_id(), src);
    if let Some(p) = pictures.pictures.borrow().get(&key).and_then(Weak::upgrade) {
        return Ok(p);
    }
    let make = |src| Picture::new(name.clone(), texture.clone(), Some(src)).map_err(|e| e.to_string());
    let [x, y, sw, sh] = src.map(u64::from);
    let inside = sw > 0 && sh > 0 && x + sw <= u64::from(w) && y + sh <= u64::from(h);
    let picture = if src == [0, 0, w, h] || !inside {
        make(src)? // nothing to admit, or the author's mistake, said as such rather than as a budget
    } else {
        let budget = TextureBudget::of(lua).map_err(|e| e.to_string())?;
        budget.fits(0, (sw * sh * 4) as usize)?;
        let region = make(src)?;
        budget.0.borrow_mut().push(Live::Region(Arc::downgrade(&region)));
        region
    };
    pictures.pictures.borrow_mut().insert(key, Arc::downgrade(&picture));
    Ok(picture)
}

pub(super) fn install(lua: &Lua, gfx: &Table) -> mlua::Result<()> {
    lua.set_app_data(TextureBudget::default());
    lua.set_app_data(Pictures::default());
    gfx.set(
        "load_texture",
        lua.create_function(|lua, path: String| {
            let texture = read_texture(lua, &path).map_err(Error::runtime)?;
            lua.create_userdata(LuaTexture(texture, path.into()))
        })?,
    )
}

#[cfg(test)]
mod tests;
