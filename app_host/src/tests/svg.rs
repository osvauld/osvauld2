//! `gfx.svg`: an SVG from the app's own folder, as a drawing module (docs/design/svg-drawings.md).
//!
//! Driven through a real app VM, because the file only reaches Lua through the app's `files` map.

use super::*;

fn app(files: &[(&str, &str)]) -> LuaApp<LuaMsg> {
    let src = LoroDoc::new();
    let map = src.get_map("files");
    let main = map.insert_container("main.lua", LoroText::new()).unwrap();
    main.insert(0, "return function() return ui.text{ 'x' } end").unwrap();
    for (path, body) in files {
        let t = map.insert_container(*path, LoroText::new()).unwrap();
        t.insert(0, body).unwrap();
    }
    src.commit();
    let app = LuaApp::open(src, Rc::new(|_| Ok(None)), noop_wake(), identity()).unwrap();
    assert_eq!(app.error, None);
    app
}

/// Compact readings of a spec, so a test states the whole answer in one string.
const READ: &str = r##"
local function n(v) return string.format("%g", math.floor(v * 100 + 0.5) / 100) end
function parts(spec)
    local out = {}
    for _, p in ipairs(spec.parts) do
        table.insert(out, p.id .. (p.parent and ("<" .. p.parent) or "") .. "@" .. n(p.pivot[1])
            .. "," .. n(p.pivot[2]) .. "#" .. #(p.shapes or {}))
    end
    return table.concat(out, " ")
end
function path(cmds)
    local out = {}
    for _, c in ipairs(cmds) do
        local s = { c[1] }
        for i = 2, #c do table.insert(s, n(c[i])) end
        table.insert(out, table.concat(s, " "))
    end
    return table.concat(out, "|")
end
function kinds(cmds)
    local out = {}
    for _, c in ipairs(cmds) do table.insert(out, c[1]) end
    return table.concat(out, " ")
end
function dropped(report)
    local out = {}
    for _, d in ipairs(report.dropped) do table.insert(out, d.feature .. ":" .. d.id) end
    table.sort(out)
    return table.concat(out, " ")
end
function shape(spec, part, i)
    for _, p in ipairs(spec.parts) do
        if p.id == part then return p.shapes[i or 1] end
    end
end
"##;

fn eval(app: &LuaApp<LuaMsg>, body: &str) -> mlua::Result<String> {
    app.vm.load(format!("{READ}\n{body}")).eval::<String>()
}

fn error(app: &LuaApp<LuaMsg>, body: &str) -> String {
    match eval(app, body) {
        Ok(v) => panic!("expected an error, got {v:?}"),
        Err(e) => e.to_string(),
    }
}

const SQUARE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
  <rect x="0" y="0" width="10" height="10" fill="#ff0000"/></svg>"##;

// ---------------------------------------------------------------- step 1: reading the file

/// L-V1
#[test]
fn gfx_svg_reads_a_file_from_the_app_and_gfx_drawing_takes_it() {
    let app = app(&[("art/car.svg", SQUARE)]);
    assert_eq!(
        eval(&app, "local spec = gfx.svg('art/car.svg') gfx.drawing(spec) return parts(spec)").unwrap(),
        "root@10,10#1"
    );
}

/// L-V2
#[test]
fn gfx_svg_sees_only_the_apps_own_svg_files() {
    let app = app(&[("art/car.svg", SQUARE)]);
    for bad in ["../car.svg", "/etc/car.svg", "art/../art/car.svg", "main.lua", "art/car"] {
        let e = error(&app, &format!("gfx.svg({bad:?}) return ''"));
        assert!(e.contains("gfx.svg"), "{bad}: {e}");
    }
    let e = error(&app, "gfx.svg('nope.svg') return ''");
    assert!(e.contains("It has: art/car.svg"), "{e}");
    assert!(error(&app, "gfx.svg(3) return ''").contains("gfx.svg"));
}

/// L-V3
#[test]
fn a_bad_svg_is_refused_whole() {
    let huge = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"><!--{}--></svg>", "x".repeat(4 << 20));
    let bomb = r#"<?xml version="1.0"?><!DOCTYPE svg [<!ENTITY a "aaaa">]>
        <svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><text>&a;</text></svg>"#;
    let long = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><path fill=\"#000\" d=\"M0 0 {}\"/></svg>",
        "L1 1 L0 1 ".repeat(100_001)
    );
    let app = app(&[
        ("broken.svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect"),
        ("huge.svg", &huge),
        ("bomb.svg", bomb),
        ("long.svg", &long),
    ]);
    for (name, says) in [
        ("broken.svg", "broken.svg"),
        ("huge.svg", "4 MiB"),
        ("bomb.svg", "ENTITY"),
        ("long.svg", "200000"),
    ] {
        let e = error(&app, &format!("gfx.svg('{name}') return ''"));
        assert!(e.contains(name) && e.contains(says), "{name}: {e}");
    }
}

