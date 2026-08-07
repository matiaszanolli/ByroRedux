//! Regression tests for `bs_geometry_bounding_sphere_mismatch` (#2098 /
//! SF2D2-01).
//!
//! Pins the cross-check between a BSGeometry block's raw authored
//! `bounding_sphere` radius and the actual max distance-from-center extent
//! of the decoded vertex positions.

use super::bs_geometry::bs_geometry_bounding_sphere_mismatch;

/// A sphere that tightly (or loosely) bounds the vertices reports no
/// mismatch — the steady-state vanilla case.
#[test]
fn matching_scale_reports_no_mismatch() {
    let center = [0.0, 0.0, 0.0];
    let positions = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]];
    // Exact extent is 1.0; author a slightly loose sphere (1.2) — normal
    // Bethesda authoring practice, must not be flagged.
    assert_eq!(
        bs_geometry_bounding_sphere_mismatch(center, 1.2, &positions),
        None
    );
}

/// A sphere radius far smaller than the vertex extent from the same
/// center — the audit's ~70x unit-scale-divergence hypothesis — is
/// reported.
#[test]
fn far_too_small_sphere_is_reported() {
    let center = [0.0, 0.0, 0.0];
    let positions = [[70.0, 0.0, 0.0], [0.0, 70.0, 0.0], [-70.0, 0.0, 0.0]];
    // Sphere radius 1.0 against a ~70-unit vertex extent: exactly the
    // ~70x havok-scale-mismatch failure mode the issue describes.
    let mismatch = bs_geometry_bounding_sphere_mismatch(center, 1.0, &positions)
        .expect("a ~70x too-small sphere must be reported");
    assert_eq!(mismatch.sphere_radius, 1.0);
    assert_eq!(mismatch.vertex_extent_radius, 70.0);
}

/// A merely loose/conservative bound — sphere bigger than the tight
/// vertex extent — is normal authoring practice and must NOT be flagged;
/// this function only ever flags a sphere too SMALL.
#[test]
fn oversized_sphere_is_not_flagged() {
    let center = [0.0, 0.0, 0.0];
    let positions = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    assert_eq!(
        bs_geometry_bounding_sphere_mismatch(center, 1000.0, &positions),
        None,
        "an oversized (loose) authored bound must never be reported as a mismatch"
    );
}

/// A radius just barely above the 10% minimum-ratio floor must not be
/// flagged — pins the threshold is inclusive, not fencepost-off-by-one.
#[test]
fn ratio_at_the_floor_is_not_flagged() {
    let center = [0.0, 0.0, 0.0];
    let positions = [[10.0, 0.0, 0.0]];
    // extent = 10.0; MIN_RATIO = 0.1 ⇒ floor radius = 1.0.
    assert_eq!(
        bs_geometry_bounding_sphere_mismatch(center, 1.0, &positions),
        None
    );
    // Just under the floor must be flagged.
    assert!(bs_geometry_bounding_sphere_mismatch(center, 0.99, &positions).is_some());
}

/// Zero/negative radius, or an empty vertex list, is never flagged — no
/// bound was authored, or there is nothing to compare against.
#[test]
fn no_radius_or_no_vertices_is_never_flagged() {
    let center = [0.0, 0.0, 0.0];
    let positions = [[1.0, 0.0, 0.0]];
    assert_eq!(
        bs_geometry_bounding_sphere_mismatch(center, 0.0, &positions),
        None
    );
    assert_eq!(
        bs_geometry_bounding_sphere_mismatch(center, -1.0, &positions),
        None
    );
    assert_eq!(
        bs_geometry_bounding_sphere_mismatch(center, 1.0, &[]),
        None
    );
}

/// A degenerate single-point mesh (every vertex sits exactly at `center`,
/// vertex_extent_radius == 0.0) must not be flagged — there's no
/// meaningful scale to compare a nonzero sphere against.
#[test]
fn zero_extent_at_center_is_not_flagged() {
    let center = [5.0, 5.0, 5.0];
    let positions = [[5.0, 5.0, 5.0], [5.0, 5.0, 5.0]];
    assert_eq!(
        bs_geometry_bounding_sphere_mismatch(center, 1.0, &positions),
        None
    );
}

/// The extent is measured from the AUTHORED center, not the origin — an
/// off-origin bounding sphere with vertices tightly clustered around its
/// own center must not be spuriously flagged just because the center
/// itself is far from `[0,0,0]`.
#[test]
fn extent_is_measured_from_authored_center_not_origin() {
    let center = [1000.0, 1000.0, 1000.0];
    let positions = [
        [1000.5, 1000.0, 1000.0],
        [999.5, 1000.0, 1000.0],
        [1000.0, 1000.5, 1000.0],
    ];
    // Extent from `center` is only 0.5; a radius of 1.0 comfortably
    // covers it despite both being tiny relative to the center's own
    // magnitude.
    assert_eq!(
        bs_geometry_bounding_sphere_mismatch(center, 1.0, &positions),
        None
    );
}
