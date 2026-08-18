// Shared constants that appear in both Rust renderer code and GLSL shaders.
//
// `build.rs` generates `shaders/include/shader_constants.glsl` from this
// same data (both files `include!` `shader_constants_data.rs`). Every
// affected shader then `#include "include/shader_constants.glsl"` at the
// top, compiled with `glslangValidator -V -I crates/renderer/shaders …`.
//
// Adding a constant: edit `shader_constants_data.rs`, run
// `cargo build -p byroredux-renderer` (re-gen header), recompile shaders.

// Pull in all pub consts from the single source of truth.
include!("shader_constants_data.rs");

/// Total cluster count (derived — not emitted to GLSL header separately).
pub const TOTAL_CLUSTERS: u32 = CLUSTER_TILES_X * CLUSTER_TILES_Y * CLUSTER_SLICES_Z;

/// Per-vertex size in bytes (derived from VERTEX_STRIDE_FLOATS).
pub const VERTEX_STRIDE_BYTES: u64 = VERTEX_STRIDE_FLOATS as u64 * 4;

/// Skinned-vertex output size in bytes (derived from
/// `SKIN_OUTPUT_STRIDE_FLOATS`). This is both the `SkinSlot::output_buffer`
/// allocation stride and the `vertexStride` handed to the skinned-BLAS
/// build/refit — they are the same number by construction, which is the
/// point of deriving it here rather than spelling `12` at three sites
/// (#2170).
pub const SKIN_OUTPUT_STRIDE_BYTES: u64 = SKIN_OUTPUT_STRIDE_FLOATS as u64 * 4;

/// Conservative camera-space radius for exterior LOD shadow casters. A
/// receiver can retain a shadow through `SHADOW_FADE_END`, and its directional
/// ray can travel another `DIRECTIONAL_SHADOW_TRACE_DISTANCE` toward a caster.
pub const LOD_SHADOW_CASTER_DISTANCE: f32 = SHADOW_FADE_END + DIRECTIONAL_SHADOW_TRACE_DISTANCE;

/// Whether a debug visualization is a correctness oracle that must bypass
/// fog, caustics, bloom, temporal upscaling, grading, exposure, and tone
/// mapping. This includes categorical/scalar views and the direct/indirect
/// term-isolation views: automatic exposure can otherwise turn a zero-light
/// Cornell rung grey and destroy the meaning of a black pixel.
pub const fn debug_viz_requires_raw_output(flags: u32) -> bool {
    flags & (DBG_VIZ_SELECTED_LIGHT | DBG_VIZ_DIRECT | DBG_VIZ_RAW_INDIRECT) != 0
        || (flags & DBG_VIZ_RT_LOD) == DBG_VIZ_RT_LOD
}

