use super::*;

const COMPONENTS: &str = r##"
local wheels={}
for i,at in ipairs({{-0.9,-0.15,0.9},{0.9,-0.15,0.9},{-0.9,-0.15,-0.9},{0.9,-0.15,-0.9}}) do
    wheels[i]={id='wheel'..i,body='platform',at=at,radius=0.35,width=0.26,rest=0.28,travel=0.18,
        stiffness=2500,damping=500,grip=1.1}
end
local game=gfx.world3d({id='game',gravity={0,-1.62,0},scene=gfx.scene3d({
    camera={eye={8,5,8},target={0,1,0}},objects={{id='platform',color='#aaccee'}},
}),bodies={{id='floor',box={100,1,100},position={0,-0.5,0}},
    {id='platform',position={0,1.2,0},box={1.6,0.35,2.6},dynamic=true,mass=400,center={0,-0.1,0}}},wheels=wheels})
local function actuate(torque)
    local commands={}
    for i=1,4 do commands[i]={id='wheel'..i,torque=torque} end
    game:set_wheels(commands)
end
return function()
    assert(game:get('missing')==nil)
    local pose=game:get('platform')
    return ui.col({
        ui.scene3d({id='view',w=400,h=300,scene=game:scene()}),
        ui.text({id='height',tostring(pose.position[2])}),
        ui.button({id='motor',on_click=function() actuate(42) end}),
        ui.button({id='reset',on_click=function() game:reset('platform') end}),
    })
end
"##;
#[test]
fn wheel_components_drive_from_lua_and_failed_reload_preserves_live_world() {
    let mut app = app(COMPONENTS);
    assert!(app.error.is_none(), "{:?}", app.error);
    app.view();
    app.advance_simulation(0.0, true);
    for frame in 1..=240 {
        app.advance_simulation(frame as f64 / 60.0, true);
        app.view();
    }
    let settled = state(&app);
    assert!(settled.wheels.iter().all(|s| s.contact));
    app.update(LuaMsg::Call(Key::new("motor", "on_click")));
    for frame in 241..=360 {
        app.advance_simulation(frame as f64 / 60.0, true);
        app.view();
    }
    let moving = state(&app);
    assert!(
        moving
            .entities
            .iter()
            .find(|s| s.authored.id == "platform")
            .unwrap()
            .resolved
            .position[2]
            > 1.0
    );
    for bad in [
        COMPONENTS.replace("mass=400", "mass=-1"),
        COMPONENTS.replace("radius=0.35", "radius=0"),
        COMPONENTS.replace("mass=400", "mass='400'"),
        COMPONENTS.replace(
            "position={0,1.2,0}",
            "rotation={'0',0,0,1},position={0,1.2,0}",
        ),
        COMPONENTS.replace("mass=400", "typo=1,mass=400"),
        COMPONENTS.replace("body='platform'", "body='floor'"),
        COMPONENTS.replace(
            "id='platform',color",
            "id='decal',attach='nowhere',color",
        ),
        COMPONENTS.replace("return ui.col({", "actuate(1); return ui.col({"),
        COMPONENTS.replace("mass=400", "mass=401"),
        COMPONENTS.replace("wheels=wheels", "vehicles={}"),
        COMPONENTS
            .replace("actuate(42)", "game:drive('platform',{throttle=1})")
            .replace(
                "return ui.col({",
                "game:drive('platform',{}); return ui.col({",
            ),
    ] {
        app.write_source_file("main.lua", &bad).unwrap();
        assert!(app.reload().is_err(), "accepted {bad}");
        assert_eq!(state(&app), moving);
    }
    app.write_source_file("main.lua", COMPONENTS).unwrap();
    app.reload().unwrap();
    assert_eq!(state(&app), moving);
    for commands in [
        "{{id='wheel1',torque='1'}}",
        "{{id='wheel1',throttle=1}}",
        "{{id='wheel1',brake=-1}}",
        "{{id='wheel1',steering=0/0}}",
        "{{id='wheel1',motor={velocity=1,max_torque='1'}}}",
        "{{id='wheel1',torque=20},{id='missing',torque=30}}",
        "{{id='wheel1',torque=20},{id='wheel1',torque=30}}",
        "{{id='wheel1',torque=1,motor={velocity=1,max_torque=1}}}",
    ] {
        let source = COMPONENTS.replace("actuate(42)", &format!("game:set_wheels({commands})"));
        app.write_source_file("main.lua", &source).unwrap();
        app.reload().unwrap();
        app.view();
        let before = state(&app);
        app.update(LuaMsg::Call(Key::new("motor", "on_click")));
        assert_eq!(state(&app), before, "{commands}");
        assert!(!app.console(1).is_empty());
    }
}
#[test]
fn wheel_hover_can_only_actuate_and_permission_guard_restores_after_error() {
    let source = COMPONENTS.replace(
        "id='motor',on_click",
        "id='motor',on_hover=function() actuate(42); game:reset('platform') end,on_click",
    );
    let mut app = app(&source);
    assert!(app.error.is_none(), "{:?}", app.error);
    app.view();
    app.update(LuaMsg::CallHover(
        Key::new("motor", "on_hover"),
        "enter",
        0.0,
        0.0,
        Shape::default(),
        true,
        None,
    ));
    assert_eq!(state(&app).wheels[0].input.torque, 42.0);
    assert!(app.console(1)[0].contains("reset is only allowed"));
    assert!(!app.worlds3d.control_input.get());
    assert!(!app.worlds3d.commanding.get());
    let bad = source.replace("return ui.col({", "actuate(0); return ui.col({");
    let before = state(&app);
    app.write_source_file("main.lua", &bad).unwrap();
    assert!(app.reload().is_err());
    assert_eq!(state(&app), before);
}

