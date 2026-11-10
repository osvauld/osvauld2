//! Pictures in drawings and frames (docs/design/2d-characters.md L-C3, L-C4).

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use mlua::{AnyUserData, Lua};
use runtime::frame::Picture;
use runtime::scene3d::texture::payload_bytes;
use runtime::vello::kurbo::{Affine, Point};

use super::TextureBudget;
use crate::gfx::LuaFrame;
use crate::tests::png;

const ATLAS: &str = "characters/x/atlas.png";
const PAKA: &[u8] = include_bytes!("../../../../scripts/fixtures/paka/Character 1 - Paka1.png");

/// 64 x 32: red, green over blue, yellow.
fn quads() -> Vec<u8> {
    png(64, 32, |x, y| match (x >= 32, y >= 16) {
        (false, false) => [255, 0, 0, 255],
        (true, false) => [0, 255, 0, 255],
        (false, true) => [0, 0, 255, 255],
        (true, true) => [255, 255, 0, 255],
    })
}

/// A VM with these binary files; the count is how many times one was read.
fn vm(files: Vec<(&str, Vec<u8>)>) -> (Lua, Rc<Cell<usize>>) {
    let lua = Lua::new();
    crate::gfx::install(&lua).unwrap();
    let files: Vec<(String, Vec<u8>)> = files.into_iter().map(|(p, b)| (p.to_string(), b)).collect();
    let names: Vec<String> = files.iter().map(|(p, _)| p.clone()).collect();
    let reads = Rc::new(Cell::new(0));
    let counted = reads.clone();
    crate::gfx::glb::set_reader(
        &lua,
        Rc::new(move |path| {
            counted.set(counted.get() + 1);
            let found = files.iter().find(|(p, _)| p == path);
            found.map(|(_, b)| b.clone()).ok_or_else(|| format!("no binary file {path:?}"))
        }),
    );
    crate::gfx::glb::set_listing(&lua, Rc::new(move || names.clone()));
    (lua, reads)
}

/// A one-part, one-shape drawing whose shape is `fields`.
fn one_shape(fields: &str) -> String {
    format!(
        r#"return gfx.drawing({{ size = {{64, 64}}, parts = {{
            {{ id = "p", pivot = {{0, 0}}, shapes = {{ {{ {fields} }} }} }} }} }}):pose()"#
    )
}

fn frame(lua: &Lua, source: &str) -> Arc<runtime::frame::Frame> {
    let f: AnyUserData = lua.load(source).eval().unwrap_or_else(|e| panic!("{source}: {e}"));
    f.borrow::<LuaFrame>().unwrap().0.clone()
}

fn srcs(pictures: &[Arc<Picture>]) -> Vec<[u32; 4]> {
    pictures.iter().map(|p| p.src()).collect()
}

