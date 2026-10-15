//! Tangent-space extraction + synthesis.
//!
//! Extra-data tangent capture, Mikkelsen-style synthesis fallback, and
//! Zup→Yup quaternion fix-up for BS tangent payloads.

use byroredux_core::bytes::le_f32_at;
use crate::scene::NifScene;
use crate::types::{BlockRef, NiPoint3};

pub fn extract_tangents_from_extra_data(
    scene: &NifScene,
    extra_data_refs: &[BlockRef],
    normals_zup: &[NiPoint3],
    num_verts: usize,
) -> Vec<[f32; 4]> {
    if num_verts == 0 || normals_zup.len() != num_verts {
        return Vec::new();
    }
    let expected_size = num_verts * 24;

    for ref_idx in extra_data_refs {
        let Some(idx) = ref_idx.index() else { continue };
        let Some(block) = scene.blocks.get(idx) else {
            continue;
        };
        let Some(ed) = block
            .as_any()
            .downcast_ref::<crate::blocks::extra_data::NiExtraData>()
        else {
            continue;
        };
        if ed.type_name != "NiBinaryExtraData" {
            continue;
        }
        let name = ed.name.as_deref().unwrap_or("");
        if name != "Tangent space (binormal & tangent vectors)" {
            continue;
        }
        let Some(blob) = ed.binary_data.as_ref() else {
            continue;
        };
        if blob.len() != expected_size {
            // Size-mismatched blob — skip rather than risk a partial
            // decode. nifly's writer pads to numVerts × 24 exactly
            // so a mismatch indicates either authored corruption or
            // a parser drift we want to learn about, not paper over.
            log::warn!(
                "Tangent-space extra data size mismatch: expected {} bytes \
                 (numVerts={}), got {}. Skipping tangent decode; renderer \
                 will fall back to screen-space derivative TBN.",
                expected_size,
                num_verts,
                blob.len()
            );
            continue;
        }

        let mut tangents = Vec::with_capacity(num_verts);
        // #786 — the on-disk blob layout (per nifly's I/O at
        // `Geometry.cpp:81-84`) is `[tangents..., bitangents...]`,
        // but Bethesda's `tangents` field actually holds ∂P/∂V and
        // `bitangents` holds ∂P/∂U (see `CalcTangentSpace` swap at
        // `Geometry.cpp:2084-2085`). Our shader Path 1 expects
        // `vertexTangent.xyz = ∂P/∂U`, so read the bitangent half
        // (second 12-byte stride) into our tangent and use the
        // tangent half for the sign derivation only. Pre-fix the
        // halves were assigned in nifly order, mismatching the
        // shader's standard-convention `mat3(T, B, N)` and producing
        // the chrome-walls regression on FNV (R-N2 / #786).
        let bethesda_bitangent_offset = num_verts * 12;
        for (i, n_zup) in normals_zup.iter().enumerate().take(num_verts) {
            let bethesda_t_off = i * 12;
            let bethesda_b_off = bethesda_bitangent_offset + i * 12;
            // Read Vector3 (3 × f32 LE).
            // The `blob.len() == expected_size` check above bounds every
            // offset below, so the `unwrap_or` is unreachable.
            let f32_at = |offset: usize| le_f32_at(blob, offset).unwrap_or(0.0);
            let bethesda_tx = f32_at(bethesda_t_off);
            let bethesda_ty = f32_at(bethesda_t_off + 4);
            let bethesda_tz = f32_at(bethesda_t_off + 8);
            let bethesda_bx = f32_at(bethesda_b_off);
            let bethesda_by = f32_at(bethesda_b_off + 4);
            let bethesda_bz = f32_at(bethesda_b_off + 8);

            // Z-up → Y-up basis change applied to both direction
            // vectors: same `(x, y, z) → (x, z, -y)` swap used for
            // positions / normals throughout import.
            //
            // After the swap, our `t_yup` is ∂P/∂U (read from
            // Bethesda's bitangent half) and `b_yup` is ∂P/∂V
            // (read from Bethesda's tangent half).
            let t_yup = byroredux_core::math::coord::zup_to_yup_pos([
                bethesda_bx,
                bethesda_by,
                bethesda_bz,
            ]);
            let b_yup = byroredux_core::math::coord::zup_to_yup_pos([
                bethesda_tx,
                bethesda_ty,
                bethesda_tz,
            ]);

            // Normal in Y-up — use the matching vertex normal.
            let n_yup = byroredux_core::math::coord::zup_to_yup_pos([n_zup.x, n_zup.y, n_zup.z]);

            // Bitangent sign: sign(dot(B, cross(N, T))). With T = ∂P/∂U
            // and B = ∂P/∂V on a standard right-handed UV winding the
            // sign lands at +1 — the textbook case; UV-mirrored shells
            // flip it so the shader's bitangent stays consistent across
            // the mirror seam. Shared with the inline + SSE producers so
            // the operand order can't drift (see `bitangent_sign` / #1516).
            let sign = crate::types::bitangent_sign(n_yup, t_yup, b_yup);

            tangents.push([t_yup[0], t_yup[1], t_yup[2], sign]);
        }
        return tangents;
    }
    // No authored tangent blob — caller falls through to
    // `synthesize_tangents` which runs nifly's CalcTangentSpace
    // algorithm on positions + normals + UVs + triangles. This
    // hits on most FNV / FO3 / Oblivion interior content where
    // Bethesda relied on the runtime to compute tangents at load
    // time. See #783.
    Vec::new()
}

