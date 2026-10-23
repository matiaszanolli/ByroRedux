use super::persistent_position_is_local;

#[test]
fn persistent_3d_filter_uses_floor_for_negative_cells_and_chebyshev_radius() {
    assert!(persistent_position_is_local(
        [4.25 * 4096.0, -19.75 * 4096.0, 0.0],
        (4, -20),
        0,
    ));
    assert!(persistent_position_is_local(
        [5.0 * 4096.0, -21.0 * 4096.0, 0.0],
        (4, -20),
        1,
    ));
    assert!(!persistent_position_is_local(
        [6.0 * 4096.0, -20.0 * 4096.0, 0.0],
        (4, -20),
        1,
    ));
    assert!(!persistent_position_is_local([-0.01, 0.0, 0.0], (0, 0), 0,));
}
