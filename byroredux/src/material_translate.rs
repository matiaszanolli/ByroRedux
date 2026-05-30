//! NIFAL (NIF Abstraction Layer) — the **material** translation boundary.
//!
//! [`translate_material`] is the **single** site that turns a raw,
//! per-game [`ImportedMesh`] (with BGSM/BGEM already merged into it by
//! [`crate::asset_provider`]'s `merge_bgsm_into_mesh`) into the engine's
//! canonical [`Material`] ECS component. Every consumer downstream of
//! `Material` reads game-agnostic, fully-resolved data — the per-game
//! quirks are resolved here, exactly once. This is the material slice of
//! NIFAL, the engine's cross-game canonical translation tier.
//!
//! Before this module existed, the `Material` struct literal was built
//! verbatim at two sites — [`crate::cell_loader`]'s `spawn` (REFR cell
//! placement) and [`crate::scene`]'s `nif_loader` (loose-NIF load) —
//! ~110 near-identical lines each, kept in sync by hand. That
//! duplication was itself a translation leak: a field added to one site
//! and not the other silently diverged the two load paths. Both sites
//! now call this boundary.
//!
//! Architecture: see `docs/engine/nifal.md`. The canonical tier is the
//! ECS `Material` component itself (it already lives in `byroredux_core`,
//! is game-agnostic, and is what the renderer reads) — this boundary is
//! the `translate()` step, not a new type.

use byroredux_core::ecs::components::material::{EffectFalloff, Material};
use byroredux_nif::import::ImportedMesh;

/// Spawn-resolved texture-slot paths the caller computes before
/// translation: the REFR XATO/XTNM/XTXR overlay (cell loader) has
/// already been applied and each populated [`byroredux_core::string::
/// FixedString`] handle resolved to an owned `String`.
///
/// Only the slots the canonical [`Material`] carries are listed here;
/// the parallax / env / env_mask slots become separate
/// `*MapHandle` components and are resolved by the caller directly off
/// its own `eff_*` / `owned_*` locals.
pub(crate) struct ResolvedPaths {
    pub texture_path: Option<String>,
    pub material_path: Option<String>,
    pub normal_map: Option<String>,
    pub glow_map: Option<String>,
    pub detail_map: Option<String>,
    pub gloss_map: Option<String>,
    pub dark_map: Option<String>,
    pub greyscale_texture: Option<String>,
}

