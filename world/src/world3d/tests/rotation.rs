use super::*;

#[test]
fn tilted_box_collider_moves_the_marble_downhill_deterministically() {
    let mut recipe = specs();
    recipe[0].rotation = glam::Quat::from_rotation_z(-20_f32.to_radians()).to_array();
    let mut a = World3d::default();
    let mut b = World3d::default();
    a.reconcile(recipe.clone()).unwrap();
    b.reconcile(recipe).unwrap();
    for _ in 0..240 {
        a.step();
        b.step();
    }
    assert!(a.body("marble").unwrap().position[0] > 0.5);
    assert_eq!(a.inspect(), b.inspect());
}

#[test]
fn retained_rotation_is_a_reset_target_not_a_live_pose_edit() {
    let mut world = World3d::default();
    world.reconcile(specs()).unwrap();
    for _ in 0..40 {
        world.step();
    }
    let before = world.inspect();
    let mut recipe = specs();
    recipe[0].rotation = glam::Quat::from_rotation_z(-0.4).to_array();
    recipe[1].rotation = glam::Quat::from_rotation_x(0.6).to_array();
    recipe[1].position[1] = 8.0;
    world.reconcile(recipe.clone()).unwrap();
    for (old, new) in before.entities.iter().zip(world.inspect().entities) {
        assert_eq!(old.resolved, new.resolved);
    }
    world.reset("platform").unwrap();
    world.reset("marble").unwrap();
    for spec in recipe {
        let reset = world.body(&spec.id).unwrap();
        assert_eq!(reset.position, spec.position);
        assert!(
            glam::Quat::from_array(reset.rotation)
                .dot(glam::Quat::from_array(spec.rotation))
                .abs()
                > 0.99999
        );
        assert_eq!(reset.velocity, [0.0; 3]);
        assert_eq!(reset.angular_velocity, [0.0; 3]);
    }
}

#[test]
fn invalid_rotations_reject_atomically_and_nonunit_rotations_normalize() {
    let mut world = World3d::default();
    world.reconcile(specs()).unwrap();
    world.step();
    let before = world.inspect();
    for rotation in [
        [0.0; 4],
        [f32::NAN, 0.0, 0.0, 1.0],
        [0.0, f32::INFINITY, 0.0, 1.0],
        [0.0, 0.0, 0.0, 10_001.0],
        [0.0, 0.0, 0.0, 0.00001],
    ] {
        let mut bad = specs();
        bad[0].position[1] = 8.0;
        bad[1].rotation = rotation;
        assert!(world.reconcile(bad).is_err());
        assert_eq!(world.inspect(), before);
    }
    let mut recipe = specs();
    recipe[0].rotation = [0.0, 0.0, 2.0, 2.0];
    world.reconcile(recipe).unwrap();
    world.reset("platform").unwrap();
    let q = glam::Quat::from_array(world.body("platform").unwrap().rotation);
    assert!((q.length() - 1.0).abs() < 0.00001);
    assert!((q.z - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.00001);
}

#[test]
fn invalid_rotation_in_one_world_preserves_every_world_in_reload_batch() {
    let mut worlds = Worlds3d::default();
    let mut recipes = WorldRecipes3d::from([("a".into(), specs()), ("b".into(), specs())]);
    worlds.reconcile(recipes.clone()).unwrap();
    let before = worlds.get("a").unwrap().inspect();
    recipes.get_mut("a").unwrap()[0].rotation = glam::Quat::from_rotation_z(0.4).to_array();
    recipes.get_mut("b").unwrap()[0].rotation = [0.0; 4];
    assert!(worlds.reconcile(recipes).is_err());
    assert_eq!(worlds.get("a").unwrap().inspect(), before);
}
