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
use std::sync::Arc;
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
use crate::components::{
    AlphaBlend, CellLightingRes, CellRootIndex, DarkMapHandle, ExtraTextureMaps, NormalMapHandle,
    SkyParamsRes, TerrainTileSlot, TwoSided, WeatherDataRes, WeatherTransitionRes,
};

mod euler;
mod index;
mod load;
mod load_order;
mod nif_import_registry;
mod partial;
mod precombined;
mod references;
mod refr;
mod spawn;
mod terrain;
mod transition;
mod unload;
mod water;
mod exterior;

pub use index::LoadedCellIndex;
pub use transition::{
    load_interior_cell, log_transition_header, position_zup_to_yup, reposition_camera,
    rotation_zup_to_yup_quat, take_pending_transition, unload_current_interior, CurrentCellRoot,
    LoadedPluginSet, PendingCellTransition, PendingCellTransitionSlot, TransitionDestination,
};

// Public re-exports — keep the existing `crate::cell_loader::FOO`
// call sites in main.rs / streaming.rs / commands.rs working without
// further changes. `#[allow(unused_imports)]` because not every
// re-exported item is consumed by this crate's own binary — several
// only show up in external crates (tests, other workspace members)
// or as the public API surface.
#[allow(unused_imports)]
pub(crate) use nif_import_registry::{CachedNifImport, NifImportRegistry};
#[allow(unused_imports)]
pub(crate) use refr::{
    build_refr_texture_overlay, expand_pkin_placements, expand_scol_placements, RefrTextureOverlay,
};

pub use euler::set_refr_rotation_mode_diag;
pub(crate) use euler::{euler_zup_to_quat_yup, euler_zup_to_quat_yup_refr};

#[allow(unused_imports)]
pub use exterior::{
    build_exterior_world_context, load_one_exterior_cell, ExteriorWorldContext, OneCellLoadInfo,
};
#[allow(unused_imports)]
pub use load::{load_cell_with_masters, CellLoadResult};
#[allow(unused_imports)]
pub(crate) use load::resolve_cell_lighting;
pub(crate) use load::apply_interior_cell_lighting;
pub use unload::unload_cell;
pub(crate) use partial::finish_partial_import;

// Test-only re-exports so the `use super::*;` patterns inside the
// child test modules see the helpers they exercise. Production code
// paths reach these via their owning submodule path.
#[cfg(test)]
pub(crate) use load::stamp_cell_root;
#[cfg(test)]
pub(crate) use spawn::{
    count_spawnable_nif_lights, is_spawnable_nif_light, light_radius_or_default,
};
#[cfg(test)]
pub(crate) use unload::release_victim_item_instances;
#[cfg(test)]
pub(crate) use unload::collect_victim_gpu_handles;

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
        EFFECT_LIT, EFFECT_PALETTE_ALPHA, EFFECT_PALETTE_COLOR, EFFECT_SOFT,
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
    flags
}

/// Pack the three BGSM v>2 boolean flags
/// ([`ImportedMesh::is_pbr`] / [`ImportedMesh::has_translucency`] /
/// [`ImportedMesh::model_space_normals`]) into the `material_flag::BGSM_*`
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
/// [`ImportedMesh::is_pbr`]: byroredux_nif::import::ImportedMesh::is_pbr
/// [`ImportedMesh::has_translucency`]: byroredux_nif::import::ImportedMesh::has_translucency
/// [`ImportedMesh::model_space_normals`]: byroredux_nif::import::ImportedMesh::model_space_normals
pub(crate) fn pack_bgsm_material_flags(mesh: &byroredux_nif::import::ImportedMesh) -> u32 {
    use byroredux_renderer::vulkan::material::material_flag::{
        BGSM_AUTHORED, BGSM_MODEL_SPACE_NORMALS, BGSM_PBR, BGSM_TRANSLUCENCY,
        BGSM_TRANSLUCENCY_MIX_ALBEDO, BGSM_TRANSLUCENCY_THICK_OBJECT, EFFECT_PALETTE_COLOR,
    };
    let mut flags = 0u32;
    // `BGSM_AUTHORED` — set when `merge_bgsm_into_mesh` resolved a
    // BGSM/BGEM file successfully (independent of `bgsm.pbr`, which
    // vanilla FO4 virtually never authors — sampled: 0 of 793
    // metal/cargo BGSMs in `Fallout4 - Materials.ba2`). Drives the
    // spec-glossiness F0 derivation in the fragment shader; see
    // `material_flag::BGSM_AUTHORED` for the rationale.
    if mesh.from_bgsm {
        flags |= BGSM_AUTHORED;
    }
    if mesh.is_pbr {
        flags |= BGSM_PBR;
    }
    if mesh.has_translucency {
        flags |= BGSM_TRANSLUCENCY;
    }
    if mesh.model_space_normals {
        flags |= BGSM_MODEL_SPACE_NORMALS;
    }
    // #1353 / FO4-D8-07 — FO4 BGSM grayscale-to-palette. EFFECT_PALETTE_COLOR
    // IS `SLSF1::Greyscale_To_PaletteColor`; setting it on a BGSM lit material
    // (one that authored a `greyscale_texture`, captured as
    // `bgsm_greyscale_lut_path`) makes the lit-path palette remap in
    // triangle.frag sample the resolved GreyscaleLutHandle by diffuse
    // luminance. The effect-mesh path sets the same bit via
    // `pack_effect_shader_flags`; the two live in different material-kind
    // shader branches so there is no conflict.
    if mesh.bgsm_greyscale_lut_path.is_some() {
        flags |= EFFECT_PALETTE_COLOR;
    }
    // #1147 Phase 2b — translucency parameter-shape bits. Only
    // meaningful when `BGSM_TRANSLUCENCY` is also set, but pack them
    // unconditionally so the shader's predicate `is_thick` /
    // `mix_albedo` reads the authored value directly. The shader
    // already gates the whole SSS block on `BGSM_TRANSLUCENCY`.
    if mesh.translucency_thick_object {
        flags |= BGSM_TRANSLUCENCY_THICK_OBJECT;
    }
    if mesh.translucency_mix_albedo {
        flags |= BGSM_TRANSLUCENCY_MIX_ALBEDO;
    }
    flags
}

