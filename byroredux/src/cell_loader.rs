//! Cell scene loader — loads cells from ESM + BSA into ECS entities.
//!
//! Supports both interior cells (by editor ID) and exterior cells (by grid coords).
//! Resolves placed references (REFR/ACHR) to base objects, loads NIFs,
//! and spawns ECS entities with correct world-space transforms.
//!
//! The pipeline is split across submodules:
//!
//! - [`load`] — interior entry point (`load_cell_with_masters`) +
//!   `CellLoadResult` + `resolve_cell_lighting` + the shared
//!   `stamp_cell_root` lifecycle helper.
//! - [`unload`] — `unload_cell` teardown path + per-victim inventory
//!   instance release.
//! - [`exterior`] — `ExteriorWorldContext`, `build_exterior_world_context`,
//!   and the single-cell streaming entry point `load_one_exterior_cell`.
//! - [`references`] — the inner REFR walk, BSA NIF/SPT parse + cache
//!   path, and dispatch into `spawn`.
//! - [`spawn`] — per-placement ECS entity creation (meshes, lights,
//!   particles, collision shapes).
//! - [`partial`] — `finish_partial_import`, the main-thread drain for
//!   the M40 streaming worker.
//! - [`euler`] — Z-up → Y-up Euler-angle → quaternion conversion +
//!   `--rotation-mode` diagnostic switch.

// Items in scope here are visible to child test modules via
// `use super::*;` — kept lean but covering everything the test files
// historically relied on through the (pre-split) flat `cell_loader.rs`.
#[cfg(test)]
#[allow(unused_imports)]
use byroredux_core::ecs::components::{Inventory, ItemInstanceId};
#[cfg(test)]
#[allow(unused_imports)]
use byroredux_core::ecs::resources::ItemInstancePool;
#[cfg(test)]
#[allow(unused_imports)]
use byroredux_core::ecs::storage::EntityId;
#[cfg(test)]
#[allow(unused_imports)]
use byroredux_core::ecs::{
    CellRoot, GlobalTransform, LightSource, Material, MeshHandle, ParticleEmitter, TextureHandle,
    Transform, World,
};
#[cfg(test)]
#[allow(unused_imports)]
use byroredux_core::math::{Quat, Vec3};
#[cfg(test)]
#[allow(unused_imports)]
use std::collections::{HashMap, HashSet};
#[cfg(test)]
#[allow(unused_imports)]
use std::sync::Arc;

#[cfg(test)]
#[allow(unused_imports)]
use crate::components::{
    AlphaBlend, CellLightingRes, CellRootIndex, MaterialTextureHandles, NormalMapHandle,
    SkyParamsRes, TerrainTileSlot, TwoSided, WeatherDataRes, WeatherTransitionRes,
};

mod euler;
mod exterior;
mod index;
mod load;
mod load_order;
mod lod_bands;
mod lod_coverage;
mod lod_support;
mod nif_import_registry;
mod object_lod;
mod partial;
mod persistent_ref_index;
mod placement_lod;
pub(crate) mod precombined;
pub(crate) mod references;
mod refr;
pub(crate) mod spawn;
mod terrain;
mod terrain_lod;
mod terrain_lod_btr;
mod terrain_seam;
mod transition;
mod unload;
mod water;
mod work_budget;

pub use index::LoadedCellIndex;
#[allow(unused_imports)]
pub use transition::{
    load_interior_cell, log_transition_header, position_zup_to_yup, queue_door_transition,
    reposition_camera, rotation_zup_to_yup_quat, take_pending_transition, unload_current_interior,
    CurrentCellContext, CurrentCellRoot, CurrentExteriorContext, InteriorCellRequest,
    LoadedPluginSet, PendingCellTransition, PendingCellTransitionSlot, QueueDoorTransitionError,
    QueuedDoorTransition, TransitionDestination,
};
// `clear_current_exterior_identity` is an internal teardown helper (only
// `streaming_helpers::drain_streaming_state` calls it) — `pub(crate)`, not
// part of the crate's external `pub` surface the block above re-exports.
pub(crate) use transition::clear_current_exterior_identity;

