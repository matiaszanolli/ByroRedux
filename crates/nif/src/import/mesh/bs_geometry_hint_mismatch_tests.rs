//! Regression tests for `bs_geometry_hint_mismatch` (SF2-03 / #1830).
//!
//! Pins the cross-check between a `BSGeometryMesh` slot's NIF-level
//! `tri_size`/`num_verts` hints and the vertex/triangle counts of the
//! body that actually resolved for it.

use super::bs_geometry::bs_geometry_hint_mismatch;

/// Matching hints produce no mismatch — the steady-state vanilla case.
#[test]
fn matching_hints_report_no_mismatch() {
    let num_verts = 100;
    let tri_count = 50;
    let tri_size = tri_count * 6; // 3 u16 indices per triangle
    assert_eq!(
        bs_geometry_hint_mismatch(tri_size, num_verts, num_verts, tri_count),
        None,
    );
}

/// A `num_verts` hint that disagrees with the resolved vertex count is
/// reported, even when `tri_size` matches.
#[test]
fn num_verts_mismatch_is_reported() {
    let tri_count = 50;
    let tri_size = tri_count * 6;
    let mismatch = bs_geometry_hint_mismatch(tri_size, 100, 99, tri_count)
        .expect("num_verts disagreement must be reported");
    assert_eq!(mismatch.num_verts_hint, 100);
    assert_eq!(mismatch.num_verts_resolved, 99);
    assert_eq!(mismatch.tri_size_hint, mismatch.tri_size_resolved);
}

/// A `tri_size` hint that disagrees with the resolved triangle count is
/// reported, even when `num_verts` matches.
#[test]
fn tri_size_mismatch_is_reported() {
    let num_verts = 100;
    let tri_count = 50;
    let mismatch = bs_geometry_hint_mismatch(999, num_verts, num_verts, tri_count)
        .expect("tri_size disagreement must be reported");
    assert_eq!(mismatch.tri_size_hint, 999);
    assert_eq!(mismatch.tri_size_resolved, tri_count * 6);
    assert_eq!(mismatch.num_verts_hint, mismatch.num_verts_resolved);
}

/// Zero counts on both sides (an empty resolved body) still compare
/// equal — this function is only ever called after the caller's
/// non-empty check, but the boundary behaviour is documentary.
#[test]
fn zero_counts_match_is_not_a_mismatch() {
    assert_eq!(bs_geometry_hint_mismatch(0, 0, 0, 0), None);
}

/// A triangle-count overflow that would overflow `u32` on the byte-size
/// multiply saturates rather than wrapping, so a hostile/corrupt
/// resolved body can't produce a false "match" via wraparound.
#[test]
fn tri_count_overflow_saturates_instead_of_wrapping() {
    let huge_tri_count = u32::MAX;
    let mismatch = bs_geometry_hint_mismatch(123, 0, 0, huge_tri_count)
        .expect("saturated tri_size must still disagree with the small hint");
    assert_eq!(mismatch.tri_size_resolved, u32::MAX);
}