/// L-V3: no file outside the app is ever opened; an image is only reported.
#[test]
fn an_svg_never_reads_files_it_points_at() {
    let app = app(&[(
        "pic.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10">
            <image id="photo" href="/etc/passwd" width="10" height="10"/>
            <image id="old" xlink:href="file:///etc/hostname" width="10" height="10"/>
            <rect width="2" height="2" fill="#000"/></svg>"##,
    )]);
    assert_eq!(
        eval(&app, "local s, r = gfx.svg('pic.svg') return parts(s) .. ' / ' .. dropped(r)").unwrap(),
        "root@5,5#1 / image:old image:photo"
    );
}

// ---------------------------------------------------------------- step 2: geometry

/// L-V4
#[test]
fn the_view_box_is_applied_and_size_is_the_rendered_size() {
    let app = app(&[
        (
            "offset.svg",
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="10 10 50 50" width="100" height="100">
                <rect x="10" y="10" width="5" height="5" fill="#000"/></svg>"##,
        ),
        (
            "bare.svg",
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 30 40"><rect width="1" height="1" fill="#000"/></svg>"##,
        ),
    ]);
    assert_eq!(
        eval(&app, "local s = gfx.svg('offset.svg') return s.size[1] .. 'x' .. s.size[2] .. ' ' .. path(s.parts[1].shapes[1].path)").unwrap(),
        "100x100 move 0 0|line 10 0|line 10 10|line 0 10|close"
    );
    assert_eq!(
        eval(&app, "local s = gfx.svg('bare.svg') return s.size[1] .. 'x' .. s.size[2]").unwrap(),
        "30x40"
    );
}

/// L-V5
#[test]
fn named_groups_become_parts_in_draw_order() {
    let app = app(&[(
        "car.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="100" height="100">
            <defs><g id="wheel"><circle cx="0" cy="0" r="5" fill="#000"/></g></defs>
            <rect width="100" height="10" fill="#00f"/>
            <g id="body">
                <rect x="10" y="20" width="60" height="20" fill="#f00"/>
                <g id="window"><rect x="20" y="22" width="10" height="8" fill="#0ff"/></g>
                <rect x="40" y="22" width="10" height="8" fill="#ff0"/>
                <g id="lamp"><rect x="66" y="24" width="4" height="4" fill="#fff"/></g>
                <rect x="10" y="38" width="60" height="2" fill="#333"/>
            </g>
            <!-- the clone's own id ("wheel") folds into the use's part -->
            <use id="wheel_rear" xlink:href="#wheel" x="20" y="40"/>
            <use id="wheel_front" href="#wheel" x="60" y="40"/>
            <rect y="90" width="100" height="10" fill="#0f0"/>
        </svg>"##,
    )]);
    assert_eq!(
        eval(&app, "local s = gfx.svg('car.svg') gfx.drawing(s) return parts(s)").unwrap(),
        "root@50,50#1 body@40,30#1 window<body@25,26#1 body~2<body@40,30#1 lamp<body@68,26#1 \
         body~3<body@40,30#1 wheel_rear@20,40#1 wheel_front@60,40#1 root~2<root@50,50#1"
    );
}

/// L-V6
#[test]
fn transforms_are_baked_into_the_points() {
    let app = app(&[(
        "t.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <g transform="translate(50 50)">
                <rect transform="rotate(90) scale(2 1)" width="10" height="5" fill="#000"/>
            </g></svg>"##,
    )]);
    // (x, y) → scale → (2x, y) → rotate 90° → (−y, 2x) → translate → (50 − y, 50 + 2x)
    assert_eq!(
        eval(&app, "return path(gfx.svg('t.svg').parts[1].shapes[1].path)").unwrap(),
        "move 50 50|line 50 70|line 45 70|line 45 50|close"
    );
}