// Public re-exports — keep the existing `crate::cell_loader::FOO`
// call sites in main.rs / streaming.rs / commands.rs working without
// further changes. `#[allow(unused_imports)]` because not every
// re-exported item is consumed by this crate's own binary — several
// only show up in external crates (tests, other workspace members)
// or as the public API surface.
#[allow(unused_imports)]
pub(crate) use nif_import_registry::{
    canonical_model_path_key, CachedNifImport, NifImportRegistry,
};
#[allow(unused_imports)]
pub(crate) use refr::{
    build_refr_texture_overlay, expand_pkin_placements, expand_scol_placements, RefrTextureOverlay,
};

pub(crate) use euler::euler_zup_to_quat_yup_refr;
pub use euler::set_refr_rotation_mode_diag;

pub(crate) use exterior::{
    begin_worldspace_persistent_cell, ExteriorCellApplyJob, ExteriorCellApplyProgress,
    PersistentCellApplyJob, PersistentCellApplyProgress,
};
#[allow(unused_imports)]
pub use exterior::{
    build_exterior_world_context, load_one_exterior_cell, ExteriorWorldContext, OneCellLoadInfo,
};
pub(crate) use load::apply_interior_cell_lighting;
#[allow(unused_imports)]
pub(crate) use load::resolve_cell_lighting;
#[allow(unused_imports)]
pub use load::{load_cell_with_masters, validate_cell_loadable, CellLoadResult};
// Re-exported for the headless `--list-cells` catalogue, which needs the
// same masters-first merged index the cell loader builds but stops after
// the parse phase.
pub(crate) use load_order::parse_record_indexes_in_load_order;
pub(crate) use lod_coverage::{
    find_full_detail_overlaps, find_overlaps, find_terrain_full_detail_overlaps, ChurnTracker,
};
pub(crate) use lod_support::{worldspace_lod_grid_origin, LodReconcileInput, LodWorkBudget};
#[allow(unused_imports)]
pub(crate) use object_lod::{stream_object_lod_blocks, unload_object_lod_block, ObjectLodBlock};
pub(crate) use partial::finish_partial_import;
pub(crate) use placement_lod::{
    stream_placement_lod_blocks, unload_placement_lod_block, PlacementLodBlock,
};
pub(crate) use terrain_lod::{stream_lod_blocks, unload_lod_block};
pub(crate) use terrain_seam::{check_seam, SeamDirection};
pub use unload::{unload_cell, unload_cells, UnloadPhaseTimings};
pub(crate) use water::{spawn_lod_water_plane, unload_lod_water_plane};
pub(crate) use work_budget::FrameTimeBudget;

// Test-only re-exports so the `use super::*;` patterns inside the
// child test modules see the helpers they exercise. Production code
// paths reach these via their owning submodule path.
#[cfg(test)]
pub(crate) use load::{register_cell_root, stamp_cell_root, stamp_cell_root_range};
#[cfg(test)]
pub(crate) use spawn::{
    count_spawnable_nif_lights, is_spawnable_nif_light, light_radius_or_default, spawn_nif_lights,
};
#[cfg(test)]
pub(crate) use unload::collect_victim_gpu_handles;
#[cfg(test)]
pub(crate) use unload::release_victim_item_instances;
#[cfg(test)]
pub(crate) use unload::release_victim_rapier_bodies;

