//! Per-vertex normal synthesis for geometry that authors none (#3541).
//!
//! The sibling of [`super::tangent::synthesize_tangents_yup`]. Bethesda's
//! distant-LOD and FaceGen content routinely ships without a normal lane,
//! and every extraction path used to substitute a constant `[0, 1, 0]`
//! world-up normal — so that geometry flat-shades against a single
//! direction regardless of its actual surface.
//!
//! Measured share of affected shapes, per game:
//!
//! | Game | Affected | Share | Concentration |
//! |---|---|---|---|
//! | FO4 `.bto`/`.btr` LOD | 14,054 / 15,614 | 90.0% | 9,073 files; near `.nif` 62/130,480 |
//! | Oblivion distant-terrain LOD | 100 / 100 | 100% | all of `meshes\landscape\lod\*.nif` |
//! | Skyrim, all imported | 19,657 / 96,123 | 20.4% | LOD `Land` + FaceGen heads + skin patches |
//! | Starfield `.mesh` | 0 / 675,407 | 0% | no distant-LOD `.mesh` corpus exists |
//!
//! Proven not to be a mis-parse: Skyrim's `MaleHeadNord` has
//! `raw_bytes = 17,960 = 898 × 20`, and UV + colour + skin account for every
//! byte — there is no normal lane on disk. Bethesda's runtime recomputes
//! head normals after the FaceGen morph, so substituting a constant is a
//! structural gap on this side, not a content authoring gap.
//!
//! Per NIFAL doctrine this lives at the parser→canonical boundary: gated on
//! actual absence rather than on game or mesh class, and never re-derived at
//! render time.

/// Derive per-vertex normals from face geometry, area-weighted.
///
/// Both inputs are already in renderer **Y-up** space — every call site
/// converts positions through `zup_point_to_yup` first, and the Z-up → Y-up
/// transform is a proper rotation (det = +1), so triangle winding and
/// therefore cross-product orientation are preserved. Deriving in Y-up
/// rather than converting Z-up normals afterwards keeps this a single step
/// and matches [`super::tangent::synthesize_tangents_yup`].
///
/// **Area weighting**, not normalise-per-face: the un-normalised cross
/// product's magnitude is twice the triangle's area, so accumulating it
/// directly weights each face by its own area. That is the standard choice
/// for triangulated terrain, where a vertex is shared by triangles of very
/// different sizes and the large ones should dominate — normalising per face
/// first would let a sliver at a LOD seam pull the normal as hard as the
/// quad it borders.
///
/// A vertex whose accumulated normal is degenerate (isolated, or shared only
/// by zero-area triangles) falls back to `[0, 1, 0]` — the same value the
/// whole mesh used to get, so the worst case is unchanged rather than a new
/// failure mode.
///
/// Returns an empty vector when there is nothing to derive from, which the
/// call sites treat as "keep the constant fallback".
pub fn synthesize_normals_yup(positions_yup: &[[f32; 3]], triangles: &[[u16; 3]]) -> Vec<[f32; 3]> {
    let n = positions_yup.len();
    if n == 0 || triangles.is_empty() {
        return Vec::new();
    }

    let mut accum = vec![[0.0f32; 3]; n];
    for tri in triangles {
        let (i0, i1, i2) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
        if i0 >= n || i1 >= n || i2 >= n {
            continue;
        }
        let p0 = positions_yup[i0];
        let p1 = positions_yup[i1];
        let p2 = positions_yup[i2];
        let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
        // Un-normalised: |e1 × e2| == 2 × area, which IS the weight.
        let face = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        if !face.iter().all(|c| c.is_finite()) {
            continue;
        }
        for &i in &[i0, i1, i2] {
            accum[i][0] += face[0];
            accum[i][1] += face[1];
            accum[i][2] += face[2];
        }
    }

    accum
        .into_iter()
        .map(|v| {
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if len > 1e-12 && len.is_finite() {
                [v[0] / len, v[1] / len, v[2] / len]
            } else {
                // Isolated vertex or only zero-area faces — no worse than the
                // constant this pass replaces.
                [0.0, 1.0, 0.0]
            }
        })
        .collect()
}

/// [`synthesize_normals_yup`] with the historical `[0, 1, 0]` fill as the
/// fallback, for call sites that already hold `[u16; 3]` triangles.
pub fn synthesize_normals_or_default(
    positions_yup: &[[f32; 3]],
    triangles: &[[u16; 3]],
) -> Vec<[f32; 3]> {
    let derived = synthesize_normals_yup(positions_yup, triangles);
    if derived.len() == positions_yup.len() {
        derived
    } else {
        vec![[0.0, 1.0, 0.0]; positions_yup.len()]
    }
}