#[cfg(test)]
mod pack_bgsm_material_flags_tests {
    //! Regression for #1147 Phase 2a (#1077 follow-up). Pins the
    //! contract that the bool-to-bit-OR packer matches the
    //! `material_flag::BGSM_*` layout the Phase 2b shader consumer
    //! will read.

    use super::pack_bgsm_material_flags;
    use byroredux_nif::import::ImportedMesh;
    use byroredux_renderer::vulkan::material::material_flag::{
        BGSM_MODEL_SPACE_NORMALS, BGSM_PBR, BGSM_TRANSLUCENCY, EFFECT_PALETTE_COLOR,
    };

    /// Build an empty-but-valid `ImportedMesh` with all 3 BGSM flags
    /// clear. Only the fields the packer reads are set here; the rest
    /// rely on `..Default::default()` (the type has no Default impl,
    /// so this helper hand-constructs the relevant subset).
    fn empty_mesh() -> ImportedMesh {
        ImportedMesh {
            positions: Vec::new(),
            colors: Vec::new(),
            normals: Vec::new(),
            tangents: Vec::new(),
            uvs: Vec::new(),
            indices: Vec::new(),
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: 1.0,
            texture_path: None,
            material_path: None,
            name: None,
            has_alpha: false,
            src_blend_mode: 6,
            dst_blend_mode: 7,
            alpha_test: false,
            alpha_threshold: 0.0,
            alpha_test_func: 6,
            two_sided: false,
            is_decal: false,
            normal_map: None,
            glow_map: None,
            detail_map: None,
            gloss_map: None,
            dark_map: None,
            parallax_map: None,
            env_map: None,
            env_mask: None,
            specular_map: None,
            lighting_map: None,
            flow_map: None,
            wrinkle_map: None,
            is_pbr: false,
            has_translucency: false,
            model_space_normals: false,
            from_bgsm: false,
            bgem_glass: false,
            metalness_override: None,
            roughness_override: None,
            // #1147 Phase 2b — translucency suite (zero default).
            translucency_subsurface_color: [0.0; 3],
            translucency_transmissive_scale: 0.0,
            translucency_turbulence: 0.0,
            translucency_thick_object: false,
            translucency_mix_albedo: false,
            parallax_max_passes: None,
            parallax_height_scale: None,
            vertex_color_mode: 2,
            texture_clamp_mode: 0,
            emissive_color: [0.0; 3],
            emissive_mult: 0.0,
            emissive_source: byroredux_core::ecs::components::material::EmissiveSource::None,
            specular_color: [1.0; 3],
            diffuse_color: [1.0; 3],
            ambient_color: [1.0; 3],
            specular_strength: 1.0,
            glossiness: 80.0,
            // #1241 — BSLSP PBR scalars; test helper sticks to
            // MaterialInfo defaults (mirror of the BSLSP parser stub).
            refraction_strength: 0.0,
            lighting_effect_1: 0.0,
            lighting_effect_2: 0.0,
            subsurface_rolloff: 0.0,
            rimlight_power: 0.0,
            backlight_power: 0.0,
            grayscale_to_palette_scale: 1.0,
            bgsm_greyscale_lut_path: None,
            fresnel_power: 5.0,
            uv_offset: [0.0; 2],
            uv_scale: [1.0; 2],
            mat_alpha: 1.0,
            env_map_scale: 1.0,
            parent_node: None,
            skin: None,
            z_test: true,
            z_write: true,
            z_function: 3,
            local_bound_center: [0.0; 3],
            local_bound_radius: 0.0,
            effect_shader: None,
            material_kind: 0,
            shader_type_fields: byroredux_nif::import::ShaderTypeFields::default(),
            no_lighting_falloff: None,
            wireframe: false,
            flat_shading: false,
            flags: 0,
            bs_lod_cutoffs: None,
            bs_sub_index: None,
        }
    }

