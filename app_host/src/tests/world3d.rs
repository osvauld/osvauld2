use super::*;

const SOURCE: &str = r##"
local game = gfx.world3d({ id = "game", scene = gfx.scene3d({
    camera = { eye = {8,5,8}, target = {0,1,0} },
    objects = { {id="marble", position={0,3,0}, color="#ffee66"} },
}), bodies = {
    {id="platform", box={6,0.5,4}, position={0,0,0}},
    {id="marble", sphere=0.25, position={0,3,0}, dynamic=true},
} })
local visible = true
return function()
    return ui.col({
        visible and ui.scene3d({id="view", scene=game:scene(), w=400, h=300}) or false,
        ui.button({id="reset", on_click=function() game:reset("marble") end}),
        ui.button({id="hide", on_click=function() visible=false end}),
        ui.button({id="show", on_click=function() visible=true end}),
    })
end
"##;

fn app(source: &str) -> LuaApp<LuaMsg> {
    let src = LoroDoc::new();
    let t = src
        .get_map("files")
        .insert_container("main.lua", LoroText::new())
        .unwrap();
    t.insert(0, source).unwrap();
    src.commit();
    LuaApp::open(src, Rc::new(|_| Ok(None)), noop_wake(), identity()).unwrap()
}

fn state(app: &LuaApp<LuaMsg>) -> world::world3d::WorldInspection3d {
    app.inspect_worlds3d().remove("game").unwrap()
}

fn fall(app: &mut LuaApp<LuaMsg>) {
    app.view();
    app.advance_simulation(0.0, true);
    for frame in 1..=30 {
        app.advance_simulation(f64::from(frame) / 60.0, true);
        app.view();
    }
}

#[test]
fn world3d_lua_native_ticks_resolved_scene_reset_and_settle() {
    let mut app = app(SOURCE);
    assert!(app.error.is_none(), "{:?}", app.error);
    fall(&mut app);
    let before = state(&app);
    assert_eq!(before.tick, 60);
    assert!(before.entities[0].resolved.position[1] < 3.0);
    assert_eq!(before.entities[0].authored.position[1], 3.0);
    app.view();
    assert_eq!(state(&app), before, "views are observational");
    app.update(LuaMsg::Call(Key::new("reset", "on_click")));
    assert_eq!(state(&app).entities[0].resolved.position[1], 3.0);
    for frame in 31..=700 {
        app.advance_simulation(f64::from(frame) / 60.0, true);
        app.view();
    }
    let settled = state(&app);
    assert!(settled.entities[0].resolved.sleeping);
    assert!((settled.entities[0].resolved.position[1] - 0.5).abs() < 0.02);
}

#[test]
fn world3d_reload_failure_preserves_pose_clock_recipe_and_running_vm() {
    let mut app = app(SOURCE);
    fall(&mut app);
    let before = state(&app);
    for bad in [
        SOURCE.replace("sphere=0.25", "sphere=0.5"),
        SOURCE.replace("local visible = true", "error('load failed')"),
        SOURCE.replace("return ui.col({", "error('view failed'); return ui.col({"),
        SOURCE.replace("return ui.col({", "game:reset('marble'); return ui.col({"),
    ] {
        app.write_source_file("main.lua", &bad).unwrap();
        assert!(app.reload().is_err());
        assert_eq!(state(&app), before);
    }
    app.advance_simulation(31.0 / 60.0, true);
    assert_eq!(state(&app).tick, before.tick + 2);
    app.write_source_file(
        "main.lua",
        &SOURCE.replace(
            "position={0,3,0}, dynamic=true",
            "position={0,8,0}, dynamic=true",
        ),
    )
    .unwrap();
    let before = state(&app);
    app.reload().unwrap();
    assert_eq!(
        state(&app).entities[0].resolved,
        before.entities[0].resolved
    );
    app.view();
    app.update(LuaMsg::Call(Key::new("reset", "on_click")));
    assert_eq!(state(&app).entities[0].resolved.position[1], 8.0);
}

#[test]
fn world3d_hidden_or_inactive_worlds_resume_without_catchup() {
    let mut app = app(SOURCE);
    fall(&mut app);
    app.update(LuaMsg::Call(Key::new("hide", "on_click")));
    app.view();
    let before = state(&app);
    assert!(!app.advance_simulation(10.0, true));
    assert_eq!(state(&app), before);
    app.update(LuaMsg::Call(Key::new("show", "on_click")));
    app.view();
    app.advance_simulation(20.0, true);
    assert_eq!(state(&app), before);
    app.advance_simulation(20.0 + 1.0 / 60.0, true);
    assert_eq!(state(&app).tick, before.tick + 2);
    let before = state(&app);
    app.advance_simulation(30.0, false);
    app.advance_simulation(40.0, true);
    assert_eq!(state(&app), before);
}

#[test]
fn world3d_rejects_unknown_fields_bad_shapes_and_frame_or_module_resets() {
    for bad in [
        SOURCE.replace("dynamic=true", "dynmaic=true"),
        SOURCE.replace("sphere=0.25", "sphere=-1"),
        SOURCE.replace("sphere=0.25", "sphere=0.25, box={1,1,1}"),
        SOURCE.replace("local visible = true", "game:reset('marble')"),
        SOURCE.replace("id = \"game\"", "id = \"\""),
    ] {
        let app = app(&bad);
        assert!(app.error.is_some(), "accepted invalid source: {bad}");
        assert!(app.inspect_worlds3d().is_empty());
    }
    let mut app = app(&SOURCE.replace(
        "on_click=function() game:reset",
        "on_frame=function() game:reset",
    ));
    app.view();
    let before = state(&app);
    app.update(LuaMsg::CallFrame(Key::new("reset", "on_frame"), 0.1, 1.0));
    assert_eq!(state(&app), before);
    assert!(app.console(1)[0].contains("only allowed in input handlers"));
}