/// L-V7
#[test]
fn every_basic_shape_becomes_path_commands() {
    let app = app(&[(
        "shapes.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" fill="#000" stroke="#000">
            <circle cx="10" cy="10" r="5"/>
            <ellipse cx="30" cy="10" rx="5" ry="3"/>
            <line x1="0" y1="50" x2="10" y2="60"/>
            <polyline points="0,70 10,70 10,80" fill="none"/>
            <polygon points="20,70 30,70 30,80"/>
            <path d="M40 40 A 10 10 0 0 1 60 40 Q 70 50 60 60 T 40 60 Z"/>
            <rect x="70" y="70" width="20" height="20" rx="4"/>
        </svg>"##,
    )]);
    let got = eval(
        &app,
        "local out = {} for i, sh in ipairs(gfx.svg('shapes.svg').parts[1].shapes) do out[i] = kinds(sh.path) end return table.concat(out, ' / ')",
    )
    .unwrap();
    let shapes: Vec<&str> = got.split(" / ").collect();
    assert_eq!(shapes.len(), 7, "{got}");
    let curved = |k: &str| k.starts_with("move") && (k.contains("cubic") || k.contains("quad"));
    assert!(curved(shapes[0]) && shapes[0].ends_with("close"), "circle: {}", shapes[0]);
    assert!(curved(shapes[1]), "ellipse: {}", shapes[1]);
    assert_eq!(shapes[2], "move line");
    assert_eq!(shapes[3], "move line line");
    assert_eq!(shapes[4], "move line line close");
    assert!(shapes[5].starts_with("move cubic") && shapes[5].contains("quad") && shapes[5].ends_with("close"), "path: {}", shapes[5]);
    assert!(curved(shapes[6]), "rounded rect: {}", shapes[6]);
}

/// L-V8
#[test]
fn a_part_pivots_on_data_pivot_or_its_box_centre() {
    let app = app(&[
        (
            "p.svg",
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
                <g id="arm" data-pivot="12 34"><rect width="50" height="10" fill="#000"/></g>
                <g id="leg" transform="translate(10 10)"><rect width="20" height="40" fill="#000"/></g>
            </svg>"##,
        ),
        (
            "bad.svg",
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
                <g id="arm" data-pivot="12"><rect width="5" height="1" fill="#000"/></g></svg>"##,
        ),
    ]);
    assert_eq!(eval(&app, "return parts(gfx.svg('p.svg'))").unwrap(), "arm@12,34#1 leg@20,30#1");
    let e = error(&app, "gfx.svg('bad.svg') return ''");
    assert!(e.contains("arm") && e.contains("data-pivot"), "{e}");
}

// ---------------------------------------------------------------- step 3: paint

/// L-V9
#[test]
fn fills_carry_colour_opacity_gradients_and_rule() {
    let app = app(&[(
        "f.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <defs>
                <linearGradient id="lin" x1="0" y1="0" x2="10" y2="0" gradientUnits="userSpaceOnUse" spreadMethod="reflect">
                    <stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff" stop-opacity="0.5"/>
                </linearGradient>
                <radialGradient id="rad" cx="50" cy="50" r="20" gradientUnits="userSpaceOnUse">
                    <stop offset="0" stop-color="#fff"/><stop offset="1" stop-color="#000"/>
                </radialGradient>
            </defs>
            <rect width="10" height="10" fill="#336699" fill-opacity="0.5"/>
            <rect width="10" height="10" fill="url(#lin)" transform="translate(20 0)"/>
            <rect x="30" y="30" width="40" height="40" fill="url(#rad)"/>
            <path d="M0 0 H10 V10 H0 Z M2 2 H8 V8 H2 Z" fill="#000" fill-rule="evenodd"/>
            <g transform="skewX(45)"><rect width="10" height="10" fill="url(#lin)"/></g>
        </svg>"##,
    )]);
    let got = eval(
        &app,
        r#"local s = gfx.svg('f.svg') gfx.drawing(s)
        local sh = s.parts[1].shapes
        local l, r, k = sh[2].fill.linear, sh[3].fill.radial, sh[5].fill.linear
        return table.concat({
            sh[1].fill,
            path({ l.from, l.to }), l.extend, l.stops[1][1] .. l.stops[1][2], l.stops[2][1] .. l.stops[2][2],
            path({ r.center }), r.radius,
            sh[4].rule or 'nonzero', sh[1].rule or 'nonzero',
            path({ k.from, k.to }),
        }, ' ')"#,
    )
    .unwrap();
    // userSpaceOnUse follows the element's transform: translate(20 0) moves from/to by 20.
    // skewX(45) maps (x, y) → (x + y, y), so the vertical stripes lean: the value is now
    // (x − y)/10, whose gradient vector is (5, −5) from the origin.
    assert_eq!(
        got,
        "#33669980 20 0|30 0 reflect 0#ff0000ff 1#0000ff80 50 50 20 evenodd nonzero 0 0|5 -5"
    );
}