/// Synthesize per-vertex tangents from Z-up NIF data when the source
/// ships no authored tangents — the entry point the classic
/// `NiTriShape` / `BSTriShape` paths call.
///
/// #5793 — a thin wrapper over [`synthesize_tangents_yup`], the single
/// producer. The Z-up flavour used to be a line-for-line second copy of
/// the Lengyel accumulation + Gram-Schmidt finalize, and every guard
/// fix landed on one copy first (#2632 normalized N in Y-up only, and
/// #3177 was literally its Z-up catch-up ten days later; #3176's
/// zero-tangent seed reached both only because the audit named both).
///
/// Converting the inputs up front is bit-identical to the old
/// accumulate-in-Z-up-then-swap finalize for the non-degenerate path:
/// `zup_to_yup_pos` is an exact component permutation + negation
/// (`(x, y, z) → (x, z, -y)`), rotations commute with the per-triangle
/// normalize, and a rotated sum equals the sum of rotated parts. The
/// degenerate-tangent fallback's seed permutation now also runs in
/// Y-up; the two old spellings picked different-but-equally-valid
/// directions there (any N-orthogonal unit vector is correct — the
/// divergence was documented in AUDIT_INCREMENTAL_2026-05-22 ID-4 and
/// is retired with the copy).
///
/// Inputs:
///   - `vertices` / `normals_zup`: Z-up world-space, length == num_verts
///   - `uvs`: per-vertex 2D coordinates, length == num_verts
///   - `triangles`: u16 indices, 3 per triangle
///
/// Output and the full algorithm/convention rationale (#786: tangent =
/// ∂P/∂U, nifly's Bethesda swap un-done): see [`synthesize_tangents_yup`].
pub fn synthesize_tangents(
    vertices: &[NiPoint3],
    normals_zup: &[NiPoint3],
    uvs: &[[f32; 2]],
    triangles: &[[u16; 3]],
) -> Vec<[f32; 4]> {
    let n = vertices.len();
    if n == 0 || normals_zup.len() != n || uvs.len() != n {
        return Vec::new();
    }
    let to_yup = |v: &NiPoint3| {
        byroredux_core::math::coord::zup_to_yup_pos([v.x, v.y, v.z])
    };
    let positions_yup: Vec<[f32; 3]> = vertices.iter().map(to_yup).collect();
    let normals_yup: Vec<[f32; 3]> = normals_zup.iter().map(to_yup).collect();
    synthesize_tangents_yup(&positions_yup, &normals_yup, uvs, triangles)
}

