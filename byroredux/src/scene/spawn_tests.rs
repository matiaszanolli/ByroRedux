use super::select_door_spawn_position;
use byroredux_core::math::Vec3;

#[test]
fn exterior_spawn_skips_persistent_door_in_neighboring_cell() {
    // Regression: interactive `(3,-19)` bootstrap has only that tile's
    // collision ready, while the persistent CELL may already have materialised
    // this row -20 door. It used to win solely because it iterated first.
    let neighboring_persistent_door = Vec3::new(14_740.0, 8_200.0, 78_988.0);
    let foreground_door = Vec3::new(14_500.0, 8_100.0, 75_700.0);

    assert_eq!(
        select_door_spawn_position(
            [neighboring_persistent_door, foreground_door],
            Some((3, -19)),
        ),
        Some(foreground_door),
    );
}

#[test]
fn exterior_spawn_returns_none_when_only_neighboring_doors_exist() {
    let neighboring_persistent_door = Vec3::new(14_740.0, 8_200.0, 78_988.0);

    assert_eq!(
        select_door_spawn_position([neighboring_persistent_door], Some((3, -19))),
        None,
        "the caller must ground against the foreground terrain instead",
    );
}

#[test]
fn interior_spawn_preserves_first_door_behavior() {
    let first = Vec3::new(10.0, 20.0, 30.0);
    let second = Vec3::new(40.0, 50.0, 60.0);

    assert_eq!(
        select_door_spawn_position([first, second], None),
        Some(first)
    );
}
