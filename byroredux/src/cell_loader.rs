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
mod load;
mod load_order;
mod nif_import_registry;
mod partial;
mod references;
mod refr;
mod spawn;
mod terrain;
mod unload;
mod water;
mod exterior;

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
