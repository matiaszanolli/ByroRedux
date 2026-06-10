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

use crate::components::{ExtraTextureMaps, NormalMapHandle};
use byroredux_core::ecs::components::material::{EffectFalloff, Material};
use byroredux_core::ecs::{EntityId, World};
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

/// High bit OR'd into the gloss texture slot to tell the fragment shader
/// "sample the per-pixel spec/smoothness mask from the NORMAL map's alpha
/// channel" — the Skyrim/Gamebryo normal-alpha-as-spec convention. The
/// gloss slot then points at the normal map's bindless handle. This bit is
/// applied per-draw in `render::static_meshes` because it is a transient
/// texture-binding instruction, not canonical material state; the matching
/// *roughness* scalar is resolved once at spawn by
/// [`resolve_normal_alpha_spec_roughness`] (#1480 / REN-D22-NEW-01).
pub(crate) const NORMAL_ALPHA_SPEC_BIT: u32 = 0x8000_0000;

/// The normal-alpha-as-spec population gate (Skyrim/Gamebryo era): a lit
/// surface (`material_kind < 100`, low metalness, ~zero env-map scale) that
/// ships a normal map but no dedicated gloss map. Excludes glass/effect
/// kinds (>= 100, own roughness) and the FNV/FO4 env-mapped population. The
/// inputs are the exact values both the spawn write-back and the render
/// path read from the `Material` / `NormalMapHandle` / `ExtraTextureMaps`
/// components, so the gate cannot diverge between the two call sites.
pub(crate) fn normal_alpha_spec_applies(
    material_kind: u32,
    metalness: f32,
    env_map_scale: f32,
    normal_map_index: u32,
    gloss_map_index: u32,
) -> bool {
    material_kind < 100
        && metalness < 0.3
        && env_map_scale <= 0.3
        && normal_map_index != 0
        && gloss_map_index == 0
}

/// Canonical roughness for the normal-alpha-as-spec population, or `None`
/// when the gate does not apply (caller keeps the `resolve_pbr`-resolved
/// roughness). With an alpha-bearing normal, the normal's alpha is the
/// per-pixel smoothness mask and the base roughness is seeded SMOOTH from
/// the authored glossiness; an alpha-less normal with an authored
/// above-neutral specular strength roughens from that instead. Both
/// formulas mirror the legacy per-draw derivation verbatim, so relocating
/// them to spawn is value-identical (#1480 / REN-D22-NEW-01). Idempotent —
/// it derives from `glossiness` / `specular_strength`, never from the
/// current `roughness`, so re-running it never drifts.
pub(crate) fn normal_alpha_spec_roughness(
    material_kind: u32,
    metalness: f32,
    env_map_scale: f32,
    glossiness: f32,
    specular_strength: f32,
    normal_map_index: u32,
    gloss_map_index: u32,
    normal_has_alpha: bool,
) -> Option<f32> {
    if !normal_alpha_spec_applies(
        material_kind,
        metalness,
        env_map_scale,
        normal_map_index,
        gloss_map_index,
    ) {
        return None;
    }
    if normal_has_alpha {
        Some((1.0 - glossiness / 100.0).clamp(0.05, 0.95))
    } else if specular_strength > 1.2 {
        Some((0.85 - (specular_strength - 1.0) * 0.1).clamp(0.4, 0.85))
    } else {
        None
    }
}