/// Pack `BSEffectShaderProperty` flag booleans (captured in Stage 1 by
/// `BsEffectShaderData::effect_{soft,palette_color,palette_alpha,lit}`)
/// into a `GpuMaterial::material_flags`-format u32 so the renderer can
/// OR the word straight into per-frame material entries without per-bit
/// re-encoding at the import→render boundary.
///
/// `None` (mesh has no `BSEffectShaderProperty`) yields `0`; the
/// FO3/FNV `BSShaderNoLightingProperty` path also flows through this
/// helper at `scene.rs` / `cell_loader.rs` with a `None` arg because
/// that block lacks the SLSF1/SLSF2 vocabulary entirely.
///
/// Bit layout is pinned by `byroredux_renderer::vulkan::material::material_flag::EFFECT_*`
/// — see #890 Stage 2.
pub(crate) fn pack_effect_shader_flags(
    eff: Option<&byroredux_nif::import::BsEffectShaderData>,
) -> u32 {
    use byroredux_renderer::vulkan::material::material_flag::{
        EFFECT_LIT, EFFECT_LI_SHIFT, EFFECT_PALETTE_ALPHA, EFFECT_PALETTE_COLOR, EFFECT_SOFT,
    };
    let Some(es) = eff else {
        return 0;
    };
    let mut flags = 0u32;
    if es.effect_soft {
        flags |= EFFECT_SOFT;
    }
    if es.effect_palette_color {
        flags |= EFFECT_PALETTE_COLOR;
    }
    if es.effect_palette_alpha {
        flags |= EFFECT_PALETTE_ALPHA;
    }
    if es.effect_lit {
        flags |= EFFECT_LIT;
    }
    // Pack the 0-255 lighting influence byte into bits 16-23 of material_flags.
    // The fragment shader's EFFECT_LIT branch reads this as a [0,1] scale via
    // `float((materialFlags >> MAT_FLAG_EFFECT_LI_SHIFT) & 0xFFu) / 255.0`.
    // When EFFECT_LIT is unset the byte is present but ignored.
    flags |= (es.lighting_influence as u32) << EFFECT_LI_SHIFT;
    flags
}