/// Convert a slice of raw Z-up BSTriShape tangent records into the
/// Y-up form `Vertex.tangent` consumes. The on-disk record's `xyz`
/// already follows the existing audit convention (Bethesda's
/// "bitangent" triplet stored as our tangent direction = ∂P/∂U) and
/// the `w` carries the bitangent sign. Sign is rotation-invariant so
/// only the .xyz components need axis swap, matching the
/// `(x, y, z) → (x, z, -y)` convention applied to positions / normals
/// throughout import. See #795 / SK-D1-03 + #796 / SK-D1-04.
pub fn bs_tangents_zup_to_yup(zup: &[[f32; 4]]) -> Vec<[f32; 4]> {
    // #1617 — swap the .xyz through the coord SoT; the .w sign rides through
    // unchanged. Bit-identical to the prior inline `[x, z, -y, w]`.
    zup.iter()
        .map(|t| {
            let [x, y, z] = byroredux_core::math::coord::zup_to_yup_pos([t[0], t[1], t[2]]);
            [x, y, z, t[3]]
        })
        .collect()
}

/// The single tangent-synthesis producer (#5793), for inputs already in
/// the renderer's Y-up coordinate space — namely Starfield `BSGeometry`
/// (positions / normals arrive in **source mesh-local Z-up units** —
/// `BSGeometryMeshData.vertices`'s documented contract — and are
/// basis-changed through `zup_to_yup_pos` at import, b9e961eeb) and SSE
/// skin-reconstructed `BSTriShape` (positions / normals / uvs filled
/// from `try_reconstruct_sse_geometry` which writes Y-up). Z-up callers
/// route through the [`synthesize_tangents`] wrapper, which converts
/// inputs up front instead of maintaining a second copy of this body.
///
/// Inputs:
///   - `positions_yup` / `normals_yup`: already-Y-up, length == num_verts
///   - `uvs`: per-vertex 2D coordinates, length == num_verts
///   - `triangles`: u16 indices, 3 per triangle
///
/// Output: `Vec<[f32; 4]>` length == num_verts. Empty when any input is
/// missing or vertex counts don't line up; caller falls back to the
/// shader's screen-space derivative TBN path (Path 2).
///
/// Aligned with the deferred-comment recommendation in
/// `import/mesh/bs_geometry.rs:112-118` (#1086) — Starfield BSGeometry
/// can swap its Path-2 fallback for Path-1 by routing through here
/// when authored UDEC3 tangents are absent. Also unblocks #1204
/// (SSE-reconstructed BSTriShape with empty `VF_TANGENTS`).
pub fn synthesize_tangents_yup(
    positions_yup: &[[f32; 3]],
    normals_yup: &[[f32; 3]],
    uvs: &[[f32; 2]],
    triangles: &[[u16; 3]],
) -> Vec<[f32; 4]> {
    let n = positions_yup.len();
    if n == 0 || normals_yup.len() != n || uvs.len() != n {
        return Vec::new();
    }

    let mut tan_u = vec![[0.0f32; 3]; n];
    let mut tan_v = vec![[0.0f32; 3]; n];

    for tri in triangles {
        let i1 = tri[0] as usize;
        let i2 = tri[1] as usize;
        let i3 = tri[2] as usize;
        if i1 >= n || i2 >= n || i3 >= n {
            continue;
        }

        let v1 = positions_yup[i1];
        let v2 = positions_yup[i2];
        let v3 = positions_yup[i3];
        let w1 = uvs[i1];
        let w2 = uvs[i2];
        let w3 = uvs[i3];

        let x1 = v2[0] - v1[0];
        let x2 = v3[0] - v1[0];
        let y1 = v2[1] - v1[1];
        let y2 = v3[1] - v1[1];
        let z1 = v2[2] - v1[2];
        let z2 = v3[2] - v1[2];

        let s1 = w2[0] - w1[0];
        let s2 = w3[0] - w1[0];
        let t1 = w2[1] - w1[1];
        let t2 = w3[1] - w1[1];

        let det = s1 * t2 - s2 * t1;
        let r = if det >= 0.0 { 1.0 } else { -1.0 };

        let mut sdir = [
            (t2 * x1 - t1 * x2) * r,
            (t2 * y1 - t1 * y2) * r,
            (t2 * z1 - t1 * z2) * r,
        ];
        let mut tdir = [
            (s1 * x2 - s2 * x1) * r,
            (s1 * y2 - s2 * y1) * r,
            (s1 * z2 - s2 * z1) * r,
        ];

        normalize_inplace(&mut sdir);
        normalize_inplace(&mut tdir);

        for &i in &[i1, i2, i3] {
            tan_u[i][0] += sdir[0];
            tan_u[i][1] += sdir[1];
            tan_u[i][2] += sdir[2];
            tan_v[i][0] += tdir[0];
            tan_v[i][1] += tdir[1];
            tan_v[i][2] += tdir[2];
        }
    }

    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        // #2632 / SF2D2-D2-04 — `normals_yup` is unit-length only to
        // quantization for a UDEC3-decoded source (Starfield BSGeometry's
        // `unpack_udec3_xyzw` performs no normalization); the Gram-Schmidt
        // projection below (and the degenerate branch's permutation +
        // cross product) is only correct for `|n| == 1`. Normalize the
        // local copy once here rather than requiring every caller to
        // pre-normalize — cheap, and a no-op for already-unit input
        // (SSE-reconstructed BSTriShape normals).
        let mut n_yup = normals_yup[i];
        normalize_inplace(&mut n_yup);
        let tangent_in = tan_u[i];
        let bitangent_in = tan_v[i];

        let (tangent_yup, bitangent_yup) =
            if vec3_is_zero(&tangent_in) || vec3_is_zero(&bitangent_in) {
                // Degenerate fallback (nifly: permute N components). Since
                // we're already Y-up, the permutation lives in Y-up space
                // too — pick `[n_yup.y, n_yup.z, n_yup.x]` and let B fall
                // out of `cross(N, T)` so the basis is right-handed.
                //
                // Note: this permutation is NOT the Y-up image of the Z-up
                // flavor's `[n_zup.y, n_zup.z, n_zup.x]` permutation —
                // both produce valid orthogonal tangents for the degenerate
                // case but the chosen direction differs by space. Documented
                // divergence (AUDIT_INCREMENTAL_2026-05-22 ID-4); both are
                // acceptable since the degenerate case has no canonical
                // tangent and any orthogonal-to-N direction is correct.
                //
                // #2632 / SF2D2-D2-04 — a raw cyclic permutation of N's
                // components is NOT generally orthogonal to N (e.g. any N
                // with all-equal components permutes to itself), so
                // `cross(N, t_y)` on the raw permutation could yield a
                // non-orthogonal-to-N, non-unit bitangent. Gram-Schmidt
                // the permuted vector against N and normalize before the
                // cross product, matching the non-degenerate branch below.
                let t_y_raw = [n_yup[1], n_yup[2], n_yup[0]];
                let dot_nt = n_yup[0] * t_y_raw[0] + n_yup[1] * t_y_raw[1] + n_yup[2] * t_y_raw[2];
                let mut t_y = [
                    t_y_raw[0] - n_yup[0] * dot_nt,
                    t_y_raw[1] - n_yup[1] * dot_nt,
                    t_y_raw[2] - n_yup[2] * dot_nt,
                ];
                normalize_inplace(&mut t_y);
                if vec3_is_zero(&t_y) {
                    // #3176 — exactly the case the comment above names:
                    // N's cyclic permutation is parallel to N itself (e.g.
                    // N == (1,1,1)/sqrt(3)), so the projection removed the
                    // entire vector. Fall back to a seed guaranteed
                    // non-parallel to N instead of shipping a zero tangent.
                    t_y = fallback_orthogonal_seed(n_yup);
                }
                let b_y = [
                    n_yup[1] * t_y[2] - n_yup[2] * t_y[1],
                    n_yup[2] * t_y[0] - n_yup[0] * t_y[2],
                    n_yup[0] * t_y[1] - n_yup[1] * t_y[0],
                ];
                (t_y, b_y)
            } else {
                // Gram-Schmidt against N (no Z-up→Y-up swap — inputs are
                // already in renderer space).
                let mut t_yup = tangent_in;
                let mut b_yup = bitangent_in;

                normalize_inplace(&mut t_yup);
                let dot_nt = n_yup[0] * t_yup[0] + n_yup[1] * t_yup[1] + n_yup[2] * t_yup[2];
                t_yup = [
                    t_yup[0] - n_yup[0] * dot_nt,
                    t_yup[1] - n_yup[1] * dot_nt,
                    t_yup[2] - n_yup[2] * dot_nt,
                ];
                normalize_inplace(&mut t_yup);

                normalize_inplace(&mut b_yup);
                let dot_nb = n_yup[0] * b_yup[0] + n_yup[1] * b_yup[1] + n_yup[2] * b_yup[2];
                b_yup = [
                    b_yup[0] - n_yup[0] * dot_nb,
                    b_yup[1] - n_yup[1] * dot_nb,
                    b_yup[2] - n_yup[2] * dot_nb,
                ];
                let dot_tb = t_yup[0] * b_yup[0] + t_yup[1] * b_yup[1] + t_yup[2] * b_yup[2];
                b_yup = [
                    b_yup[0] - t_yup[0] * dot_tb,
                    b_yup[1] - t_yup[1] * dot_tb,
                    b_yup[2] - t_yup[2] * dot_tb,
                ];
                normalize_inplace(&mut b_yup);
                (t_yup, b_yup)
            };

        // Bitangent sign: sign(dot(B, cross(N, T))) — shared helper so the
        // operand order stays pinned across every tangent producer (#1516).
        let sign = crate::types::bitangent_sign(n_yup, tangent_yup, bitangent_yup);

        out.push([tangent_yup[0], tangent_yup[1], tangent_yup[2], sign]);
    }
    out
}