/// Resolve the normal-alpha-as-spec roughness ONCE at spawn and write it
/// into the canonical [`Material::roughness`], instead of recomputing it
/// per draw in the render path. This is the #1480 / REN-D22-NEW-01 contract
/// fix: the renderer reads the resolved scalar directly (NIFAL
/// resolve-once), with no render-time heuristic mutating canonical state.
///
/// Reads the SAME components the render path reads (`Material`,
/// `NormalMapHandle`, `ExtraTextureMaps`), so the written value is
/// byte-identical to the legacy per-draw result — only its home (the
/// canonical field, now visible to `mat.*` / `material_dump` tooling) and
/// its timing (once at spawn, not every frame) change. Idempotent (see
/// [`normal_alpha_spec_roughness`]). Call after all three components are
/// attached to `entity`.
pub(crate) fn resolve_normal_alpha_spec_roughness(world: &mut World, entity: EntityId) {
    let Some((material_kind, metalness, env_map_scale, glossiness, specular_strength)) =
        world.get::<Material>(entity).map(|m| {
            (
                m.material_kind,
                m.metalness,
                m.env_map_scale,
                m.glossiness,
                m.specular_strength,
            )
        })
    else {
        return;
    };
    let (normal_map_index, normal_has_alpha) = world
        .get::<NormalMapHandle>(entity)
        .map(|n| (n.0, n.1))
        .unwrap_or((0, false));
    let gloss_map_index = world
        .get::<ExtraTextureMaps>(entity)
        .map(|e| e.gloss)
        .unwrap_or(0);
    if let Some(r) = normal_alpha_spec_roughness(
        material_kind,
        metalness,
        env_map_scale,
        glossiness,
        specular_strength,
        normal_map_index,
        gloss_map_index,
        normal_has_alpha,
    ) {
        if let Some(m) = world.get_mut::<Material>(entity) {
            m.roughness = r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Inputs that pass the gate (lit Skyrim-era matte surface w/ normal map,
    // no gloss map): material_kind 0, metalness 0, env_map_scale 0,
    // normal_map_index 7, gloss_map_index 0.
    const PASS: (u32, f32, f32, u32, u32) = (0, 0.0, 0.0, 7, 0);

    #[test]
    fn alpha_normal_seeds_smooth_roughness_from_glossiness() {
        // glossiness 80 → 1.0 - 0.80 = 0.20 (f32: ~0.19999999).
        let r = normal_alpha_spec_roughness(PASS.0, PASS.1, PASS.2, 80.0, 1.0, PASS.3, PASS.4, true);
        assert!((r.unwrap() - 0.20).abs() < 1e-5, "{r:?}");
    }

    #[test]
    fn alphaless_normal_uses_specular_strength_when_above_neutral() {
        // specular_strength 2.0 → 0.85 - (1.0)*0.1 = 0.75.
        let r =
            normal_alpha_spec_roughness(PASS.0, PASS.1, PASS.2, 80.0, 2.0, PASS.3, PASS.4, false);
        assert!((r.unwrap() - 0.75).abs() < 1e-5, "{r:?}");
    }

    #[test]
    fn alphaless_normal_with_neutral_specular_keeps_resolved_roughness() {
        // specular_strength 1.0 (<= 1.2) and no normal alpha → None (caller
        // keeps the translate-resolved roughness, no override).
        let r =
            normal_alpha_spec_roughness(PASS.0, PASS.1, PASS.2, 80.0, 1.0, PASS.3, PASS.4, false);
        assert_eq!(r, None);
    }

    #[test]
    fn gate_excludes_glass_metal_envmapped_glossmapped_and_normalless() {
        // material_kind >= 100 (glass/effect — own roughness).
        assert!(!normal_alpha_spec_applies(100, 0.0, 0.0, 7, 0));
        // metalness >= 0.3 (metal — Disney/legacy own path).
        assert!(!normal_alpha_spec_applies(0, 0.3, 0.0, 7, 0));
        // env_map_scale > 0.3 (FNV/FO4 env-mapped population).
        assert!(!normal_alpha_spec_applies(0, 0.0, 0.31, 7, 0));
        // no normal map.
        assert!(!normal_alpha_spec_applies(0, 0.0, 0.0, 0, 0));
        // dedicated gloss map present.
        assert!(!normal_alpha_spec_applies(0, 0.0, 0.0, 7, 5));
        // baseline passes.
        assert!(normal_alpha_spec_applies(0, 0.0, 0.0, 7, 0));
    }

    #[test]
    fn roughness_clamps_to_renderer_ranges() {
        // glossiness 100 → 0.0, clamped up to 0.05 floor.
        assert_eq!(
            normal_alpha_spec_roughness(PASS.0, PASS.1, PASS.2, 100.0, 1.0, PASS.3, PASS.4, true),
            Some(0.05)
        );
        // huge specular_strength → 0.85 - big, clamped to 0.4 floor.
        assert_eq!(
            normal_alpha_spec_roughness(PASS.0, PASS.1, PASS.2, 80.0, 99.0, PASS.3, PASS.4, false),
            Some(0.4)
        );
    }

    #[test]
    fn derivation_is_idempotent_over_roughness() {
        // The formula ignores the current roughness (derives from glossiness /
        // specular_strength), so re-deriving after a prior write is a no-op —
        // the property that makes the resolve-at-spawn relocation safe to run
        // more than once (#1480).
        let first =
            normal_alpha_spec_roughness(PASS.0, PASS.1, PASS.2, 65.0, 1.0, PASS.3, PASS.4, true);
        let second =
            normal_alpha_spec_roughness(PASS.0, PASS.1, PASS.2, 65.0, 1.0, PASS.3, PASS.4, true);
        assert_eq!(first, second);
        assert!((first.unwrap() - 0.35).abs() < 1e-5, "{first:?}");
    }
}