    #[test]
    fn all_three_flags_off_produces_zero() {
        let mesh = empty_mesh();
        assert_eq!(pack_bgsm_material_flags(&mesh), 0);
    }

    #[test]
    fn all_three_flags_on_produces_full_union() {
        let mut mesh = empty_mesh();
        mesh.is_pbr = true;
        mesh.has_translucency = true;
        mesh.model_space_normals = true;

        let packed = pack_bgsm_material_flags(&mesh);
        let expected = BGSM_PBR | BGSM_TRANSLUCENCY | BGSM_MODEL_SPACE_NORMALS;
        assert_eq!(packed, expected);
        // Sanity-check the canonical bit layout the Phase 2b shader
        // consumer will read against — bits 5, 6, 7.
        assert_eq!(packed & 0x20, BGSM_PBR);
        assert_eq!(packed & 0x40, BGSM_TRANSLUCENCY);
        assert_eq!(packed & 0x80, BGSM_MODEL_SPACE_NORMALS);
    }

    #[test]
    fn individual_flags_produce_individual_bits() {
        let mut mesh = empty_mesh();
        mesh.is_pbr = true;
        assert_eq!(pack_bgsm_material_flags(&mesh), BGSM_PBR);

        let mut mesh = empty_mesh();
        mesh.has_translucency = true;
        assert_eq!(pack_bgsm_material_flags(&mesh), BGSM_TRANSLUCENCY);

        let mut mesh = empty_mesh();
        mesh.model_space_normals = true;
        assert_eq!(pack_bgsm_material_flags(&mesh), BGSM_MODEL_SPACE_NORMALS);
    }

    /// #1353 / FO4-D8-07 — a BGSM that authored a greyscale-to-palette LUT
    /// (`bgsm_greyscale_lut_path`, set by `merge_bgsm_into_mesh`) must pack
    /// `EFFECT_PALETTE_COLOR` (= SLSF1 Greyscale_To_PaletteColor) so the
    /// lit-path palette remap in triangle.frag fires. Absent the path, the
    /// bit must stay clear.
    #[test]
    fn bgsm_greyscale_lut_path_sets_effect_palette_color() {
        let mesh = empty_mesh();
        assert_eq!(
            pack_bgsm_material_flags(&mesh) & EFFECT_PALETTE_COLOR,
            0,
            "no greyscale LUT path → palette flag must stay clear"
        );

        let mut mesh = empty_mesh();
        mesh.bgsm_greyscale_lut_path = Some("textures\\actors\\ghoul_palette.dds".to_string());
        assert_eq!(
            pack_bgsm_material_flags(&mesh) & EFFECT_PALETTE_COLOR,
            EFFECT_PALETTE_COLOR,
            "BGSM greyscale LUT path must pack EFFECT_PALETTE_COLOR (#1353)"
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
        let bgsm_bits = BGSM_PBR | BGSM_TRANSLUCENCY | BGSM_MODEL_SPACE_NORMALS;
        let prior_bits =
            VERTEX_COLOR_EMISSIVE | EFFECT_SOFT | EFFECT_PALETTE_COLOR | EFFECT_PALETTE_ALPHA | EFFECT_LIT;
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
mod euler_zup_to_quat_yup_tests;
#[cfg(test)]
mod nif_import_registry_tests;
#[cfg(test)]
mod finish_partial_tests;
#[cfg(test)]
mod refr_texture_overlay_tests;
#[cfg(test)]
mod pkin_expansion_tests;
#[cfg(test)]
mod scol_expansion_tests;
#[cfg(test)]
mod terrain_splat_tests;
#[cfg(test)]
mod sky_params_cleanup_tests;
#[cfg(test)]
mod nif_light_spawn_gate_tests;
#[cfg(test)]
mod lgtm_fallback_tests;
#[cfg(test)]
mod unload_skin_cleanup_tests;
#[cfg(test)]
mod placement_root_subtree_tests;
#[cfg(test)]
mod root_index_tests;
#[cfg(test)]
mod inventory_release_tests;
#[cfg(test)]
mod unload_greyscale_lut_tests;
