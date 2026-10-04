use super::*;

#[test]
fn set_commands_are_strict_atomic_and_input_scoped() {
    let mut app = app(&SOURCE.replace(
        "game:reset(\"marble\")",
        "game:set('marble', {pos={1,8,0},velocity={2,0,0},spin={0,90,0}})",
    ));
    fall(&mut app);
    app.update(LuaMsg::Call(Key::new("reset", "on_click")));
    let before = state(&app);
    let body = &before.entities[0].resolved;
    assert_eq!(body.position, [1.0, 8.0, 0.0]);
    assert_eq!(body.velocity, [2.0, 0.0, 0.0]);
    assert!((body.angular_velocity[1] - std::f32::consts::FRAC_PI_2).abs() < 0.00001);
    for bad in [
        "pos={8,8,8},spin={0,math.huge,0}",
        "pos={8,8,8},rotation={0,0,0,0}",
        "pos={8,8,8},typo=true",
        "pos={8,8,8},velocity={[1]=0,[3]=0}",
        "pos={1,2}",
        "pos={1,'2',3}",
        "rotation={0,0,0,'1'}",
    ] {
        let source = SOURCE.replace(
            "game:reset(\"marble\")",
            &format!("game:set('marble', {{{bad}}})"),
        );
        let mut rejected = super::app(&source);
        rejected.view();
        let before = state(&rejected);
        rejected.update(LuaMsg::Call(Key::new("reset", "on_click")));
        assert_eq!(state(&rejected), before);
        assert!(!rejected.console(1).is_empty());
    }
    let mut blocked = super::app(&SOURCE.replace(
        "on_click=function() game:reset(\"marble\")",
        "on_frame=function() game:set('marble',{pos={8,8,8}})",
    ));
    blocked.view();
    let before = state(&blocked);
    blocked.update(LuaMsg::CallFrame(Key::new("reset", "on_frame"), 0.1, 1.0));
    assert_eq!(state(&blocked), before);
    assert!(blocked.console(1)[0].contains("only allowed in input handlers"));
    let blocked =
        super::app(&SOURCE.replace("local visible = true", "game:set('marble',{pos={8,8,8}})"));
    assert!(blocked.error.is_some());
}

#[test]
fn paused_scene_renders_resolved_poses_and_resumes_without_catchup() {
    let mut app = app(&SOURCE
        .replace(
            "local visible = true",
            "local visible = true\nlocal paused = true",
        )
        .replace("game:scene()", "game:scene(nil,{running=not paused})")
        .replace("visible=true", "paused=false"));
    app.view();
    assert!(!app.advance_simulation(0.0, true));
    assert!(!app.advance_simulation(20.0, true));
    let before = state(&app);
    assert_eq!(before.tick, 0);
    app.update(LuaMsg::Call(Key::new("show", "on_click")));
    app.view();
    app.advance_simulation(20.0, true);
    assert_eq!(state(&app), before);
    app.advance_simulation(20.0 + 1.0 / 60.0, true);
    assert_eq!(state(&app).tick, 2);
    for options in [
        "running='yes'",
        "paused=true",
        "running=false, extra=true",
        "false",
    ] {
        let invalid =
            super::app(&SOURCE.replace("game:scene()", &format!("game:scene(nil,{{{options}}})")));
        // Construction does not call view on initial load; its error is reported on description.
        invalid.view();
        assert!(!invalid.console(1).is_empty(), "accepted {options}");
    }
}
