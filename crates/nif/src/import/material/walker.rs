//! Items extracted from ../mod.rs (refactor stage C).
//!
//! Lead types: extract_vertex_colors, extract_material_info, extract_material_info_from_refs.
//!
//! `extract_material_info_from_refs` (#2059) is a thin orchestrator over
//! `dedicated_shader` (Skyrim+ `shader_property_ref` / `alpha_property_ref`)
//! and `legacy_properties` (FO3/FNV/Oblivion `NiProperty` chain) — see those
//! sibling modules for the actual per-property extraction logic.

use super::dedicated_shader;
use super::legacy_properties;
use super::*;
use byroredux_core::string::StringPool;

/// Extract vertex colors using a pre-computed `MaterialInfo`.
///
/// Reads `mat.vertex_color_mode` and `mat.diffuse_color` directly instead
/// of re-walking the property list. Pre-#438 this function ignored its
/// `_mat` parameter and re-scanned the shape + inherited properties twice
/// (once for vertex-color mode, once for diffuse fallback), costing 3×
/// the property-list work per NiTriShape on top of the initial
/// `extract_material_info` scan at the caller.
pub(crate) fn extract_vertex_colors(
    _scene: &NifScene,
    _shape: &NiTriShape,
    data: &GeomData,
    _inherited_props: &[BlockRef],
    mat: &MaterialInfo,
) -> Vec<[f32; 4]> {
    let num_verts = data.vertices.len();

    // O4-03 / #695 — `Emissive` and `AmbientDiffuse` both surface the
    // authored per-vertex colors; `Ignore` falls back to the per-mesh
    // diffuse constant.
    //
    //   * `AmbientDiffuse` (default): per-vertex colors modulate albedo
    //     in the shader's `texColor.rgb * fragColor` line.
    //   * `Emissive`: per-vertex colors drive self-illumination —
    //     flickering torches, glowing signs, baked emissive cards. The
    //     fragment shader treats `fragColor` as the per-vertex emissive
    //     payload (gated by `MAT_FLAG_VERTEX_COLOR_EMISSIVE` on
    //     `GpuMaterial.materialFlags`) and skips the `albedo *=
    //     fragColor` modulation. Pre-fix this branch fell through to the
    //     diffuse-color fallback below and silently dropped the
    //     authored emissive payload, leaving torches and signs flat-lit.
    //   * `Ignore`: vertex colors disabled by the property; treat as if
    //     the data block had none.
    //
    // The alpha lane is preserved for both surfacing paths — authored
    // per-vertex modulation on hair-tip cards, eyelash strips, and
    // BSEffectShader meshes is the source of truth for those surfaces.
    // See #618.
    if surfaces_authored_vertex_colors(mat.vertex_color_mode, !data.vertex_colors.is_empty()) {
        return data.vertex_colors.to_vec();
    }

    let d = mat.diffuse_color;
    vec![[d[0], d[1], d[2], 1.0]; num_verts]
}

/// Decision predicate for [`extract_vertex_colors`]: should the authored
/// per-vertex color array reach the renderer (vs. the per-material
/// diffuse fallback)? Pulled out as a free function so the gating logic
/// is testable without fabricating a [`NifScene`] / [`NiTriShape`] /
/// [`GeomData`] tuple — the previous embedding inlined the rule and
/// silently dropped the `Emissive` payload (#695).
pub(super) fn surfaces_authored_vertex_colors(
    mode: VertexColorMode,
    has_authored_colors: bool,
) -> bool {
    has_authored_colors
        && matches!(
            mode,
            VertexColorMode::AmbientDiffuse | VertexColorMode::Emissive
        )
}

/// Extract all material properties from a NiTriShape in a single pass.
///
/// `inherited_props` carries property BlockRefs accumulated from parent
/// NiNodes during the scene graph walk. Gamebryo propagates properties
/// down the hierarchy — child shapes inherit parent properties unless
/// they override them with their own. Shape-level properties take
/// priority; inherited properties fill in any gaps. See #208.
pub(crate) fn extract_material_info(
    scene: &NifScene,
    shape: &NiTriShape,
    inherited_props: &[BlockRef],
    pool: &mut StringPool,
) -> MaterialInfo {
    extract_material_info_from_refs(
        scene,
        shape.shader_property_ref,
        shape.alpha_property_ref,
        &shape.av.properties,
        inherited_props,
        pool,
    )
}

/// Block-ref-parameterised core of [`extract_material_info`].
///
/// Both the `NiTriShape` path (via the thin wrapper above) and the
/// `BsTriShape` path share this implementation so parity drift
/// between them — NIF-404 / NIF-403 — can't re-emerge. BsTriShape
/// passes empty slices for `direct_properties` and `inherited_props`
/// because Skyrim+ geometry binds properties via the dedicated
/// `shader_property_ref` / `alpha_property_ref` fields rather than
/// the legacy NiProperty chain. See #129.
pub(crate) fn extract_material_info_from_refs(
    scene: &NifScene,
    shader_property_ref: BlockRef,
    alpha_property_ref: BlockRef,
    direct_properties: &[BlockRef],
    inherited_props: &[BlockRef],
    pool: &mut StringPool,
) -> MaterialInfo {
    let mut info = MaterialInfo::default();

    // Skyrim+ dedicated refs (#2059 — split into `dedicated_shader.rs`).
    // Alpha MUST run before the shader-property block so the BSEffectShader
    // implicit-blend gate (#1202) can consult `alpha_property_consumed`.
    dedicated_shader::apply_dedicated_alpha_property(scene, alpha_property_ref, &mut info);
    dedicated_shader::apply_dedicated_shader_property(scene, shader_property_ref, pool, &mut info);

    // FO3/FNV/Oblivion legacy NiProperty chain (#2059 — split into
    // `legacy_properties.rs`). Empty slices for BsTriShape (Skyrim+ binds
    // via the dedicated refs above only).
    legacy_properties::apply_legacy_property_chain(
        scene,
        direct_properties,
        inherited_props,
        pool,
        &mut info,
    );

    // Zero out specular strength **and color** when the property is
    // disabled. We do this once at the end so later code (pipeline
    // selection, draw command population) doesn't need to know about
    // the flag.
    //
    // #696 — clearing `specular_strength` alone is insufficient on
    // glass-classified meshes. The IOR glass branch in
    // `triangle.frag:1004` does `specStrength = max(specStrength,
    // 3.0)`, which silently re-promotes the spec term on every glass
    // surface even when the NIF said `NiSpecularProperty { flags: 0 }`.
    // The downstream BRDF multiplies (`specStrength * specColor` at
    // lines 1293 + 1396) then gate on the *color* — zeroing it here
    // collapses both glass-IOR and standard paths to zero spec
    // contribution as the original engine would.
    if !info.specular_enabled {
        info.specular_strength = 0.0;
        info.specular_color = [0.0, 0.0, 0.0];
    }

    info
}
