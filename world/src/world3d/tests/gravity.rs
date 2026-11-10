use super::*;

#[test]
fn gravity_changes_wake_sleepers_but_same_value_is_observational() {
    let mut world = World3d::default();
    world.reconcile(specs()).unwrap();
    for _ in 0..1200 {
        world.step();
    }
    assert!(world.body("marble").unwrap().sleeping);
    let before = world.inspect();
    world.set_gravity([0.0, -9.81, 0.0]).unwrap();
    assert_eq!(world.inspect(), before);
    world.set_gravity([0.0, 1.62, 0.0]).unwrap();
    assert!(!world.body("marble").unwrap().sleeping);
    assert!(world.needs_ticks());
    for _ in 0..60 {
        world.step();
    }
    assert!(world.body("marble").unwrap().position[1] > 0.6);
    let before = world.inspect();
    for invalid in [f32::NAN, f32::INFINITY, 10001.0] {
        assert!(world.set_gravity([0.0, invalid, 0.0]).is_err());
        assert_eq!(world.inspect(), before);
    }
}

#[test]
fn gravity_recipe_preflight_cannot_change_an_earlier_world() {
    let recipe = || WorldRecipe3d {
        bodies: specs(),
        ..Default::default()
    };
    let mut worlds = Worlds3d::default();
    worlds
        .reconcile(WorldRecipes3d::from([
            ("a".into(), recipe()),
            ("b".into(), recipe()),
        ]))
        .unwrap();
    let before = worlds.get("a").unwrap().inspect();
    let mut staged = WorldRecipes3d::from([("a".into(), recipe()), ("b".into(), recipe())]);
    staged.get_mut("a").unwrap().gravity = [0.0, -1.62, 0.0];
    staged.get_mut("b").unwrap().gravity = [f32::NAN, 0.0, 0.0];
    assert!(worlds.reconcile(staged).is_err());
    assert_eq!(worlds.get("a").unwrap().inspect(), before);
}

#[test]
fn world_gravity_is_isolated_and_can_accelerate_along_any_axis() {
    let mut worlds = Worlds3d::default();
    worlds
        .reconcile(WorldRecipes3d::from([
            (
                "moon".into(),
                WorldRecipe3d {
                    bodies: specs(),
                    gravity: [0.0, -1.62, 0.0],
                    ..Default::default()
                },
            ),
            (
                "sideways".into(),
                WorldRecipe3d {
                    bodies: specs(),
                    gravity: [2.0, 0.0, 0.0],
                    ..Default::default()
                },
            ),
        ]))
        .unwrap();
    for _ in 0..60 {
        worlds.get_mut("moon").unwrap().step();
        worlds.get_mut("sideways").unwrap().step();
    }
    let moon = worlds.get("moon").unwrap().body("marble").unwrap();
    let sideways = worlds.get("sideways").unwrap().body("marble").unwrap();
    assert!((moon.velocity[1] + 0.81).abs() < 0.01);
    assert_eq!(moon.position[0], 0.0);
    assert!((sideways.velocity[0] - 1.0).abs() < 0.01);
    assert_eq!(sideways.position[1], 3.0);
}
