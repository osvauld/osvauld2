use super::*;

const TERRAIN: &str = r##"
local mesh = gfx.mesh({vertices={
    {position={-12,2.4,-6},normal={0,1,0}},
    {position={12,-2.4,-6},normal={0,1,0}},
    {position={-12,2.4,6},normal={0,1,0}},
    {position={12,-2.4,6},normal={0,1,0}},
},indices={1,3,2,2,3,4}})
local game = gfx.world3d({id="game",gravity={0,-1.62,0},scene=gfx.scene3d({
    camera={eye={10,8,12},target={0,0,0}},objects={
        {id="ground",mesh=mesh,color="#888888"}, {id="ball",position={-4,3,0},color="#ffcc66"},
    },
}),bodies={
    {id="ground",position={0,0,0},mesh=mesh},
    {id="ball",position={-4,3,0},sphere=0.35,dynamic=true},
}})
return function() return ui.scene3d({id="view",scene=game:scene(),w=400,h=300}) end
"##;

#[test]
fn world3d_terrain_supports_a_ball_and_equivalent_reload_retains_its_pose() {
    let mut app = app(TERRAIN);
    assert!(app.error.is_none(), "{:?}", app.error);
    app.view();
    app.advance_simulation(0.0, true);
    for frame in 1..=360 {
        app.advance_simulation(f64::from(frame) / 60.0, true);
        app.view();
    }
    let before = state(&app);
    let ball = &before.entities[0].resolved;
    assert!(ball.position[0] > -3.0, "did not roll downhill: {ball:?}");
    assert!(
        (ball.position[1] + 0.2 * ball.position[0] - 0.35 * 1.04_f32.sqrt()).abs() < 0.05,
        "{ball:?}"
    );
    app.write_source_file(
        "main.lua",
        &TERRAIN.replace("normal={0,1,0}", "normal={0.2,1,0}"),
    )
    .unwrap();
    app.reload().unwrap();
    assert_eq!(
        state(&app),
        before,
        "cosmetic normals/resource ids cannot reset physics"
    );
    for bad in [
        TERRAIN.replace(
            "position={0,0,0},mesh=mesh",
            "position={0,0,0},mesh=mesh,dynamic=true",
        ),
        TERRAIN.replace(
            "position={0,0,0},mesh=mesh",
            "position={0,0,0},mesh=mesh,sensor=true",
        ),
        TERRAIN.replace("position={-12,2.4,-6}", "position={-12,2.5,-6}"),
        TERRAIN.replace("mesh=mesh},", "mesh='bad'},"),
        TERRAIN.replace(
            "position={0,0,0},mesh=mesh",
            "position={0,0,0},mesh=mesh,sphere=1",
        ),
        TERRAIN.replace("position={-12,2.4,-6}", "position={-10001,2.4,-6}"),
        TERRAIN.replace("indices={1,3,2,2,3,4}", "indices={1,1,2,2,3,4}"),
    ] {
        app.write_source_file("main.lua", &bad).unwrap();
        assert!(app.reload().is_err(), "accepted {bad}");
        assert_eq!(
            state(&app),
            before,
            "failed terrain edit mutated the solver"
        );
    }
}
