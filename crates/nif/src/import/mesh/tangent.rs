//! Tangent-space extraction + synthesis.
//!
//! Extra-data tangent capture, Mikkelsen-style synthesis fallback, and
//! Zup→Yup quaternion fix-up for BS tangent payloads.



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
        for i in 0..num_verts {
            let bethesda_t_off = i * 12;
            let bethesda_b_off = bethesda_bitangent_offset + i * 12;
            // Read Vector3 (3 × f32 LE).
            let bethesda_tx = read_f32_le_at(blob, bethesda_t_off);
            let bethesda_ty = read_f32_le_at(blob, bethesda_t_off + 4);
            let bethesda_tz = read_f32_le_at(blob, bethesda_t_off + 8);
            let bethesda_bx = read_f32_le_at(blob, bethesda_b_off);
            let bethesda_by = read_f32_le_at(blob, bethesda_b_off + 4);
            let bethesda_bz = read_f32_le_at(blob, bethesda_b_off + 8);

            // Z-up → Y-up basis change applied to both direction
            // vectors: same `(x, y, z) → (x, z, -y)` swap used for
            // positions / normals throughout import.
            //
            // After the swap, our `t_yup` is ∂P/∂U (read from
            // Bethesda's bitangent half) and `b_yup` is ∂P/∂V
            // (read from Bethesda's tangent half).
            let t_yup = [bethesda_bx, bethesda_bz, -bethesda_by];
            let b_yup = [bethesda_tx, bethesda_tz, -bethesda_ty];

            // Normal in Y-up — use the matching vertex normal.
            let n_zup = normals_zup[i];
            let n_yup = [n_zup.x, n_zup.z, -n_zup.y];

            // Bitangent sign: sign(dot(B, cross(N, T))). With T = ∂P/∂U
            // and B = ∂P/∂V on a standard right-handed UV winding,
            // `cross(N, T) ≈ ∂P/∂V` so `dot(B, cross_nt) > 0` and the
            // sign lands at +1 — the textbook case. UV-mirrored shells
            // produce `< 0`, flipping the shader's bitangent so the
            // tangent-space normal sample stays consistent across the
            // mirror seam. Zero (degenerate) defaults to +1.
            let cross_nt = [
                n_yup[1] * t_yup[2] - n_yup[2] * t_yup[1],
                n_yup[2] * t_yup[0] - n_yup[0] * t_yup[2],
                n_yup[0] * t_yup[1] - n_yup[1] * t_yup[0],
            ];
            let dot_b_cross =
                b_yup[0] * cross_nt[0] + b_yup[1] * cross_nt[1] + b_yup[2] * cross_nt[2];
            let sign = if dot_b_cross < 0.0 { -1.0 } else { 1.0 };

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

#[inline]
pub fn read_f32_le_at(blob: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes([
        blob[offset],
        blob[offset + 1],
        blob[offset + 2],
        blob[offset + 3],
    ])
}

/// Synthesize per-vertex tangents from positions + normals + UVs +
/// triangles when the source NIF doesn't ship authored tangents.
///
/// Most Oblivion / FO3 / FNV interior content stores no
/// `NiBinaryExtraData("Tangent space ...")` blob — Bethesda's runtime
/// computes tangents at load time using a per-triangle accumulator
/// (see `nifly::NiTriShapeData::CalcTangentSpace`). This function
/// ports nifly's algorithm to produce the same per-vertex
/// tangent + bitangent vectors and packs them into the
/// `[Tx, Ty, Tz, bitangent_sign]` shape the shader expects.
///
/// Inputs:
///   - `vertices` / `normals`: Z-up world-space, length == num_verts
///   - `uvs`: per-vertex 2D coordinates, length == num_verts
///   - `triangles`: u16 indices, 3 per triangle
///
/// Output: `Vec<[f32; 4]>` length == num_verts. Empty when any input
/// is missing or vertex counts don't line up; caller falls back to
/// the shader's screen-space derivative TBN path.
///
/// Algorithm (per nifly Geometry.cpp:2026-2106):
///   1. For each triangle, compute sdir (∂P/∂U) and tdir (∂P/∂V)
///      from position + UV deltas, with sign correction for flipped UVs.
///   2. Accumulate per-vertex sdir and tdir (averaged across adjacent
///      triangles).
///   3. Per vertex: orthogonalize T (= sdir) against N (Gram-Schmidt),
///      then B (= tdir) against both N and T. Degenerate cases (zero
///      sdir or tdir) fall back to a permutation of the normal.
///   4. Derive bitangent_sign as `sign(dot(B, cross(N, T)))` so the
///      shader's `sign × cross(N, T)` reconstruction matches the
///      authored handedness.
///
/// Convention (#786): unlike nifly which preserves Bethesda's swapped
/// labelling (`bitangents = sdir`, `tangents = tdir`), we store
/// `tangent = sdir = ∂P/∂U` so the value in `Vertex.tangent.xyz`
/// matches the textbook (Lengyel) convention the renderer's
/// `mat3(T, B, N) * tangentNormal` evaluates against. nifly's swap
/// existed to round-trip the on-disk format losslessly; we don't
/// write NIFs back, so we unswap on read for shader consistency.
///
/// Z-up → Y-up conversion is applied to the final tangent + the N used
/// for sign derivation, mirroring the authored-decode path so both
/// produce vectors in the same coordinate space.
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

    // Per-vertex accumulators for the U and V axis derivatives.
    // `tan_u[i]` accumulates ∂P/∂U (sdir), `tan_v[i]` accumulates
    // ∂P/∂V (tdir). nifly's `CalcTangentSpace` swaps the labels at
    // output (`bitangents = tan1 = ∂P/∂U`, `tangents = tan2 = ∂P/∂V`)
    // because Bethesda's NIF on-disk layout names them that way. Our
    // shader's Path 1 (#783) builds `mat3(T, B, N)` against the
    // Lengyel/textbook convention (T = ∂P/∂U), and Path 2 (screen-
    // space derivative fallback) does the same. Pre-#786 we ported
    // nifly's swap verbatim and stored ∂P/∂V in `Vertex.tangent.xyz`,
    // mismatching the shader and producing the chrome regression on
    // FNV `GSDocMitchellHouse` (R-N2 / #786). Confirmed by
    // `DBG_VIZ_TANGENT` reading green on chrome fragments — Path 1
    // was firing with a 90°-rotated TBN basis.
    let mut tan_u = vec![[0.0f32; 3]; n];
    let mut tan_v = vec![[0.0f32; 3]; n];

    for tri in triangles {
        let i1 = tri[0] as usize;
        let i2 = tri[1] as usize;
        let i3 = tri[2] as usize;
        if i1 >= n || i2 >= n || i3 >= n {
            continue;
        }

        let v1 = vertices[i1];
        let v2 = vertices[i2];
        let v3 = vertices[i3];
        let w1 = uvs[i1];
        let w2 = uvs[i2];
        let w3 = uvs[i3];

        let x1 = v2.x - v1.x;
        let x2 = v3.x - v1.x;
        let y1 = v2.y - v1.y;
        let y2 = v3.y - v1.y;
        let z1 = v2.z - v1.z;
        let z2 = v3.z - v1.z;

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

    // Per-vertex finalize: Gram-Schmidt against N, then derive
    // bitangent sign for the shader's reconstruction.
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let n_zup = normals_zup[i];
        let n_yup = [n_zup.x, n_zup.z, -n_zup.y];

        // #786 — store `tangent = ∂P/∂U` (standard Lengyel
        // convention), inverting nifly's Bethesda-convention swap
        // (`bitangents = tan1 = ∂P/∂U`). Our shader's `mat3(T, B, N)
        // * tangentNormal` evaluates to `T*tn.x + B*tn.y + N*tn.z`
        // and the BC5 normal map authors `tn.x` along the texture
        // U axis; for that pairing to be correct, the value sitting
        // in `vertexTangent.xyz` must be ∂P/∂U.
        let tangent_zup = tan_u[i];
        let bitangent_zup = tan_v[i];

        let (tangent_yup, bitangent_yup) =
            if vec3_is_zero(&tangent_zup) || vec3_is_zero(&bitangent_zup) {
                // Degenerate fallback (nifly: permute N components).
                let t_z = [n_zup.y, n_zup.z, n_zup.x];
                let t_y = [t_z[0], t_z[2], -t_z[1]];
                let b_y = [
                    n_yup[1] * t_y[2] - n_yup[2] * t_y[1],
                    n_yup[2] * t_y[0] - n_yup[0] * t_y[2],
                    n_yup[0] * t_y[1] - n_yup[1] * t_y[0],
                ];
                (t_y, b_y)
            } else {
                // Convert Z-up → Y-up first so the orthogonalization
                // happens in the same coordinate space as the shader
                // reads (consistent with the authored-decode path).
                let mut t_yup = [tangent_zup[0], tangent_zup[2], -tangent_zup[1]];
                let mut b_yup = [bitangent_zup[0], bitangent_zup[2], -bitangent_zup[1]];

                normalize_inplace(&mut t_yup);
                // T = T - N * dot(N, T)
                let dot_nt = n_yup[0] * t_yup[0] + n_yup[1] * t_yup[1] + n_yup[2] * t_yup[2];
                t_yup = [
                    t_yup[0] - n_yup[0] * dot_nt,
                    t_yup[1] - n_yup[1] * dot_nt,
                    t_yup[2] - n_yup[2] * dot_nt,
                ];
                normalize_inplace(&mut t_yup);

                normalize_inplace(&mut b_yup);
                // B = B - N * dot(N, B)
                let dot_nb = n_yup[0] * b_yup[0] + n_yup[1] * b_yup[1] + n_yup[2] * b_yup[2];
                b_yup = [
                    b_yup[0] - n_yup[0] * dot_nb,
                    b_yup[1] - n_yup[1] * dot_nb,
                    b_yup[2] - n_yup[2] * dot_nb,
                ];
                // B = B - T * dot(T, B)
                let dot_tb = t_yup[0] * b_yup[0] + t_yup[1] * b_yup[1] + t_yup[2] * b_yup[2];
                b_yup = [
                    b_yup[0] - t_yup[0] * dot_tb,
                    b_yup[1] - t_yup[1] * dot_tb,
                    b_yup[2] - t_yup[2] * dot_tb,
                ];
                normalize_inplace(&mut b_yup);
                (t_yup, b_yup)
            };

        // Bitangent sign: sign(dot(B, cross(N, T))).
        let cross_nt = [
            n_yup[1] * tangent_yup[2] - n_yup[2] * tangent_yup[1],
            n_yup[2] * tangent_yup[0] - n_yup[0] * tangent_yup[2],
            n_yup[0] * tangent_yup[1] - n_yup[1] * tangent_yup[0],
        ];
        let dot_b_cross = bitangent_yup[0] * cross_nt[0]
            + bitangent_yup[1] * cross_nt[1]
            + bitangent_yup[2] * cross_nt[2];
        let sign = if dot_b_cross < 0.0 { -1.0 } else { 1.0 };

        out.push([tangent_yup[0], tangent_yup[1], tangent_yup[2], sign]);
    }
    out
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
    zup.iter().map(|t| [t[0], t[2], -t[1], t[3]]).collect()
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

