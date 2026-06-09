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

    // Stage 2 (`feedback_format_translation.md`) — derive PBR
    // (metalness, roughness) at import time. BGSM merge downstream
    // overwrites both for BGSM-resolved Skyrim+/FO4 meshes; legacy
    // inline-shader BSLightingShaderProperty meshes keep these.
    let legacy_pbr = mat.classify_legacy_pbr(pool);

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
    } else if !normals.is_empty() && !uvs.is_empty() && !positions.is_empty() {
        // #1204 — SSE-reconstructed BSTriShape whose vertex descriptor
        // lacks `VF_TANGENTS`: `shape.normals` / `shape.uvs` are empty
        // (the geometry lives in `positions` / `normals` / `uvs` from
        // `try_reconstruct_sse_geometry`, all already Y-up). Without
        // this branch every such mesh fell through to `Vec::new()`,
        // forcing Path-2 (screen-space derivative TBN) and inheriting
        // the #1104 UV-mirror handedness bug. Route to the Y-up
        // synthesis sibling so Path-1 fires instead.
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
        // #1076 / FO4-D6-002 — NIF shader-texture-set slots
        // don't expose these; populated downstream by
        // `merge_bgsm_into_mesh` from BGSM/BGEM v>2.
        specular_map: None,
        lighting_map: None,
        flow_map: None,
        wrinkle_map: None,
        // #1077 / FO4-D6-003 — BGSM-only shader flags; NIF
        // shader-texture-set doesn't surface these. Populated
        // downstream by `merge_bgsm_into_mesh` from BgsmFile.
        is_pbr: false,
        has_translucency: false,
        model_space_normals: false,
        from_bgsm: false,
        bgem_glass: false,
        // Stage 2 — legacy PBR translation. BGSM merge overwrites for
        // BGSM-resolved Skyrim+/FO4 meshes; non-BGSM BSLightingShader-
        // Property paths keep these classifier-derived values.
        metalness_override: Some(legacy_pbr.metalness),
        roughness_override: Some(legacy_pbr.roughness),
        // #1147 Phase 2b — BGSM v>=8 translucency suite.
        translucency_subsurface_color: [0.0; 3],
        translucency_transmissive_scale: 0.0,
        translucency_turbulence: 0.0,
        translucency_thick_object: false,
        translucency_mix_albedo: false,
        parallax_max_passes: mat.parallax_max_passes,
        parallax_height_scale: mat.parallax_height_scale,
        vertex_color_mode: mat.vertex_color_mode as u8,
        // #610 — diffuse-slot `TexClampMode` from BSShader /
        // BSEffectShader (BsTriShape's effective clamp source).
        texture_clamp_mode: mat.texture_clamp_mode,
        emissive_color: mat.emissive_color,
        emissive_mult: mat.emissive_mult,
        emissive_source: mat.emissive_source,
        specular_color: mat.specular_color,
        diffuse_color: mat.diffuse_color,
        ambient_color: mat.ambient_color,
        specular_strength: mat.specular_strength,
        glossiness: mat.glossiness,
        // #1241 — BSLSP PBR scalars. BsTriShape is the Skyrim+ / FO4 /
        // FO76 path so these carry authored values per BSVER band
        // (Skyrim: lighting_effect_1/2; FO4 130–139: subsurface_rolloff
        // + rim/back power; FO4+ 130+: grayscale + fresnel + refraction).
        refraction_strength: mat.refraction_strength,
        lighting_effect_1: mat.lighting_effect_1,
        lighting_effect_2: mat.lighting_effect_2,
        subsurface_rolloff: mat.subsurface_rolloff,
        rimlight_power: mat.rimlight_power,
        backlight_power: mat.backlight_power,
        grayscale_to_palette_scale: mat.grayscale_to_palette_scale,
        // BGSM greyscale LUT path is resolved later by `merge_bgsm_into_mesh`
        // (the NIF extractor has no BGSM file in scope here). See #1353.
        bgsm_greyscale_lut_path: None,
        fresnel_power: mat.fresnel_power,
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
        // #1207 / NIF-DIM4-07 — surface FO4 BSLODTriShape distant-LOD
        // triangle-count cutoffs (parser already captured them via
        // `BsTriShapeKind::LOD`). Future M35 LOD selector will consult
        // these. `None` on every non-LOD variant.
        bs_lod_cutoffs: match &shape.kind {
            BsTriShapeKind::LOD { lod0, lod1, lod2 } => Some([*lod0, *lod1, *lod2]),
            _ => None,
        },
        // #1206 / NIF-DIM4-06 — surface BSSubIndexTriShape segmentation
        // payload (parser already captured the full segments table +
        // shared SSF metadata via `BsTriShapeKind::SubIndex`). Future
        // dismemberment / body-part-segmentation system will consult
        // this. `None` on every non-SubIndex variant.
        bs_sub_index: match &shape.kind {
            BsTriShapeKind::SubIndex(data) => Some((**data).clone()),
            _ => None,
        },
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
