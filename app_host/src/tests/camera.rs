//! The camera from Lua: `camera = { … }` on `ui.world`, `set_camera`, `to_world`/`to_screen`.
//! Plan: docs/design/camera.md (C5, C7–C9).

use super::*;

/// A 400 × 300 box on a map with the hero at (1000, 500), the boss at (3000, 900) and a solid
/// chest at (2000, 1000). The world's `camera` is the global `cam`; the hero's box is 32 × 48,
/// so following it centres (1016, 524).
fn map() -> LuaApp<LuaMsg> {
    app_of(&format!(
        "{HERO} cam = {{ follow = 'hero' }} \
         return function() return ui.world({{ id = 'map', width = 400, height = 300, camera = cam, \
           {{ id = 'hero', pos = {{ 1000, 500 }}, drawing = hero }}, \
           {{ id = 'boss', pos = {{ 3000, 900 }}, drawing = hero }}, \
           {{ id = 'chest', pos = {{ 2000, 1000 }}, drawing = hero, collider = {{ rect = {{ 32, 48 }} }} }} }}) end"
    ))
}

fn camera(app: &LuaApp<LuaMsg>) -> world::CameraInspection {
    app.worlds.borrow()["map"].camera().expect("the map has a camera")
}

fn tick(app: &mut LuaApp<LuaMsg>, at: f64) {
    app.update(LuaMsg::TickWorld("map".into(), 1.0 / 60.0, at));
    let _ = app.view();
}

/// C5.
#[test]
fn set_camera_points_the_camera_until_the_description_changes() {
    let mut app = map();
    assert_eq!(camera(&app).at, (1016.0, 524.0), "on the hero from the first frame");
    assert_eq!(said(&app, "world('map'):set_camera({ at = { 2000, 1000 } })"), "nil");
    let c = camera(&app);
    assert_eq!((c.at, c.follow), ((2000.0, 1000.0), None), "looking here stops following");
    let _ = app.view();
    assert_eq!(camera(&app).at, (2000.0, 1000.0), "the same description does not undo it");

    said(&app, "world('map'):set_camera({ follow = 'boss' })");
    tick(&mut app, 1.0 / 60.0);
    assert_eq!(camera(&app).at, (3016.0, 924.0));

    said(&app, "cam = { follow = 'hero', ease = 4 }");
    let _ = app.view();
    let c = camera(&app);
    assert_eq!((c.follow.as_deref(), c.ease), (Some("hero"), Some(4.0)), "a changed description applies");
    said(&app, "cam = nil");
    let _ = app.view();
    assert!(app.worlds.borrow()["map"].camera().is_none(), "described away");
}

#[test]
fn set_camera_is_refused_while_view_describes() {
    let app = app_of(&format!(
        "{HERO} return function() \
           local w = ui.world({{ id = 'map', width = 400, height = 300, camera = {{ at = {{ 0, 0 }} }} }}) \
           probe = select(2, pcall(function() world('map'):set_camera({{ at = {{ 1, 1 }} }}) end)) \
           return w end"
    ));
    let _ = app.view();
    let probe = said(&app, "return tostring(probe)");
    assert!(probe.contains("only in a handler"), "{probe}");
}

/// C7: the Lua half; the hit itself is `a_following_camera_centres_its_target_from_the_first_frame`
/// in `world`, and the smoke clicks through the shell.
#[test]
fn screen_points_become_world_points_and_back() {
    let mut app = map();
    tick(&mut app, 1.0 / 60.0); // questions answer as of the last step: the chest's body is in
    said(&app, "world('map'):set_camera({ at = { 2016, 1024 } })");
    // The box's centre is the camera's point: the chest's middle.
    assert_eq!(said(&app, "local p = world('map'):to_world({ 200, 150 }) return p[1] .. ',' .. p[2]"), "2016,1024");
    assert_eq!(said(&app, "return world('map'):at(world('map'):to_world({ 200, 150 }))[1]"), "chest");
    let back = "local w = world('map') local p = w:to_screen(w:to_world({ 7, 9 })) return p[1] .. ',' .. p[2]";
    assert_eq!(said(&app, back), "7,9");
    // A question: allowed while view describes.
    let app = app_of(&format!(
        "{HERO} return function() \
           local w = ui.world({{ id = 'map', width = 400, height = 300, camera = {{ at = {{ 1000, 1000 }} }} }}) \
           local ok, p = pcall(function() return world('map'):to_world({{ 0, 0 }}) end) \
           probe = ok and (p[1] .. ',' .. p[2]) or tostring(p) \
           return w end"
    ));
    let _ = app.view();
    assert_eq!(said(&app, "return probe"), "800,850");
}

/// C8.
#[test]
fn a_reload_keeps_where_the_camera_is_and_takes_the_rest_from_the_new_description() {
    let mut app = map();
    said(&app, "world('map'):set_camera({ at = { 2000, 1000 } })");
    let map = app.src.doc.get_map("files");
    let Some(ValueOrContainer::Container(Container::Text(t))) = map.get("main.lua") else {
        panic!("no main.lua");
    };
    let body = t.to_string().replace("cam = { follow = 'hero' }", "cam = { follow = 'boss', ease = 2 }");
    t.delete(0, t.len_unicode()).unwrap();
    t.insert(0, &body).unwrap();
    app.src.doc.commit();
    app.reload().unwrap();
    let _ = app.view();
    let c = camera(&app);
    assert_eq!(c.at, (2000.0, 1000.0), "where it was");
    assert_eq!((c.follow.as_deref(), c.ease), (Some("boss"), Some(2.0)), "the new description");
}

/// C9.
#[test]
fn a_camera_is_checked_strictly() {
    let app = map();
    for (cam, wanted) in [
        ("{ follow = 'hero', at = { 1, 2 } }", "follow and at"),
        ("{ ease = 0 }", "ease must be a finite number above zero"),
        ("{ bounds = { 0, 0, 10 } }", "bounds must be { x, y, w, h }"),
        ("{ bounds = { 0, 0, 10, -1 } }", "bounds must be { x, y, w, h }"),
        ("{ zoom = 2 }", "unknown field zoom"),
        ("{ follow = 5 }", "camera.follow must be an entity id"),
        ("5", "camera must be a table"),
    ] {
        said(&app, &format!("cam = {cam}"));
        let _ = app.view();
        let console = app.console(3).join("\n");
        assert!(console.contains(wanted), "{cam}: wanted {wanted:?} in {console}");
    }
    for (look, wanted) in [
        ("{}", "set_camera needs at or follow"),
        ("{ at = { 1, 2 }, follow = 'hero' }", "set_camera needs at or follow"),
        ("{ ease = 3 }", "unknown field ease"),
        ("{ at = { 0 / 0, 1 } }", "at must be finite numbers"),
    ] {
        let e = said(&app, &format!("world('map'):set_camera({look})"));
        assert!(e.contains(wanted), "{look}: wanted {wanted:?} in {e}");
    }
    said(&app, "cam = nil");
    let _ = app.view();
    let e = said(&app, "world('map'):set_camera({ follow = 'hero' })");
    assert!(e.contains("the world has no camera"), "{e}");
}