/// Translate a raw [`ImportedMesh`] + caller-resolved paths into the
/// canonical [`Material`] component.
///
/// Resolution performed here (the "single source of truth"):
///   - all material scalars / colors / flags copied across;
///   - `effect_shader_flags` packed as the union of the BSEffectShader
///     SLSF bits ([`crate::cell_loader::pack_effect_shader_flags`]), the
///     BGSM v>2 PBR/translucency/model-space-normals bits
///     ([`crate::cell_loader::pack_bgsm_material_flags`]), and any
///     `extra_material_flags` the caller supplies (the cell loader's
///     REFR-overlay model-space-normals bit; `0` for loose-NIF loads);
///   - PBR scalars resolved: for NIF-imported content the keyword
///     classifier already ran at import time (`classify_legacy_pbr` in the
///     NIF mesh extractors) and populated `mesh.metalness_override/
///     roughness_override` as `Some(…)`, so [`Material::resolve_pbr`] here
///     only clamps — its classifier arm is a sentinel-backstop (only fires
///     when the override is `NaN`, i.e. for future non-NIF paths). BGSM/BGEM
///     content also arrives pre-classified as `Some`. Either way every
///     material exits with explicit `(metalness, roughness)` scalars; no
///     render-time fallback. `feedback_format_translation.md` Stage 1.
///     (Structure: classify-at-import + clamp-at-translate. See #1346.)
///   - glass classified once, alpha-aware
///     ([`crate::helpers::classify_glass_into_material`]), after the PBR
///     resolve so the forced glass roughness wins.
pub(crate) fn translate_material(
    mesh: &ImportedMesh,
    paths: ResolvedPaths,
    extra_material_flags: u32,
) -> Material {
    let mut material = Material {
        emissive_color: mesh.emissive_color,
        emissive_mult: mesh.emissive_mult,
        emissive_source: mesh.emissive_source,
        specular_color: mesh.specular_color,
        specular_strength: mesh.specular_strength,
        diffuse_color: mesh.diffuse_color,
        ambient_color: mesh.ambient_color,
        glossiness: mesh.glossiness,
        uv_offset: mesh.uv_offset,
        uv_scale: mesh.uv_scale,
        alpha: mesh.mat_alpha,
        env_map_scale: mesh.env_map_scale,
        normal_map: paths.normal_map,
        texture_path: paths.texture_path.clone(),
        material_path: paths.material_path,
        glow_map: paths.glow_map,
        detail_map: paths.detail_map,
        gloss_map: paths.gloss_map,
        dark_map: paths.dark_map,
        vertex_color_mode: mesh.vertex_color_mode,
        alpha_test: mesh.alpha_test,
        alpha_threshold: mesh.alpha_threshold,
        alpha_test_func: mesh.alpha_test_func,
        material_kind: mesh.material_kind,
        wireframe: mesh.wireframe,
        flat_shading: mesh.flat_shading,
        z_test: mesh.z_test,
        z_write: mesh.z_write,
        z_function: mesh.z_function,
        shader_type_fields: if mesh.shader_type_fields.is_empty() {
            None
        } else {
            Some(Box::new(mesh.shader_type_fields.to_core()))
        },
        // #620 / #451 — BSEffectShaderProperty falloff cone (Skyrim+) OR
        // BSShaderNoLightingProperty falloff cone (FO3/FNV sibling).
        // BSShaderNoLighting fills `soft_falloff_depth = 0.0` (no
        // soft-depth field on that block).
        effect_falloff: mesh
            .effect_shader
            .as_ref()
            .map(|es| EffectFalloff {
                start_angle: es.falloff_start_angle,
                stop_angle: es.falloff_stop_angle,
                start_opacity: es.falloff_start_opacity,
                stop_opacity: es.falloff_stop_opacity,
                soft_falloff_depth: es.soft_falloff_depth,
            })
            .or_else(|| {
                mesh.no_lighting_falloff.as_ref().map(|nl| EffectFalloff {
                    start_angle: nl.start_angle,
                    stop_angle: nl.stop_angle,
                    start_opacity: nl.start_opacity,
                    stop_opacity: nl.stop_opacity,
                    soft_falloff_depth: 0.0,
                })
            }),
        // #890 Stage 2 / #1077 Phase 2a — union of the BSEffect SLSF
        // bits, the BGSM v>2 bits, and the caller's extra bits (REFR
        // overlay model-space-normals on the cell path). All three
        // contributors target the same `material_flag::*` layout so a
        // single OR yields the word `GpuMaterial.material_flags` consumes.
        effect_shader_flags: crate::cell_loader::pack_effect_shader_flags(
            mesh.effect_shader.as_ref(),
        ) | crate::cell_loader::pack_bgsm_material_flags(mesh)
            | extra_material_flags,
        // #1147 Phase 2b — BGSM v>=8 translucency suite; only meaningful
        // when `pack_bgsm_material_flags` set MAT_FLAG_BGSM_TRANSLUCENCY.
        translucency_subsurface_color: mesh.translucency_subsurface_color,
        translucency_transmissive_scale: mesh.translucency_transmissive_scale,
        translucency_turbulence: mesh.translucency_turbulence,
        // #890 Stage 2c — BSEffectShaderProperty greyscale LUT path;
        // resolved to a bindless handle at draw-build time.
        greyscale_texture: paths.greyscale_texture,
        // Canonical PBR — seed authored BGSM/BGEM scalars
        // (`merge_bgsm_into_mesh`) or a NaN sentinel for legacy
        // inline-shader content; `resolve_pbr` below fills any sentinel
        // from the keyword classifier and clamps to the renderer ranges.
        metalness: mesh.metalness_override.unwrap_or(f32::NAN),
        roughness: mesh.roughness_override.unwrap_or(f32::NAN),
    };
    material.resolve_pbr();
    crate::helpers::classify_glass_into_material(
        &mut material,
        mesh.name.as_deref(),
        paths.texture_path.as_deref(),
        mesh.has_alpha,
        mesh.is_decal || mesh.alpha_test,
        mesh.bgem_glass,
    );
    material
}
