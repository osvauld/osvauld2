//! C-1's engine half (docs/design/2d-characters.md): `demo_apps/character_lab` with the converted
//! Paka, opened through `LuaApp` as the shell opens it. The rig and every clip file load
//! unchanged, sample and pose, and parts land where the converter's own evaluator puts them.

use super::*;
use runtime::vello::kurbo::Point;
use std::path::Path;
use std::time::Instant;

const LAB: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../demo_apps/character_lab");

/// The app's `.lua`/`.osv` files as source and its `.png`s as binary files, as `upload_folder`
/// sends them.
fn lab() -> LuaApp<LuaMsg> {
    fn walk(dir: &Path, out: &mut Vec<(String, std::path::PathBuf)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                let rel = path.strip_prefix(LAB).unwrap().to_str().unwrap().replace('\\', "/");
                out.push((rel, path));
            }
        }
    }
    let mut all = Vec::new();
    walk(Path::new(LAB), &mut all);
    all.sort();
    let src = LoroDoc::new();
    let map = src.get_map("files");
    for (rel, path) in &all {
        if rel.ends_with(".lua") || rel.ends_with(".osv") {
            let t = map.insert_container(rel.as_str(), LoroText::new()).unwrap();
            t.insert(0, &std::fs::read_to_string(path).unwrap()).unwrap();
        }
    }
    src.commit();
    let blobs: Rc<RefCell<HashMap<String, Vec<u8>>>> = Rc::default();
    for (rel, path) in all.iter().filter(|(rel, _)| rel.ends_with(".png")) {
        let store = |bytes: &[u8]| {
            let id = format!("blob{}", blobs.borrow().len());
            blobs.borrow_mut().insert(id.clone(), bytes.to_vec());
            Ok(id)
        };
        bind_binary_file(&src, rel, &std::fs::read(path).unwrap(), store).unwrap();
    }
    let files: Files = Rc::new(move |blob| Ok(blobs.borrow().get(blob).cloned()));
    LuaApp::open_with_files(src, Rc::new(|_| Ok(None)), files, noop_wake(), identity()).unwrap()
}

/// (clip, frame at 60 a second, part, a point on the part at rest, where it is then). Generated
/// from `scripts/test_spine_to_lua.py`'s evaluator (`worlds(rig, sample(clip, t))`) over the
/// converted files; its bone origins agree with Spine's own solve to 1e-4. The rest points are
/// each part's pivot and 10 units along x from it, and the far corner of the sword's picture.
#[rustfmt::skip]
const POINTS: &[(&str, u32, &str, (f64, f64), (f64, f64))] = &[
    ("run", 6, "head_main", (221.162, 138.434), (200.401821, 110.093160)),
    ("run", 6, "head_main", (231.162, 138.434), (200.078422, 100.098391)),
    ("run", 6, "hoof-back", (255.430, 146.450), (172.668559, 213.269865)),
    ("run", 6, "hoof-back", (265.430, 146.450), (166.262719, 220.948746)),
    ("run", 6, "hoof_front", (257.548, 184.542), (207.843728, 229.375431)),
    ("run", 6, "hoof_front", (267.548, 184.542), (207.718884, 239.374652)),
    ("run", 6, "slot:sword", (285.868934, 66.814997), (204.949085, 205.614308)),
    ("run", 18, "head_main", (221.162, 138.434), (199.739123, 115.084034)),
    ("run", 18, "head_main", (231.162, 138.434), (199.366279, 105.090987)),
    ("run", 18, "hoof-back", (255.430, 146.450), (215.735444, 225.357781)),
    ("run", 18, "hoof-back", (265.430, 146.450), (215.243148, 235.345656)),
    ("run", 18, "hoof_front", (257.548, 184.542), (167.580193, 221.331333)),
    ("run", 18, "hoof_front", (267.548, 184.542), (161.425863, 229.213226)),
    ("run", 18, "slot:sword", (285.868934, 66.814997), (208.955987, 208.228297)),
    ("run", 30, "head_main", (221.162, 138.434), (200.758681, 110.813914)),
    ("run", 30, "head_main", (231.162, 138.434), (200.597031, 100.815221)),
    ("run", 30, "hoof-back", (255.430, 146.450), (202.774214, 229.211116)),
    ("run", 30, "hoof-back", (265.430, 146.450), (202.075091, 239.186647)),
    ("run", 30, "hoof_front", (257.548, 184.542), (187.229900, 221.123628)),
    ("run", 30, "hoof_front", (267.548, 184.542), (183.306828, 230.321971)),
    ("run", 30, "slot:sword", (285.868934, 66.814997), (209.853199, 202.784731)),
    ("attack", 5, "head_main", (221.162, 138.434), (199.107963, 113.230225)),
    ("attack", 5, "head_main", (231.162, 138.434), (198.268256, 103.026963)),
    ("attack", 5, "hoof-back", (255.430, 146.450), (216.658907, 229.290903)),
    ("attack", 5, "hoof-back", (265.430, 146.450), (216.675914, 239.290889)),
    ("attack", 5, "hoof_front", (257.548, 184.542), (173.962435, 229.244685)),
    ("attack", 5, "hoof_front", (267.548, 184.542), (172.408363, 239.123190)),
    ("attack", 5, "slot:sword_forward", (450.873862, 126.839036), (389.023167, 4.600737)),
    ("attack", 15, "head_main", (221.162, 138.434), (198.433318, 111.039102)),
    ("attack", 15, "head_main", (231.162, 138.434), (196.619536, 100.921226)),
    ("attack", 15, "hoof-back", (255.430, 146.450), (216.192272, 229.291525)),
    ("attack", 15, "hoof-back", (265.430, 146.450), (216.164305, 239.291486)),
    ("attack", 15, "hoof_front", (257.548, 184.542), (174.005537, 229.292298)),
    ("attack", 15, "hoof_front", (267.548, 184.542), (172.547357, 239.185413)),
    ("attack", 15, "slot:sword_forward", (450.873862, 126.839036), (208.262538, -72.903104)),
    ("attack", 25, "head_main", (221.162, 138.434), (201.763491, 121.881571)),
    ("attack", 25, "head_main", (231.162, 138.434), (202.085402, 111.882463)),
    ("attack", 25, "hoof-back", (255.430, 146.450), (221.327239, 229.255122)),
    ("attack", 25, "hoof-back", (265.430, 146.450), (221.794384, 239.244204)),
    ("attack", 25, "hoof_front", (257.548, 184.542), (173.807854, 229.031674)),
    ("attack", 25, "hoof_front", (267.548, 184.542), (171.881543, 238.844386)),
    ("attack", 25, "slot:sword_forward", (450.873862, 126.839036), (296.332618, -6.330643)),
];

