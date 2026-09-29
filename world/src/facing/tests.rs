use super::*;

#[test]
fn the_larger_component_of_a_move_picks_the_facing() {
    assert_eq!(Dir::of(1.0, 0.0, Dir::Down), Dir::Right);
    assert_eq!(Dir::of(-1.0, 0.2, Dir::Down), Dir::Left);
    assert_eq!(Dir::of(0.0, -1.0, Dir::Right), Dir::Up);
    assert_eq!(Dir::of(0.3, 1.0, Dir::Up), Dir::Down);
}

#[test]
fn an_exact_diagonal_keeps_the_facing_when_it_can() {
    assert_eq!(Dir::of(1.0, 1.0, Dir::Down), Dir::Down);
    assert_eq!(Dir::of(1.0, 1.0, Dir::Right), Dir::Right);
    assert_eq!(
        Dir::of(1.0, 1.0, Dir::Up),
        Dir::Right,
        "neither is current: horizontal"
    );
}