/// L-V10: strokes keep width, colour, caps, joins and dashes (drawing strokes gain the named
/// fields); only a gradient stroke can't be carried.
#[test]
fn strokes_keep_their_style_and_report_a_gradient() {
    let app = app(&[(
        "s.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <defs><linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" x2="10"><stop offset="0" stop-color="#000"/><stop offset="1" stop-color="#fff"/></linearGradient></defs>
            <g transform="scale(2)">
                <path id="edge" d="M0 0 L10 0" stroke="#00ff00" stroke-width="3" stroke-opacity="0.5" fill="none"/>
            </g>
            <path id="dash" d="M0 10 L10 10" stroke="#000" stroke-dasharray="2 3" stroke-dashoffset="1" fill="none"/>
            <path id="round" d="M0 20 L10 20 L10 30" stroke="#000" stroke-linecap="round" stroke-linejoin="bevel" fill="none"/>
            <path id="shaded" d="M0 40 L10 40" stroke="url(#g)" stroke-width="2" fill="none"/>
        </svg>"##,
    )]);
    let got = eval(
        &app,
        r#"local s, r = gfx.svg('s.svg') gfx.drawing(s)
        local sh = s.parts[1].shapes
        local e, d, o = sh[1].stroke, sh[2].stroke, sh[3].stroke
        return table.concat({ e[1], e[2], e.cap, e.join, tostring(sh[1].fill),
            table.concat(d.dashes, ','), d.dash_offset, o.cap, o.join, #sh }, ' ') .. ' / ' .. dropped(r)"#,
    )
    .unwrap();
    assert_eq!(got, "6 #00ff0080 butt miter nil 2,3 1 round bevel 3 / stroke-paint:shaded");
}

/// L-V11
#[test]
fn what_a_drawing_cannot_carry_is_listed_never_silently_lost() {
    let app = app(&[(
        "x.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
            <defs>
                <filter id="blur"><feGaussianBlur stdDeviation="2"/></filter>
                <mask id="m"><rect width="5" height="5" fill="#fff"/></mask>
                <clipPath id="c"><rect width="5" height="5"/></clipPath>
                <pattern id="pat" width="4" height="4" patternUnits="userSpaceOnUse"><rect width="2" height="2" fill="#000"/></pattern>
            </defs>
            <g id="glow" filter="url(#blur)"><rect width="10" height="10" fill="#000"/></g>
            <g id="masked" mask="url(#m)"><rect width="10" height="10" fill="#000"/></g>
            <g id="clipped" clip-path="url(#c)"><rect width="10" height="10" fill="#000"/></g>
            <rect id="tiles" width="10" height="10" fill="url(#pat)"/>
            <g id="mix" style="mix-blend-mode: multiply"><rect width="10" height="10" fill="#000"/></g>
            <text id="label" x="0" y="50">hi</text>
            <path id="under" d="M0 0 L5 5" stroke="#000" fill="#f00" paint-order="stroke"/>
            <g id="faded" opacity="0.5">
                <rect width="10" height="10" fill="#000"/><rect x="5" width="10" height="10" fill="#000"/>
            </g>
            <g id="one" opacity="0.5"><rect width="10" height="10" fill="#ff0000"/></g>
        </svg>"##,
    )]);
    let got = eval(
        &app,
        "local s, r = gfx.svg('x.svg') gfx.drawing(s) return dropped(r) .. ' / ' .. shape(s, 'one').fill .. ' ' .. shape(s, 'faded', 2).fill",
    )
    .unwrap();
    assert_eq!(
        got,
        "blend:mix clip-path:clipped filter:glow group-opacity:faded mask:masked paint-order:under \
         pattern:tiles text:label / #ff000080 #00000080"
    );
}

// ---------------------------------------------------------------- step 4: through the real path

/// L-V12: a posed SVG drawing is a frame whose items are named by the SVG's group ids — the names
/// a hover reports as `e.shape`.
#[test]
fn a_posed_svg_drawing_is_named_by_its_group_ids() {
    let app = app(&[(
        "car.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50">
            <g id="body"><rect width="100" height="30" fill="#c00"/></g>
            <g id="wheel_front"><circle cx="80" cy="40" r="10" fill="#000"/></g></svg>"##,
    )]);
    let visual: mlua::AnyUserData = app
        .vm
        .load("local spec = gfx.svg('car.svg') return gfx.drawing(spec):pose({ wheel_front = { rot = 90 } })")
        .eval()
        .unwrap();
    let frame = visual.borrow::<gfx::LuaFrame>().unwrap();
    let ids: Vec<_> = frame.0.items().iter().map(|i| i.id().unwrap().to_string()).collect();
    assert_eq!(ids, ["body", "wheel_front"]);

    // The natural slip — the report lands in drawing's options — says how to fix itself.
    let e = app.vm.load("return gfx.drawing(gfx.svg('car.svg'))").eval::<mlua::Value>().unwrap_err();
    assert!(e.to_string().contains("gfx.drawing((gfx.svg("), "{e}");
}