/// L-C3: the shape the converter writes is accepted with its `transform` in kurbo order; each
/// mistake is an error naming the shape.
#[test]
fn a_picture_shape_is_accepted_and_its_mistakes_name_the_shape() {
    let wide = png(2049, 1, |_, _| [0, 0, 0, 255]);
    let (lua, _) = vm(vec![
        (ATLAS, quads()),
        ("props/crate.jpg", b"never read".to_vec()),
        ("models/m.glb", b"not a picture".to_vec()),
        ("wide.png", wide),
    ]);

    // x' = a·x + c·y + e, y' = b·x + d·y + f: the green 32 x 16 turned and ×3 covers
    // x 0..48, y 96..192. Read as {a, c, b, d, …} it would land on x 48..96 instead.
    let turned = frame(&lua, &one_shape(&format!(
        r#"image = "{ATLAS}", src = {{32, 0, 32, 16}}, transform = {{0, 3, -3, 0, 48, 96}}"#
    )));
    assert_eq!(srcs(&turned.pictures()), [[32, 0, 32, 16]]);
    assert_eq!(turned.pictures()[0].name(), ATLAS);
    assert_eq!(turned.hit(Point::new(24.0, 150.0)).map(|h| h.id.to_string()).as_deref(), Some("p"));
    assert!(turned.hit(Point::new(60.0, 150.0)).is_none());
    let whole = frame(&lua, &one_shape(&format!(r#"image = "{ATLAS}""#)));
    assert_eq!(srcs(&whole.pictures()), [[0, 0, 64, 32]]);
    assert_eq!(whole.pictures()[0].copied_bytes(), 0, "all of it is the texture's own pixels");

    let shape = "drawing part 1 shape 1";
    let cases = [
        (
            format!(r#"image = "characters/x/missing.png""#),
            format!("{shape}.image: no characters/x/missing.png in this app. It has: {ATLAS}, props/crate.jpg, wide.png"),
        ),
        (
            format!(r#"image = "{ATLAS}", src = {{40, 0, 32, 16}}"#),
            format!("{shape}.src: picture src {{40, 0, 32, 16}} is empty or outside the 64x32 picture"),
        ),
        (format!(r#"image = "{ATLAS}", src = {{0, 0, 0, 4}}"#), format!("{shape}.src: picture src {{0, 0, 0, 4}} is empty")),
        (format!(r#"image = "{ATLAS}", src = {{0.5, 0, 1, 1}}"#), format!("{shape}.src must be {{x, y, w, h}} in whole pixels")),
        (format!(r#"image = "{ATLAS}", src = {{0, 0, 1}}"#), format!("{shape}.src must be {{x, y, w, h}} in whole pixels")),
        (
            format!(r##"image = "{ATLAS}", path = {{ {{"move", 0, 0}}, {{"line", 1, 1}} }}, fill = "#fff""##),
            format!("{shape}: a shape has image or path, not both"),
        ),
        (format!(r##"image = "{ATLAS}", fill = "#fff""##), format!("{shape}: unknown field fill")),
        (format!(r#"image = "{ATLAS}", transform = {{1, 0, 0, 1, 0}}"#), format!("{shape}: transform needs six coefficients")),
        ("image = 3".to_string(), format!("{shape}.image must be a file name, got number")),
        (r#"image = "wide.png""#.to_string(), format!("{shape}.image: wide.png: image is 2049x1; textures are at most 2048x2048")),
    ];
    for (fields, wanted) in cases {
        let err = lua.load(one_shape(&fields)).exec().unwrap_err().to_string();
        assert!(err.contains(&wanted), "{fields}\nwanted {wanted:?}\n   got {err}");
    }

    // Past the budget: 64 textures held, the drawing's picture is one too many.
    drop((turned, whole));
    lua.load("collectgarbage() collectgarbage()").exec().unwrap();
    assert_eq!(TextureBudget::of(&lua).unwrap().live(), (0, 0));
    lua.load(format!("KEEP = {{}} for i = 1, 64 do KEEP[i] = gfx.load_texture({ATLAS:?}) end"))
        .exec()
        .unwrap();
    let err = lua.load(one_shape(&format!(r#"image = "{ATLAS}""#))).exec().unwrap_err().to_string();
    let wanted = format!("{shape}.image: {ATLAS}: textures exceed the VM's 64-texture or 128 MiB budget");
    assert!(err.contains(&wanted), "{err}");
}

/// L-C4: forty shapes on one file read it, decode it and count it once; a region named twice is
/// copied once; all of it comes back when nothing holds it.
#[test]
fn one_file_named_by_forty_shapes_is_read_decoded_and_counted_once() {
    let (lua, reads) = vm(vec![(ATLAS, quads())]);
    let source = format!(
        r#"
        local function shapes(from)
            local list = {{}}
            for i = from, from + 19 do
                local src = ({{ {{0, 0, 32, 16}}, {{32, 0, 32, 16}} }})[i % 3 + 1]
                list[#list + 1] = {{ image = {ATLAS:?}, src = src, transform = {{1, 0, 0, 1, i, 0}} }}
            end
            return list
        end
        local spec = {{ size = {{128, 64}}, parts = {{
            {{ id = "a", pivot = {{0, 0}}, shapes = shapes(1) }},
            {{ id = "b", parent = "a", pivot = {{0, 0}}, shapes = shapes(21) }},
        }} }}
        DRAWINGS = {{ gfx.drawing(spec) }}
        POSED = DRAWINGS[1]:pose()
        return POSED
        "#
    );
    let posed = frame(&lua, &source);
    assert_eq!(reads.get(), 1, "read and decoded once");
    // i = 1 is the second region, 2 the whole picture, 3 the first region.
    assert_eq!(srcs(&posed.pictures()), [[32, 0, 32, 16], [0, 0, 64, 32], [0, 0, 32, 16]]);
    let held = (1, payload_bytes(64, 32) + 2 * 32 * 16 * 4);
    assert_eq!(TextureBudget::of(&lua).unwrap().live(), held, "one texture and two region copies");

    // A second drawing of the same file while the first lives: nothing new.
    lua.load(format!("DRAWINGS[2] = gfx.drawing({{ size = {{8, 8}}, parts = {{ {{ id = 'c', pivot = {{0, 0}}, shapes = {{ {{ image = {ATLAS:?}, src = {{0, 0, 32, 16}} }} }} }} }} }})"))
        .exec()
        .unwrap();
    assert_eq!(reads.get(), 1);
    assert_eq!(TextureBudget::of(&lua).unwrap().live(), held);

    // The drawing holds exactly one of the 64.
    lua.load(format!("KEEP = {{}} for i = 1, 63 do KEEP[i] = gfx.load_texture({ATLAS:?}) end"))
        .exec()
        .unwrap();
    let err = lua.load(format!("gfx.load_texture({ATLAS:?})")).exec().unwrap_err();
    assert!(err.to_string().contains("64-texture"), "{err}");

    drop(posed);
    lua.load("KEEP, DRAWINGS, POSED = nil, nil, nil collectgarbage() collectgarbage()").exec().unwrap();
    assert_eq!(TextureBudget::of(&lua).unwrap().live(), (0, 0));
}

/// S-C5's host half: `gfx.image` puts a texture handle on a frame at natural size, whole or a
/// region, named for the pointer.
#[test]
fn gfx_image_draws_a_texture_handle_on_a_frame() {
    let (lua, _) = vm(vec![(ATLAS, quads())]);
    let f = frame(
        &lua,
        &format!(
            r#"local t = gfx.load_texture({ATLAS:?})
            return gfx.frame({{ width = 64, height = 48,
                gfx.image({{ texture = t, id = "crate" }}),
                gfx.image({{ texture = t, src = {{32, 16, 32, 16}}, transform = {{1, 0, 0, 1, 0, 32}}, id = "lid" }}),
            }})"#
        ),
    );
    assert_eq!(srcs(&f.pictures()), [[0, 0, 64, 32], [32, 16, 32, 16]]);
    assert_eq!(f.pictures()[0].name(), ATLAS);
    assert_eq!(f.pictures()[0].copied_bytes(), 0);
    let at = |x, y| f.hit(Point::new(x, y)).map(|h| h.id.to_string());
    assert_eq!((at(63.0, 31.0).as_deref(), at(10.0, 40.0).as_deref(), at(40.0, 40.0)), (Some("crate"), Some("lid"), None));

    for (source, wanted) in [
        ("gfx.frame({ width = 1, height = 1, gfx.image({}) })", "image needs texture, a gfx texture"),
        ("gfx.frame({ width = 1, height = 1, gfx.image({ texture = 'x.png' }) })", "image.texture must be a gfx texture, got string"),
        ("gfx.frame({ width = 1, height = 1, gfx.image({ texture = gfx.load_texture('characters/x/atlas.png'), scale = 2 }) })", "image: unknown field scale"),
        ("gfx.frame({ width = 1, height = 1, gfx.image({ texture = gfx.load_texture('characters/x/atlas.png'), src = {0, 30, 4, 4} }) })", "image.src: picture src {0, 30, 4, 4} is empty or outside the 64x32 picture"),
    ] {
        let err = lua.load(source).exec().unwrap_err().to_string();
        assert!(err.contains(wanted), "{source}\nwanted {wanted:?}\n   got {err}");
    }
}

/// One shape exactly as `scripts/spine_to_lua.py` writes it for Paka's sword, over the pack's own
/// atlas: the converter's contract and this one agree.
#[test]
fn a_converted_paka_shape_loads_unchanged() {
    let (lua, _) = vm(vec![("characters/paka/atlas.png", PAKA.to_vec())]);
    let posed = frame(
        &lua,
        r#"return gfx.drawing({ size = { 1972, 1195 }, parts = {
		{ id = "slot:sword", pivot = { 600.56, 348.52 }, shapes = {
			{ image = "characters/paka/atlas.png", src = { 2, 161, 472, 747 },
			  transform = { 0.507978, -0.861373, 0.861365, 0.507991, 542.138454, 361.175453 } },
		} },
        } }):pose()"#,
    );
    assert_eq!(srcs(&posed.pictures()), [[2, 161, 472, 747]]);
    let place = Affine::new([0.507978, -0.861373, 0.861365, 0.507991, 542.138454, 361.175453]);
    let middle = place * Point::new(236.0, 373.5);
    assert_eq!(posed.hit(middle).map(|h| h.id.to_string()).as_deref(), Some("slot:sword"));
}
