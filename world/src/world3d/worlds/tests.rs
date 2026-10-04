use super::*;
use crate::world3d::Shape3d;

fn recipe(height: f32) -> Vec<EntitySpec3d> {
    vec![EntitySpec3d {
        id: "marble".into(),
        shape: Shape3d::Sphere(0.25),
        position: [0.0, height, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        dynamic: true,
        sensor: false,
    }]
}

fn recipes() -> WorldRecipes3d {
    BTreeMap::from([
        ("a".into(), recipe(3.0)),
        ("b".into(), recipe(4.0)),
        ("omitted".into(), recipe(5.0)),
    ])
}

#[test]
fn failed_batch_changes_neither_earlier_worlds_nor_omitted_worlds() {
    let mut worlds = Worlds3d::default();
    worlds.reconcile(recipes()).unwrap();
    let a = worlds.get_mut("a").unwrap();
    a.advance(0.0, true).unwrap();
    a.advance(1.0, true).unwrap();
    let before: BTreeMap<_, _> = worlds
        .worlds
        .iter()
        .map(|(id, world)| (id.clone(), world.inspect()))
        .collect();
    let mut staged = BTreeMap::from([("a".into(), recipe(9.0)), ("b".into(), recipe(10.0))]);
    staged.get_mut("b").unwrap()[0].shape = Shape3d::Sphere(0.5);
    assert_eq!(
        worlds.reconcile(staged),
        Err(Worlds3dError::Recipe {
            id: "b".into(),
            source: World3dError::Changed("marble".into()),
        })
    );
    for (id, state) in &before {
        assert_eq!(worlds.get(id).unwrap().inspect(), *state);
    }
    // Authored reset targets are unchanged too, not just the current poses.
    worlds.get_mut("a").unwrap().reset("marble").unwrap();
    assert_eq!(
        worlds.get("a").unwrap().body("marble").unwrap().position,
        [0.0, 3.0, 0.0]
    );
}

#[test]
fn preflight_is_observational_and_success_retains_the_solver_and_clock() {
    let mut worlds = Worlds3d::default();
    worlds.reconcile(recipes()).unwrap();
    let a = worlds.get_mut("a").unwrap();
    a.advance(0.0, true).unwrap();
    a.advance(1.0 / 60.0, true).unwrap();
    let before = a.inspect();
    let mut staged = recipes();
    staged.insert("a".into(), recipe(9.0));
    staged.remove("omitted");
    worlds.validate_reconcile(&staged).unwrap();
    assert_eq!(worlds.get("a").unwrap().inspect(), before);
    assert!(worlds.get("omitted").is_some());
    worlds.reconcile(staged).unwrap();
    let a = worlds.get_mut("a").unwrap();
    assert_eq!(a.tick(), before.tick);
    assert_eq!(a.body("marble").unwrap(), before.entities[0].resolved);
    assert_eq!(a.advance(2.0 / 60.0, true).unwrap().steps, 2);
    a.reset("marble").unwrap();
    assert_eq!(a.body("marble").unwrap().position, [0.0, 9.0, 0.0]);
    assert!(worlds.get("omitted").is_none());
}

#[test]
fn invalid_new_world_and_collection_budgets_reject_without_mutation() {
    let mut worlds = Worlds3d::default();
    worlds.reconcile(recipes()).unwrap();
    let mut staged = recipes();
    staged.insert("new".into(), recipe(f32::NAN));
    assert!(matches!(
        worlds.reconcile(staged),
        Err(Worlds3dError::Recipe { .. })
    ));
    assert!(worlds.get("new").is_none());
    let mut staged = recipes();
    staged.insert("".into(), recipe(1.0));
    assert_eq!(worlds.reconcile(staged), Err(Worlds3dError::Id("".into())));
    let staged = (0..9)
        .map(|n| (format!("world-{n}"), recipe(1.0)))
        .collect();
    assert_eq!(worlds.reconcile(staged), Err(Worlds3dError::Budget));
    assert_eq!(worlds.worlds.len(), 3);
    worlds.reconcile(BTreeMap::new()).unwrap();
    assert!(worlds.worlds.is_empty());
}

#[test]
fn collections_are_app_local_even_when_authored_ids_match() {
    let mut left = Worlds3d::default();
    let mut right = Worlds3d::default();
    left.reconcile(recipes()).unwrap();
    right.reconcile(recipes()).unwrap();
    left.get_mut("a").unwrap().step();
    assert_eq!(left.get("a").unwrap().tick(), 1);
    assert_eq!(right.get("a").unwrap().tick(), 0);
    assert_eq!(
        right.get("a").unwrap().body("marble").unwrap().position,
        [0.0, 3.0, 0.0]
    );
}
