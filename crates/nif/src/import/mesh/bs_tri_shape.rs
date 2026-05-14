//! Skyrim+ `BSTriShape` mesh extraction.
//!
//! `extract_bs_tri_shape` / `_local` — packed-half-float vertex stream
//! variant used by Skyrim, FO4, FO76.



use crate::blocks::tri_shape::BsTriShape;
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
    let reconstructed = if shape.vertices.is_empty() && shape.triangles.is_empty() {
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

    let (positions, indices, sse_normals, sse_uvs, sse_colors, sse_tangents) =
        if let Some(geom) = reconstructed {
            (
                geom.positions,
                geom.indices,
                Some(geom.normals),
                Some(geom.uvs),
                Some(geom.colors),
                Some(geom.tangents),
            )
        } else {
            let positions: Vec<[f32; 3]> = shape.vertices.iter().map(zup_point_to_yup).collect();
            let indices: Vec<u32> = shape
                .triangles
                .iter()
                .flat_map(|tri| [tri[0] as u32, tri[1] as u32, tri[2] as u32])
                .collect();
            (positions, indices, None, None, None, None)
        };

    let normals: Vec<[f32; 3]> = if let Some(n) = sse_normals {
        n
    } else if !shape.normals.is_empty() {
        shape.normals.iter().map(zup_point_to_yup).collect()
    } else {
        vec![[0.0, 1.0, 0.0]; positions.len()]
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

    // #430 — capture ShaderTypeFields before the `mat` move.
    let shader_type_fields = mat.shader_type_fields();

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
    let tangents: Vec<[f32; 4]> = if let Some(t) = sse_tangents.filter(|v| !v.is_empty()) {
        t
    } else if !shape.tangents.is_empty() {
        bs_tangents_zup_to_yup(&shape.tangents)
    } else if !shape.normals.is_empty() && !shape.uvs.is_empty() {
        // Synthesize from positions + normals + UVs + triangles (all
        // raw Z-up — `synthesize_tangents` does the axis swap
        // internally, matching the NiTriShape path's behaviour).
        let triangles_for_synth: Vec<[u16; 3]> = if shape.triangles.is_empty() {
            // SSE-reconstructed mesh whose inline triangle array is
            // empty — rebuild from `indices`. BSTriShape caps at u16
            // indices on disk so the cast is safe; if the mesh ever
            // exceeds 65k vertices the synth simply produces fewer
            // tangents and the empty result triggers the shader's
            // Path-2 fallback (no regression vs pre-fix behaviour).
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
        };
        synthesize_tangents(
            &shape.vertices,
            &shape.normals,
            &shape.uvs,
            &triangles_for_synth,
        )
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
        name: shape.av.net.name.clone(),
        texture_path: mat.texture_path,
        material_path: mat.material_path,
        has_alpha: mat.alpha_blend,
        src_blend_mode: mat.src_blend_mode,
        dst_blend_mode: mat.dst_blend_mode,
        alpha_test: mat.alpha_test,
        alpha_threshold: mat.alpha_threshold,
        alpha_test_func: mat.alpha_test_func,
        two_sided: mat.two_sided,
        is_decal: mat.is_decal,
        normal_map: mat.normal_map,
        // BsTriShape (Skyrim+) routes all texture slots through
        // BSShaderTextureSet — the legacy NiTexturingProperty
        // glow/detail/gloss/dark slots don't apply. Skyrim+ glow is in
        // BSShaderTextureSet slot 2 (`mat.glow_map`), which the shared
        // extractor already reads.
        glow_map: mat.glow_map,
        detail_map: mat.detail_map,
        gloss_map: mat.gloss_map,
        dark_map: mat.dark_map,
        parallax_map: mat.parallax_map,
        env_map: mat.env_map,
        env_mask: mat.env_mask,
        parallax_max_passes: mat.parallax_max_passes,
        parallax_height_scale: mat.parallax_height_scale,
        vertex_color_mode: mat.vertex_color_mode as u8,
        // #610 — diffuse-slot `TexClampMode` from BSShader /
        // BSEffectShader (BsTriShape's effective clamp source).
        texture_clamp_mode: mat.texture_clamp_mode,
        emissive_color: mat.emissive_color,
        emissive_mult: mat.emissive_mult,
        specular_color: mat.specular_color,
        diffuse_color: mat.diffuse_color,
        ambient_color: mat.ambient_color,
        specular_strength: mat.specular_strength,
        glossiness: mat.glossiness,
        uv_offset: mat.uv_offset,
        uv_scale: mat.uv_scale,
        mat_alpha: mat.alpha,
        env_map_scale: mat.env_map_scale,
        parent_node: None,
        skin,
        // BSTriShape (Skyrim+) has no NiZBufferProperty binding; the
        // shared extractor preserves Gamebryo runtime defaults
        // (z_test+write on, LESSEQUAL) when no NiZBufferProperty is
        // found — which is always on this path.
        z_test: mat.z_test,
        z_write: mat.z_write,
        z_function: mat.z_function,
        local_bound_center,
        local_bound_radius,
        effect_shader: mat.effect_shader,
        material_kind: mat.material_kind,
        shader_type_fields,
        // BSShaderNoLightingProperty is an FO3/FNV-era property and
        // doesn't bind to BsTriShape (Skyrim+); the shared extractor
        // won't populate it here. See #451.
        no_lighting_falloff: mat.no_lighting_falloff,
        wireframe: mat.wireframe,
        flat_shading: mat.flat_shading,
        flags: shape.av.flags,
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