#[test]
fn actual_rover_lua_defines_partial_drive_speed_reverse_and_brake_priority() {
    let source=COMPONENTS.replace("platform","rover").replace("'wheel'..i","'rover-wheel-'..i")
        .replace("local function actuate(torque)","local V=require('vehicle'); local function actuate(torque)")
        .replace("actuate(42)","V.apply(game,{throttle=0.5})")
        .replace("ui.button({id='reset'", "ui.button({id='brake',on_click=function() V.apply(game,{throttle=1,brake=1,steering=1}) end}), ui.button({id='reverse',on_click=function() V.apply(game,{throttle=-0.5}) end}), ui.button({id='reset'");
    let src = LoroDoc::new();
    for (path, text) in [
        ("main.lua", source.as_str()),
        (
            "vehicle.lua",
            include_str!("../../../../demo_apps/moon_rover/vehicle.lua")
                .replace("speed_limit = 6", "speed_limit = 1")
                .as_str(),
        ),
    ] {
        src.get_map("files")
            .insert_container(path, LoroText::new())
            .unwrap()
            .insert(0, text)
            .unwrap();
    }
    src.commit();
    let mut app = LuaApp::open(src, Rc::new(|_| Ok(None)), noop_wake(), identity()).unwrap();
    assert!(app.error.is_none(), "{:?}", app.error);
    app.view();
    app.advance_simulation(0.0, true);
    for frame in 1..=240 {
        app.advance_simulation(frame as f64 / 60.0, true);
        app.view();
    }
    app.update(LuaMsg::Call(Key::new("motor", "on_click")));
    for wheel in &state(&app).wheels {
        let motor = wheel.input.motor.unwrap();
        assert!((motor.velocity - 1.0 / 0.35).abs() < 0.001);
        assert_eq!(motor.max_torque, 21.0);
    }
    for frame in 241..=1440 {
        app.advance_simulation(frame as f64 / 60.0, true);
        app.view();
    }
    let speed = state(&app)
        .entities
        .iter()
        .find(|e| e.authored.id == "rover")
        .unwrap()
        .resolved
        .velocity[2];
    assert!(
        (speed - 1.0).abs() < 0.05,
        "Lua-selected motor speed was not respected: {speed}"
    );
    app.update(LuaMsg::Call(Key::new("brake", "on_click")));
    for (i, wheel) in state(&app).wheels.iter().enumerate() {
        assert!(wheel.input.motor.is_none());
        assert_eq!(wheel.input.brake, 105.0);
        assert!(
            (wheel.input.steering - if i < 2 { -28.0_f32.to_radians() } else { 0.0 }).abs() < 1e-6
        );
    }
    app.update(LuaMsg::Call(Key::new("reverse", "on_click")));
    for frame in 1441..=1800 {
        app.advance_simulation(frame as f64 / 60.0, true);
        app.view();
    }
    let speed = state(&app)
        .entities
        .iter()
        .find(|e| e.authored.id == "rover")
        .unwrap()
        .resolved
        .velocity[2];
    assert!((speed + 1.0).abs() < 0.05, "{speed}");
    assert!(app.console(1).is_empty());
}

/// Lua reads a wheel's live pose and hands the world this frame's pose for a visual: the
/// override is authored (in the host's frame) and composed like any attached visual.
#[test]
fn lua_reads_wheel_poses_and_overrides_visual_poses_per_frame() {
    let source = COMPONENTS
        .replace(
            "objects={{id='platform',color='#aaccee'}}",
            "objects={{id='platform',color='#aaccee'},{id='arm',attach='platform',color='#ffffff'}}",
        )
        .replace(
            "ui.scene3d({id='view',w=400,h=300,scene=game:scene()}),",
            "ui.scene3d({id='view',w=400,h=300,scene=game:scene(nil,{poses={arm={position={0,2,0}}}})}),
        ui.text({id='hub',tostring(game:get('wheel1').position[2])}),
        assert(not pcall(game.scene,game,nil,{poses={ghost={}}})) and false,",
        );
    let mut app = app(&source);
    assert!(app.error.is_none(), "{:?}", app.error);
    app.view();
    app.advance_simulation(0.0, true);
    for frame in 1..=60 {
        app.advance_simulation(frame as f64 / 60.0, true);
        app.view();
    }
    let platform = state(&app)
        .entities
        .into_iter()
        .find(|s| s.authored.id == "platform")
        .unwrap()
        .resolved;
    let scene = find_scene(&app.view().info()).expect("scene3d leaf");
    let arm = scene.objects.iter().find(|o| &*o.id == "arm").unwrap();
    let turn = glam::Quat::from_array(platform.rotation);
    let expected = glam::Vec3::from(platform.position) + turn * glam::Vec3::new(0.0, 2.0, 0.0);
    assert!(glam::Vec3::from(arm.position).distance(expected) < 1e-4, "{:?}", arm.position);
    assert!(app.error.is_none(), "{:?}", app.error);
}

fn find_scene(info: &runtime::ElInfo) -> Option<runtime::scene3d::SceneInspection> {
    info.scene3d
        .clone()
        .or_else(|| info.children.iter().find_map(find_scene))
}
