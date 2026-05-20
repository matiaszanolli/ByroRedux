//! NiTriShape, BsTriShape, BSSubIndexTriShape, NiAdditionalGeometryData parsers.
//!
//! Indexed triangle geometry blocks across the NIF format generations:
//!
//! - Pre-Skyrim (`NiTriShape` + `NiTriShapeData`, `NiTriStrips`, `BSLODTriShape`) live in
//!   [`ni_tri_shape`]. The data block carries vertex / normal / UV / triangle arrays as
//!   separate fields keyed off the parent `NiGeometry` base.
//! - Skyrim SE+ (`BSTriShape` + its four wire-distinct subclasses LOD / MeshLOD /
//!   SubIndex / Dynamic) live in [`bs_tri_shape`]. The packed-vertex layout interleaves
//!   positions / normals / tangents / colors via the `vertex_desc` bitfield. The
//!   subclasses share a single Rust struct disambiguated by the
//!   [`BsTriShapeKind`] discriminator (#560 / #404).
//! - FO3+ `NiAdditionalGeometryData` / `BSPackedAdditionalGeometryData` live in [`agd`].
//!   These attach per-vertex auxiliary channels (tangents / bitangents / blend weights)
//!   to a separate `NiGeometryData`. 4,039 vanilla blocks were previously demoted to
//!   `NiUnknown` before #547 wired them.
//!
//! ## Module layout
//!
//! Split out of the prior 1875-LOC monolith (TD9-005 / #1118):
//!
//! - [`ni_tri_shape`] — `NiTriShape`, `NiTriStrips`, `NiLodTriShape`, `NiTriShapeData`,
//!   `NiTriStripsData`, and the `parse_geometry_data_base*` helpers shared with
//!   `NiPSysData` (re-exported as `pub(crate)` so `blocks::particle` keeps its
//!   `super::tri_shape::parse_psys_geometry_data_base` path).
//! - [`bs_tri_shape`] — `BsTriShapeKind`, `BsTriShape`, the `BsGeometry*` sub-segment
//!   types, `BsSubIndexTriShapeData`, plus the private `read_vertex_skin_data` and
//!   `byte_to_normal` helpers used only by the packed-vertex parser.
//! - [`agd`] — `NiAdditionalGeometryData`, `BSPackedAdditionalGeometryData` (sharing the
//!   same Rust struct via [`NiAgdKind`]), `NiAgdDataStream`, `NiAgdDataBlock`.
//!
//! Shared low-level helpers (`renormalize_skin_weights`, `half_to_f32`) live in this
//! module root, matching the established pattern from `blocks/collision/`.
//!
//! Every public type is `pub use`-d at this module's root so external callers
//! (`crate::blocks::tri_shape::TypeName`) keep working unchanged.

mod agd;
mod bs_tri_shape;
mod ni_tri_shape;

pub use agd::{NiAdditionalGeometryData, NiAgdDataBlock, NiAgdDataStream, NiAgdKind};
pub use bs_tri_shape::{
    BsGeometryPerSegmentSharedData, BsGeometrySegmentData, BsGeometrySegmentSharedData,
    BsGeometrySubSegment, BsSubIndexTriShapeData, BsTriShape, BsTriShapeKind,
};
pub use ni_tri_shape::{
    NiLodTriShape, NiTriShape, NiTriShapeData, NiTriStrips, NiTriStripsData,
};

// Re-export crate-internal parse helpers so `blocks::particle::parse` and the sibling
// version test files keep accessing them via the `tri_shape::` path.
pub(crate) use ni_tri_shape::{parse_geometry_data_base, parse_psys_geometry_data_base};

// --- Shared low-level helpers ---

/// Renormalize a 4-influence weight tuple to unit sum so half-float
/// quantization drift can't accumulate per-frame jitter on the GPU
/// skinning path.
///
/// `triangle.vert` computes the matrix-palette result as a straight
/// weighted sum without dividing by `wsum` (it only uses `wsum` to
/// detect the rigid-fallback case `wsum < 0.001`). Half-float
/// quantization produces ~1-part-in-1024 error per component, so a
/// 4-influence vertex can drift up to ~0.4% off unit sum and the
/// rendered skin position drifts the same fraction.
///
/// Skip the renormalization when the sum is already within `1e-4`
/// of `1.0` (well-formed content) or below `1e-6` (the rigid-fallback
/// path the vertex shader detects). See #889.
#[inline]
pub(crate) fn renormalize_skin_weights(w: [f32; 4]) -> [f32; 4] {
    let sum = w[0] + w[1] + w[2] + w[3];
    if (sum - 1.0).abs() <= 1e-4 || sum <= 1e-6 {
        return w;
    }
    let inv = 1.0 / sum;
    [w[0] * inv, w[1] * inv, w[2] * inv, w[3] * inv]
}

/// Convert IEEE 754 half-precision float (u16) to f32.
///
/// #945 / SK-D1-NEW-04: deduplicated — the canonical implementation
/// lives in `crate::import::mesh::half_to_f32` (re-exported from the
/// private `decode` submodule via `pub(crate) use decode::*`). This
/// wrapper stays as `pub(crate)` so callers within `blocks/` keep the
/// local name.
#[inline]
pub(crate) fn half_to_f32(h: u16) -> f32 {
    crate::import::mesh::half_to_f32(h)
}

#[cfg(test)]
mod renormalize_skin_weights_tests {
    use super::renormalize_skin_weights;

    /// Regression for #889: a 4-influence vertex with weights
    /// summing to 0.997 (typical sub-unit drift after half-float
    /// decode) must round-trip with a sum of 1.0 ± 1e-4.
    #[test]
    fn drifted_weights_renormalize_to_unit_sum() {
        let drift: [f32; 4] = [0.30, 0.30, 0.30, 0.097];
        let normed = renormalize_skin_weights(drift);
        let sum: f32 = normed.iter().sum();
        assert!(
            (sum - 1.0).abs() <= 1e-4,
            "post-renorm sum {sum} not within 1e-4 of 1.0"
        );
        // Ratios preserved.
        let ratio = normed[0] / normed[3];
        let original_ratio = drift[0] / drift[3];
        assert!((ratio - original_ratio).abs() < 1e-3);
    }

    /// Already-unit-sum weights pass through untouched (no float
    /// noise injected on well-formed content).
    #[test]
    fn unit_sum_weights_pass_through_unchanged() {
        let exact: [f32; 4] = [0.5, 0.25, 0.15, 0.10];
        let normed = renormalize_skin_weights(exact);
        assert_eq!(normed, exact);
    }

    /// Within-tolerance drift (0.99995) is treated as unit-sum and
    /// passes through unchanged — avoids touching content that's
    /// already within float error of well-formed.
    #[test]
    fn within_tolerance_drift_passes_through() {
        let near_unit: [f32; 4] = [0.49998, 0.24999, 0.15, 0.09998];
        let normed = renormalize_skin_weights(near_unit);
        assert_eq!(normed, near_unit);
    }

    /// Rigid-fallback weights (sum below 1e-6) pass through so the
    /// vertex shader's `wsum < 0.001` rigid-fallback branch still
    /// triggers. Renormalising would push them to spurious unit
    /// sum and break the fallback.
    #[test]
    fn near_zero_weights_preserve_rigid_fallback_path() {
        let zeroish: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
        let normed = renormalize_skin_weights(zeroish);
        assert_eq!(normed, zeroish);
    }
}
