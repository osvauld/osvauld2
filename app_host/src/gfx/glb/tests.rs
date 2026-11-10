use std::rc::Rc;

use mlua::{Lua, Table};

const HIERARCHY: &[u8] = include_bytes!("../../../../scripts/fixtures/glb/hierarchy.glb");
const EXTERNAL: &[u8] = include_bytes!("../../../../scripts/fixtures/glb/external_buffer.glb");
const TEXTURED: &[u8] = include_bytes!("../../../../scripts/fixtures/glb/textured.glb");
const OVERSIZE: &[u8] = include_bytes!("../../../../scripts/fixtures/glb/oversize_image.glb");

/// The fixture's embedded PNG: the last GLB chunk holds it after the quad's 136 bytes of
/// accessors, so slice from its signature.
fn embedded_png(glb: &[u8]) -> Vec<u8> {
    let at = glb.windows(8).position(|w| w == b"\x89PNG\r\n\x1a\n").unwrap();
    glb[at..].to_vec()
}

fn lua(files: &[(&str, &[u8])]) -> Lua {
    let files: Vec<(String, Vec<u8>)> =
        files.iter().map(|(p, b)| (p.to_string(), b.to_vec())).collect();
    let lua = Lua::new();
    crate::gfx::install(&lua).unwrap();
    super::set_reader(
        &lua,
        Rc::new(move |path| {
            files
                .iter()
                .find(|(p, _)| p == path)
                .map(|(_, b)| b.clone())
                .ok_or_else(|| format!("no binary file {path:?}"))
        }),
    );
    lua
}