/// Raw-output decision for the full structured/legacy debug contract.
pub const fn render_debug_requires_raw_output(flags: u32, mode: u32) -> bool {
    if mode == RENDER_DEBUG_LEGACY_FLAGS {
        debug_viz_requires_raw_output(flags)
    } else {
        mode != RENDER_DEBUG_FINAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // #1860 — `DBG_BITS` moved to `shader_constants_data.rs` (shared with
    // `build.rs`'s header emit, see that file's doc comment) so it's a
    // single source of truth for the emit, the value-pin below, the
    // no-redeclare guard (`triangle_frag_dbg_bits_not_redeclared`), and
    // the count-parity test right below. `use super::*` (top of this
    // module) brings it into scope from the `include!`d data file.

    /// #1860 — pins that `DBG_BITS` cannot silently drift behind a new
    /// `pub const DBG_*` again: every constant declared in
    /// `shader_constants_data.rs` must have a matching catalog entry.
    /// Counts `pub const DBG_` occurrences in the data file's source text
    /// rather than re-declaring the list, so this test fails the moment a
    /// new DBG_* constant is added without a catalog entry — the exact
    /// gap #1482 fixed once and #1860 found had regrown to 5 constants.
    #[test]
    fn dbg_bits_catalog_covers_every_dbg_constant() {
        let data_src = include_str!("shader_constants_data.rs");
        // Exclude `DBG_BITS` itself — it's the catalog, typed
        // `&[(&str, u32)]`, not one of the `u32` bit constants it lists.
        let declared = data_src
            .lines()
            .filter(|l| l.trim_start().starts_with("pub const DBG_"))
            .filter(|l| !l.trim_start().starts_with("pub const DBG_BITS"))
            .count();
        assert_eq!(
            DBG_BITS.len(),
            declared,
            "DBG_BITS has {} entries but shader_constants_data.rs declares {} \
             `pub const DBG_*` constants — a new DBG_* constant was added \
             without a matching DBG_BITS catalog entry",
            DBG_BITS.len(),
            declared,
        );
    }

    #[test]
    fn max_bones_per_mesh_matches_core() {
        assert_eq!(
            MAX_BONES_PER_MESH as usize,
            byroredux_core::ecs::components::MAX_BONES_PER_MESH,
            "shader_constants::MAX_BONES_PER_MESH must equal \
             byroredux_core::ecs::components::MAX_BONES_PER_MESH"
        );
    }

    #[test]
    fn correctness_debug_views_require_raw_frame_graph_output() {
        assert!(debug_viz_requires_raw_output(DBG_VIZ_SELECTED_LIGHT));
        assert!(debug_viz_requires_raw_output(DBG_VIZ_SHADOW_VISIBILITY));
        assert!(debug_viz_requires_raw_output(DBG_VIZ_MATERIAL_LOBES));
        assert!(debug_viz_requires_raw_output(DBG_VIZ_RT_LOD));
        assert!(debug_viz_requires_raw_output(DBG_VIZ_DIRECT));
        assert!(debug_viz_requires_raw_output(DBG_VIZ_RAW_INDIRECT));
        for mode in 1..=RENDER_DEBUG_MODE_MAX {
            assert!(render_debug_requires_raw_output(0, mode));
        }
        assert!(!render_debug_requires_raw_output(0, RENDER_DEBUG_FINAL));
        assert!(render_debug_requires_raw_output(
            DBG_VIZ_DIRECT,
            RENDER_DEBUG_LEGACY_FLAGS
        ));
    }

    #[test]
    fn vertex_stride_matches_vertex_struct() {
        assert_eq!(
            (VERTEX_STRIDE_FLOATS * 4) as usize,
            std::mem::size_of::<crate::Vertex>(),
            "VERTEX_STRIDE_FLOATS ({VERTEX_STRIDE_FLOATS}) × 4 must equal size_of::<Vertex>()"
        );
        for (name, shader_offset, rust_offset) in [
            (
                "color",
                VERTEX_COLOR_OFFSET_FLOATS,
                std::mem::offset_of!(crate::Vertex, color),
            ),
            (
                "normal",
                VERTEX_NORMAL_OFFSET_FLOATS,
                std::mem::offset_of!(crate::Vertex, normal),
            ),
            (
                "uv",
                VERTEX_UV_OFFSET_FLOATS,
                std::mem::offset_of!(crate::Vertex, uv),
            ),
            (
                "tangent",
                VERTEX_TANGENT_OFFSET_FLOATS,
                std::mem::offset_of!(crate::Vertex, tangent),
            ),
        ] {
            assert_eq!(
                (shader_offset * 4) as usize,
                rust_offset,
                "VERTEX_{name}_OFFSET_FLOATS must match Vertex::{name}"
            );
        }
    }

    /// Verify the generated GLSL header contains the expected #define lines.
    /// This pins that build.rs actually emitted the current values.
    #[test]
    fn generated_header_contains_all_defines() {
        let header = include_str!("../shaders/include/shader_constants.glsl");
        for (name, expected) in [
            ("CLUSTER_TILES_X", format!("#define CLUSTER_TILES_X {CLUSTER_TILES_X}u")),
            ("CLUSTER_TILES_Y", format!("#define CLUSTER_TILES_Y {CLUSTER_TILES_Y}u")),
            ("CLUSTER_SLICES_Z", format!("#define CLUSTER_SLICES_Z {CLUSTER_SLICES_Z}u")),
            ("MAX_LIGHTS_PER_CLUSTER", format!("#define MAX_LIGHTS_PER_CLUSTER {MAX_LIGHTS_PER_CLUSTER}u")),
            ("MAX_LIGHTS", format!("#define MAX_LIGHTS {MAX_LIGHTS}u")),
            ("RESERVOIR_LIGHT_BITS", format!("#define RESERVOIR_LIGHT_BITS {RESERVOIR_LIGHT_BITS}u")),
            ("RESERVOIR_LIGHT_MASK", format!("#define RESERVOIR_LIGHT_MASK {RESERVOIR_LIGHT_MASK}u")),
            ("RESERVOIR_SURFACE_MASK", format!("#define RESERVOIR_SURFACE_MASK {RESERVOIR_SURFACE_MASK}u")),
            ("VERTEX_STRIDE_FLOATS", format!("#define VERTEX_STRIDE_FLOATS {VERTEX_STRIDE_FLOATS}u")),
            // #2234 (REN-D9-01) — was emitted by build.rs but missing from
            // this pin-list, unlike its VERTEX_STRIDE_FLOATS/MAX_BONES_PER_MESH/
            // SKIN_WORKGROUP_SIZE siblings that bake into the same skin
            // compute shader's committed `.spv`.
            ("SKIN_OUTPUT_STRIDE_FLOATS", format!("#define SKIN_OUTPUT_STRIDE_FLOATS {SKIN_OUTPUT_STRIDE_FLOATS}u")),
            ("MAX_BONES_PER_MESH", format!("#define MAX_BONES_PER_MESH {MAX_BONES_PER_MESH}u")),
            // No `u` suffix — used in a `layout(local_size_x = …)` qualifier (#1758).
            ("SKIN_WORKGROUP_SIZE", format!("#define SKIN_WORKGROUP_SIZE {SKIN_WORKGROUP_SIZE}")),
            ("MATERIAL_KIND_GLASS", format!("#define MATERIAL_KIND_GLASS {MATERIAL_KIND_GLASS}u")),
            ("MATERIAL_KIND_EFFECT_SHADER", format!("#define MATERIAL_KIND_EFFECT_SHADER {MATERIAL_KIND_EFFECT_SHADER}u")),
            ("MATERIAL_KIND_NO_LIGHTING", format!("#define MATERIAL_KIND_NO_LIGHTING {MATERIAL_KIND_NO_LIGHTING}u")),
            ("MATERIAL_KIND_FIRE_REFRACTION", format!("#define MATERIAL_KIND_FIRE_REFRACTION {MATERIAL_KIND_FIRE_REFRACTION}u")),
            ("GLASS_RAY_BUDGET", format!("#define GLASS_RAY_BUDGET {GLASS_RAY_BUDGET}u")),
            ("GLASS_RAY_COST", format!("#define GLASS_RAY_COST {GLASS_RAY_COST}u")),
            ("WORKGROUP_X", format!("#define WORKGROUP_X {WORKGROUP_X}")),
            ("WORKGROUP_Y", format!("#define WORKGROUP_Y {WORKGROUP_Y}")),
            ("WORKGROUP_Z", format!("#define WORKGROUP_Z {WORKGROUP_Z}")),
            ("THREADS_PER_CLUSTER", format!("#define THREADS_PER_CLUSTER {THREADS_PER_CLUSTER}")),
            ("BLOOM_INTENSITY", format!("#define BLOOM_INTENSITY {BLOOM_INTENSITY:?}")),
            ("VOLUME_FAR", format!("#define VOLUME_FAR {VOLUME_FAR:?}")),
            ("NORMAL_ALPHA_SPEC_BIT", format!("#define NORMAL_ALPHA_SPEC_BIT {NORMAL_ALPHA_SPEC_BIT}u")),
            ("WATER_CALM", format!("#define WATER_CALM {WATER_CALM}u")),
            ("WATER_RIVER", format!("#define WATER_RIVER {WATER_RIVER}u")),
            ("WATER_RAPIDS", format!("#define WATER_RAPIDS {WATER_RAPIDS}u")),
            ("WATER_WATERFALL", format!("#define WATER_WATERFALL {WATER_WATERFALL}u")),
            ("FOG_VOLUME_CLUSTER_DIM", format!("#define FOG_VOLUME_CLUSTER_DIM {FOG_VOLUME_CLUSTER_DIM}u")),
            ("MAX_FOG_VOLUMES_PER_CLUSTER", format!("#define MAX_FOG_VOLUMES_PER_CLUSTER {MAX_FOG_VOLUMES_PER_CLUSTER}u")),
            ("FOG_VOLUME_PROFILE_HOMOGENEOUS", format!("#define FOG_VOLUME_PROFILE_HOMOGENEOUS {FOG_VOLUME_PROFILE_HOMOGENEOUS:?}")),
            ("FOG_VOLUME_PROFILE_SMOKE", format!("#define FOG_VOLUME_PROFILE_SMOKE {FOG_VOLUME_PROFILE_SMOKE:?}")),
            ("FOG_VOLUME_PROFILE_FLAME", format!("#define FOG_VOLUME_PROFILE_FLAME {FOG_VOLUME_PROFILE_FLAME:?}")),
            ("FOG_VOLUME_PROFILE_EXPLOSION", format!("#define FOG_VOLUME_PROFILE_EXPLOSION {FOG_VOLUME_PROFILE_EXPLOSION:?}")),
            // #1920 — 10 defines `build.rs` emits that this value-pin had
            // never covered (found by an audit sweep alongside the former
            // shadow-mask constants, which shipped without a pin).
            ("CLUSTER_NEAR", format!("#define CLUSTER_NEAR {CLUSTER_NEAR:?}")),
            ("CLUSTER_FAR_FLOOR", format!("#define CLUSTER_FAR_FLOOR {CLUSTER_FAR_FLOOR:?}")),
            ("CLUSTER_FAR_FALLBACK", format!("#define CLUSTER_FAR_FALLBACK {CLUSTER_FAR_FALLBACK:?}")),
            ("VERTEX_COLOR_OFFSET_FLOATS", format!("#define VERTEX_COLOR_OFFSET_FLOATS {VERTEX_COLOR_OFFSET_FLOATS}u")),
            ("VERTEX_NORMAL_OFFSET_FLOATS", format!("#define VERTEX_NORMAL_OFFSET_FLOATS {VERTEX_NORMAL_OFFSET_FLOATS}u")),
            ("VERTEX_UV_OFFSET_FLOATS", format!("#define VERTEX_UV_OFFSET_FLOATS {VERTEX_UV_OFFSET_FLOATS}u")),
            ("VERTEX_TANGENT_OFFSET_FLOATS", format!("#define VERTEX_TANGENT_OFFSET_FLOATS {VERTEX_TANGENT_OFFSET_FLOATS}u")),
            ("VISIBILITY_LAYER_ARCHITECTURE", format!("#define VISIBILITY_LAYER_ARCHITECTURE {VISIBILITY_LAYER_ARCHITECTURE}u")),
            ("VISIBILITY_LAYER_STATIC_PROP", format!("#define VISIBILITY_LAYER_STATIC_PROP {VISIBILITY_LAYER_STATIC_PROP}u")),
            ("VISIBILITY_LAYER_DYNAMIC_ACTOR", format!("#define VISIBILITY_LAYER_DYNAMIC_ACTOR {VISIBILITY_LAYER_DYNAMIC_ACTOR}u")),
            ("VISIBILITY_LAYER_FOLIAGE", format!("#define VISIBILITY_LAYER_FOLIAGE {VISIBILITY_LAYER_FOLIAGE}u")),
            ("VISIBILITY_LAYER_GLASS", format!("#define VISIBILITY_LAYER_GLASS {VISIBILITY_LAYER_GLASS}u")),
            ("VISIBILITY_LAYER_EFFECT", format!("#define VISIBILITY_LAYER_EFFECT {VISIBILITY_LAYER_EFFECT}u")),
            ("VISIBILITY_MASK_ALL_OPAQUE", format!("#define VISIBILITY_MASK_ALL_OPAQUE {VISIBILITY_MASK_ALL_OPAQUE}u")),
            ("VISIBILITY_MASK_SOLID", format!("#define VISIBILITY_MASK_SOLID {VISIBILITY_MASK_SOLID}u")),
            ("VISIBILITY_MASK_FULL", format!("#define VISIBILITY_MASK_FULL {VISIBILITY_MASK_FULL}u")),
            ("ATTENUATION_MODEL_LEGACY_SOFT_RANGE", format!("#define ATTENUATION_MODEL_LEGACY_SOFT_RANGE {ATTENUATION_MODEL_LEGACY_SOFT_RANGE}u")),
            ("ATTENUATION_MODEL_INVERSE_SQUARE", format!("#define ATTENUATION_MODEL_INVERSE_SQUARE {ATTENUATION_MODEL_INVERSE_SQUARE}u")),
            ("WORLD_UNITS_PER_METER", format!("#define WORLD_UNITS_PER_METER {WORLD_UNITS_PER_METER:?}")),
            ("ADIABATIC_FLAME_TEMPERATURE_K", format!("#define ADIABATIC_FLAME_TEMPERATURE_K {ADIABATIC_FLAME_TEMPERATURE_K:?}")),
            ("EXPLOSION_EXPANSION_TIME_SECONDS", format!("#define EXPLOSION_EXPANSION_TIME_SECONDS {EXPLOSION_EXPANSION_TIME_SECONDS:?}")),
            ("EXPLOSION_IMPULSE_DURATION_SECONDS", format!("#define EXPLOSION_IMPULSE_DURATION_SECONDS {EXPLOSION_IMPULSE_DURATION_SECONDS:?}")),
            ("COMBUSTION_OVERPRESSURE_DISSIPATION_PER_SECOND", format!("#define COMBUSTION_OVERPRESSURE_DISSIPATION_PER_SECOND {COMBUSTION_OVERPRESSURE_DISSIPATION_PER_SECOND:?}")),
            ("COMBUSTION_MAX_PRESSURE_ACCELERATION_MPS2", format!("#define COMBUSTION_MAX_PRESSURE_ACCELERATION_MPS2 {COMBUSTION_MAX_PRESSURE_ACCELERATION_MPS2:?}")),
            ("COMBUSTION_VORTICITY_CONFINEMENT_SPEED_MPS", format!("#define COMBUSTION_VORTICITY_CONFINEMENT_SPEED_MPS {COMBUSTION_VORTICITY_CONFINEMENT_SPEED_MPS:?}")),
            ("COMBUSTION_MAX_VORTICITY_ACCELERATION_MPS2", format!("#define COMBUSTION_MAX_VORTICITY_ACCELERATION_MPS2 {COMBUSTION_MAX_VORTICITY_ACCELERATION_MPS2:?}")),
            ("COMBUSTION_TURBULENCE_COARSE_EDDY_SCALE_METERS", format!("#define COMBUSTION_TURBULENCE_COARSE_EDDY_SCALE_METERS {COMBUSTION_TURBULENCE_COARSE_EDDY_SCALE_METERS:?}")),
            ("COMBUSTION_TURBULENCE_DETAIL_EDDY_SCALE_METERS", format!("#define COMBUSTION_TURBULENCE_DETAIL_EDDY_SCALE_METERS {COMBUSTION_TURBULENCE_DETAIL_EDDY_SCALE_METERS:?}")),
            ("COMBUSTION_TURBULENCE_COARSE_RISE_SPEED_MPS", format!("#define COMBUSTION_TURBULENCE_COARSE_RISE_SPEED_MPS {COMBUSTION_TURBULENCE_COARSE_RISE_SPEED_MPS:?}")),
            ("COMBUSTION_TURBULENCE_DETAIL_RISE_SPEED_MPS", format!("#define COMBUSTION_TURBULENCE_DETAIL_RISE_SPEED_MPS {COMBUSTION_TURBULENCE_DETAIL_RISE_SPEED_MPS:?}")),
            ("COMBUSTION_AEROSOL_DISSIPATION_PER_SECOND", format!("#define COMBUSTION_AEROSOL_DISSIPATION_PER_SECOND {COMBUSTION_AEROSOL_DISSIPATION_PER_SECOND:?}")),
            ("FLAME_FUEL_BOUNDARY_HEIGHT_FRACTION", format!("#define FLAME_FUEL_BOUNDARY_HEIGHT_FRACTION {FLAME_FUEL_BOUNDARY_HEIGHT_FRACTION:?}")),
            ("FLAME_REACTION_ZONE_HEIGHT_FRACTION", format!("#define FLAME_REACTION_ZONE_HEIGHT_FRACTION {FLAME_REACTION_ZONE_HEIGHT_FRACTION:?}")),
            ("FLAME_REACTION_ZONE_FADE_START_FRACTION", format!("#define FLAME_REACTION_ZONE_FADE_START_FRACTION {FLAME_REACTION_ZONE_FADE_START_FRACTION:?}")),
            ("FLAME_SOURCE_LATERAL_SPEED_MPS", format!("#define FLAME_SOURCE_LATERAL_SPEED_MPS {FLAME_SOURCE_LATERAL_SPEED_MPS:?}")),
            ("FLAME_SOURCE_VELOCITY_RESPONSE_PER_SECOND", format!("#define FLAME_SOURCE_VELOCITY_RESPONSE_PER_SECOND {FLAME_SOURCE_VELOCITY_RESPONSE_PER_SECOND:?}")),
            ("COMBUSTION_LIGHT_GRID_X", format!("#define COMBUSTION_LIGHT_GRID_X {COMBUSTION_LIGHT_GRID_X}u")),
            ("COMBUSTION_LIGHT_GRID_Y", format!("#define COMBUSTION_LIGHT_GRID_Y {COMBUSTION_LIGHT_GRID_Y}u")),
            ("COMBUSTION_LIGHT_GRID_Z", format!("#define COMBUSTION_LIGHT_GRID_Z {COMBUSTION_LIGHT_GRID_Z}u")),
            ("COMBUSTION_LIGHT_GRID_COUNT", format!("#define COMBUSTION_LIGHT_GRID_COUNT {COMBUSTION_LIGHT_GRID_COUNT}u")),
            ("COMBUSTION_LIGHT_HALF_EXTENT_XZ_METERS", format!("#define COMBUSTION_LIGHT_HALF_EXTENT_XZ_METERS {COMBUSTION_LIGHT_HALF_EXTENT_XZ_METERS:?}")),
            ("COMBUSTION_LIGHT_HALF_EXTENT_Y_METERS", format!("#define COMBUSTION_LIGHT_HALF_EXTENT_Y_METERS {COMBUSTION_LIGHT_HALF_EXTENT_Y_METERS:?}")),
            ("COMBUSTION_LIGHT_FIXED_SCALE", format!("#define COMBUSTION_LIGHT_FIXED_SCALE {COMBUSTION_LIGHT_FIXED_SCALE:?}")),
            ("COMBUSTION_LIGHT_VOLUME_FIXED_SCALE", format!("#define COMBUSTION_LIGHT_VOLUME_FIXED_SCALE {COMBUSTION_LIGHT_VOLUME_FIXED_SCALE:?}")),
            ("SHADOW_FADE_START", format!("#define SHADOW_FADE_START {SHADOW_FADE_START:?}")),
            ("SHADOW_FADE_END", format!("#define SHADOW_FADE_END {SHADOW_FADE_END:?}")),
            ("DIRECTIONAL_SHADOW_TRACE_DISTANCE", format!("#define DIRECTIONAL_SHADOW_TRACE_DISTANCE {DIRECTIONAL_SHADOW_TRACE_DISTANCE:?}")),
            ("GI_HIT_LIGHT_CAP", format!("#define GI_HIT_LIGHT_CAP {GI_HIT_LIGHT_CAP}u")),
            ("GI_SAMPLE_LUMINANCE_CLAMP", format!("#define GI_SAMPLE_LUMINANCE_CLAMP {GI_SAMPLE_LUMINANCE_CLAMP:?}")),
            ("CAUSTIC_FIXED_SCALE", format!("#define CAUSTIC_FIXED_SCALE {CAUSTIC_FIXED_SCALE:?}")),
            ("RT_ABLATION_DIRECT_SHADOW", format!("#define RT_ABLATION_DIRECT_SHADOW {RT_ABLATION_DIRECT_SHADOW}u")),
            ("RT_ABLATION_GI", format!("#define RT_ABLATION_GI {RT_ABLATION_GI}u")),
            ("RT_ABLATION_REFLECTION_GLASS", format!("#define RT_ABLATION_REFLECTION_GLASS {RT_ABLATION_REFLECTION_GLASS}u")),
            ("RT_ABLATION_ALL_RAYS", format!("#define RT_ABLATION_ALL_RAYS {RT_ABLATION_ALL_RAYS}u")),
            ("RT_COMPILE_ABLATION_MASK", format!("#define RT_COMPILE_ABLATION_MASK {RT_COMPILE_ABLATION_MASK}u")),
            ("ENABLE_LEGACY_WRS", format!("#define ENABLE_LEGACY_WRS {ENABLE_LEGACY_WRS}")),
            // DBG_* bits are pinned below via the shared DBG_BITS catalog
            // (every constant, count-checked by
            // dbg_bits_catalog_covers_every_dbg_constant) — see #1482 / #1860.
            ("INSTANCE_FLAG_NON_UNIFORM_SCALE", format!("#define INSTANCE_FLAG_NON_UNIFORM_SCALE {INSTANCE_FLAG_NON_UNIFORM_SCALE}u")),
            ("INSTANCE_FLAG_ALPHA_BLEND", format!("#define INSTANCE_FLAG_ALPHA_BLEND {INSTANCE_FLAG_ALPHA_BLEND}u")),
            ("INSTANCE_FLAG_CAUSTIC_SOURCE", format!("#define INSTANCE_FLAG_CAUSTIC_SOURCE {INSTANCE_FLAG_CAUSTIC_SOURCE}u")),
            ("INSTANCE_FLAG_TERRAIN_SPLAT", format!("#define INSTANCE_FLAG_TERRAIN_SPLAT {INSTANCE_FLAG_TERRAIN_SPLAT}u")),
            ("INSTANCE_RENDER_LAYER_SHIFT", format!("#define INSTANCE_RENDER_LAYER_SHIFT {INSTANCE_RENDER_LAYER_SHIFT}u")),
            ("INSTANCE_RENDER_LAYER_MASK", format!("#define INSTANCE_RENDER_LAYER_MASK {INSTANCE_RENDER_LAYER_MASK}u")),
            ("INSTANCE_FLAG_FLAT_SHADING", format!("#define INSTANCE_FLAG_FLAT_SHADING {INSTANCE_FLAG_FLAT_SHADING}u")),
            ("INSTANCE_FLAG_DIFFUSE_ALPHA", format!("#define INSTANCE_FLAG_DIFFUSE_ALPHA {INSTANCE_FLAG_DIFFUSE_ALPHA}u")),
            ("MAT_FLAG_VERTEX_COLOR_EMISSIVE", format!("#define MAT_FLAG_VERTEX_COLOR_EMISSIVE {MAT_FLAG_VERTEX_COLOR_EMISSIVE}u")),
            ("MAT_FLAG_EFFECT_SOFT", format!("#define MAT_FLAG_EFFECT_SOFT {MAT_FLAG_EFFECT_SOFT}u")),
            ("MAT_FLAG_EFFECT_PALETTE_COLOR", format!("#define MAT_FLAG_EFFECT_PALETTE_COLOR {MAT_FLAG_EFFECT_PALETTE_COLOR}u")),
            ("MAT_FLAG_EFFECT_PALETTE_ALPHA", format!("#define MAT_FLAG_EFFECT_PALETTE_ALPHA {MAT_FLAG_EFFECT_PALETTE_ALPHA}u")),
            ("MAT_FLAG_EFFECT_LIT", format!("#define MAT_FLAG_EFFECT_LIT {MAT_FLAG_EFFECT_LIT}u")),
            ("MAT_FLAG_PBR_BSDF", format!("#define MAT_FLAG_PBR_BSDF {MAT_FLAG_PBR_BSDF}u")),
            ("MAT_FLAG_TRANSLUCENCY", format!("#define MAT_FLAG_TRANSLUCENCY {MAT_FLAG_TRANSLUCENCY}u")),
            ("MAT_FLAG_MODEL_SPACE_NORMALS", format!("#define MAT_FLAG_MODEL_SPACE_NORMALS {MAT_FLAG_MODEL_SPACE_NORMALS}u")),
            ("MAT_FLAG_TRANSLUCENCY_THICK_OBJECT", format!("#define MAT_FLAG_TRANSLUCENCY_THICK_OBJECT {MAT_FLAG_TRANSLUCENCY_THICK_OBJECT}u")),
            ("MAT_FLAG_TRANSLUCENCY_MIX_ALBEDO", format!("#define MAT_FLAG_TRANSLUCENCY_MIX_ALBEDO {MAT_FLAG_TRANSLUCENCY_MIX_ALBEDO}u")),
            ("MAT_FLAG_THIN_GLASS", format!("#define MAT_FLAG_THIN_GLASS {MAT_FLAG_THIN_GLASS}u")),
            ("MAT_FLAG_EFFECT_LI_SHIFT", format!("#define MAT_FLAG_EFFECT_LI_SHIFT {MAT_FLAG_EFFECT_LI_SHIFT}u")),
            // BGSM_AUTHORED intentionally NOT mirrored to GLSL — see build.rs.
        ] {
            assert!(
                header.contains(&expected),
                "shader_constants.glsl missing or wrong value for {name}: expected `{expected}`",
            );
        }
        // Every DBG_* bit, driven from the shared catalog so this
        // value-pin can never again cover a subset (#1482 / #1860).
        for (name, value) in DBG_BITS {
            let expected = format!("#define {name} {value}u");
            assert!(
                header.contains(&expected),
                "shader_constants.glsl missing or wrong value for {name}: expected `{expected}`",
            );
        }
        for (name, value) in RENDER_DEBUG_MODES {
            let expected = format!("#define {name} {value}u");
            assert!(
                header.contains(&expected),
                "shader_constants.glsl missing or wrong value for {name}: expected `{expected}`",
            );
        }
    }

    /// Direct-light shadow reach is scene policy, not cell-kind policy.
    /// Pin every directional-shadow consumer to the generated constants so
    /// interior geometry, exterior terrain, water, and volumetrics cannot
    /// quietly grow different trace/fade distances.
    #[test]
    fn directional_shadow_consumers_share_distance_contract() {
        let triangle = include_str!("../shaders/triangle.frag");
        let lighting = include_str!("../shaders/include/lighting.glsl");
        let water = include_str!("../shaders/water.frag");
        let volumetrics = include_str!("../shaders/volumetrics_inject.comp");

        assert!(triangle.contains("smoothstep(SHADOW_FADE_START, SHADOW_FADE_END, worldDist)"));
        for (name, source) in [
            ("triangle.frag", triangle),
            ("lighting.glsl", lighting),
            ("water.frag", water),
            ("volumetrics_inject.comp", volumetrics),
        ] {
            assert!(
                source.contains("DIRECTIONAL_SHADOW_TRACE_DISTANCE"),
                "{name} must use the shared directional shadow distance"
            );
        }
    }

    #[test]
    fn every_shadow_query_pass_uses_the_shared_policy_contract() {
        for (name, source) in [
            ("triangle.frag", include_str!("../shaders/triangle.frag")),
            ("water.frag", include_str!("../shaders/water.frag")),
            (
                "volumetrics_inject.comp",
                include_str!("../shaders/volumetrics_inject.comp"),
            ),
            (
                "caustic_splat.comp",
                include_str!("../shaders/caustic_splat.comp"),
            ),
        ] {
            assert!(
                source.contains("#include \"include/shadow_common.glsl\""),
                "{name} must decode the shared per-light shadow policy"
            );
        }
        for (name, source) in [
            ("triangle.frag", include_str!("../shaders/triangle.frag")),
            ("water.frag", include_str!("../shaders/water.frag")),
        ] {
            assert!(
                source.contains("#include \"include/shadow_transport.glsl\""),
                "{name} must use shared material-aware shadow transport"
            );
        }
    }

    /// An empty light buffer is an ingestion/scene result, not permission to
    /// invent a renderer-owned sun. Pin removal of the former hard-coded
    /// no-cluster directional arm: L0 must stay exactly black so a dead light
    /// ingestion or upload path cannot masquerade as plausible illumination.
    #[test]
    fn zero_lights_do_not_synthesize_a_directional_source() {
        let triangle = include_str!("../shaders/triangle.frag");
        assert!(
            triangle.contains("Zero submitted lights means zero direct-light transport"),
            "the zero-light transport contract must remain explicit"
        );
        assert!(
            !triangle.contains("Fallback: single directional light")
                && !triangle.contains("normalize(vec3(0.4, 0.8, 0.5))"),
            "zero lights must not fall back to a synthetic directional source"
        );
    }

    /// Verify all affected shaders include the shared header.
    ///
    /// #1780 (D14-LOW-01) — this allow-list MUST cover every shader that
    /// consumes a generated macro from `shader_constants.glsl`; a shader
    /// that drops the `#include` would otherwise compile against undefined
    /// identifiers (`WORKGROUP_X`, `INSTANCE_FLAG_CAUSTIC_SOURCE`, …) and no
    /// `cargo test` would catch it (the SPIR-V is pre-compiled). The list
    /// previously omitted six header-including shaders — `caustic_splat.comp`
    /// (uses `INSTANCE_FLAG_CAUSTIC_SOURCE` + `WORKGROUP_X/Y`), `water.frag`
    /// (`WATER_*` + `CAUSTIC_FIXED_SCALE`), and the four compute passes whose
    /// `local_size_x = WORKGROUP_X` qualifier reads the header
    /// (`ssao.comp`, `svgf_atrous.comp`, `svgf_temporal.comp`, `taa.comp`).
    /// Cross-check when adding a shader: `grep -L` the include across
    /// `shaders/*.{comp,frag,vert}` and reconcile against this list.
    #[test]
    fn affected_shaders_include_constants_header() {
        for (shader, src) in [
            (
                "cluster_cull.comp",
                include_str!("../shaders/cluster_cull.comp"),
            ),
            ("triangle.frag", include_str!("../shaders/triangle.frag")),
            ("triangle.vert", include_str!("../shaders/triangle.vert")),
            (
                "skin_vertices.comp",
                include_str!("../shaders/skin_vertices.comp"),
            ),
            (
                "skin_palette.comp",
                include_str!("../shaders/skin_palette.comp"),
            ),
            ("composite.frag", include_str!("../shaders/composite.frag")),
            (
                "bloom_downsample.comp",
                include_str!("../shaders/bloom_downsample.comp"),
            ),
            (
                "bloom_upsample.comp",
                include_str!("../shaders/bloom_upsample.comp"),
            ),
            (
                "volumetrics_inject.comp",
                include_str!("../shaders/volumetrics_inject.comp"),
            ),
            (
                "volumetrics_integrate.comp",
                include_str!("../shaders/volumetrics_integrate.comp"),
            ),
            // #1780 — previously-unlisted header consumers.
            (
                "caustic_splat.comp",
                include_str!("../shaders/caustic_splat.comp"),
            ),
            ("water.frag", include_str!("../shaders/water.frag")),
            ("ssao.comp", include_str!("../shaders/ssao.comp")),
            (
                "svgf_atrous.comp",
                include_str!("../shaders/svgf_atrous.comp"),
            ),
            (
                "svgf_temporal.comp",
                include_str!("../shaders/svgf_temporal.comp"),
            ),
            ("taa.comp", include_str!("../shaders/taa.comp")),
        ] {
            assert!(
                src.contains("#include \"include/shader_constants.glsl\""),
                "{shader}: must `#include \"include/shader_constants.glsl\"` at the top",
            );
        }
    }

    /// TD4-203 / #1126 — `composite.frag` must NOT redeclare
    /// `BLOOM_INTENSITY` as a `const float`. The `#define`d value
    /// from the included `shader_constants.glsl` is the single source
    /// of truth. A local `const float BLOOM_INTENSITY = ...;` after
    /// `#include` shadows the macro and breaks recompile-from-source
    /// (textually substitutes to `const float 0.15 = 0.15;`). Positive
    /// coverage that the value flows through correctly lives in
    /// `generated_header_contains_all_defines` (verifies the `#define`
    /// is emitted with the right value).
    #[test]
    fn composite_frag_bloom_intensity_not_redeclared() {
        let src = include_str!("../shaders/composite.frag");
        assert!(
            !src.contains("const float BLOOM_INTENSITY"),
            "composite.frag must not redeclare BLOOM_INTENSITY — \
             the #define from shader_constants.glsl is the source of truth (#1126)",
        );
    }

    /// TD4-204 / #1126 — same shape as the BLOOM_INTENSITY check above.
    #[test]
    fn composite_frag_volume_far_not_redeclared() {
        let src = include_str!("../shaders/composite.frag");
        assert!(
            !src.contains("const float VOLUME_FAR"),
            "composite.frag must not redeclare VOLUME_FAR — \
             the #define from shader_constants.glsl is the source of truth (#1126)",
        );
    }

    /// TD4-205 / #1256 — Water motion-kind enum source-of-truth.
    /// Pre-#1256 water.frag declared local `const uint WATER_CALM = 0u;`
    /// (etc.) duplicating the `#define`s in `shader_constants.glsl`.
    /// #1256 made water.frag `#include` the generated header so the
    /// constants flow through directly; the local `const uint`
    /// declarations now collide with the macros at compile time
    /// (preventing the redeclaration class of bug).
    ///
    /// Post-#1256 this test verifies water.frag does NOT redeclare —
    /// mirror of `triangle_frag_dbg_bits_not_redeclared` (line 189)
    /// pattern. The positive coverage that the values flow through
    /// correctly lives in `generated_header_contains_all_defines`
    /// (verifies each `#define` is emitted with the right value).
    #[test]
    fn water_frag_motion_enum_matches() {
        let src = include_str!("../shaders/water.frag");
        for name in [
            "WATER_CALM",
            "WATER_RIVER",
            "WATER_RAPIDS",
            "WATER_WATERFALL",
        ] {
            let needle = format!("const uint {name}");
            assert!(
                !src.contains(&needle),
                "water.frag must not redeclare {name} — \
                 the #define from shader_constants.glsl is the source of truth (#1256)",
            );
        }
    }

    /// TD4-206 / #1162 — `triangle.frag` must NOT redeclare any of the
    /// `DBG_*` bit flags (the shared `DBG_BITS` catalog) as `const uint`.
    /// The `#define`d values from the included `shader_constants.glsl` are
    /// the single source of truth. A local `const uint DBG_FOO = 0xN u;`
    /// after `#include` shadows the macro and breaks recompile-from-source
    /// (textually substitutes to `const uint 1u = 0x1u;`). Positive
    /// coverage that the value flows through correctly lives in
    /// `generated_header_contains_all_defines` (verifies each `#define`
    /// is emitted with the right value) — both tests now iterate the same
    /// `DBG_BITS` list, so they cannot drift (#1482).
    #[test]
    fn triangle_frag_dbg_bits_not_redeclared() {
        let src = include_str!("../shaders/triangle.frag");
        for (name, _) in DBG_BITS {
            let needle = format!("const uint {name}");
            assert!(
                !src.contains(&needle),
                "triangle.frag must not redeclare {name} — \
                 the #define from shader_constants.glsl is the source of truth (#1162)",
            );
        }
    }

    #[test]
    fn material_lobe_view_is_an_explicit_compound_selector() {
        assert_eq!(
            DBG_VIZ_MATERIAL_LOBES,
            DBG_VIZ_MATERIAL_STATE | DBG_VIZ_SELECTED_LIGHT
        );
        let src = include_str!("../shaders/triangle.frag");
        assert!(src.contains("(dbgFlags & DBG_VIZ_MATERIAL_LOBES) == DBG_VIZ_MATERIAL_LOBES"));
        assert!(src.contains("MAT_FLAG_TRANSLUCENCY"));
        assert!(src.contains("MAT_FLAG_PBR_BSDF"));
        let lobe_view = src
            .find("(dbgFlags & DBG_VIZ_MATERIAL_LOBES) == DBG_VIZ_MATERIAL_LOBES")
            .expect("material-lobe compound view branch");
        let glass_ior = src
            .find("if (glassIORAllowed)")
            .expect("thick-glass IOR branch");
        assert!(
            lobe_view < glass_ior,
            "material-lobe oracle must run before thick glass returns early"
        );
    }

    #[test]
    fn rt_lod_view_precedes_constituent_debug_views() {
        assert_eq!(DBG_VIZ_RT_LOD, DBG_VIZ_MATERIAL_STATE | DBG_VIZ_GI_BOUNCE);
        let src = include_str!("../shaders/triangle.frag");
        let compound = src
            .find("(dbgFlags & DBG_VIZ_RT_LOD) == DBG_VIZ_RT_LOD")
            .expect("rtLOD compound view branch");
        let constituent = src
            .find("(dbgFlags & DBG_VIZ_GI_BOUNCE) != 0u")
            .expect("GI constituent view branch");
        assert!(compound < constituent);
        let glass_ior = src
            .find("if (glassIORAllowed)")
            .expect("thick-glass IOR branch");
        assert!(
            compound < glass_ior,
            "rtLOD oracle must run before thick glass returns early"
        );
    }

    #[test]
    fn rt_lod_scale_and_counters_are_explicit_diagnostic_contracts() {
        let shader = include_str!("../shaders/triangle.frag");
        let bindings = include_str!("../shaders/include/bindings.glsl");
        let draw = include_str!("vulkan/context/draw.rs");
        assert!(shader.contains("renderDebug.y == 0u"));
        assert!(shader.contains("uintBitsToFloat(renderDebug.y)"));
        assert!(shader.contains("bool rtLodTelemetryEnabled = renderDebug.z != 0u"));
        for counter in [
            "rtLodFragments",
            "rtLodBin0",
            "rtLodBin1",
            "rtLodBin2",
            "rtLodBin3",
            "rtReflectionTraced",
            "rtReflectionLodCulled",
            "rtGiTraced",
            "rtGiLodCulled",
        ] {
            assert!(
                bindings.contains(counter),
                "missing shader ABI counter {counter}"
            );
            assert!(shader.contains(&format!("rayBudget.{counter}")));
        }
        assert!(draw.contains("rt_test_lod_scale_bits.unwrap_or(0)"));
        assert!(draw.contains("u32::from(self.renderer_config.rt_test_lod_telemetry)"));
        assert!(
            shader.find("if (rtLodTelemetryEnabled)").unwrap()
                < shader.find("atomicAdd(rayBudget.rtLodFragments").unwrap(),
            "shipping frames must skip the diagnostic atomics"
        );
    }

    #[test]
    fn shadow_visibility_view_is_raw_and_distinguishes_no_sample() {
        assert_eq!(
            DBG_VIZ_SHADOW_VISIBILITY,
            DBG_VIZ_SELECTED_LIGHT | DBG_VIZ_DIRECT
        );
        let src = include_str!("../shaders/triangle.frag");
        let visibility = src
            .find("(dbgFlags & DBG_VIZ_SHADOW_VISIBILITY) == DBG_VIZ_SHADOW_VISIBILITY")
            .expect("shadow-visibility compound view branch");
        let selected = src
            .find("(dbgFlags & DBG_VIZ_SELECTED_LIGHT) != 0u")
            .expect("selected-light constituent view branch");
        let direct = src
            .find("(dbgFlags & DBG_VIZ_DIRECT) != 0u")
            .expect("direct constituent view branch");
        assert!(visibility < selected && visibility < direct);
        assert!(src.contains("selectedVisibilityDebug = visibility"));
        assert!(src.contains("vec3(1.0, 0.0, 1.0)"));
    }

    /// TD4-208 / #1151 — `cluster_cull.comp` must NOT redeclare
    /// `THREADS_PER_CLUSTER` as a `const uint`. The `#define`d value
    /// from the included `shader_constants.glsl` is the single source
    /// of truth. Positive coverage that the value flows through
    /// correctly lives in `generated_header_contains_all_defines`.
    #[test]
    fn cluster_cull_threads_per_cluster_not_redeclared() {
        let src = include_str!("../shaders/cluster_cull.comp");
        assert!(
            !src.contains("const uint THREADS_PER_CLUSTER"),
            "cluster_cull.comp must not redeclare THREADS_PER_CLUSTER — \
             the #define from shader_constants.glsl is the source of truth (#1151)",
        );
    }

    /// #2229 / REN-D3-02 — `volumetrics_inject.comp` must NOT redeclare
    /// `FOG_VOLUME_CLUSTER_DIM`/`MAX_FOG_VOLUMES_PER_CLUSTER` (or the
    /// pre-fix shorter name `FOG_CLUSTER_DIM`) as local `const uint`s. Both
    /// were previously hand-written directly in the shader with no shared
    /// source against the Rust-side `vulkan::volumetrics` copies — the
    /// exact defect class #1190/#1401 already fixed once for other
    /// constant pairs. Positive coverage that the values flow through
    /// lives in `generated_header_contains_all_defines`.
    #[test]
    fn volumetrics_fog_cluster_constants_not_redeclared() {
        let src = include_str!("../shaders/volumetrics_inject.comp");
        for needle in [
            "const uint FOG_CLUSTER_DIM",
            "const uint FOG_VOLUME_CLUSTER_DIM",
            "const uint MAX_FOG_VOLUMES_PER_CLUSTER",
        ] {
            assert!(
                !src.contains(needle),
                "volumetrics_inject.comp must not redeclare {needle} — \
                 the #define from shader_constants.glsl is the source of truth (#2229)",
            );
        }
        for name in [
            "FOG_VOLUME_PROFILE_HOMOGENEOUS",
            "FOG_VOLUME_PROFILE_SMOKE",
            "FOG_VOLUME_PROFILE_FLAME",
            "FOG_VOLUME_PROFILE_EXPLOSION",
        ] {
            assert!(
                !src.contains(&format!("const float {name}")),
                "volumetrics_inject.comp must consume generated {name}, not redeclare it",
            );
        }
    }

    #[test]
    fn volumetric_debug_view_isolated_after_froxel_integration() {
        let src = include_str!("../shaders/composite.frag");
        assert!(src.contains("debugMode != RENDER_DEBUG_VOLUMETRIC_TERM"));
        assert!(src.contains("sampledVolume = vol;"));
        assert!(src.contains("debugMode == RENDER_DEBUG_VOLUMETRIC_TERM"));
        assert!(src.contains("max(mappedRadiance, vec3(opacity))"));
    }

    /// #2045 (TD7-101) — `triangle.frag` must NOT redeclare
    /// `INST_RENDER_LAYER_SHIFT`/`_MASK` (the pre-fix hand-written
    /// names) or `INSTANCE_RENDER_LAYER_SHIFT`/`_MASK` (the generated
    /// names) as a local `const uint`. Pre-fix, these two were
    /// hand-written directly in the shader with no lockstep test — a
    /// regression back to that pattern would silently drop the
    /// `#define`d values from `shader_constants.glsl`.
    #[test]
    fn triangle_frag_render_layer_bits_not_redeclared() {
        let src = include_str!("../shaders/triangle.frag");
        for needle in [
            "const uint INST_RENDER_LAYER_SHIFT",
            "const uint INST_RENDER_LAYER_MASK",
            "const uint INSTANCE_RENDER_LAYER_SHIFT",
            "const uint INSTANCE_RENDER_LAYER_MASK",
        ] {
            assert!(
                !src.contains(needle),
                "triangle.frag must not redeclare {needle} — \
                 the #define from shader_constants.glsl is the source of truth (#2045)",
            );
        }
    }

    /// #1190 (TD4-NEW-01) — `triangle.frag` must NOT redeclare any
    /// `MAT_FLAG_*` bit as a local `const uint`. The `#define`d
    /// values from the included `shader_constants.glsl` are the
    /// single source of truth, mirrored from `material_flag::*` in
    /// `crates/renderer/src/vulkan/material.rs`. A local
    /// `const uint MAT_FLAG_FOO = 0xN u;` after `#include` shadows
    /// the macro and breaks recompile-from-source (textually
    /// substitutes to `const uint 1u = 0x1u;`).
    #[test]
    fn triangle_frag_mat_flag_bits_not_redeclared() {
        let src = include_str!("../shaders/triangle.frag");
        for name in [
            "MAT_FLAG_VERTEX_COLOR_EMISSIVE",
            "MAT_FLAG_EFFECT_SOFT",
            "MAT_FLAG_EFFECT_PALETTE_COLOR",
            "MAT_FLAG_EFFECT_PALETTE_ALPHA",
            "MAT_FLAG_EFFECT_LIT",
            "MAT_FLAG_THIN_GLASS",
        ] {
            let needle = format!("const uint {name}");
            assert!(
                !src.contains(&needle),
                "triangle.frag must not redeclare {name} — \
                 the #define from shader_constants.glsl is the source of truth (#1190)",
            );
        }
    }

    /// #1401 — `triangle.frag` must NOT redeclare `MATERIAL_KIND_*`
    /// as local `const uint`. The `#define`d values from the included
    /// `shader_constants.glsl` are the single source of truth,
    /// mirrored from `scene_buffer/constants.rs`. A local
    /// `const uint MATERIAL_KIND_GLASS = 100u;` after `#include`
    /// shadows the macro and breaks recompile-from-source.
    #[test]
    fn triangle_frag_material_kind_not_redeclared() {
        let src = include_str!("../shaders/triangle.frag");
        for name in [
            "MATERIAL_KIND_GLASS",
            "MATERIAL_KIND_EFFECT_SHADER",
            "MATERIAL_KIND_NO_LIGHTING",
            "MATERIAL_KIND_FIRE_REFRACTION",
        ] {
            let needle = format!("const uint {name}");
            assert!(
                !src.contains(&needle),
                "triangle.frag must not redeclare {name} — \
                 the #define from shader_constants.glsl is the source of truth (#1401)",
            );
        }
    }

    /// Fire refraction runs in the blended composition phase. Its HDR
    /// replacement coverage is deliberately strength², while auxiliary
    /// albedo/indirect coverage stays zero so the proxy cannot reappear as
    /// a dark rectangle during the later composite.
    #[test]
    fn fire_refraction_preserves_opaque_auxiliary_buffers() {
        let src = include_str!("../shaders/triangle.frag");
        let start = src
            .find("if (mat.materialKind == MATERIAL_KIND_FIRE_REFRACTION)")
            .expect("fire-refraction shader branch");
        let end = src[start..]
            .find("// ── FO3/FNV BSShaderNoLightingProperty")
            .map(|offset| start + offset)
            .expect("end of fire-refraction shader branch");
        let branch = &src[start..end];

        assert!(branch.contains("float proxyCoverage = distortionStrength * distortionStrength;"));
        assert!(branch.contains("outColor = vec4(distortedScene.rgb, proxyCoverage);"));
        assert!(branch.contains("outRawIndirect = vec4(0.0);"));
        assert!(branch.contains("outAlbedo = vec4(0.0);"));
    }

    /// #1401 — Pin shader-side `MATERIAL_KIND_*` values against the
    /// authoritative Rust constants in `scene_buffer/constants.rs`.
    #[test]
    fn material_kind_matches_scene_buffer_consts() {
        use crate::vulkan::scene_buffer::{
            MATERIAL_KIND_EFFECT_SHADER as SB_EFFECT_SHADER,
            MATERIAL_KIND_FIRE_REFRACTION as SB_FIRE_REFRACTION, MATERIAL_KIND_GLASS as SB_GLASS,
            MATERIAL_KIND_NO_LIGHTING as SB_NO_LIGHTING,
        };
        assert_eq!(MATERIAL_KIND_GLASS, SB_GLASS);
        assert_eq!(MATERIAL_KIND_EFFECT_SHADER, SB_EFFECT_SHADER);
        assert_eq!(MATERIAL_KIND_NO_LIGHTING, SB_NO_LIGHTING);
        assert_eq!(MATERIAL_KIND_FIRE_REFRACTION, SB_FIRE_REFRACTION);
    }

    /// Shared scan for `<accessor> & N` where `N` is a bare numeric
    /// literal instead of a `#define`d `INSTANCE_FLAG_*` name. `accessor`
    /// is matched as a plain substring (e.g. `"inst.flags"` for the
    /// triangle shaders' struct-field access, or `"flags"` for
    /// `caustic_splat.comp`'s local variable — case-sensitive, so it
    /// doesn't false-match `sceneFlags` / `render_debug_flags`, whose
    /// `Flags`/`_flags` casing or trailing context never lands on a
    /// bare `accessor & digit` pattern). Skips comment lines. The regex
    /// would be `accessor\s*&\s*\d+u`, but a hand-rolled scan keeps the
    /// test free of regex deps.
    fn assert_no_bare_flags_literal(path: &str, src: &str, accessor: &str) {
        for (lineno, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("/*") {
                continue;
            }
            let Some(start) = line.find(accessor) else {
                continue;
            };
            let rest = &line[start + accessor.len()..];
            // The next non-whitespace char must be either nothing
            // (declaration like `<accessor> = ...`), `.` (field
            // access — there is none today, but future-proof),
            // or `&`. If it's `&`, the token immediately after
            // the `&` and whitespace must NOT be a digit.
            let rest_trimmed = rest.trim_start();
            let Some(after_amp) = rest_trimmed.strip_prefix('&') else {
                continue;
            };
            let after_amp_trimmed = after_amp.trim_start();
            let Some(first_char) = after_amp_trimmed.chars().next() else {
                continue;
            };
            assert!(
                !first_char.is_ascii_digit(),
                "{path}:{} uses bare numeric literal on `{accessor}`; \
                 use the `INSTANCE_FLAG_*` `#define` from shader_constants.glsl. \
                 Offending line: `{}`",
                lineno + 1,
                line.trim(),
            );
        }
    }

    /// #1190 (TD4-NEW-01) — `triangle.frag` + `triangle.vert` must
    /// NOT test `inst.flags` with bare numeric literals. Every
    /// active `inst.flags & N` site must use a `#define`d
    /// `INSTANCE_FLAG_*` name from the included
    /// `shader_constants.glsl`. The flat_shading bit (formerly
    /// pinned at 128u by a single-purpose test) is now covered here
    /// alongside every other instance-flag bit.
    ///
    /// This catches both the recurrence of the
    /// `& 128u` / `& 8u` / `& 2u` / `& 1u` patterns the original
    /// audit flagged, and any future hand-rolled bit added without
    /// going through `shader_constants_data.rs`.
    #[test]
    fn triangle_shaders_use_named_instance_flag_constants() {
        for (path, src) in [
            ("triangle.frag", include_str!("../shaders/triangle.frag")),
            ("triangle.vert", include_str!("../shaders/triangle.vert")),
        ] {
            assert_no_bare_flags_literal(path, src, "inst.flags");
        }
    }

    /// #1234 / #1934 (CAUSTIC-D14-01) — `caustic_splat.comp` reads its
    /// per-instance flags into a local `flags` variable
    /// (`uint flags = instances[instIdx].flags;`) and tests it as
    /// `flags & INSTANCE_FLAG_CAUSTIC_SOURCE`. The #1234 fix (bare `4u` →
    /// the named constant) had no regression coverage: this shader isn't
    /// in `triangle_shaders_use_named_instance_flag_constants`'s list, and
    /// even if it were, that test searches for the `inst.flags` struct-
    /// access token, not caustic's local-variable accessor — a revert to
    /// `flags & 4u` would compile clean and pass the whole suite. Reuses
    /// the same bare-literal scan with `"flags"` as the accessor.
    #[test]
    fn caustic_splat_comp_uses_named_instance_flag_constant() {
        assert_no_bare_flags_literal(
            "caustic_splat.comp",
            include_str!("../shaders/caustic_splat.comp"),
            "flags",
        );
    }

    /// #1190 (TD4-NEW-01) — The shader-side mirror of `INSTANCE_FLAG_*`
    /// in `shader_constants_data.rs` must equal the authoritative
    /// Rust-side values in `scene_buffer/constants.rs`. Two layers,
    /// one truth: drift here means the shader and the CPU pipeline
    /// disagree on which bit means which thing.
    #[test]
    fn instance_flag_bits_match_scene_buffer_consts() {
        use crate::vulkan::scene_buffer::{
            INSTANCE_FLAG_ALPHA_BLEND as SB_ALPHA_BLEND,
            INSTANCE_FLAG_CAUSTIC_SOURCE as SB_CAUSTIC_SOURCE,
            INSTANCE_FLAG_DIFFUSE_ALPHA as SB_DIFFUSE_ALPHA,
            INSTANCE_FLAG_FLAT_SHADING as SB_FLAT_SHADING,
            INSTANCE_FLAG_NON_UNIFORM_SCALE as SB_NON_UNIFORM_SCALE,
            INSTANCE_FLAG_TERRAIN_SPLAT as SB_TERRAIN_SPLAT,
        };
        assert_eq!(INSTANCE_FLAG_NON_UNIFORM_SCALE, SB_NON_UNIFORM_SCALE);
        assert_eq!(INSTANCE_FLAG_ALPHA_BLEND, SB_ALPHA_BLEND);
        assert_eq!(INSTANCE_FLAG_CAUSTIC_SOURCE, SB_CAUSTIC_SOURCE);
        assert_eq!(INSTANCE_FLAG_TERRAIN_SPLAT, SB_TERRAIN_SPLAT);
        assert_eq!(INSTANCE_FLAG_FLAT_SHADING, SB_FLAT_SHADING);
        assert_eq!(INSTANCE_FLAG_DIFFUSE_ALPHA, SB_DIFFUSE_ALPHA);
    }

    /// #2045 (TD7-101) — `INSTANCE_RENDER_LAYER_SHIFT`/`_MASK` were
    /// previously hand-written directly in `triangle.frag` with no
    /// lockstep test, unlike every other `INSTANCE_FLAG_*` bit pinned by
    /// `instance_flag_bits_match_scene_buffer_consts` above. Now sourced
    /// from the generated header; this pins the shader-side mirror in
    /// `shader_constants_data.rs` equal to the authoritative
    /// `scene_buffer::constants` values so `RenderLayer`'s bit-packing
    /// can't silently drift between the two layers.
    #[test]
    fn instance_render_layer_bits_match_scene_buffer_consts() {
        use crate::vulkan::scene_buffer::{
            INSTANCE_RENDER_LAYER_MASK as SB_RENDER_LAYER_MASK,
            INSTANCE_RENDER_LAYER_SHIFT as SB_RENDER_LAYER_SHIFT,
        };
        assert_eq!(INSTANCE_RENDER_LAYER_SHIFT, SB_RENDER_LAYER_SHIFT);
        assert_eq!(INSTANCE_RENDER_LAYER_MASK, SB_RENDER_LAYER_MASK);
    }

    /// #1190 (TD4-NEW-01) — Same pin, for `MAT_FLAG_*` against
    /// `material_flag::*` in `vulkan/material.rs`.
    #[test]
    fn material_flag_bits_match_material_consts() {
        use crate::vulkan::material::material_flag;
        assert_eq!(
            MAT_FLAG_VERTEX_COLOR_EMISSIVE,
            material_flag::VERTEX_COLOR_EMISSIVE
        );
        assert_eq!(MAT_FLAG_EFFECT_SOFT, material_flag::EFFECT_SOFT);
        assert_eq!(
            MAT_FLAG_EFFECT_PALETTE_COLOR,
            material_flag::EFFECT_PALETTE_COLOR
        );
        assert_eq!(
            MAT_FLAG_EFFECT_PALETTE_ALPHA,
            material_flag::EFFECT_PALETTE_ALPHA
        );
        assert_eq!(MAT_FLAG_EFFECT_LIT, material_flag::EFFECT_LIT);
        // Bits 5-9 — Disney BSDF / SSS / model-space-normals suite
        // (#1285, was hand-written in triangle.frag without this pin).
        assert_eq!(MAT_FLAG_PBR_BSDF, material_flag::PBR_BSDF);
        assert_eq!(MAT_FLAG_TRANSLUCENCY, material_flag::TRANSLUCENCY);
        assert_eq!(
            MAT_FLAG_MODEL_SPACE_NORMALS,
            material_flag::MODEL_SPACE_NORMALS
        );
        assert_eq!(
            MAT_FLAG_TRANSLUCENCY_THICK_OBJECT,
            material_flag::TRANSLUCENCY_THICK_OBJECT
        );
        assert_eq!(
            MAT_FLAG_TRANSLUCENCY_MIX_ALBEDO,
            material_flag::TRANSLUCENCY_MIX_ALBEDO
        );
        assert_eq!(MAT_FLAG_THIN_GLASS, material_flag::THIN_GLASS);
        // #2826 (REN-D19-02) — model-space normal map Z-source bit.
        assert_eq!(
            MAT_FLAG_MSN_HAS_AUTHORED_Z,
            material_flag::MSN_HAS_AUTHORED_Z
        );
        // Lighting-influence shift — a byte-field offset, not a single-bit flag.
        assert_eq!(MAT_FLAG_EFFECT_LI_SHIFT, material_flag::EFFECT_LI_SHIFT);
        // BGSM_AUTHORED intentionally NOT mirrored to GLSL — see build.rs.
    }

    /// #1799 / PERF-D5-NEW-01 — the shipped default must keep the legacy
    /// WRS arm preprocessed OUT of `triangle.frag`. Flipping this back to
    /// `1` (e.g. to A/B) is a deliberate, source-controlled, recompile-
    /// required action; it must never silently become the shipped default.
    #[test]
    fn legacy_wrs_arm_defaults_to_disabled() {
        assert_eq!(
            ENABLE_LEGACY_WRS, 0,
            "ENABLE_LEGACY_WRS must default to 0 (compiled out) — flipping \
             it to 1 re-enables the per-frame register/local-memory cost \
             this issue exists to eliminate"
        );
    }

    /// #1799 / PERF-D5-NEW-01 — the legacy 16-slot WRS reservoir arrays
    /// (`resLight`/`resWSel`) must be declared strictly inside an
    /// `#if ENABLE_LEGACY_WRS` / `#endif` block, not merely read/written
    /// behind a runtime `if`. A runtime-only guard around the *usage*
    /// doesn't stop the compiler from still declaring — and therefore
    /// budgeting the per-invocation register / local-memory footprint
    /// of — the arrays on every frame, including the ~100% of production
    /// frames that take the ReSTIR path and never touch them.
    #[test]
    fn triangle_frag_legacy_wrs_arrays_are_compile_time_gated() {
        let src = include_str!("../shaders/triangle.frag");

        let gate_pos = src
            .find("#if ENABLE_LEGACY_WRS")
            .expect("triangle.frag must have an ENABLE_LEGACY_WRS compile-time gate");
        let decl_pos = src
            .find("uint  resLight[NUM_RESERVOIRS];")
            .expect("triangle.frag must declare the legacy resLight reservoir array");
        let endif_pos = src[gate_pos..]
            .find("#endif")
            .map(|i| gate_pos + i)
            .expect("the ENABLE_LEGACY_WRS gate must be closed with #endif");

        assert!(
            gate_pos < decl_pos && decl_pos < endif_pos,
            "resLight[NUM_RESERVOIRS] must be declared strictly inside the \
             FIRST #if ENABLE_LEGACY_WRS / #endif block (#1799 / PERF-D5-NEW-01)"
        );
    }

    /// The renderer-evaluation suite relies on these switches representing
    /// independent estimator dimensions. Keep this contract close to the
    /// shader source so a refactor cannot silently turn the A/B captures into
    /// equivalent modes.
    #[test]
    fn triangle_frag_restir_reuse_dimensions_are_independently_gated() {
        let src = include_str!("../shaders/triangle.frag");

        assert!(
            src.contains("bool useSpatial = !disableRestirReuse")
                && src.contains("(dbgFlags & DBG_DISABLE_SPATIAL) == 0u;"),
            "DBG_DISABLE_SPATIAL must independently gate spatial reservoir reuse"
        );
        assert!(
            src.contains("bool useTemporal = !disableRestirReuse")
                && src.contains("(dbgFlags & DBG_DISABLE_TEMPORAL) == 0u;"),
            "DBG_DISABLE_TEMPORAL must independently gate temporal reservoir reuse"
        );
        assert!(
            src.contains("(dbgFlags & DBG_DISABLE_RESTIR) != 0u;")
                && src.matches("!disableRestirReuse").count() >= 2,
            "DBG_DISABLE_RESTIR must disable both reuse dimensions even when \
             the legacy WRS arm is compiled out"
        );
        // #2554 — the `&& shadowFade > 0.01` clause this used to pin was
        // removed; gating reservoir reuse on the distance fade is part of
        // what zeroed distant lights. This test's subject is the `useTemporal`
        // gate itself, which is unchanged. The fade's own contract now lives
        // in `restir::tests::restir_far_field_converges_to_unshadowed_radiance`.
        assert!(
            src.contains("if (useTemporal && stableTemporalSurface"),
            "temporal reprojection must be conditional on useTemporal"
        );
    }

    #[test]
    fn triangle_frag_direct_visualization_excludes_indirect_attachments() {
        let src = include_str!("../shaders/triangle.frag");
        let branch = src
            .split("} else if (viewDirectOnly) {")
            .nth(1)
            .expect("triangle.frag must implement the structured direct-only view");
        let branch = branch
            .split("} else if")
            .next()
            .expect("direct-only branch must terminate before normal output");
        assert!(branch.contains("outColor = vec4(directLight, 1.0);"));
        assert!(branch.contains("outRawIndirect = vec4(0.0);"));
        assert!(branch.contains("outAlbedo = vec4(1.0);"));
    }

    #[test]
    fn triangle_frag_clamps_gi_sample_luminance_at_svgf_boundary() {
        let src = include_str!("../shaders/triangle.frag");
        assert!(
            src.contains("float boundedPathLum = pathLuminance(boundedPathSample);")
                && src.contains("boundedPathSample *= GI_SAMPLE_LUMINANCE_CLAMP / boundedPathLum;")
                && src.contains("indirect = boundedPathSample;"),
            "the complete GI path sample must be chroma-preserving luminance-clamped before SVGF"
        );
        assert!(
            !src.contains("indirect = min(pathRadiance, vec3(8.0));"),
            "the old per-channel 8x ceiling admits white fireflies into SVGF"
        );
    }

    #[test]
    fn triangle_frag_does_not_modulate_ambient_with_single_sample_gi_ao() {
        let src = include_str!("../shaders/triangle.frag");
        assert!(
            src.contains("float combinedAO = ((dbgFlags & DBG_DISABLE_AO) != 0u) ? 1.0 : ao;"),
            "ambient occlusion must use the stable SSAO signal"
        );
        assert!(
            !src.contains("min(ao, rtAO)")
                && !src.contains("float rtAO")
                && !src.contains("rtAO = mix"),
            "the one-sample GI path must not become an undenoised AO multiplier"
        );
    }

    #[test]
    fn triangle_frag_metallic_ambient_is_demodulated_exactly_once() {
        let src = include_str!("../shaders/triangle.frag");
        assert!(
            src.contains("vec3 metallicAmbient = sceneFlags.yzw * metalness * 0.5;"),
            "the indirect metallic ambient must stay lighting-only so the \
             composite's albedo multiplication supplies conductor tint once"
        );
        assert!(
            !src.contains("metallicAmbient = sceneFlags.yzw * albedo"),
            "including albedo here would produce albedo-squared after composite"
        );
    }

    /// Skyrim interiors frequently carry all authored ambience in XCLL's
    /// DALC cube and leave the legacy flat ambient black. Rough conductors
    /// outside the sharp RT-reflection reach must therefore use the DALC cube
    /// as their low-frequency environment probe, sampled in the reflection
    /// direction rather than the surface-normal direction. Because DALC is
    /// diffuse irradiance, the specular path must convert it to an approximate
    /// incident radiance instead of injecting the full irradiance as a chrome
    /// reflection.
    #[test]
    fn triangle_frag_metal_reflection_fallback_uses_energy_normalized_dalc_probe() {
        let src = include_str!("../shaders/triangle.frag");

        assert!(
            src.contains("ambientFallback = dalcFlags.x > 0.5")
                && src.contains("? sampleDalcCube(R) * (1.0 / PI)"),
            "rough-metal reflection fallback must convert DALC irradiance along R to radiance"
        );
    }

    /// Rough reflection detail is already blurred by the hit mip and its
    /// energy is attenuated once when added to `Lo`. Applying the same
    /// `(1 - roughness)` factor to the ray/fallback mix suppresses authored
    /// rough-metal scene detail a second time.
    #[test]
    fn triangle_frag_rough_reflection_detail_is_not_double_attenuated() {
        let src = include_str!("../shaders/triangle.frag");

        assert!(
            src.contains("envColor = mix(ambientFallback, reflResult.rgb, rayFade);")
                && !src.contains("reflClarity * rayFade"),
            "rough reflection must use distance/LOD fade only; roughness is handled by mip blur and the final energy attenuation"
        );
    }

    /// Thin glass must be a surface-wide behavior decision. A regression to
    /// the per-fragment IOR budget gate recreates the close-range checkerboard
    /// when a large dome consumes the remaining budget non-uniformly.
    #[test]
    fn triangle_frag_thin_glass_bypasses_volume_ior_and_faces_viewer() {
        let src = include_str!("../shaders/triangle.frag");

        assert!(
            src.contains("bool glassIORAllowed = isGlass && !isThinGlass"),
            "thin glass must be excluded before the per-fragment ray-budget claim"
        );
        assert!(
            src.contains("if (dot(glassViewNormal, V) < 0.0)"),
            "two-sided glass must orient its smooth coverage normal toward the viewer"
        );
        assert!(
            src.contains("N = glassViewNormal;"),
            "the non-IOR glass base path must use the same view-facing macro normal"
        );
    }

    /// Material identity must not depend on a per-fragment LOD or roughness
    /// value. The old tier-3 arm set `isGlass=false`, so a glass mesh crossed
    /// into opaque legacy/PBR shading with distance; derivative variation made
    /// the transition happen triangle-by-triangle on large curved meshes.
    #[test]
    fn triangle_frag_keeps_glass_identity_and_ior_across_rt_lods() {
        let src = include_str!("../shaders/triangle.frag");
        let executable: String = src
            .lines()
            .map(str::trim_start)
            .filter(|line| !line.starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            executable.contains("bool isGlass = mat.materialKind == MATERIAL_KIND_GLASS;"),
            "glass classification must key only on the canonical material kind"
        );
        assert!(
            !executable.contains("isGlass = false") && !executable.contains("roughness < 0.35"),
            "roughness/LOD must not demote glass into an opaque shading family"
        );
        assert!(
            executable.contains(
                "bool glassIORAllowed = isGlass && !isThinGlass\n&& reflectionGlassRayEnabled && !isWindow;"
            ) && !executable.contains("rtLOD < RT_LOD_IOR"),
            "thick glass transmission must be adaptive-quality bounded, not distance-disabled"
        );
    }

    /// Complex legacy meshes can place many independently-authored glass
    /// submeshes along one view ray. Interface depth grows only when the
    /// adaptive controller has measured headroom, and every extra query is
    /// included in telemetry. The tier must apply coherently to all eligible
    /// fragments; unordered atomic admission produces visible stipple.
    #[test]
    fn triangle_frag_scales_glass_interface_depth_with_honest_ray_cost() {
        let src = include_str!("../shaders/triangle.frag");

        assert!(src.contains("refractPassthruBudget = 2 + int(budgetTier) * 2;"));
        assert!(src.contains("glassRayCost = GLASS_RAY_COST + budgetTier * 2u;"));
        assert!(src.contains("const int MAX_REFRACT_PASSTHRUS = 8;"));
        assert!(src.contains("passthru < refractPassthruBudget"));
        assert!(src.contains("atomicAdd(rayBudget.rayBudgetCount, glassRayCost)"));
        assert!(
            !src.contains("old + glassRayCost <= rayBudget.glassRayLimit"),
            "glass IOR must not depend on unordered atomic winners; alpha glass \
             bypasses history, so the split is permanent salt-and-pepper noise"
        );
        assert!(
            !src.contains("REFRACT_PASSTHRU_BUDGET = 2"),
            "glass traversal must not regress to the fixed two-interface limit"
        );
    }

    /// Same-IOR overlapping shells are one optical medium. A blind boolean
    /// toggle at every committed glass hit treats an internal body/wing/scale
    /// overlap as another air boundary and produces a triangle-shaped chrome
    /// mosaic on articulated glass meshes.
    #[test]
    fn triangle_frag_tracks_glass_medium_depth_from_hit_facing() {
        let src = include_str!("../shaders/triangle.frag");
        assert!(src.contains("int glassMediumDepth = 1;"));
        assert!(src.contains("rayQueryGetIntersectionFrontFaceEXT(refrRQ, true)"));
        assert!(src.contains("bool entersGlass = !wasInsideGlass && nextMediumDepth > 0;"));
        assert!(src.contains("bool exitsGlass = wasInsideGlass && nextMediumDepth == 0;"));
        assert!(
            !src.contains("bool rayInsideGlass"),
            "glass traversal must not blindly toggle medium state at every overlap"
        );
    }

    /// Regression for #2245 (REN-D19-01): `material_sampling.glsl`'s
    /// derivative-based fallback (`perturbNormal` Path 2, and
    /// `parallaxDisplaceUV`'s matching branch) must not carry a
    /// `sign(det)` correction on top of `cross(N, T)` — the un-divided
    /// Lengyel numerator `T` already carries that sign (see the doc
    /// comment on `perturbNormal`'s Path 2 for the derivation), so a
    /// second multiply cancels it and silently reintroduces the exact
    /// mirrored-UV bug #1104 shipped to fix. Source-text guard: this bug
    /// class is invisible to `cargo test` otherwise (GLSL isn't compiled
    /// or executed by the Rust test suite).
    #[test]
    fn material_sampling_derivative_fallback_does_not_double_flip_handedness() {
        let src = include_str!("../shaders/include/material_sampling.glsl");
        assert!(
            !src.contains("screenSign") && !src.contains("uvDet"),
            "material_sampling.glsl must not reintroduce a second sign(det) \
             correction on the derivative-based T/B reconstruction (#2245)"
        );
        assert!(
            src.contains("vec3 B = cross(N, T);"),
            "perturbNormal's Path 2 must reconstruct B as a plain cross(N, T), \
             matching what the already-signed T reconstructs correctly (#2245)"
        );
    }

    /// Pin the underlying math itself (independent of GLSL, which the
    /// Rust test suite never compiles or executes): for a UV-mirrored
    /// fragment (Jacobian determinant < 0), `cross(N, T)` built from the
    /// un-divided Lengyel numerator T must already equal the true
    /// bitangent (`B_raw / det`) with no further sign correction. Two
    /// independent mirror constructions (single-axis-negate, axis-swap)
    /// both confirmed by hand before this fix landed.
    #[test]
    fn path2_cross_product_reconstructs_true_bitangent_under_uv_mirroring() {
        use byroredux_core::math::Vec3;

        struct Case {
            d_pdx: Vec3,
            d_pdy: Vec3,
            n: Vec3,
            d_uvdx: [f32; 2],
            d_uvdy: [f32; 2],
        }
        let cases = [
            Case {
                // Single-axis-negate mirror: flip U's screen-space gradient only.
                d_pdx: Vec3::new(1.0, 0.0, 0.0),
                d_pdy: Vec3::new(0.0, 1.0, 0.0),
                n: Vec3::new(0.0, 0.0, 1.0),
                d_uvdx: [-1.0, 0.0],
                d_uvdy: [0.0, 1.0],
            },
            Case {
                // Axis-swap mirror: U and V screen-space gradients swapped.
                d_pdx: Vec3::new(1.0, 0.0, 0.0),
                d_pdy: Vec3::new(0.0, 1.0, 0.0),
                n: Vec3::new(0.0, 0.0, 1.0),
                d_uvdx: [0.0, 1.0],
                d_uvdy: [1.0, 0.0],
            },
        ];

        for case in cases {
            let det = case.d_uvdx[0] * case.d_uvdy[1] - case.d_uvdx[1] * case.d_uvdy[0];
            assert!(det < 0.0, "fixture must be a mirrored (det < 0) case");

            let t_raw = case.d_pdx * case.d_uvdy[1] - case.d_pdy * case.d_uvdx[1];
            let b_raw = case.d_pdy * case.d_uvdx[0] - case.d_pdx * case.d_uvdy[0];
            let b_true = (b_raw / det).normalize();

            // perturbNormal's Path 2, current (post-fix) formula.
            let mut t = t_raw.normalize();
            t = (t - case.n * t.dot(case.n)).normalize();
            let b_reconstructed = case.n.cross(t);

            assert!(
                (b_reconstructed - b_true).length() < 1e-4,
                "cross(N, T) must match the true bitangent under mirroring: \
                 got {b_reconstructed:?}, want {b_true:?}"
            );
        }
    }
}
