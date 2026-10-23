use super::{exterior_water_tile_transform, resolved_exterior_water_height};

#[test]
fn absent_xclw_inherits_worldspace_water() {
    assert_eq!(
        resolved_exterior_water_height(false, None, Some(-14_000.0)),
        Some(-14_000.0)
    );
}

#[test]
fn explicit_xclw_overrides_or_suppresses_worldspace_water() {
    assert_eq!(
        resolved_exterior_water_height(true, Some(-250.0), Some(-14_000.0)),
        Some(-250.0)
    );
    assert_eq!(
        resolved_exterior_water_height(true, None, Some(-14_000.0)),
        None,
        "an authored no-water sentinel must keep the cell dry"
    );
}

#[test]
fn exterior_water_tiles_share_world_edges_after_coordinate_conversion() {
    let ((x0, z0), half) = exterior_water_tile_transform(12, -7);
    let ((x1, _z1), half_next_x) = exterior_water_tile_transform(13, -7);
    let ((_x2, z2), half_next_z) = exterior_water_tile_transform(12, -6);

    assert_eq!(half, half_next_x);
    assert_eq!(half, half_next_z);
    assert_eq!(x0 + half, x1 - half_next_x);
    assert_eq!(z0 - half, z2 + half_next_z);
}
