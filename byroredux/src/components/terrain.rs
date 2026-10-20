//! Terrain splat-tile and ground-cover components. (#5467 split from `components.rs`)

use byroredux_core::ecs::{Component, SparseSetStorage};
// #5173 — the per-lane ground-cover arrays below size from the same splat
// constant the terrain tile + shader loops size from, so a lane bump cannot
// leave this component silently 8-wide while the rest of the chain moves.
use byroredux_renderer::shader_constants::TERRAIN_SPLAT_LAYERS;

/// Terrain splat-layer tile index into the renderer's
/// `GpuTerrainTile` SSBO (scene set 1, binding 10). Attached only to
/// LAND terrain entities when ATXT/VTXT splat layers are present.
/// `render.rs` forwards this into `DrawCommand::terrain_tile_index`,
/// which `draw.rs` packs into the top 16 bits of `GpuInstance.flags`
/// alongside the `INSTANCE_FLAG_TERRAIN_SPLAT` bit. See #470.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TerrainTileSlot(pub(crate) u32);
impl Component for TerrainTileSlot {
    type Storage = SparseSetStorage<Self>;
}

/// Y-up world XZ of an exterior LAND tile's (row 0, col 0) vertex — the
/// origin every terrain-attribute sampler inverts the grid mapping against.
///
/// #4052. `cell_loader/terrain.rs` builds the tile's 33×33 vertices from
/// `grid_x`/`grid_y` and then forgets them; nothing downstream could recover
/// which exterior cell a given terrain entity covers. That is exactly the
/// **chunk-to-instance association** `exal-groundcover.md` §11.1 names as the
/// missing link on its path A: a ground-cover chunk finds its terrain
/// vertices through the covering cell's `GpuInstance.vertex_offset`, and this
/// component is how a world-space chunk finds the covering cell in the first
/// place.
///
/// Stored as the resolved origin rather than the `(grid_x, grid_y)` pair it
/// derives from because every consumer wants the Y-up XZ: keeping the raw
/// grid coordinates would put the same `origin_y` → `-z` flip in each of
/// them, and that sign is the one mistake in this mapping that still renders
/// plausible terrain (see `include/terrain_sample.glsl`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct TerrainCellOrigin {
    pub(crate) origin_xz: [f32; 2],
}
impl Component for TerrainCellOrigin {
    type Storage = SparseSetStorage<Self>;
}

/// The two per-cell inputs the ground-cover density field needs that the
/// terrain vertices cannot supply (#4054).
///
/// Both are resolved once at terrain spawn, from data the spawn path already
/// has and nothing downstream keeps: the cell's `LTEX` layer names (which
/// become `cover_affinity` weights, §3's `affinity` term) and its resolved
/// water height (§3's `moisture` term). Sitting on the terrain entity beside
/// [`TerrainCellOrigin`] means the per-frame chunk collection reads one
/// entity per cell rather than re-walking the plugin index every frame.
// Not `Copy` since #4642: `authored_grass` carries per-lane `Vec<u32>`
// grass arrays.
#[derive(Debug, Clone, Default)]
pub(crate) struct TerrainCoverInputs {
    /// `cover_affinity` per LAND splat layer, in the same layer order the
    /// vertex splat lanes use. Unpainted slots keep the default fill; under
    /// #4903's ordered mix a zero-weight lane cannot affect the composed
    /// affinity either way, so the fill is uniformity, not correctness.
    /// Sized from `TERRAIN_SPLAT_LAYERS` (#5173).
    pub(crate) layer_affinity: [f32; TERRAIN_SPLAT_LAYERS as usize],
    /// The BTXT base LTEX's own `cover_affinity` (#4903) — the ground the
    /// terrain shows wherever no splat layer paints. The density field
    /// composes `base_affinity` first and mixes each lane over it in the
    /// diffuse loop's order, so an unpainted vertex reads its authored
    /// affinity (snow stays barren, grassland stays vegetated) instead of a
    /// fabricated default, and a partially painted one shows the base under
    /// the paint.
    pub(crate) base_affinity: f32,
    /// Y-up water-plane height, or
    /// [`byroredux_core::ecs::components::groundcover::NO_WATER_HEIGHT`].
    ///
    /// **The sentinel is load-bearing.** §3's `moisture` term must resolve to
    /// 1.0 where there is no water, not 0.0: the density field is a pure
    /// product, so one undefined factor takes the whole field, and the failure
    /// would be an entire high-desert worldspace with no ground cover and
    /// nothing in the log to say why.
    pub(crate) water_y: f32,
    /// The terrain-tile SSBO slot this cell's splat layers live in, so
    /// §12.3's ground-colour coupling can sample the layer diffuse
    /// textures the terrain itself shades with (#4056). `u32::MAX` when
    /// the cell has no splat terrain (`allocate_terrain_tile` was never
    /// called) — a valid slot index can be 0, so "absent" cannot be 0.
    pub(crate) terrain_tile_slot: u32,
    /// The resolved BTXT base diffuse handle (0 = unresolved). Rides the
    /// cover inputs, not only the terrain tile, because a cell with a base
    /// and no ATXT paint gets no tile — §12.3's ground-colour coupling must
    /// still root its blades in the base it shows (#5174).
    pub(crate) base_diffuse_index: u32,
    /// Authored `GRAS` FormID lists associated with the same LAND splat
    /// lanes as `layer_affinity` — each lane carries its LTEX's
    /// full `GNAM` array in authored order (#4642), empty when the layer
    /// has no `LTEX.GNAM` link (or is the executable's default land
    /// texture). #4906 — the authored-model tier's cell cover masks are
    /// built from these (`render::groundcover::cover_masks_for_cell`), so a
    /// record grows only on ground whose textures list it.
    pub(crate) authored_grass: [Vec<u32>; TERRAIN_SPLAT_LAYERS as usize],
    /// The BTXT base LTEX's own `GNAM` list (#4906) — the grasses of the
    /// ground wherever no splat layer paints, like `base_affinity` for the
    /// density field.
    pub(crate) base_authored_grass: Vec<u32>,
}
impl Component for TerrainCoverInputs {
    type Storage = SparseSetStorage<Self>;
}

/// Which textures each lane of a LAND tile takes its height map from
/// (#6195): the BTXT base, then the splat lanes in vertex order. Kept on the
/// terrain entity so a map that finishes generating after the cell spawned
/// can find the tile lanes waiting on it, and so the debugger can report
/// every layer's height source.
#[derive(Debug, Clone, Default)]
pub(crate) struct TerrainLayerHeights {
    pub(crate) base: Option<crate::asset_provider::terrain_height::LayerHeightSources>,
    pub(crate) layers:
        [Option<crate::asset_provider::terrain_height::LayerHeightSources>; TERRAIN_SPLAT_LAYERS as usize],
}
impl Component for TerrainLayerHeights {
    type Storage = SparseSetStorage<Self>;
}

// SystemList moved to byroredux_core::ecs::resources::SystemList
