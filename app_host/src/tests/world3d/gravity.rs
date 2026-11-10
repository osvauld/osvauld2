use super::*;

fn source(gravity: &str) -> String {
    SOURCE.replace(
        "id = \"game\", scene",
        &format!("id = \"game\", {gravity} scene"),
    )
}

#[test]
fn world3d_gravity_is_configurable_defaults_to_earth_and_preserves_reload_state() {
    let lunar = source("gravity={0,-1.62,0},");
    let mut moon = app(&lunar);
    assert!(moon.error.is_none(), "{:?}", moon.error);
    fall(&mut moon);
    let before = state(&moon);
    assert!((before.entities[0].resolved.velocity[1] + 0.81).abs() < 0.01);
    let mut earth = app(SOURCE);
    fall(&mut earth);
    assert!((state(&earth).entities[0].resolved.velocity[1] + 4.905).abs() < 0.01);
    for invalid in [
        source("gravity={0,math.huge,0},"),
        source("gravity={0,-1.62},"),
        source("gravity={0,-10001,0},"),
        source("gravity={0,'-1.62',0},"),
        source("gravity={0,-1.62,0,extra=1},"),
        source("gravity={0,-9.81,0},")
            .replace("return ui.col({", "error('trial failed'); return ui.col({"),
    ] {
        moon.write_source_file("main.lua", &invalid).unwrap();
        assert!(moon.reload().is_err(), "accepted {invalid}");
        assert_eq!(state(&moon), before);
    }
    moon.write_source_file("main.lua", &source("gravity={0,0,0},"))
        .unwrap();
    moon.reload().unwrap();
    assert_eq!(state(&moon).tick, before.tick);
    assert_eq!(state(&moon).entities, before.entities);
    moon.view();
    for frame in 31..=60 {
        moon.advance_simulation(f64::from(frame) / 60.0, true);
        moon.view();
    }
    assert!((state(&moon).entities[0].resolved.velocity[1] + 0.81).abs() < 0.01);
}