/// C-1: the full rig and all 26 clip files (25 animations and `setup`) through the engine.
#[test]
fn the_character_lab_plays_the_converted_paka() {
    let started = Instant::now();
    let app = lab();
    let opened = started.elapsed();
    assert!(app.error.is_none(), "{:?}", app.error);
    fn kinds(info: &runtime::ElInfo, out: &mut Vec<(String, &'static str, usize)>) {
        if let Some(id) = &info.id {
            out.push((id.clone(), info.kind, info.pictures.len()));
        }
        info.children.iter().for_each(|c| kinds(c, out));
    }
    let shown = |app: &LuaApp<LuaMsg>| {
        let mut out = Vec::new();
        kinds(&app.view().info(), &mut out);
        assert!(app.console(20).is_empty(), "{:?}", app.console(20));
        out
    };
    let paused = shown(&app);
    let paka = paused.iter().find(|(id, ..)| id == "char:paka").expect("Paka's frame");
    // 30 slots have a picture; `run` hides the sword in hand and both blinks.
    assert_eq!(paka.2, 27, "one picture per shown slot");
    // Playing, the stage is a world with the clip on Paka.
    app.vm.load(r#"ui.state("lab", {}).playing = true"#).exec().unwrap();
    let playing = shown(&app);
    assert!(playing.iter().any(|(id, ..)| id == "world") && !playing.iter().any(|(id, ..)| id == "char:paka"));
    app.vm.load(r#"ui.state("lab", {}).playing = false"#).exec().unwrap();

    // Everything else, as a person clicking through the picker would load it.
    let before = app.vm.used_memory();
    let started = Instant::now();
    let (clips, keys): (u32, u32) = app
        .vm
        .load(
            r#"
            local cast = require("cast")
            local c = cast.characters[1]
            local function count(spec)
                local n = 0
                for _, props in pairs(spec.tracks) do
                    for _, ks in pairs(props) do n = n + #ks end
                end
                return n
            end
            local clips, keys = 0, 0
            for _, name in ipairs(cast.names) do
                cast.clip(c, name)
                clips, keys = clips + 1, keys + count(require(c.clips .. name))
            end
            gfx.clip(require("characters/paka/setup"))
            return clips + 1, keys
            "#,
        )
        .eval()
        .unwrap();
    let loaded = started.elapsed();
    let grown = app.vm.used_memory() - before;
    assert_eq!((clips, keys), (26, 55_008));

    // Nine times across each clip, posed: every picture and every track.
    let started = Instant::now();
    let poses: u32 = app
        .vm
        .load(
            r#"
            local cast = require("cast")
            local c = cast.characters[1]
            local n = 0
            for _, name in ipairs(cast.names) do
                local clip = cast.clip(c, name)
                for i = 0, 8 do
                    c.drawing:pose(clip:at(clip.length * i / 8))
                    n = n + 1
                end
            end
            return n
            "#,
        )
        .eval()
        .unwrap();
    let posed = started.elapsed();
    assert_eq!(poses, 25 * 9);

    let get = |lua: &str| app.vm.load(lua).eval::<mlua::AnyUserData>().unwrap();
    let drawing = get(r#"return require("cast").characters[1].drawing"#)
        .borrow::<gfx::LuaDrawing>()
        .unwrap()
        .0
        .clone();
    let clip = |name: &str| {
        get(&format!(r#"local cast = require("cast") return cast.clip(cast.characters[1], "{name}")"#))
            .borrow::<gfx::LuaClip>()
            .unwrap()
            .0
            .clone()
    };
    let (run, attack) = (clip("run"), clip("attack"));
    let mut worst = 0.0f64;
    for &(name, frame, part, rest, want) in POINTS {
        let poses = (if name == "run" { &run } else { &attack }).sample(frame as f64 / 60.0);
        let got = drawing.part_at(part, &poses).unwrap() * Point::new(rest.0, rest.1);
        let off = (got - Point::new(want.0, want.1)).hypot();
        assert!(off < 0.5, "{name} frame {frame} {part} at {got:?}, want {want:?}");
        worst = worst.max(off);
    }
    eprintln!(
        "character_lab: open (rig, atlas, run) {opened:?}; the other clips {loaded:?}, +{:.1} MiB of Lua heap; \
         {poses} poses {posed:?}; parts within {worst:.2e} of the evaluator",
        grown as f64 / (1 << 20) as f64
    );
}

/// S-C7's engine half: the robots (`characters/robot/`) are Paka's parts with vector shapes, play
/// Paka's own clip handles, and every part sits where Paka's does at the same clip and time.
#[test]
fn the_robots_play_pakas_clips_as_vector_shapes() {
    let app = lab();
    assert!(app.error.is_none(), "{:?}", app.error);
    let (shapes, poses): (u32, u32) = app
        .vm
        .load(
            r#"
            local cast = require("cast")
            local paka = require("characters/paka/rig")
            local shapes, poses = 0, 0
            for i = 2, #cast.characters do
                local c = cast.characters[i]
                local rig = require(c.rig)
                assert(#rig.parts == #paka.parts, c.id)
                for k, p in ipairs(rig.parts) do
                    local q = paka.parts[k]
                    assert(p.id == q.id and p.parent == q.parent
                        and p.pivot[1] == q.pivot[1] and p.pivot[2] == q.pivot[2], c.id .. " " .. p.id)
                    for _, s in ipairs(p.shapes or {}) do
                        assert(s.image == nil and s.path, c.id .. " " .. p.id .. ": not a vector shape")
                        shapes = shapes + 1
                    end
                end
                for _, name in ipairs(cast.names) do
                    local clip = cast.clip(c, name)
                    assert(rawequal(clip, cast.clip(cast.characters[1], name)), "Paka's own clip")
                    for j = 0, 4 do
                        c.drawing:pose(clip:at(clip.length * j / 4))
                        poses = poses + 1
                    end
                end
            end
            return shapes, poses
            "#,
        )
        .eval()
        .unwrap();
    assert_eq!(poses, 3 * 25 * 5);
    assert!(shapes > 300, "{shapes} shapes");
    assert!(app.console(20).is_empty(), "{:?}", app.console(20));

    let drawing = |i: usize| {
        app.vm
            .load(format!(r#"return require("cast").characters[{i}].drawing"#))
            .eval::<mlua::AnyUserData>()
            .unwrap()
            .borrow::<gfx::LuaDrawing>()
            .unwrap()
            .0
            .clone()
    };
    let ids: Vec<String> = app
        .vm
        .load(r#"local ids = {} for _, p in ipairs(require("characters/paka/rig").parts) do table.insert(ids, p.id) end return ids"#)
        .eval()
        .unwrap();
    let paka = drawing(1);
    for name in ["run", "attack", "jump"] {
        let clip = app
            .vm
            .load(format!(r#"local cast = require("cast") return cast.clip(cast.characters[1], "{name}")"#))
            .eval::<mlua::AnyUserData>()
            .unwrap()
            .borrow::<gfx::LuaClip>()
            .unwrap()
            .0
            .clone();
        for robot in 2..=4 {
            let robot = drawing(robot);
            for frame in [0, 9, 21, 33] {
                let poses = clip.sample(frame as f64 / 60.0);
                for id in &ids {
                    let (a, b) = (paka.part_at(id, &poses).unwrap(), robot.part_at(id, &poses).unwrap());
                    assert_eq!(a.as_coeffs(), b.as_coeffs(), "{name} frame {frame} {id}");
                }
            }
        }
    }
}