/// Pack source-normalized external-material properties
/// ([`ImportedMaterial::is_pbr`], [`ImportedMaterial::has_translucency`],
/// [`ImportedMaterial::model_space_normals`], palette mode, and related
/// shape flags) into the canonical
/// `material_flag::{PBR_BSDF, TRANSLUCENCY, MODEL_SPACE_NORMALS, …}`
/// bit layout. Sibling to [`pack_effect_shader_flags`] — both contribute
/// to the same `Material.effect_shader_flags` u32 by OR-composition at
/// the importer boundary, then ride through to
/// `GpuMaterial.material_flags` via `DrawCommand::to_gpu_material`.
///
/// Phase 2a (this packer) lands the data plumbing; Phase 2b (the shader
/// consumers) has since shipped — `triangle.frag` reads `MAT_FLAG_PBR_BSDF`
/// (Disney lobe), `MAT_FLAG_TRANSLUCENCY` (SSS), `MAT_FLAG_MODEL_SPACE_NORMALS`
/// (normal decode), and the `THICK_OBJECT` / `MIX_ALBEDO` SSS-shape bits. See
/// #1077 / FO4-D6-003 (Phase 2a) and #1147 (Phase 2b). The same bits are also
/// set by `pack_effect_shader_flags` for the effect-mesh path.
///
/// [`ImportedMaterial::is_pbr`]: byroredux_nif::import::ImportedMaterial::is_pbr
/// [`ImportedMaterial::has_translucency`]: byroredux_nif::import::ImportedMaterial::has_translucency
/// [`ImportedMaterial::model_space_normals`]: byroredux_nif::import::ImportedMaterial::model_space_normals
pub(crate) fn pack_imported_material_flags(
    material: &byroredux_nif::import::ImportedMaterial,
) -> u32 {
    use byroredux_renderer::vulkan::material::material_flag::{
        BGSM_AUTHORED, EFFECT_PALETTE_ALPHA, EFFECT_PALETTE_COLOR, MODEL_SPACE_NORMALS, PBR_BSDF,
        THIN_GLASS, TRANSLUCENCY, TRANSLUCENCY_MIX_ALBEDO, TRANSLUCENCY_THICK_OBJECT,
    };
    let mut flags = 0u32;
    // `BGSM_AUTHORED` — set when `merge_external_material` resolved a
    // BGSM/BGEM file successfully (independent of `bgsm.pbr`, which
    // vanilla FO4 virtually never authors — sampled: 0 of 793
    // metal/cargo BGSMs in `Fallout4 - Materials.ba2`). The
    // spec-glossiness → metallic-roughness translation is entirely
    // CPU-side (`merge_external_material` writes `metalness_override` /
    // `roughness_override`, then `translate_material` resolves them into
    // `Material.{metalness,roughness}`); the shader is format-agnostic
    // and does NOT branch on this flag. The bit rides through to the GPU
    // material for debug-server inspection only — see
    // `material_flag::BGSM_AUTHORED` and `shader_constants_data.rs`.
    if material.from_bgsm {
        flags |= BGSM_AUTHORED;
    }
    if material.is_pbr {
        flags |= PBR_BSDF;
    }
    if material.has_translucency {
        flags |= TRANSLUCENCY;
    }
    if material.model_space_normals {
        flags |= MODEL_SPACE_NORMALS;
    }
    if material.thin_glass {
        flags |= THIN_GLASS;
    }
    // #1353 / FO4-D8-07 — FO4 BGSM grayscale-to-palette. EFFECT_PALETTE_COLOR
    // IS `SLSF1::Greyscale_To_PaletteColor`; setting it on a BGSM lit material
    // (one that authored a `greyscale_texture`, captured in the common
    // `greyscale_lut` role) makes the lit-path palette remap sample the
    // resolved MaterialTextureHandles entry by diffuse luminance. The
    // effect-mesh path sets the same bit via
    // `pack_effect_shader_flags`; the two live in different material-kind
    // shader branches so there is no conflict.
    //
    // #1580 — BGEM's `grayscale_to_palette_alpha` bool (BGSM has no
    // alpha-variant field, so it's always false there) picks
    // EFFECT_PALETTE_ALPHA in addition to the color bit, matching how
    // `pack_effect_shader_flags` derives the two bits independently for
    // the inline-effect-shader path.
    //
    // #2643 (SF-D9-2026-08-07-04) — the BGEM format permits authoring
    // BOTH the shared `grayscale_to_palette_color` bit and BGEM's own
    // `grayscale_to_palette_alpha` bit at once. This used to be an
    // if/else keyed on `bgsm_greyscale_lut_is_alpha` alone, so a BGEM
    // authoring both bits packed EFFECT_PALETTE_ALPHA only, silently
    // dropping the color variant. `bgsm_greyscale_lut_color` and
    // `bgsm_greyscale_lut_is_alpha` are now independent flags — OR each
    // bit in on its own, exactly like `pack_effect_shader_flags` does for
    // `effect_palette_color`/`effect_palette_alpha`.
    //
    // #2108 (SF-D9-01) — gated on `bgsm_greyscale_lut_enabled`, the
    // authored `grayscale_to_palette_color`/`_alpha` ENABLE bit
    // (`asset_provider::merge_external_material`), NOT on
    // `greyscale_lut.is_some()` alone. The slot is a legal,
    // always-serialized field — a template-inherited or mis-authored
    // BGSM/BGEM can fill it while leaving the remap disabled, and the
    // inline NIF effect-shader path (`pack_effect_shader_flags` above)
    // already gates on the real SLSF enable bit, not texture presence;
    // this mirrors that. `is_some()` stays as a belt-and-braces guard —
    // both merge arms only ever set `bgsm_greyscale_lut_enabled` at the
    // same step they fill the texture, so it should never be `true` with
    // an empty slot, but a remap with no LUT to sample would be a no-op
    // at best and reads confusingly if it slipped through.
    if material.textures.greyscale_lut.is_some() && material.bgsm_greyscale_lut_enabled {
        if material.bgsm_greyscale_lut_color {
            flags |= EFFECT_PALETTE_COLOR;
        }
        if material.bgsm_greyscale_lut_is_alpha {
            flags |= EFFECT_PALETTE_ALPHA;
        }
    }
    // #1147 Phase 2b — translucency parameter-shape bits. Only
    // meaningful when `TRANSLUCENCY` is also set, but pack them
    // unconditionally so the shader's predicate `is_thick` /
    // `mix_albedo` reads the authored value directly. The shader
    // already gates the whole SSS block on `TRANSLUCENCY`.
    if material.translucency_thick_object {
        flags |= TRANSLUCENCY_THICK_OBJECT;
    }
    if material.translucency_mix_albedo {
        flags |= TRANSLUCENCY_MIX_ALBEDO;
    }
    flags
}