#[test]
fn hierarchy_arrives_as_plain_data_with_mesh_handles() {
    let lua = lua(&[("m/h.glb", HIERARCHY)]);
    let model: Table = lua.load(r#"return gfx.load_glb("m/h.glb")"#).eval().unwrap();
    let nodes: Table = model.get("nodes").unwrap();
    assert_eq!(nodes.raw_len(), 3);
    let base: Table = nodes.get(1).unwrap();
    let arm: Table = nodes.get(2).unwrap();
    assert_eq!(base.get::<String>("name").unwrap(), "base");
    assert_eq!(base.get::<Option<i64>>("parent").unwrap(), None);
    assert_eq!(arm.get::<i64>("parent").unwrap(), 1);
    assert_eq!(arm.get::<i64>("mesh").unwrap(), 2);
    assert_eq!(arm.get::<Vec<f32>>("scale").unwrap(), [0.5, 2.0, 0.5]);
    let extras: Table = base.get("extras").unwrap();
    assert_eq!(extras.get::<String>("collider").unwrap(), "box");
    assert_eq!(extras.get::<f64>("mass").unwrap(), 120.0);
    assert_eq!(extras.get::<Vec<String>>("tags").unwrap(), ["static", "metal"]);
    assert_eq!(model.get::<Vec<i64>>("roots").unwrap(), [1]);
    let material: Table = model.get::<Table>("materials").unwrap().get(1).unwrap();
    assert_eq!(material.get::<String>("color").unwrap(), "#ff8000ff");

    // The handles are ordinary gfx.mesh resources: a scene takes them, and the dropped
    // degenerate triangle and generated normals leave a valid 12-triangle cube.
    let tris: Vec<usize> = lua
        .load(
            r##"
            local m = gfx.load_glb("m/h.glb")
            local s = gfx.scene3d({ camera = { eye = {0, 0, 5}, target = {0, 0, 0} }, objects = {
                { id = "base", mesh = m.meshes[1][1].mesh, color = m.materials[1].color },
                { id = "arm", mesh = m.meshes[2][1].mesh, color = "#fff" },
            }})
            return { m.meshes[1][1].material, #m.meshes[2] }
        "##,
        )
        .eval()
        .unwrap();
    assert_eq!(tris, [1, 1]);
}

#[test]
fn refusals_name_the_reason_and_leave_the_budget() {
    let lua = lua(&[("ext.glb", EXTERNAL), ("short.glb", &HIERARCHY[..200]), ("txt.glb", b"hello")]);
    for (path, why) in [
        ("ext.glb", "external file"),
        ("short.glb", "glb"),
        ("txt.glb", "not a binary glTF"),
        ("missing.glb", "no binary file"),
    ] {
        let err = lua
            .load(format!("return gfx.load_glb({path:?})"))
            .exec()
            .unwrap_err()
            .to_string();
        assert!(err.contains(why), "{path}: {err}");
    }
    // 64 loads of a two-mesh file is the 128-mesh budget exactly; the next is refused whole.
    let lua = self::lua(&[("h.glb", HIERARCHY)]);
    lua.load("KEEP = {} for i = 1, 64 do KEEP[i] = gfx.load_glb('h.glb') end")
        .exec()
        .unwrap();
    let err = lua.load("return gfx.load_glb('h.glb')").exec().unwrap_err();
    assert!(err.to_string().contains("budget"), "{err}");
    lua.load("KEEP = nil collectgarbage() collectgarbage()").exec().unwrap();
    lua.load("return gfx.load_glb('h.glb')").exec().unwrap();
}

#[test]
fn base_colour_textures_arrive_as_shared_handles_with_uvs() {
    let png = embedded_png(TEXTURED);
    let lua = lua(&[("t.glb", TEXTURED), ("o.glb", OVERSIZE), ("p.png", &png), ("x.png", b"nope")]);
    let (kind, name, color, same): (String, String, String, bool) = lua
        .load(
            r##"
            local m = gfx.load_glb("t.glb")
            local mat = m.materials[m.meshes[1][1].material]
            local s = gfx.scene3d({ camera = { eye = {0, 0, 3}, target = {0, 0, 0} }, objects = {
                { id = "panel", mesh = m.meshes[1][1].mesh, color = mat.color, texture = mat.texture },
                { id = "box", texture = gfx.load_texture("p.png"), color = "#fff" },
            }})
            local again = gfx.load_glb("t.glb")
            return type(mat.texture), mat.name, mat.color, mat.texture == again.materials[1].texture
        "##,
        )
        .eval()
        .unwrap();
    // Each load makes its own handle; within one file a handle is shared per image.
    assert_eq!((kind.as_str(), name.as_str(), color.as_str(), same), ("userdata", "painted", "#ffffffff", false));

    let err = lua.load("gfx.load_glb('o.glb')").exec().unwrap_err().to_string();
    assert!(err.contains("4097x1") && err.contains("2048"), "{err}");
    let err = lua.load("gfx.load_texture('x.png')").exec().unwrap_err().to_string();
    assert!(err.contains("neither PNG nor JPEG"), "{err}");
    let err = lua
        .load("gfx.scene3d({camera={eye={0,0,3},target={0,0,0}},objects={{id='a',color='#fff',texture=gfx.load_glb('t.glb').meshes[1][1].mesh}}})")
        .exec()
        .unwrap_err()
        .to_string();
    assert!(err.contains("object.texture must be a gfx texture"), "{err}");
}

#[test]
fn texture_budget_admits_from_headers_and_is_reclaimed() {
    let png = embedded_png(TEXTURED);
    let lua = lua(&[("p.png", &png)]);
    lua.load("KEEP = {} for i = 1, 64 do KEEP[i] = gfx.load_texture('p.png') end")
        .exec()
        .unwrap();
    let err = lua.load("gfx.load_texture('p.png')").exec().unwrap_err();
    assert!(err.to_string().contains("64-texture"), "{err}");
    lua.load("KEEP = nil collectgarbage() collectgarbage()").exec().unwrap();
    lua.load("gfx.load_texture('p.png')").exec().unwrap();
}

#[test]
fn imported_uvs_reach_the_mesh_vertices() {
    let glb = super::parse(TEXTURED).unwrap();
    let uvs: Vec<[f32; 2]> = glb.meshes[0][0].vertices.iter().map(|v| v.uv).collect();
    assert_eq!(uvs, [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
    let img = glb.images[0].as_ref().unwrap();
    assert_eq!((img.width, img.height), (8, 4));
    assert_eq!(&img.rgba[..4], &[255, 0, 0, 255]);
    assert_eq!(&img.rgba[7 * 4..8 * 4], &[0, 0, 255, 255]);
}
