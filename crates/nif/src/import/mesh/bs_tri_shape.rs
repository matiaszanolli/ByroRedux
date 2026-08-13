//! Skyrim+ `BSTriShape` mesh extraction.
//!
//! `extract_bs_tri_shape` / `_local` — packed-half-float vertex stream
//! variant used by Skyrim, FO4, FO76.

use crate::blocks::tri_shape::{BsTriShape, BsTriShapeKind};
use crate::scene::NifScene;
use crate::types::NiTransform;

use super::super::coord::{zup_matrix_to_yup_quat, zup_point_to_yup};
use super::super::ImportedMesh;
use super::*;
use byroredux_core::string::StringPool;

pub fn extract_bs_tri_shape(
    scene: &NifScene,
    shape: &BsTriShape,
    world_transform: &NiTransform,
    pool: &mut StringPool,
) -> Option<ImportedMesh> {
    // Skyrim SE / FO4 skinned meshes ship `data_size == 0` on the
    // `BsTriShape` itself — the real geometry lives on the linked
    // `NiSkinPartition` as a global packed-vertex buffer plus
    // per-partition `vertex_map` arrays. Reconstruct here before the
    // early-return so every NPC body and creature renders. See #559.
    let reconstructed = if shape.triangles.is_empty() {
        try_reconstruct_sse_geometry(scene, shape)
    } else {
        None
    };

    if shape.vertices.is_empty() && reconstructed.is_none() {
        return None;
    }
    if shape.triangles.is_empty() && reconstructed.is_none() {
        return None;
    }

    let (
        positions,
        indices,
        sse_normals,
        sse_uvs,
        sse_colors,
        sse_tangents,
        sse_normals_authored,
        sse_uvs_authored,
    ) = if let Some(geom) = reconstructed {
        (
            geom.positions,
            geom.indices,
            Some(geom.normals),
            Some(geom.uvs),
            Some(geom.colors),
            Some(geom.tangents),
            geom.normals_authored,
            geom.uvs_authored,
        )
    } else {
        let positions: Vec<[f32; 3]> = shape.vertices.iter().map(zup_point_to_yup).collect();
        let indices: Vec<u32> = shape
            .triangles
            .iter()
            .flat_map(|tri| [tri[0] as u32, tri[1] as u32, tri[2] as u32])
            .collect();
        (positions, indices, None, None, None, None, false, false)
    };

    // Keep authorship separate from the populated fallback vector: synthesized
    // tangents require real normals, not the renderer-safe `[0,1,0]`
    // placeholder below (#2363, mirrored here per #2817 — the guard at the
    // 4th tangent branch below used to test `!normals.is_empty()`, which is
    // vacuous since `normals` is unconditionally populated). For the
    // SSE-reconstructed path, `ReconstructedSseGeometry::normals_authored`
    // (threaded from `decode_sse_packed_buffer`'s `VF_NORMALS` check) is the
    // real signal — `sse_normals.is_some()` alone would still be vacuous
    // since `sse_recon.rs` fabricates its own `[0,1,0]` fallback before
    // handing the (always-populated) vector up here.
    let normals_authored = if sse_normals.is_some() {
        sse_normals_authored
    } else {
        !shape.normals.is_empty()
    };
    let normals: Vec<[f32; 3]> = if let Some(n) = sse_normals {
        n
    } else if !shape.normals.is_empty() {
        shape.normals.iter().map(zup_point_to_yup).collect()
    } else {
        vec![[0.0, 1.0, 0.0]; positions.len()]
    };

    // Same authorship split as `normals_authored`, for `VF_UVS` /
    // `shape.uvs`.
    let uvs_authored = if sse_uvs.is_some() {
        sse_uvs_authored
    } else {
        !shape.uvs.is_empty()
    };
    let uvs = if let Some(u) = sse_uvs {
        u
    } else {
        shape.uvs.clone()
    };

    // Keep all 4 components — alpha lane carries authored per-vertex
    // modulation (hair tips, eyelash strips, BSEffectShader meshes).
    // See #618.
    let colors: Vec<[f32; 4]> = if let Some(c) = sse_colors {
        c
    } else if !shape.vertex_colors.is_empty() {
        shape.vertex_colors.clone()
    } else {
        vec![[1.0, 1.0, 1.0, 1.0]; positions.len()]
    };

    // Unified material extraction — shared with the NiTriShape path.
    // BsTriShape has no legacy NiProperty chain, so direct / inherited
    // slices are empty. The shared implementation handles
    // BSLightingShaderProperty / BSEffectShaderProperty, the implicit
    // effect-shader alpha blend override (#354), Double_Sided from
    // shader_flags_2, decals from shader flags, BGSM/BGEM name
    // resolution, and the ShaderTypeData → ShaderTypeFields capture
    // (#430). See #129.
    let mat = super::super::material::extract_material_info_from_refs(
        scene,
        shape.shader_property_ref,
        shape.alpha_property_ref,
        &[],
        &[],
        pool,
    );

    let t = &world_transform.translation;
    let quat = zup_matrix_to_yup_quat(&world_transform.rotation);

    // Skinning data. BSTriShape per-vertex weights live in the packed
    // vertex buffer (VF_SKINNED), decoded at parse time (#177).
    let skin = extract_skin_bs_tri_shape(scene, shape);

    // BSTriShape carries its own bounding sphere (center + radius) on the
    // block. See #217.
    let (local_bound_center, local_bound_radius) =
        extract_local_bound(shape.center, shape.radius, &positions);

    let material = mat.into_imported_material(pool, shape.av.net.name.as_deref());

    // #795 / SK-D1-03 + #796 / SK-D1-04 — per-vertex tangents.
    //
    // Three paths (precedence order):
    //   1. SSE skin-partition reconstruction populated `sse_tangents`
    //      (NPC bodies / creatures / dragons via `try_reconstruct_sse_geometry`).
    //   2. Inline `shape.tangents` populated by the BSTriShape parser
    //      when `VF_TANGENTS` is set on the vertex descriptor.
    //   3. `VF_TANGENTS` was clear (or both upstream populates dropped
    //      vertices for malformed input) — fall back to
    //      `synthesize_tangents` mirroring the NiTriShape path so
    //      Skyrim+ content lacking authored tangents still gets
    //      runtime-computed ones instead of falling through to the
    //      shader's screen-space derivative TBN.
    //
    // All three return Y-up tangents matching `Vertex.tangent`'s contract.
    //
    // The synthesis branches share a rebuilt `triangles_for_synth`
    // because shapes whose inline `shape.triangles` was emptied by
    // SSE-reconstruction need to recover the triangle list from
    // `indices`. BSTriShape caps at u16 indices on disk so the cast
    // is safe; if the mesh ever exceeds 65k vertices the synth simply
    // produces fewer tangents and the empty result triggers the
    // shader's Path-2 fallback (no regression vs pre-fix behaviour).
    // Wrapped in a closure so the allocation only fires when at least
    // one synthesis branch reaches it — the common cases
    // (`sse_tangents.is_some()` and `shape.tangents.is_empty()` == false)
    // skip the rebuild entirely (audit AUDIT_INCREMENTAL_2026-05-22 ID-3).
    let build_triangles_for_synth = || -> Vec<[u16; 3]> {
        if shape.triangles.is_empty() {
            indices
                .chunks_exact(3)
                .filter_map(|c| {
                    if c[0] <= u16::MAX as u32 && c[1] <= u16::MAX as u32 && c[2] <= u16::MAX as u32
                    {
                        Some([c[0] as u16, c[1] as u16, c[2] as u16])
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            shape.triangles.clone()
        }
    };
    let tangents: Vec<[f32; 4]> = if let Some(t) = sse_tangents.filter(|v| !v.is_empty()) {
        t
    } else if !shape.tangents.is_empty() {
        bs_tangents_zup_to_yup(&shape.tangents)
    } else if !shape.normals.is_empty() && !shape.uvs.is_empty() {
        // Synthesize from positions + normals + UVs + triangles (raw
        // Z-up inputs — `synthesize_tangents` does the axis swap
        // internally, matching the NiTriShape path's behaviour).
        synthesize_tangents(
            &shape.vertices,
            &shape.normals,
            &shape.uvs,
            &build_triangles_for_synth(),
        )
    } else if normals_authored && uvs_authored && !positions.is_empty() {
        // #1204 — SSE-reconstructed BSTriShape whose vertex descriptor
        // lacks `VF_TANGENTS`: `shape.normals` / `shape.uvs` are empty
        // (the geometry lives in `positions` / `normals` / `uvs` from
        // `try_reconstruct_sse_geometry`, all already Y-up). Without
        // this branch every such mesh fell through to `Vec::new()`,
        // forcing Path-2 (screen-space derivative TBN) and inheriting
        // the #1104 UV-mirror handedness bug. Route to the Y-up
        // synthesis sibling so Path-1 fires instead.
        //
        // #2817 — gate on `normals_authored && uvs_authored`, not
        // `!normals.is_empty() && !uvs.is_empty()`: both `normals` and
        // `uvs` are unconditionally populated a few lines up (falling back
        // to a fabricated `[0,1,0]` placeholder / `sse_recon.rs`'s own
        // `[0,0]` fallback), so the old guard reduced to
        // `!positions.is_empty()`, already tested above. An SSE buffer
        // with neither `VF_NORMALS` nor `VF_UVS` would otherwise reach
        // here with both inputs fabricated and still synthesize a
        // "tangent" basis from data that was never authored.
        synthesize_tangents_yup(&positions, &normals, &uvs, &build_triangles_for_synth())
    } else {
        Vec::new()
    };

    Some(ImportedMesh {
        positions,
        colors,
        normals,
        tangents,
        uvs,
        indices,
        translation: zup_point_to_yup(t),
        rotation: quat,
        scale: world_transform.scale,
        material,
        name: shape.av.net.name.clone(),
        parent_node: None,
        skin,
        local_bound_center,
        local_bound_radius,
        flags: shape.av.flags,
        bs_lod_cutoffs: match &shape.kind {
            BsTriShapeKind::MeshLOD { lod0, lod1, lod2 } => Some([*lod0, *lod1, *lod2]),
            _ => None,
        },
        bs_sub_index: match &shape.kind {
            BsTriShapeKind::SubIndex(data) => Some((**data).clone()),
            _ => None,
        },
        bs_geometry_lod_slot: None,
        billboard_mode: None,
    })
}

/// Extract a BsTriShape with local transform (for hierarchical import).
pub fn extract_bs_tri_shape_local(
    scene: &NifScene,
    shape: &BsTriShape,
    pool: &mut StringPool,
) -> Option<ImportedMesh> {
    extract_bs_tri_shape(scene, shape, &shape.av.transform, pool)
}