/// [`synthesize_normals_yup`] over a flat `u32` index buffer, falling back
/// to the historical constant when there is nothing to derive from.
///
/// The extraction paths carry indices as `u32` triples; the core routine
/// takes `[u16; 3]` to match `synthesize_tangents_yup`'s signature and the
/// `NiTriShapeData` triangle arrays. Indices past `u16::MAX` are skipped
/// rather than truncated — a wrong triangle is worse than a missing one,
/// and the per-vertex fallback covers any vertex left with no contribution.
pub fn derive_normals_from_u32_indices(
    positions_yup: &[[f32; 3]],
    indices: &[u32],
) -> Vec<[f32; 3]> {
    let triangles: Vec<[u16; 3]> = indices
        .chunks_exact(3)
        .filter_map(|c| {
            Some([
                u16::try_from(c[0]).ok()?,
                u16::try_from(c[1]).ok()?,
                u16::try_from(c[2]).ok()?,
            ])
        })
        .collect();
    let derived = synthesize_normals_yup(positions_yup, &triangles);
    if derived.len() == positions_yup.len() {
        derived
    } else {
        vec![[0.0, 1.0, 0.0]; positions_yup.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A flat quad in the XZ plane must derive a +Y normal, matching the
    /// winding convention every authored Bethesda normal already follows.
    #[test]
    fn a_flat_xz_quad_derives_world_up() {
        let positions = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ];
        // Winding chosen so the geometric normal is +Y.
        let triangles = [[0u16, 2, 1], [0, 3, 2]];
        let normals = synthesize_normals_yup(&positions, &triangles);
        assert_eq!(normals.len(), 4);
        for n in normals {
            assert!(
                (n[1] - 1.0).abs() < 1e-5,
                "a flat XZ quad must derive +Y, got {n:?}"
            );
        }
    }

    /// The point of the pass: a sloped surface must NOT read as world-up.
    /// This is the whole distant-terrain-LOD symptom — uniformly lit
    /// regardless of slope.
    #[test]
    fn a_slope_derives_a_tilted_normal_not_world_up() {
        // A 45-degree ramp rising along +X.
        let positions = [[0.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let normals = synthesize_normals_yup(&positions, &[[0u16, 2, 1]]);
        let n = normals[0];
        assert!(
            n[1] > 0.0 && n[0] < -0.5,
            "a ramp rising along +X must tilt its normal against the slope, \
             got {n:?}"
        );
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        assert!((len - 1.0).abs() < 1e-5, "normals must be unit length");
    }

    /// Area weighting: a vertex shared by a large face and a sliver must
    /// follow the large one. Normalising per face first would let the sliver
    /// pull as hard, which is exactly the LOD-seam artefact to avoid.
    #[test]
    fn a_sliver_does_not_outvote_the_face_it_borders() {
        // Big face in the XZ plane (+Y normal) and a tiny near-degenerate
        // triangle tilted steeply, sharing vertex 0.
        let positions = [
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 0.0, 10.0],
            [0.0001, 1.0, 0.0],
        ];
        let triangles = [[0u16, 2, 1], [0, 1, 3]];
        let normals = synthesize_normals_yup(&positions, &triangles);
        assert!(
            normals[0][1] > 0.99,
            "the 50-unit face must dominate the sliver, got {:?}",
            normals[0]
        );
    }

    /// Degenerate inputs must not panic or emit NaN.
    #[test]
    fn degenerate_input_falls_back_to_the_constant() {
        assert!(synthesize_normals_yup(&[], &[[0, 1, 2]]).is_empty());
        assert!(synthesize_normals_yup(&[[0.0; 3]], &[]).is_empty());
        // Out-of-range indices are skipped, leaving every vertex degenerate.
        let normals = synthesize_normals_yup(&[[0.0; 3], [1.0, 0.0, 0.0]], &[[0, 5, 9]]);
        assert_eq!(normals, vec![[0.0, 1.0, 0.0]; 2]);
        // A zero-area triangle contributes nothing.
        let normals = synthesize_normals_yup(&[[0.0; 3], [0.0; 3], [0.0; 3]], &[[0, 1, 2]]);
        assert_eq!(normals, vec![[0.0, 1.0, 0.0]; 3]);
    }
}