#[cfg(test)]
mod pack_imported_material_flags_tests {
    //! Regression for #1147 Phase 2a (#1077 follow-up). Pins the
    //! contract that the bool-to-bit-OR packer matches the
    //! `material_flag::BGSM_*` layout the Phase 2b shader consumer
    //! will read.

    use super::pack_imported_material_flags;
    use byroredux_nif::import::ImportedMaterial;
    use byroredux_renderer::vulkan::material::material_flag::{
        EFFECT_PALETTE_COLOR, MODEL_SPACE_NORMALS, PBR_BSDF, THIN_GLASS, TRANSLUCENCY,
    };

    /// Shared defaults keep this fixture aligned with the import contract.
    fn empty_material() -> ImportedMaterial {
        ImportedMaterial::default()
    }

    #[test]
    fn all_three_flags_off_produces_zero() {
        let material = empty_material();
        assert_eq!(pack_imported_material_flags(&material), 0);
    }

    #[test]
    fn all_three_flags_on_produces_full_union() {
        let mut material = empty_material();
        material.is_pbr = true;
        material.has_translucency = true;
        material.model_space_normals = true;

        let packed = pack_imported_material_flags(&material);
        let expected = PBR_BSDF | TRANSLUCENCY | MODEL_SPACE_NORMALS;
        assert_eq!(packed, expected);
        // Sanity-check the canonical bit layout the Phase 2b shader
        // consumer will read against — bits 5, 6, 7.
        assert_eq!(packed & 0x20, PBR_BSDF);
        assert_eq!(packed & 0x40, TRANSLUCENCY);
        assert_eq!(packed & 0x80, MODEL_SPACE_NORMALS);
    }

    #[test]
    fn individual_flags_produce_individual_bits() {
        let mut material = empty_material();
        material.is_pbr = true;
        assert_eq!(pack_imported_material_flags(&material), PBR_BSDF);

        let mut material = empty_material();
        material.has_translucency = true;
        assert_eq!(pack_imported_material_flags(&material), TRANSLUCENCY);

        let mut material = empty_material();
        material.model_space_normals = true;
        assert_eq!(pack_imported_material_flags(&material), MODEL_SPACE_NORMALS);

        let mut material = empty_material();
        material.thin_glass = true;
        assert_eq!(pack_imported_material_flags(&material), THIN_GLASS);
    }

