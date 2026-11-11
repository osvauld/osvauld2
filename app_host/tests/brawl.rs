//! L-1 (docs/design/brawl-movement.md), as far as step 3 goes: `demo_apps/brawl` opened through
//! `LuaApp` as the shell opens it, its `.lua`/`.osv` files as source and its PNGs as binary files.
//! The console is clean and the stage's pictures are all in the view, counted against the VM's
//! texture budget (64 textures, 128 MiB with mips). Steps 4-5: every clip file (Paka's and the
//! robot's own, built by `author.lua`) passes `gfx.clip`, each robot variant compiles, and the
//! fighter is drawn as the posed robot.

use app_host::{Files, LuaApp, LuaMsg, bind_binary_file};
use loro::{LoroDoc, LoroText};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

const APP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../demo_apps/brawl");

fn walk(dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk(&path, out);
        } else {
            let rel = path.strip_prefix(APP).unwrap().to_str().unwrap().replace('\\', "/");
            out.push((rel, path));
        }
    }
}

fn brawl() -> LuaApp<LuaMsg> {
    brawl_with(|_, text| text)
}

/// The app with each source file passed through `edit(path, text)` before opening.
fn brawl_with(edit: impl Fn(&str, String) -> String) -> LuaApp<LuaMsg> {
    let mut all = Vec::new();
    walk(Path::new(APP), &mut all);
    all.sort();
    let src = LoroDoc::new();
    let map = src.get_map("files");
    for (rel, path) in &all {
        if rel.ends_with(".lua") || rel.ends_with(".osv") || rel.ends_with(".svg") {
            let t = map.insert_container(rel.as_str(), LoroText::new()).unwrap();
            t.insert(0, &edit(rel, std::fs::read_to_string(path).unwrap())).unwrap();
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
    LuaApp::open_with_files(src, Rc::new(|_| Ok(None)), files, std::sync::Arc::new(|| {}), Rc::new(|m| m))
        .expect("brawl opens")
}

fn walk_info(info: &runtime::ElInfo, f: &mut dyn FnMut(&runtime::ElInfo)) {
    f(info);
    info.children.iter().for_each(|c| walk_info(c, f));
}

/// L-1 (step 3, revised in maps step 2): the app and its map's art load clean; the PNGs the map's
/// assets name are the pictures in the view; the count and bytes against the texture budget are printed for the step note.
#[test]
fn brawl_opens_with_its_map_art_inside_the_texture_budget() {
    let started = Instant::now();
    let app = brawl();
    let opened = started.elapsed();
    // The stage decodes a few pictures a view (a debug build spends its 1 s on about a megapixel):
    // look until "n/n", and say how many views that took.
    let mut views = 0;
    let view = loop {
        let view = app.view().info();
        views += 1;
        assert!(app.console(20).is_empty(), "view {views}: {:?}", app.console(20));
        let mut loaded = None;
        walk_info(&view, &mut |i| {
            if i.id.as_deref() == Some("art:loaded") {
                loaded = i.text.clone();
            }
        });
        let loaded = loaded.expect("the art readout");
        let (n, total) = loaded.split_once('/').unwrap();
        if n == total {
            break view;
        }
        assert!(views < 60, "still {loaded} after {views} views");
    };
    let all_views = started.elapsed();

    let mut ids = BTreeSet::new();
    let mut texts = Vec::new();
    let mut pictures = BTreeMap::new(); // by Arc address, each once
    walk_info(&view, &mut |info| {
        if let Some(id) = &info.id {
            ids.insert(id.clone());
        }
        if let (Some(id), Some(text)) = (&info.id, &info.text) {
            texts.push((id.clone(), text.clone()));
        }
        for p in &info.pictures {
            pictures.insert(std::sync::Arc::as_ptr(p) as usize, p.clone());
        }
    });
    assert!(ids.contains("playfield"), "the playfield is described: {ids:?}");
    let camera = texts.iter().find(|(id, _)| id == "camera:view").expect("the camera readout");
    assert!(camera.1.starts_with("x=") && camera.1.contains("zoom="), "{camera:?}");

    // L-1 (revised, docs/design/brawl-maps.md): the PNGs in the view are exactly those the map's
    // assets name (the hidden `map:art` readout, from the compiled scene), not "every PNG in art/".
    let mut all = Vec::new();
    walk(Path::new(APP), &mut all);
    let named: BTreeSet<String> = texts
        .iter()
        .find(|(id, _)| id == "map:art")
        .expect("the map:art readout")
        .1
        .split_whitespace()
        .filter(|f| f.ends_with(".png"))
        .map(String::from)
        .collect();
    let on_disk: BTreeSet<String> = all.iter().map(|(rel, _)| rel.clone()).filter(|rel| rel.starts_with("art/") && rel.ends_with(".png")).collect();
    assert!(named.is_subset(&on_disk), "every named picture is a file: {:?}", named.difference(&on_disk).collect::<Vec<_>>());
    let drawn: BTreeSet<String> = pictures.values().map(|p| p.name().to_string()).collect();
    assert_eq!(drawn, named, "the pictures in the view are the PNGs the map's assets name");
    assert_eq!(texts.iter().find(|(id, _)| id == "map:state").map(|(_, t)| t.as_str()), Some("yard"));

    // The budget, as the VM counts it: each texture once (mips included), each copied region too.
    let mut textures = BTreeMap::new();
    for p in pictures.values() {
        textures.insert(p.texture().resource_id(), p.texture().payload_bytes());
    }
    let mips: usize = textures.values().sum();
    let copies: usize = pictures.values().map(|p| p.copied_bytes()).sum();
    let mib = (mips + copies) as f64 / (1024.0 * 1024.0);
    println!(
        "brawl map art: {} pictures in the view, {} textures, {:.2} MiB with mips ({:.2} MiB of it copied regions); module in {:?}, all pictures after {} views, {:?}",
        pictures.len(),
        textures.len(),
        mib,
        copies as f64 / (1024.0 * 1024.0),
        opened,
        views,
        all_views
    );
    assert!(textures.len() <= 64, "{} textures", textures.len());
    assert!(mips + copies <= 128 * 1024 * 1024, "{mib} MiB");
    assert!(textures.len() >= 12, "the stage uses about fifteen pictures");
    assert!(app.console(20).is_empty(), "{:?}", app.console(20));

    // L-1 (steps 4-5): every clip file in the app passed `gfx.clip` (at module scope: Paka's two and
    // the robot's eight, built by author.lua) and every robot variant compiled (one a view,
    // look.lua), and the fighter is the posed robot.
    let clip_files = all
        .iter()
        .filter(|(rel, _)| (rel.starts_with("characters/paka/clips/") || rel.starts_with("characters/robot/clips/")) && rel.ends_with(".lua"))
        .count();
    let robot_files = all
        .iter()
        .filter(|(rel, _)| {
            rel.starts_with("characters/robot/")
                && !rel.starts_with("characters/robot/clips/")
                && rel.ends_with(".lua")
                && !rel.ends_with("/build.lua")
                && !rel.ends_with("/author.lua")
        })
        .count();
    assert_eq!((clip_files, robot_files), (10, 3), "two of Paka's clips, eight of the robot's and three robots are in the folder");
    let text = |name: &str| texts.iter().find(|(id, _)| id == name).map(|(_, t)| t.clone()).unwrap_or_default();
    assert_eq!(text("look:loaded"), format!("clips={clip_files} robots={robot_files}/{robot_files}"));
    assert!(ids.contains("fighter:robot"), "the robot is drawn: {ids:?}");
    assert!(text("look:state").starts_with("robot=scout clip=idle index=0 "), "{}", text("look:state"));
    assert!(text("look:cost").starts_with("poses=1 "), "one pose, then the cache: {}", text("look:cost"));
    println!("brawl look: {}; {}", text("look:loaded"), text("look:cost"));
}

/// L-3 (docs/design/brawl-maps.md, step 1): a broken asset (`tree_2`'s `size` edited in the source
/// before opening) puts `Library.check`'s error, naming `props.tree_2.size`, in the hidden
/// `library:problem` readout (printed too); the app still opens and draws the stage as plain rectangles. With the library intact the check is silent
/// (L-1's clean console) and the soft SVG cloud went through `gfx.svg` and `gfx.drawing`.
#[test]
fn a_broken_brawl_asset_is_named_and_the_stage_is_plain() {
    let app = brawl_with(|path, text| {
        if path != "maps/assets/props.lua" {
            return text;
        }
        let broken = text.replace("size = { 445, 615 }", "size = { 445 }");
        assert_ne!(broken, text, "tree_2's size is where the test edits it");
        broken
    });
    let view = app.view().info();
    // The plan asks for the console; an app's `print` reaches the shell's stdout, not `console()`
    // (no Rust in this plan), so the error is read from the hidden readout instead.
    let console = app.console(20);
    let mut loaded = None;
    let mut problem = None;
    walk_info(&view, &mut |i| match i.id.as_deref() {
        Some("art:loaded") => loaded = i.text.clone(),
        Some("library:problem") => problem = i.text.clone(),
        _ => {}
    });
    assert_eq!(loaded.as_deref(), Some("0/0"), "plain rectangles: no pictures to load; console {console:?}");
    let problem = problem.expect("the hidden library:problem readout");
    assert!(problem.starts_with("props.tree_2.size: needs {w, h}"), "{problem}");
    let mut pictures = 0;
    walk_info(&view, &mut |i| pictures += i.pictures.len());
    assert_eq!(pictures, 0, "no stage pictures are drawn");
    println!("brawl L-3: {problem}; console: {console:?}");
}

/// L-2 (docs/design/brawl-maps.md, step 5): each map in turn, with `maps/index.lua` reordered in the
/// source before opening so that map comes first. The console is clean, `map:state` names the map,
/// and it stays within a map's budget (32 textures, 48 MiB with mips and copies). Counts are printed.
#[test]
fn each_brawl_map_opens_clean_inside_its_own_budget() {
    for id in ["yard", "isles"] {
        let app = brawl_with(|path, text| {
            if path != "maps/index.lua" {
                return text;
            }
            let other = if id == "yard" { "isles" } else { "yard" };
            let edited = text.replace("return { \"yard\", \"isles\" }", &format!("return {{ \"{id}\", \"{other}\" }}"));
            assert!(edited.contains(&format!("\"{id}\"")), "the index is where the test edits it: {text}");
            edited
        });
        let mut views = 0;
        let view = loop {
            let view = app.view().info();
            views += 1;
            assert!(app.console(20).is_empty(), "{id}: view {views}: {:?}", app.console(20));
            let mut loaded = None;
            walk_info(&view, &mut |i| {
                if i.id.as_deref() == Some("art:loaded") {
                    loaded = i.text.clone();
                }
            });
            let loaded = loaded.expect("the art readout");
            let (n, total) = loaded.split_once('/').unwrap();
            if n == total {
                break view;
            }
            assert!(views < 80, "{id}: still {loaded} after {views} views");
        };
        let mut state = None;
        let mut pictures = BTreeMap::new();
        walk_info(&view, &mut |info| {
            if info.id.as_deref() == Some("map:state") {
                state = info.text.clone();
            }
            for p in &info.pictures {
                pictures.insert(std::sync::Arc::as_ptr(p) as usize, p.clone());
            }
        });
        assert_eq!(state.as_deref(), Some(id));
        let mut textures = BTreeMap::new();
        for p in pictures.values() {
            textures.insert(p.texture().resource_id(), p.texture().payload_bytes());
        }
        let mips: usize = textures.values().sum();
        let copies: usize = pictures.values().map(|p| p.copied_bytes()).sum();
        let mib = (mips + copies) as f64 / (1024.0 * 1024.0);
        println!("brawl L-2 {id}: {} pictures, {} textures, {:.2} MiB with mips and copies, {} views", pictures.len(), textures.len(), mib, views);
        assert!(textures.len() <= 32, "{id}: {} textures", textures.len());
        assert!(mips + copies <= 48 * 1024 * 1024, "{id}: {mib} MiB");
        assert!(app.console(20).is_empty(), "{id}: {:?}", app.console(20));
    }
}