/// Second-choice seed for the degenerate-tangent fallback, used when the
/// primary seed (a Gram-Schmidt-projected cyclic permutation of `N`)
/// collapses to zero — which happens exactly when the permutation is
/// itself parallel to `N` (e.g. `N == (1,1,1)/sqrt(3)`, the all-equal-
/// components case both producers' comments name as the motivation for
/// projecting in the first place). #3176.
///
/// Crosses `n` (assumed unit) with the world axis least aligned with it.
/// A unit vector's smallest component is at most `1/sqrt(3)` in
/// magnitude, so that axis can never be parallel to `n` — the cross
/// product, and therefore this seed, is never zero.
#[inline]
fn fallback_orthogonal_seed(n: [f32; 3]) -> [f32; 3] {
    let axis = if n[0].abs() <= n[1].abs() && n[0].abs() <= n[2].abs() {
        [1.0, 0.0, 0.0]
    } else if n[1].abs() <= n[2].abs() {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let mut t = [
        n[1] * axis[2] - n[2] * axis[1],
        n[2] * axis[0] - n[0] * axis[2],
        n[0] * axis[1] - n[1] * axis[0],
    ];
    normalize_inplace(&mut t);
    t
}

#[inline]
pub fn vec3_is_zero(v: &[f32; 3]) -> bool {
    v[0] * v[0] + v[1] * v[1] + v[2] * v[2] < 1e-12
}

#[inline]
pub fn normalize_inplace(v: &mut [f32; 3]) {
    let len2 = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    if len2 > 1e-12 {
        let inv = 1.0 / len2.sqrt();
        v[0] *= inv;
        v[1] *= inv;
        v[2] *= inv;
    } else {
        *v = [0.0, 0.0, 0.0];
    }
}