    /// #1353 / FO4-D8-07 — a BGSM that authored a greyscale-to-palette LUT
    /// AND enabled the remap (`bgsm_greyscale_lut_enabled`, forwarded from
    /// `grayscale_to_palette_color` by `merge_external_material`) must pack
    /// `EFFECT_PALETTE_COLOR` (= SLSF1 Greyscale_To_PaletteColor) so the
    /// lit-path palette remap in triangle.frag fires. Absent the path, the
    /// bit must stay clear.
    #[test]
    fn bgsm_greyscale_lut_path_sets_effect_palette_color() {
        let material = empty_material();
        assert_eq!(
            pack_imported_material_flags(&material) & EFFECT_PALETTE_COLOR,
            0,
            "no greyscale LUT path → palette flag must stay clear"
        );

        let mut material = empty_material();
        let mut pool = byroredux_core::string::StringPool::new();
        material.textures.greyscale_lut = Some(pool.intern("textures\\actors\\ghoul_palette.dds"));
        material.bgsm_greyscale_lut_enabled = true;
        material.bgsm_greyscale_lut_color = true;
        assert_eq!(
            pack_imported_material_flags(&material) & EFFECT_PALETTE_COLOR,
            EFFECT_PALETTE_COLOR,
            "BGSM greyscale LUT path + enable bit must pack EFFECT_PALETTE_COLOR (#1353)"
        );
    }

    /// Regression for #2108 (SF-D9-01) — a filled-but-DISABLED greyscale
    /// LUT slot (the enable bit off) must NOT pack the palette flag.
    /// Pre-fix, `pack_imported_material_flags` gated purely on
    /// `greyscale_lut.is_some()`; a template-inherited or mis-authored
    /// BGSM/BGEM that fills the (always-serialized) slot without opting
    /// into the remap got an unwanted palette-LUT remap anyway.
    #[test]
    fn greyscale_lut_present_but_disabled_does_not_set_palette_flags() {
        use byroredux_renderer::vulkan::material::material_flag::EFFECT_PALETTE_ALPHA;

        let mut material = empty_material();
        let mut pool = byroredux_core::string::StringPool::new();
        material.textures.greyscale_lut = Some(pool.intern("textures\\actors\\ghoul_palette.dds"));
        material.bgsm_greyscale_lut_enabled = false; // the authored enable bit was off
        let flags = pack_imported_material_flags(&material);
        assert_eq!(
            flags & EFFECT_PALETTE_COLOR,
            0,
            "disabled palette remap must not pack EFFECT_PALETTE_COLOR even with a filled LUT slot (#2108)"
        );
        assert_eq!(
            flags & EFFECT_PALETTE_ALPHA,
            0,
            "disabled palette remap must not pack EFFECT_PALETTE_ALPHA either"
        );
    }

    /// Regression for #1580 — a BGEM's `grayscale_to_palette_alpha` bool
    /// (forwarded onto `bgsm_greyscale_lut_is_alpha` by the BGEM merge arm
    /// in `asset_provider/material.rs`) must pack `EFFECT_PALETTE_ALPHA`.
    /// When only the alpha bit is authored (color bit left unset), only
    /// EFFECT_PALETTE_ALPHA packs — see
    /// `bgem_both_palette_bits_pack_both_effect_flags` below for the case
    /// where a BGEM authors both bits at once (#2643).
    #[test]
    fn bgem_alpha_variant_sets_effect_palette_alpha_not_color() {
        use byroredux_renderer::vulkan::material::material_flag::EFFECT_PALETTE_ALPHA;

        let mut material = empty_material();
        let mut pool = byroredux_core::string::StringPool::new();
        material.textures.greyscale_lut =
            Some(pool.intern("textures\\effects\\gradients\\fire.dds"));
        material.bgsm_greyscale_lut_enabled = true;
        material.bgsm_greyscale_lut_is_alpha = true;
        let flags = pack_imported_material_flags(&material);
        assert_eq!(
            flags & EFFECT_PALETTE_ALPHA,
            EFFECT_PALETTE_ALPHA,
            "BGEM grayscale_to_palette_alpha must pack EFFECT_PALETTE_ALPHA (#1580)"
        );
        assert_eq!(
            flags & EFFECT_PALETTE_COLOR,
            0,
            "color bit was not authored on this material, so EFFECT_PALETTE_COLOR must stay clear"
        );
    }

    /// Regression for #2643 (SF-D9-2026-08-07-04) — a BGEM authoring BOTH
    /// the shared `grayscale_to_palette_color` bit and its own
    /// `grayscale_to_palette_alpha` bit (the format permits this; the
    /// inline NIF effect-shader path already handles it via
    /// `pack_effect_shader_flags`) must pack BOTH `EFFECT_PALETTE_COLOR`
    /// and `EFFECT_PALETTE_ALPHA`. Pre-fix, `bgsm_greyscale_lut_is_alpha`
    /// alone decided COLOR-vs-ALPHA as a mutually-exclusive choice, so
    /// this input packed ALPHA only and silently dropped the color
    /// variant.
    #[test]
    fn bgem_both_palette_bits_pack_both_effect_flags() {
        use byroredux_renderer::vulkan::material::material_flag::EFFECT_PALETTE_ALPHA;

        let mut material = empty_material();
        let mut pool = byroredux_core::string::StringPool::new();
        material.textures.greyscale_lut =
            Some(pool.intern("textures\\effects\\gradients\\fire.dds"));
        material.bgsm_greyscale_lut_enabled = true;
        material.bgsm_greyscale_lut_color = true;
        material.bgsm_greyscale_lut_is_alpha = true;
        let flags = pack_imported_material_flags(&material);
        assert_eq!(
            flags & EFFECT_PALETTE_COLOR,
            EFFECT_PALETTE_COLOR,
            "both palette bits authored → EFFECT_PALETTE_COLOR must still pack (#2643)"
        );
        assert_eq!(
            flags & EFFECT_PALETTE_ALPHA,
            EFFECT_PALETTE_ALPHA,
            "both palette bits authored → EFFECT_PALETTE_ALPHA must still pack (#2643)"
        );
    }

    /// The new BGSM bits must NOT collide with the existing
    /// `EFFECT_*` (bits 1-4) or `VERTEX_COLOR_EMISSIVE` (bit 0)
    /// bits. Pins the bit-layout contract from outside the
    /// packer's source.
    #[test]
    fn bgsm_bits_do_not_collide_with_effect_bits() {
        use byroredux_renderer::vulkan::material::material_flag::{
            EFFECT_LIT, EFFECT_PALETTE_ALPHA, EFFECT_PALETTE_COLOR, EFFECT_SOFT,
            VERTEX_COLOR_EMISSIVE,
        };
        let bgsm_bits = PBR_BSDF | TRANSLUCENCY | MODEL_SPACE_NORMALS;
        let prior_bits = VERTEX_COLOR_EMISSIVE
            | EFFECT_SOFT
            | EFFECT_PALETTE_COLOR
            | EFFECT_PALETTE_ALPHA
            | EFFECT_LIT;
        assert_eq!(
            bgsm_bits & prior_bits,
            0,
            "BGSM_* bits (0x{:x}) overlap prior bits (0x{:x})",
            bgsm_bits,
            prior_bits,
        );
    }
}

#[cfg(test)]
mod attach_points_spawn_tests;
#[cfg(test)]
mod euler_zup_to_quat_yup_tests;
#[cfg(test)]
mod finish_partial_tests;
#[cfg(test)]
mod inventory_release_tests;
#[cfg(test)]
mod lgtm_fallback_tests;
#[cfg(test)]
mod nif_import_registry_tests;
#[cfg(test)]
mod nif_light_spawn_gate_tests;
#[cfg(test)]
mod pkin_expansion_tests;
#[cfg(test)]
mod placement_root_subtree_tests;
#[cfg(test)]
mod precombined_clip_handle_tests;
#[cfg(test)]
mod rapier_release_tests;
#[cfg(test)]
mod refr_texture_overlay_tests;
#[cfg(test)]
mod root_index_tests;
#[cfg(test)]
mod scol_expansion_tests;
#[cfg(test)]
mod sky_params_cleanup_tests;
#[cfg(test)]
mod terrain_splat_tests;
#[cfg(test)]
mod unload_greyscale_lut_tests;
#[cfg(test)]
mod unload_skin_cleanup_tests;
