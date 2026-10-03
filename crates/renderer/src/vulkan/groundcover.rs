//! EXAL ground cover — the scatter pass and its consumers (#4054 / #4055).
//!
//! `docs/engine/exal-groundcover.md` §3, §4, §6, §8. The whole visible
//! population of ground cover is a compute dispatch plus a handful of indirect
//! draws: no per-blade CPU work, no `GpuInstance` growth, no ECS entities.
//!
//! ## The frame
//!
//! 1. **Chunking** (§4). Each exterior cell subdivides into 8×8 chunks of 512
//!    units. The chunk is the unit of dispatch, culling and LOD selection; the
//!    host collects the visible set and uploads one record each.
//! 2. **Scatter** (`groundcover_scatter.comp`). One workgroup per chunk, each
//!    thread drawing candidate points from a progressive low-discrepancy
//!    sequence, evaluating the §3 density field and stochastically accepting.
//!    Accepted points atomically append to the chunk's fixed-capacity slice of
//!    the blade buffer and write its `VkDrawIndirectCommand`.
//! 3. **Draw** (`groundcover_blade.vert/frag`, or the debug point view). One
//!    `vkCmdDrawIndirect` over the chunk draw list, inside the main geometry
//!    pass so blades are lit and shadowed like any other fragment (§5).
//!
//! ## Why the blade buffer is sized the way it is
//!
//! [`GROUNDCOVER_MAX_CHUNKS`] × [`GROUNDCOVER_MAX_BLADES_PER_CHUNK`] ×
//! `size_of::<GpuGroundCoverBlade>()` (16 B) = 64 MiB device-local, against
//! the 4 GB total budget: 256 chunks × 16,384 blades since the candidate
//! budget rose to 256 per thread on 2026-09-16 (`7996edf61`, design §12.13) —
//! 4× the 16 MiB the original 1,024 × 1,024 and then 256 × 4,096 arenas
//! held. The chunks the distance cull can keep at the shipped 512-unit chunk
//! and 3000-unit draw distance bound at ~167 (the binary's
//! `chunk_cap_covers_every_chunk_in_reach`), so this is ~1.5× headroom. The
//! host assigns these fixed slices through a camera-centred
//! residency ring: a chunk keeps its index while resident and newly visible
//! chunks enter through a bounded placement queue. A change that makes the
//! ring exceed this physical arena still fails the capacity assertion rather
//! than silently dropping a side of the field (#4338).
//!
//! ## Ownership
//!
//! Every buffer here is allocated once at pipeline creation and reused for the
//! life of the device — nothing is per-cell, so cell load/unload cannot leak
//! through this path. The EX-08 soak's ground-cover classes
//! (`OwnershipSnapshot::groundcover_*`) therefore report *occupancy*, not
//! allocation count: what they catch is the scatter failing to release chunk
//! slices as the camera moves, which is the leak shape this design can
//! actually have.

use anyhow::{Context, Result};
use ash::vk;

use super::allocator::SharedAllocator;
use super::buffer::{GpuBuffer, NoUninit};
use super::reflect::{ReflectedShader, validate_set_layout};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use crate::shader_constants::{
    GROUNDCOVER_BLADE_SEGMENTS_MID, GROUNDCOVER_BLADE_SEGMENTS_NEAR, GROUNDCOVER_BLADES_PER_POINT,
    GROUNDCOVER_CHUNKS_PER_CELL_SIDE, GROUNDCOVER_INTERACTION_MAX_DISTURBERS,
    GROUNDCOVER_INTERACTION_TEXELS, GROUNDCOVER_INTERACTION_UNITS,
    GROUNDCOVER_INTERACTION_WORKGROUP, GROUNDCOVER_MAX_BLADES_PER_CHUNK, GROUNDCOVER_MAX_CHUNKS,
    GROUNDCOVER_SPECIES_TABLE_SIZE, GROUNDCOVER_VERTS_PER_SEGMENT,
};

const SCATTER_SPV: &[u8] = include_bytes!("../../shaders/groundcover_scatter.comp.spv");
const INTERACTION_SPV: &[u8] = include_bytes!("../../shaders/groundcover_interaction.comp.spv");
const BLADE_VERT_SPV: &[u8] = include_bytes!("../../shaders/groundcover_blade.vert.spv");
const BLADE_FRAG_SPV: &[u8] = include_bytes!("../../shaders/groundcover_blade.frag.spv");
const DEBUG_FRAG_SPV: &[u8] = include_bytes!("../../shaders/groundcover_debug.frag.spv");

/// Array capacity for the per-worldspace species palette on the GPU.
///
/// `GroundCoverPalette` is resolved from a worldspace's `GRAS` records and is
/// never empty (its `resolve` substitutes a built-in). Real content lands in
/// single digits; 32 is headroom for a heavily-modded load order without
/// making the buffer worth paging.
pub const MAX_GROUNDCOVER_SPECIES: usize = 32;

/// Chunks per exterior cell — §4's 8×8 grid.
pub const CHUNKS_PER_CELL: usize =
    (GROUNDCOVER_CHUNKS_PER_CELL_SIDE * GROUNDCOVER_CHUNKS_PER_CELL_SIDE) as usize;

/// Array capacity for resident terrain cells. Mirrors the #4052 bench's cap
/// for the same reason: `--radius 3` resides 49 cells and this leaves room for
/// a wider ring without sizing every buffer for one nobody runs.
pub const MAX_GROUNDCOVER_CELLS: usize = 128;

/// One resident exterior terrain cell. Mirrors `GroundCoverCell` in
/// `include/groundcover_scene.glsl`.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct GpuGroundCoverCell {
    pub origin_xz: [f32; 2],
    pub vertex_offset: u32,
    pub pad0: u32,
    /// `cover_affinity` for LAND splat layers 0–3 and 4–7.
    pub affinity0: [f32; 4],
    pub affinity1: [f32; 4],
    /// Y-up water-plane height, or `NO_WATER_HEIGHT`. A cell with no water
    /// **must** arrive as the sentinel — see `byroGcMoisture`, whose no-water
    /// path returns 1.0 rather than 0.0.
    pub water_y: f32,
    /// The terrain-tile SSBO slot carrying this cell's splat layer diffuse
    /// indices — §12.3's ground-colour coupling samples them at the blade
    /// base (#4056). `u32::MAX` when the cell has no splat terrain; a valid
    /// slot can be 0, so "absent" cannot be 0.
    pub terrain_tile_slot: u32,
    /// The BTXT base LTEX's own `cover_affinity` (#4903) — starts the
    /// density field's ordered mix in `byroGcAffinity`. Previously this slot
    /// was `pad1`.
    pub base_affinity: f32,
    /// The BTXT base diffuse texture handle, 0 when unresolved. §12.3's
    /// ground-colour coupling roots blades in it on cells that have no
    /// terrain tile (a base and no ATXT paint) — previously the `pad2` slot,
    /// so the record stays 64 B (#5174).
    pub base_diffuse_index: u32,
}
// SAFETY: `#[repr(C)]` over `f32`/`u32` only, explicitly padded to 64 bytes
// with named fields, so every byte is initialised by a field write.
unsafe impl NoUninit for GpuGroundCoverCell {}

/// Sentinel for [`GpuGroundCoverCell::terrain_tile_slot`] — re-exported from
/// core so the GLSL mirror and this struct cannot disagree about the value.
pub use byroredux_core::ecs::components::groundcover::GROUNDCOVER_NO_TERRAIN_TILE;

/// One fixed residency-ring slot. Mirrors `GroundCoverChunk` in the shared
/// GLSL header.  Inactive slots remain in the uploaded prefix so their index
/// continues to name the same blade-arena slab; scatter writes a zero indirect
/// command for them and never reads their cell index.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct GpuGroundCoverChunk {
    pub base_xz: [f32; 2],
    pub cell_index: u32,
    pub seed: u32,
    pub active: u32,
    pub entry_progress: f32,
    /// #5176 — the model tier's LAYOUT walks chunks in this permutation:
    /// record `o` holds the index of the `o`-th nearest chunk (vacant slots
    /// last), so an over-budget instance grant drops the farthest plants.
    pub layout_order: u32,
    pub pad: u32,
}
// SAFETY: as `GpuGroundCoverCell` — 32 bytes, all padding is named and
// initialised by the host.
unsafe impl NoUninit for GpuGroundCoverChunk {}

/// One palette entry's *rendering* fields. Mirrors `GroundCoverSpecies` in the
/// shared GLSL header; the selection fields (climate weights) resolve host-side
/// and never reach the GPU.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct GpuGroundCoverSpecies {
    /// `[height_min, height_max, width_min, width_max]`.
    pub size_range: [f32; 4],
    /// Base colour RGB + bend stiffness.
    pub base_colour: [f32; 4],
    /// Tip colour RGB + ground-coupling weight (§12.3).
    pub tip_colour: [f32; 4],
    /// Transmission colour RGB (§12.2) + sheen amount (§12.6). #4057.
    pub transmission_sheen: [f32; 4],
    /// Bindless palette-generated clump-card atlas (§6 Tier 2), in x. The
    /// remaining lanes reserve the std430 vec4 and keep card sampling
    /// per-species rather than in the per-draw push block.
    pub card_atlas: [u32; 4],
}
// SAFETY: 64 bytes of `f32` plus one fully initialised `uvec4`, no padding.
unsafe impl NoUninit for GpuGroundCoverSpecies {}

/// One accepted blade. Mirrors `GroundCoverBlade` in the shared GLSL header.
///
/// No host code ever writes one — `groundcover_scatter.comp` appends them and
/// `groundcover_blade.vert` reads them. The mirror exists so the blade buffer
/// is sized from `size_of` rather than from a literal stride that had to match
/// the GLSL record by hand (#4335): grow the record and the buffer grows with
/// it, while `name_diverging_glsl_rust_mirrors_stay_in_lockstep` fails until
/// both sides agree, instead of the scatter writing past the SSBO's end.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct GpuGroundCoverBlade {
    /// Chunk-relative XZ as 2×16-bit fixed point.
    pub packed_xz: u32,
    /// World-space Y of the blade base.
    pub base_y: f32,
    /// `seed << 8 | species_index`.
    pub seed_species: u32,
    /// `d_ground` at the root (§3), not the view-faded `d_draw`.
    pub d_ground: f32,
}

/// World units one interaction-field texel covers (§12.4).
pub const GROUNDCOVER_INTERACTION_TEXEL_UNITS: f32 =
    GROUNDCOVER_INTERACTION_UNITS / GROUNDCOVER_INTERACTION_TEXELS as f32;

/// Texels in one half of the interaction field.
const INTERACTION_TEXEL_COUNT: u64 =
    (GROUNDCOVER_INTERACTION_TEXELS as u64) * (GROUNDCOVER_INTERACTION_TEXELS as u64);

/// One disturbing entity: `(worldX, worldZ, radius, strength)` (§12.4).
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct GpuGroundCoverDisturber {
    pub world_xz: [f32; 2],
    pub radius: f32,
    pub strength: f32,
}
// SAFETY: 16 bytes of `f32`, no padding.
unsafe impl NoUninit for GpuGroundCoverDisturber {}

/// The interaction field's per-frame header. Mirrors `GcFieldStateBuffer` in
/// `groundcover_interaction.comp` and `groundcover_blade.vert`.
///
/// A buffer rather than a push constant because **both** the compute pass and
/// the blade vertex shader read it, and a push block is per-pipeline.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct GpuGroundCoverFieldState {
    /// xy = this frame's snapped origin, z = seconds since the last update,
    /// w = write half (0 or 1).
    pub current: [f32; 4],
    /// xy = last frame's snapped origin, z = disturber count, w = 1.0 when the
    /// previous half holds a field this origin can be reprojected from.
    pub previous: [f32; 4],
}
// SAFETY: 32 bytes of `f32`, no padding.
unsafe impl NoUninit for GpuGroundCoverFieldState {}

/// Push constants for the scatter dispatch.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ScatterPush {
    /// xyz = camera position, w = placed-geometry cover reach (world units).
    camera_pos: [f32; 4],
    /// xyz = render origin (`offsetRayOrigin` steps in camera-relative
    /// space), w = 1.0 when the placed-geometry cover test is enabled.
    render_origin: [f32; 4],
    chunk_count: u32,
    blades_per_chunk: u32,
    verts_per_blade: u32,
    species_count: u32,
}

/// Push constants for the blade / debug draw. 64 bytes, inside Vulkan's
/// guaranteed 128-byte `maxPushConstantsSize` floor, which is why the fields
/// are packed rather than given a `vec4` each: this device allows 256, so a
/// layout that only fits there is a portability bug no test on this machine
/// can see.
///
/// There is no view-projection here: the blade vertex shader projects with
/// the camera UBO's, the jitter-free DOF-effective matrix the rest of the
/// main pass uses, and adds the frame's jitter itself (#4296).
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct BladePush {
    /// xyz = absolute camera position, w = pixels per world unit at unit depth.
    camera_pixels: [f32; 4],
    /// xyz = render origin to subtract, w = seconds.
    origin_time: [f32; 4],
    /// xy = unit wind direction, z = speed, w = gust amplitude.
    wind: [f32; 4],
    /// x = gust frequency, y = the preceding wind-clock sample, z = species
    /// count.  The first two fields form the vertex-motion contract; keeping
    /// them adjacent makes their std430/push-constant offsets explicit.
    gust_and_timing: [f32; 4],
}

/// Everything the host publishes for one frame of ground cover.
pub use super::groundcover_stats::GroundCoverStats;
use super::groundcover_stats::{
    COUNTER_COVERED, COUNTER_EXTREMA_BASE, COUNTER_FACTOR_BASE, COUNTER_HIST_BASE,
    COUNTER_OVERFLOW, COUNTER_SLOTS,
};

pub struct GroundCoverFrame<'a> {
    pub cells: &'a [GpuGroundCoverCell],
    pub chunks: &'a [GpuGroundCoverChunk],
    pub species: &'a [GpuGroundCoverSpecies],
    /// The scatter's species selection table: up to
    /// `GROUNDCOVER_SPECIES_TABLE_SIZE` indices into `species`, in proportion
    /// to climate weight (§7). Shorter input is padded with species 0.
    pub species_table: &'a [u32],
    pub camera_pos: [f32; 3],
    /// Cell-grid-snapped render origin the projection expects to have been
    /// subtracted (#1496). Ground cover positions terrain vertices in absolute
    /// world space, so this is what closes the gap.
    pub render_origin: [f32; 3],
    /// `[dir.x, dir.y, speed, gust_amplitude]`.
    pub wind: [f32; 4],
    pub gust_frequency: f32,
    /// The one renderer-owned wind clock. `prepare` derives the prior sample
    /// from this and `delta_seconds`; ground cover never accumulates a second
    /// clock of its own.
    pub time_seconds: f32,
    /// Pixels per world unit at one unit of depth: `render_height * 0.5 /
    /// tan(fov_y * 0.5)`. §6 keys blade widening to projected pixel size
    /// rather than distance, so this has to come from the live projection —
    /// a constant here would make the widening correct at exactly one
    /// resolution and shimmer at every other.
    pub pixels_per_unit_at_unit_depth: f32,
    /// Render the accepted candidate points instead of blades (§9 Phase 1).
    pub debug_points: bool,
    /// Entities disturbing the sward this frame (§12.4), nearest first. The
    /// excess past `GROUNDCOVER_INTERACTION_MAX_DISTURBERS` is dropped, which
    /// is why the host sorts.
    pub disturbers: &'a [GpuGroundCoverDisturber],
    /// Seconds since the previous frame, for the field's decay. Clamped
    /// renderer-side: a long hitch must not clear a trail outright, and a
    /// negative or non-finite value must not revive one.
    pub delta_seconds: f32,
    /// Host cell-table overflow. The camera-centred residency ring does not
    /// truncate chunks to the blade arena; a non-zero value is therefore an
    /// explicit cell-table capacity fault, not ordinary ring fill-in (#4338).
    pub chunks_truncated: u32,
}

/// `atomicMin` seed. The clear fills the buffer with zero, which is the wrong
/// identity for a minimum — so the host seeds the two `min` slots after the
/// fill and before the dispatch.
const EXTREMA_MIN_SEED: u32 = u32::MAX;

/// Vertices one tier-0 blade emits.
pub const VERTS_PER_BLADE_NEAR: u32 =
    GROUNDCOVER_BLADE_SEGMENTS_NEAR * GROUNDCOVER_VERTS_PER_SEGMENT;
/// Vertices one tier-1 blade emits. Both representations index the same fixed
/// blade slab; their independent indirect streams carry their own stride.
pub const VERTS_PER_BLADE_MID: u32 = GROUNDCOVER_BLADE_SEGMENTS_MID * GROUNDCOVER_VERTS_PER_SEGMENT;
/// Near ribbons, reduced mid ribbons, and far clump cards.
const GROUNDCOVER_INDIRECT_STREAMS: u64 = 3;
/// Bytes per blade-tier indirect command: `groundcover_scatter.comp`'s
/// `GcDrawIndirect` is a `VkDrawIndirectCommand`. Sizes the indirect buffer
/// and strides every `cmd_draw_indirect`; the GLSL struct's std430 size is
/// pinned to it (#4956).
pub(crate) const GC_DRAW_INDIRECT_STRIDE: u64 =
    std::mem::size_of::<vk::DrawIndirectCommand>() as u64;

/// The multiplier the frame serial is scaled by before the tier index is added
/// into `BladePush::gust_and_timing[3]`, i.e. `1 << tier_bits`.
///
/// It must be a power of two strictly greater than the largest tier index, so
/// that the shader's `& (stride - 1)` recovers the tier and `>> tier_bits`
/// recovers the serial. Sized from `GROUNDCOVER_INDIRECT_STREAMS` rather than
/// written as a literal so adding a stream widens the field instead of
/// silently carrying into the serial — the #4056 bug.
const GROUNDCOVER_LOD_TIER_STRIDE: u64 = GROUNDCOVER_INDIRECT_STREAMS.next_power_of_two();

/// Bits of frame serial the packed LOD word can carry: f32 represents
/// integers exactly only to 2^24, and the word is packed as a float — so
/// `serial * stride + tier` must stay below 2^24 for the per-stream
/// `+ tier as f32` in `record_draw` to survive rounding. Masking the serial
/// to `24 − tier_bits` bits before packing guarantees that forever; without
/// it the odd tiers round away after `2^24 / stride` frames (~19.4 h at 60
/// fps) and the mid stream decodes as tier 0 while drawing mid geometry —
/// the #4498 corruption. The blue-noise tile rotation the serial feeds
/// (`vFrameSerial * uvec2(5u, 3u)`) wraps by construction, so the period is
/// free. The vertex shader masks its unpack to match.
const GROUNDCOVER_FRAME_SERIAL_BITS: u32 = 24 - GROUNDCOVER_LOD_TIER_STRIDE.trailing_zeros();
const GROUNDCOVER_FRAME_SERIAL_MASK: u64 = (1 << GROUNDCOVER_FRAME_SERIAL_BITS) - 1;

pub struct GroundCoverPipeline {
    scatter_set_layout: vk::DescriptorSetLayout,
    scatter_pipeline_layout: vk::PipelineLayout,
    scatter_pipeline: vk::Pipeline,

    /// §12.4's field update. Its own set and pipeline: it shares no buffer
    /// with the scatter, runs before it, and giving it the scatter's layout
    /// would let a future edit reach the blade buffer from here.
    interaction_set_layout: vk::DescriptorSetLayout,
    interaction_pipeline_layout: vk::PipelineLayout,
    interaction_pipeline: vk::Pipeline,

    /// Set 2 for the draw pipelines. Sets 0 and 1 are the shared bindless
    /// texture array and scene descriptor set, exactly as `water.rs` binds
    /// them — blades are lit by the same lights, through the same TLAS.
    draw_set_layout: vk::DescriptorSetLayout,
    draw_pipeline_layout: vk::PipelineLayout,
    blade_pipeline: vk::Pipeline,
    debug_pipeline: vk::Pipeline,

    descriptor_pool: vk::DescriptorPool,
    scatter_sets: Vec<vk::DescriptorSet>,
    draw_sets: Vec<vk::DescriptorSet>,

    chunk_buffers: Vec<GpuBuffer>,
    cell_buffers: Vec<GpuBuffer>,
    species_buffers: Vec<GpuBuffer>,
    /// Per frame-in-flight species selection table (§7), read by the scatter.
    species_table_buffers: Vec<GpuBuffer>,
    blade_buffer: Option<GpuBuffer>,
    indirect_buffer: Option<GpuBuffer>,
    counter_buffer: Option<GpuBuffer>,
    /// Per-slot readback of `counter_buffer`, copied at the end of the scatter
    /// so the histogram (§11.3) and the overflow tally can be reported without
    /// stalling the frame that produced them.
    counter_readback: Vec<GpuBuffer>,
    /// Both halves of §12.4's field, device-local. Persistent across frames
    /// by design — the field is stateful, and clearing it per frame is the
    /// naive version this design exists not to be.
    field_buffer: Option<GpuBuffer>,
    field_state_buffers: Vec<GpuBuffer>,
    disturber_buffers: Vec<GpuBuffer>,
    interaction_sets: Vec<vk::DescriptorSet>,
    /// The previous frame's snapped field origin, and whether the previous
    /// half holds anything to reproject from. `None` until the first update,
    /// which is what makes the first frame start from an empty field rather
    /// than from whatever `create_device_local_uninit` left behind.
    field_previous: Option<[f32; 2]>,
    field_write_half: u32,
    frame_field_state: GpuGroundCoverFieldState,
    frame_disturber_count: u32,

    bound_vertex_buffer: vk::Buffer,
    /// Chunk count each in-flight slot dispatched, so the readback taken one
    /// cycle later is attributed to the right frame.
    pending_chunks: [u32; MAX_FRAMES_IN_FLIGHT],
    /// Physical ring-slot prefix dispatched for each in-flight frame. This is
    /// distinct from `pending_chunks`: inactive holes must be scanned by
    /// scatter/draw to preserve slab indices, but are not visible chunks.
    pending_chunk_slots: [u32; MAX_FRAMES_IN_FLIGHT],
    /// `frame_chunks_truncated` for each in-flight slot, harvested alongside
    /// `pending_chunks`.
    pending_truncated: [u32; MAX_FRAMES_IN_FLIGHT],
    stats: GroundCoverStats,
    /// Chunks uploaded for the frame currently being recorded.
    frame_chunk_count: u32,
    /// Active records in `frame_chunk_count`'s fixed-slot prefix.
    frame_active_chunk_count: u32,
    /// Chunks dropped at the caps for the frame currently being recorded.
    frame_chunks_truncated: u32,
    frame_debug_points: bool,
    /// The placed-geometry cover test's reach this frame: the palette's
    /// tallest blade. A surface lower than that over a root is one a blade
    /// rooted there would pierce, which is the whole criterion — so the
    /// reach is not a tuned distance but a property of what is growing.
    frame_cover_reach: f32,
    frame_push: BladePush,
}


// #5089 — construction and per-frame halves split into submodule files
// so neither the file nor any one function keeps growing unbounded.
mod construct;
mod frame;

/// The allocation-sizing figure `docs/engine/memory-budget.md` ledgers
/// (#4300); lives with `create_buffers` in `construct.rs`.
pub use construct::groundcover_resident_bytes;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shader_constants::GROUNDCOVER_HISTOGRAM_BUCKETS;
    use crate::vulkan::groundcover_stats::GROUNDCOVER_FACTOR_NAMES;


    /// The GPU records are the shader contract. std430 rounds a `vec4` to a
    /// 16-byte boundary, so a record whose Rust side lost its explicit padding
    /// would read the next field's bytes as an affinity weight — plausible
    /// numbers, wrong ground.
    #[test]
    fn gpu_records_match_their_std430_layout() {
        assert_eq!(std::mem::size_of::<GpuGroundCoverCell>(), 64);
        assert_eq!(std::mem::size_of::<GpuGroundCoverChunk>(), 32);
        assert_eq!(std::mem::size_of::<GpuGroundCoverSpecies>(), 80);
        // §4's blade record is "~16 bytes". Nothing on the host writes one,
        // but `create_buffers` sizes the blade buffer from it, and
        // `name_diverging_glsl_rust_mirrors_stay_in_lockstep` pins the GLSL
        // record to these same four fields (#4335).
        assert_eq!(std::mem::size_of::<GpuGroundCoverBlade>(), 16);
        assert_eq!(
            (GROUNDCOVER_MAX_CHUNKS as u64)
                * (GROUNDCOVER_MAX_BLADES_PER_CHUNK as u64)
                * std::mem::size_of::<GpuGroundCoverBlade>() as u64,
            64 * 1024 * 1024,
            "the blade buffer's documented 64 MB is derived from these two caps"
        );
    }

    /// #4729 — the blade lean must follow WindField's declared "blows
    /// toward" sense (`+windDir` in engine XZ), the same direction the §8
    /// gust advection rolls its waves. The old `-windDir.y` mirrored the
    /// lean on Z, so blades leaned against their own gust waves on any
    /// wind with a north-south component.
    #[test]
    fn blade_lean_follows_the_wind_blows_toward_sense() {
        let blade = include_str!("../../shaders/groundcover_blade.vert");
        assert!(
            blade.contains("normalize(vec3(windDir.x, 0.0, windDir.y))"),
            "byroGcWindBend's leanDir must lean blades along +windDir in \
             engine XZ (#4729)"
        );
        assert!(
            !blade.contains("-windDir.y"),
            "the mirrored `-windDir.y` lean must not come back — it leaned \
             blades against their own gust waves (#4729)"
        );
        // The gust advection was already correct: the noise field sampled
        // at `base.xz - windDir·v·t` makes wave crests travel +windDir.
        // Pin it so a future "fix" cannot invert the pair's agreement.
        assert!(
            blade.contains("base.xz - windDir"),
            "gust advection must keep sampling at base - windDir·v·t so \
             crests travel along +windDir (the lean's sense)"
        );
    }

    /// #4338 — slot indices are blade-arena ownership, not a compact draw
    /// list.  A vacant ring slot must remain an explicit 32-byte record and
    /// produce a no-op indirect command; otherwise the next resident shifts
    /// into its slab even though the host-side ring says it did not move.
    #[test]
    fn inactive_residency_slots_remain_explicit_and_are_skipped_by_scatter() {
        let inactive = GpuGroundCoverChunk::default();
        assert_eq!(inactive.active, 0);
        assert_eq!(inactive.entry_progress, 0.0);
        assert_eq!(inactive.layout_order, 0);
        assert_eq!(inactive.pad, 0);

        let scene = include_str!("../../shaders/include/groundcover_scene.glsl");
        let scatter = include_str!("../../shaders/groundcover_scatter.comp");
        assert!(
            scene.contains("uint slotActive;")
                && scene.contains("float entryProgress;")
                && scene.contains("uint layoutOrder;")
                && scene.contains("uint pad;"),
            "the GLSL record must retain the host's explicit inactive-slot and grow-in lanes"
        );
        assert!(
            scatter.contains("if (chunk.slotActive == 0u)")
                && scatter.contains("gcDraws[chunkIdx].instanceCount = 0u;"),
            "scatter must turn a vacant slot into a no-op indirect draw before reading its cell"
        );
    }

    #[test]
    fn push_block_fits_the_guaranteed_minimum() {
        // Vulkan guarantees only 128 bytes of push constants. This device
        // allows 256, so a layout that overran the floor would work here and
        // fail on hardware nobody in this repo is testing on.
        assert_eq!(std::mem::size_of::<BladePush>(), 64);
        assert!(std::mem::size_of::<ScatterPush>() <= 128);
        assert_eq!(
            std::mem::offset_of!(BladePush, gust_and_timing) + std::mem::size_of::<f32>(),
            52,
            "the previous wind-clock sample is push-constant byte 52; the \
             GLSL `GcBladePush.gustAndCounts.y` motion contract must be kept \
             in lockstep (#4297)"
        );
    }



    /// #4296 — blades project with the camera UBO's view-projection and take
    /// the frame's projection jitter on `gl_Position` only, like
    /// `triangle.vert` and `water.vert`. The block must stay a prefix of
    /// `GpuCamera` through `jitter` for `gcJitter` to read the right lane.
    #[test]
    fn blades_project_with_the_jittered_camera_ubo() {
        use crate::vulkan::reflect::uniform_block_size_by_name;
        use crate::vulkan::scene_buffer::GpuCamera;
        let spv: &[u8] = include_bytes!("../../shaders/groundcover_blade.vert.spv");
        let jitter_end = std::mem::offset_of!(GpuCamera, jitter) + 16;
        assert_eq!(
            uniform_block_size_by_name(spv, "GcCameraUBO").expect("reflect blade vert"),
            Some(jitter_end as u32),
            "GcCameraUBO must mirror GpuCamera up to and including `jitter`"
        );

        let vert = include_str!("../../shaders/groundcover_blade.vert");
        assert!(
            !vert.contains(concat!("pc.", "viewProj")),
            "the push block no longer carries a view-projection; blades must \
             use the UBO's DOF-effective matrix (#4296)"
        );
        assert!(vert.contains("clip.xy += gcJitter.xy * clip.w;"));
        let positions: Vec<&str> = vert
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("gl_Position ="))
            .collect();
        assert_eq!(positions.len(), 3, "one gl_Position write per blade path");
        assert!(
            positions
                .iter()
                .all(|line| *line == "gl_Position = gcJittered(vCurrClipPos);"),
            "every blade path must jitter its (un-jittered) motion clip position: \
             {positions:?}"
        );
    }


    /// #4297 / Step 3 — every stochastic ground-cover seed is derived in the
    /// integer domain. A vendor-dependent float-trig hash makes a dithered LOD
    /// transition shear on AMD; a frame/time-derived seed instead becomes
    /// full-screen temporal noise. Keep the four live seed paths pinned here
    /// until their GLSL can share one common include. #4923 added the
    /// authored-model tier's `gcModelHash`, and its placement's other half:
    /// ranks and slab slots come from a serial per-batch pass, never atomics,
    /// so the same plants survive every frame.
    #[test]
    fn groundcover_seed_hashes_are_integer_and_frame_invariant() {
        let models = include_str!("../../shaders/groundcover_models.comp");
        let scatter = include_str!("../../shaders/groundcover_scatter.comp");
        let density = include_str!("../../shaders/include/groundcover_density.glsl");
        let blade = include_str!("../../shaders/groundcover_blade.vert");
        let scatter_hash = scatter
            .split_once("uint gcHash(uint seed, uint index)")
            .expect("scatter must retain its per-candidate integer finalizer")
            .1
            .split_once("/// #4057")
            .expect("gcHash must remain bounded before the density helpers")
            .0;
        let density_hash = density
            .split_once("vec2 byroGcHash2(vec2 cell)")
            .expect("density noise must retain its integer hash")
            .1
            .split_once("float byroGcHash1")
            .expect("byroGcHash2 must remain bounded before its scalar wrapper")
            .0;
        let blade_seed = blade
            .split_once("float gcSeedStream(uint seed, uint stream)")
            .expect("blade attributes must retain their seed stream")
            .1
            .split_once("vec3 byroGcWindBend")
            .expect("gcSeedStream must remain bounded before the wind model")
            .0;

        let model_hash = models
            .split_once("uint gcModelHash(uint seed, uint index)")
            .expect("the model tier must retain its per-candidate integer finalizer")
            .1
            .split_once("float gcUnit(")
            .expect("gcModelHash must remain bounded before gcUnit")
            .0;
        for (name, source) in [
            ("scatter candidate", scatter_hash),
            ("density noise", density_hash),
            ("blade attribute", blade_seed),
            ("model candidate", model_hash),
        ] {
            assert!(
                !source.contains("sin(")
                    && !source.contains("fract(")
                    && !source.contains("43758")
                    && !source.contains("frameIndex")
                    && !source.contains("time"),
                "{name} seed path must be an integer-only, frame-invariant hash"
            );
        }
        assert!(
            scatter_hash.contains("uint h = seed ^ (index * 0x9E3779B9u);")
                && scatter_hash.contains("h *= 0x7FEB352Du;")
                && scatter_hash.contains("h *= 0x846CA68Bu;"),
            "scatter seeds must be a pure function of chunk seed and candidate index"
        );
        assert!(
            model_hash.contains("uint h = seed ^ (index * 0x9E3779B9u);")
                && model_hash.contains("h *= 0x7FEB352Du;")
                && model_hash.contains("h *= 0x846CA68Bu;"),
            "model seeds must be the same pure function of chunk seed and candidate index"
        );
        let place = models
            .split_once("void place(")
            .expect("the model tier's placement phase")
            .1
            .split_once("void layoutShapes(")
            .expect("place must end before layoutShapes")
            .0;
        assert!(
            !place.contains("atomic"),
            "model placement must assign ranks and slots serially, never by atomics — \
             an atomic append makes the surviving plant set scheduling-dependent"
        );
        assert!(
            place.contains("uint seed = chunk.seed ^ GROUNDCOVER_MODEL_SEED_SALT;")
                && place.contains("uint h = gcModelHash(seed, candidate);"),
            "model candidates must hash from the salted chunk seed and candidate index"
        );
    }

    /// `cmd_draw_indirect` is issued with a stride of `GC_DRAW_INDIRECT_STRIDE`,
    /// which is `sizeof(VkDrawIndirectCommand)`. A mismatch would read every
    /// command but the first from the wrong offset. The GLSL `GcDrawIndirect`
    /// leg is `name_diverging_glsl_rust_mirrors_stay_in_lockstep` (#4956).
    #[test]
    fn indirect_stride_matches_the_command() {
        assert_eq!(
            GC_DRAW_INDIRECT_STRIDE,
            std::mem::size_of::<vk::DrawIndirectCommand>() as u64
        );
        assert_eq!(GC_DRAW_INDIRECT_STRIDE, 16);
    }

    /// The counter buffer packs three different things. An off-by-one here
    /// would attribute histogram buckets to chunk cursors, which reads as a
    /// plausible density distribution rather than as corruption.
    #[test]
    fn counter_layout_is_contiguous_and_ordered() {
        assert_eq!(COUNTER_HIST_BASE, GROUNDCOVER_MAX_CHUNKS as usize);
        assert_eq!(
            COUNTER_OVERFLOW,
            COUNTER_HIST_BASE + GROUNDCOVER_HISTOGRAM_BUCKETS as usize
        );
        assert_eq!(COUNTER_EXTREMA_BASE, COUNTER_OVERFLOW + 1);
        assert_eq!(COUNTER_FACTOR_BASE, COUNTER_EXTREMA_BASE + 4);
        assert_eq!(COUNTER_COVERED, COUNTER_FACTOR_BASE + 5);
        assert_eq!(COUNTER_SLOTS, COUNTER_COVERED + 1);
        let src = include_str!("../../shaders/groundcover_scatter.comp");
        assert!(
            src.contains("const uint SLOT_COVERED = SLOT_FACTOR_BASE + 5u;"),
            "the scatter's covered tally must sit immediately after the five factor \
             maxima, which is where COUNTER_COVERED reads it from"
        );
        assert!(
            src.contains("const uint EXTREMA_BASE = OVERFLOW_SLOT + 1u;"),
            "the scatter's extrema block must sit immediately after the overflow \
             tally, which is where COUNTER_EXTREMA_BASE reads it from"
        );
    }

    /// §4's 8×8 grid of 512-unit chunks has to tile the 4096-unit exterior
    /// cell exactly, or the chunk grid and the terrain grid drift apart and
    /// the seams show as density bands.
    #[test]
    fn chunk_grid_tiles_the_exterior_cell() {
        assert_eq!(CHUNKS_PER_CELL, 64);
        assert_eq!(
            GROUNDCOVER_CHUNKS_PER_CELL_SIDE as f32
                * crate::shader_constants::GROUNDCOVER_CHUNK_UNITS,
            crate::shader_constants::EXTERIOR_CELL_UNITS
        );
    }

    /// A tier-0 blade is 3 Bezier segments of two triangles each. The scatter
    /// writes `accepted * verts_per_blade` into the indirect `vertexCount`, so
    /// this factor being wrong truncates or overruns every blade in the frame.
    #[test]
    fn tier_zero_blade_vertex_count() {
        assert_eq!(VERTS_PER_BLADE_NEAR, 18);
        assert_eq!(VERTS_PER_BLADE_MID, 6);
        assert_eq!(crate::shader_constants::GROUNDCOVER_SCATTER_WORKGROUP, 64);
    }

    /// #4056 — the tier field must be wide enough for every dispatched stream,
    /// and #4498 — the packed word must stay f32-exact for the life of the
    /// process.
    ///
    /// The host packs `(serial * stride) + tier` into one float and the vertex
    /// shader unpacks it with a mask and a shift. Those three numbers are
    /// written in two languages and were not derived from each other: the mask
    /// was `1u` while three streams were dispatched, so the clump-card tier
    /// decoded as `2 & 1 == 0`. Tier 2 drew its card-strided indirect command
    /// as three-segment tuft blades — wrong geometry addressing a chunk slab
    /// it does not own — and the dropped bit carried into the serial, moving
    /// that stream's blue-noise rank a frame out of step with the others.
    ///
    /// Recomputing both halves here from `GROUNDCOVER_INDIRECT_STREAMS` is
    /// what keeps a fourth stream from reintroducing it silently. The f32
    /// half is the clock-bounded variant of the same shape: past
    /// `2^24 / stride / 60` seconds (~19.4 h at 60 fps) the unmasked serial ×
    /// stride exceeds f32's exact-integer range, the spacing widens to 2, and
    /// `record_draw`'s per-stream `+ tier as f32` rounds every odd tier away.
    #[test]
    fn lod_tier_field_is_wide_enough_for_every_indirect_stream() {
        let blade = include_str!("../../shaders/groundcover_blade.vert");
        let stride = GROUNDCOVER_LOD_TIER_STRIDE;
        assert!(
            stride.is_power_of_two() && stride > GROUNDCOVER_INDIRECT_STREAMS - 1,
            "stride {stride} cannot represent tier {}",
            GROUNDCOVER_INDIRECT_STREAMS - 1
        );
        let bits = stride.trailing_zeros();
        assert!(
            blade.contains(&format!(
                "#define GC_LOD_TIER         (GC_LOD_WORD & {}u)",
                stride - 1
            )),
            "GC_LOD_TIER must mask {bits} bits"
        );
        assert!(
            blade.contains(&format!(
                "#define GC_FRAME_SERIAL     ((GC_LOD_WORD >> {bits}u) & 0x{:X}u)",
                GROUNDCOVER_FRAME_SERIAL_MASK
            )),
            "GC_FRAME_SERIAL must shift past the {bits}-bit tier field and mask \
             the serial to the {} bits the host packed",
            GROUNDCOVER_FRAME_SERIAL_BITS
        );
        assert_eq!(GROUNDCOVER_FRAME_SERIAL_BITS, 24 - bits);
        // Every tier the draw loop dispatches must round-trip through the
        // packing the host actually writes.
        for serial in [0_u64, 1, 12_345] {
            for tier in 0..GROUNDCOVER_INDIRECT_STREAMS {
                let word = serial * stride + tier;
                assert_eq!(word & (stride - 1), tier);
                assert_eq!(word >> bits, serial);
            }
        }
        // #4498 — a serial from beyond the f32 horizon (t > 2^24 / stride /
        // 60 s), through exactly the arithmetic the two sides perform: the
        // pack masks, scales in f32; `record_draw` adds the tier in f32 per
        // stream; the shader masks the serial back out. The blue-noise tile
        // rotation the truncated serial feeds wraps by construction.
        let horizon_seconds = ((1_u64 << 24) / stride / 60) as f32;
        let t = horizon_seconds + 100.0; // any f32-exact second past it
        let raw_serial = (t.max(0.0) * 60.0).floor() as u64;
        assert!(
            raw_serial * stride > 1_u64 << 24,
            "the case must sit past the horizon where serial*stride stops \
             being f32-exact"
        );
        // The shape of the corruption the mask prevents, at that very
        // serial: unmasked, an odd tier rounds away into the word itself.
        assert_eq!(
            (raw_serial * stride) as f32 + 1.0,
            (raw_serial * stride) as f32
        );
        let masked = raw_serial & GROUNDCOVER_FRAME_SERIAL_MASK;
        for tier in 0..GROUNDCOVER_INDIRECT_STREAMS {
            let packed = (masked as f32) * stride as f32;
            let word = packed + tier as f32;
            assert_eq!(
                word as u64,
                masked * stride + tier,
                "tier {tier} rounded away past the horizon"
            );
            assert_eq!((word as u64) & (stride - 1), tier);
            assert_eq!((word as u64) >> bits, masked);
        }
    }

    /// The scatter packs three different things into one counter buffer and
    /// the host unpacks them. The two sides derive their section offsets
    /// independently — GLSL from the generated header, Rust from
    /// `shader_constants` — so this pins the GLSL text against the Rust
    /// values. A drift would attribute histogram buckets to chunk cursors,
    /// which reads as a plausible density distribution rather than as
    /// corruption, and the `every_top_level_shader_constant_has_one_provenance`
    /// gate exempts these two names on the strength of exactly this test.
    #[test]
    fn scatter_counter_layout_matches_the_host() {
        let src = include_str!("../../shaders/groundcover_scatter.comp");
        assert!(
            src.contains("const uint HIST_BASE = GROUNDCOVER_MAX_CHUNKS;"),
            "the scatter's histogram base must still be GROUNDCOVER_MAX_CHUNKS, \
             which is what COUNTER_HIST_BASE resolves to host-side"
        );
        assert!(
            src.contains(
                "const uint OVERFLOW_SLOT = GROUNDCOVER_MAX_CHUNKS + GROUNDCOVER_HISTOGRAM_BUCKETS;"
            ),
            "the scatter's overflow slot must still sit immediately after the \
             histogram, which is what COUNTER_OVERFLOW resolves to host-side"
        );
    }

    /// §3's `moisture` term must resolve to 1.0 where there is no water plane.
    /// It is the trap the issue calls out by name: in a pure product one
    /// undefined factor takes the whole field, and the symptom is an entire
    /// worldspace with no ground cover and nothing in the log.
    #[test]
    fn no_water_resolves_to_a_neutral_moisture_term() {
        let src = include_str!("../../shaders/include/groundcover_density.glsl");
        let moisture = src
            .split_once("float byroGcMoisture(")
            .expect("the density field must still define byroGcMoisture")
            .1;
        let body = moisture.split_once("\n}").expect("unterminated function").0;
        let guard = body
            .find("waterY <= GROUNDCOVER_NO_WATER")
            .expect("byroGcMoisture must branch on the no-water sentinel");
        let neutral = body[guard..]
            .find("return 1.0;")
            .expect("the no-water branch must return 1.0, not 0.0 — see #4054");
        let submerged = body[guard..].find("return 0.0;").unwrap_or(usize::MAX);
        assert!(
            neutral < submerged,
            "the no-water branch must return before any zero return; a cell with \
             no water is neutral, not hostile"
        );
    }

    /// `clump(noise)` is load-bearing, not decorative (§3): it is the only
    /// term in the product with authority above the ~128-unit splat grid, so a
    /// build that stubs it to 1.0 reproduces the vanilla patch look exactly
    /// and looks like the design failed.
    #[test]
    fn the_clump_term_is_not_stubbed() {
        let src = include_str!("../../shaders/include/groundcover_density.glsl");
        let clump = src
            .split_once("float byroGcClump(")
            .expect("the density field must still define byroGcClump")
            .1
            .split_once("\n}")
            .expect("unterminated function")
            .0;
        assert!(
            clump.contains("byroGcWorley(") && clump.contains("byroGcFbm("),
            "byroGcClump must evaluate both octaves §3 specifies — a Worley \
             field at the clump scale times a regional fBm"
        );
        assert!(
            src.contains("f.clump = byroGcClump(worldXZ);")
                && src.contains("f.affinity * f.slope * f.moisture * f.shelter * f.clump"),
            "the clump term must still be one of the five factors the product \
             combines — dropping it is what reproduces the vanilla patch look"
        );
    }

    /// §3: only the scatter's accept test may read `d_draw`; the blade record
    /// stores `d_ground`. Storing the faded value makes the shadow under a
    /// meadow lighten as the camera retreats.
    #[test]
    fn the_blade_record_stores_d_ground_not_d_draw() {
        let src = include_str!("../../shaders/groundcover_scatter.comp");
        assert!(
            src.contains("blade.dGround = dGround;"),
            "the blade record must store d_ground"
        );
        assert!(
            !src.contains("blade.dGround = dDraw"),
            "the blade record must never store the view-faded value (#4054)"
        );
        let accept = src
            .find("accept < dDraw")
            .expect("the accept test must read d_draw");
        let fade = src
            .find("byroGcDistanceFade(")
            .expect("d_draw must come from the distance fade");
        assert!(fade < accept, "d_draw must be computed before it is tested");
    }

    #[test]
    fn blade_control_points_preserve_length_after_combined_bends() {
        let src = include_str!("../../shaders/groundcover_blade.vert");
        assert!(
            src.contains("float bendFraction = min(length(bend) / max(height, GROUNDCOVER_BLADE_VECTOR_EPSILON), 1.0);")
                && src.contains("float uprightScale = sqrt(max(1.0 - bendFraction * bendFraction, 0.0));"),
            "combined wind and interaction bend must reduce vertical reach so gusts cannot grow blades"
        );
    }

    #[test]
    fn blade_wind_uses_seeded_harmonic_and_lateral_sway() {
        let src = include_str!("../../shaders/groundcover_blade.vert");
        let wind = src
            .split_once("vec3 byroGcWindBend")
            .expect("blade shader must retain its wind function")
            .1
            .split_once("void byroGcControlPoints")
            .expect("wind function must end before control-point construction")
            .0;
        for token in [
            "GROUNDCOVER_WIND_HARMONIC_FREQUENCY_MULTIPLIER",
            "GROUNDCOVER_WIND_SECONDARY_AMPLITUDE",
            "GROUNDCOVER_WIND_LATERAL_FRACTION",
            "vec3 lateralDir",
            "float lateralBend",
            "float heightSquaredScale = height * height / max(maxSpeciesHeight, GROUNDCOVER_BLADE_VECTOR_EPSILON);",
        ] {
            assert!(wind.contains(token), "wind polish must retain {token}");
        }
        assert!(
            !wind.contains("gl_VertexIndex"),
            "wind phase must remain blade-base/seed-derived rather than per vertex"
        );
    }

    #[test]
    fn wind_harmonic_cannot_repeat_its_full_phase_within_ten_seconds() {
        // 2.17 = 217 / 100: primary and secondary phases first align after
        // 100 fundamental cycles. `WindField::from_weather_byte` tops out at
        // 0.15 + 0.45 = 0.60 Hz, making that earliest full repeat 166.7 s.
        let harmonic = crate::shader_constants::GROUNDCOVER_WIND_HARMONIC_FREQUENCY_MULTIPLIER;
        assert_eq!(
            harmonic, 2.17,
            "this proof relies on the documented 217/100 ratio"
        );
        let fastest_gust_hz = 0.15 + 0.45;
        let full_phase_repeat_seconds = 100.0 / fastest_gust_hz;
        assert!(
            full_phase_repeat_seconds > 10.0,
            "the mixed wind modes must not complete a full phase loop in the ten-second review window"
        );
    }

    /// §4's overflow policy: ordered workgroup compaction saturates, it does
    /// not wrap, and candidate order remains reproducible for card clusters.
    #[test]
    fn deterministic_compaction_saturates() {
        let src = include_str!("../../shaders/groundcover_scatter.comp");
        assert!(
            src.contains("shared uint gcAcceptedLanes[GROUNDCOVER_SCATTER_WORKGROUP];")
                && src.contains("gcBatchBase = atomicAdd(gcCounters[chunkIdx], batchCount);")
                && src.contains("for (uint j = 0u; j < lane; ++j)"),
            "the scatter must compact in candidate-index order before reserving \
             its one batch range, so card clusters cannot depend on atomic order"
        );
        assert!(
            src.contains("if (slot < pc.bladesPerChunk) {")
                && src.contains("atomicAdd(gcCounters[OVERFLOW_SLOT], 1u);"),
            "a deterministic append must still count overflow rather than write \
             into the next chunk's fixed blade slice"
        );
        assert!(
            src.contains("uint accepted = min(gcCounters[chunkIdx], pc.bladesPerChunk);"),
            "the published draw must clamp the cursor: it deliberately runs \
             past the cap, so the raw value is candidates-that-tried"
        );
    }

    /// §12.2's ordering trap, pinned in the one place it can be: the shader
    /// text. Transmission must not be gated on the diffuse `max(N·L, 0)` — the
    /// near face of a lit blade is precisely the case that should glow — but
    /// it *must* still take the traced shadow, because a blade shadowed by a
    /// distant rock receives nothing. Folding it into the diffuse clamp is the
    /// shape a first implementation reaches for and the reason backlit grass
    /// so often comes out flat (#4057).
    #[test]
    fn transmission_survives_the_diffuse_clamp_but_not_the_shadow_ray() {
        let src = include_str!("../../shaders/groundcover_blade.frag");
        let body = src
            .split_once("void main()")
            .expect("the blade shader must still have a main")
            .1;
        let lobe = body
            .find("float lobe = byroGcTransmissionLobe(")
            .expect("§12.2's transmission lobe must be evaluated");
        let gate = body
            .find("if (diffuse <= 0.0 && lobe <= 0.0 && sheenLobe <= 0.0)")
            .expect(
                "the early-out must require ALL THREE lobes to be dark; a \
                 `diffuse <= 0.0` continue alone would drop every backlit \
                 blade before its transmission was ever evaluated",
            );
        assert!(
            lobe < gate,
            "the lobe must be computed before the early-out"
        );
        assert!(
            body.contains("transmitted += incoming * (lobe * bladeTransmittance);"),
            "transmission must accumulate separately from the diffuse term"
        );
        // `incoming` is the one place the traced shadow and the canopy
        // transmittance are folded in, and both lobes read it.
        assert!(
            body.contains("* shadow * canopy;"),
            "both lobes must still be gated on the traced world shadow"
        );
    }

    /// #4291 — the blade's directional loop must take the light's radiance
    /// from its colour alone, exactly as `shadowableLightRadiance`'s
    /// directional arm does (`atten = 1.0`). `params.x` is the point/spot
    /// falloff exponent and `collect_lights` writes it as `0.0` for every
    /// directional source, so any read of it inside this loop zeroes the sun,
    /// its traced shadow and the backlit transmission on every exterior blade
    /// — which is how the shader shipped from Phase 1 until this pin.
    #[test]
    fn directional_radiance_is_not_scaled_by_the_falloff_exponent() {
        let src = include_str!("../../shaders/groundcover_blade.frag");
        let body = src
            .split_once("void main()")
            .expect("the blade shader must still have a main")
            .1;
        let code: String = body
            .lines()
            .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !code.contains("params.x"),
            "the blade lights directional sources only; params.x is the \
             point/spot falloff exponent (0.0 on every directional GpuLight)"
        );
        assert!(
            code.contains("vec3 incoming = lights[i].color_type.rgb * shadow * canopy;"),
            "a directional light's incoming radiance is its colour times the \
             traced shadow and canopy transmittance"
        );
    }

    /// §12.1 and §12.5 are the same extinction through the same slab, so a
    /// build that gave occlusion its own strength parameter would let the two
    /// disagree about how thick the same grass is (§11.6's answer).
    #[test]
    fn occlusion_and_canopy_shadow_share_one_extinction() {
        let src = include_str!("../../shaders/include/groundcover_light.glsl");
        let depth = src
            .split_once("float byroGcCanopyOpticalDepth(")
            .expect("the canopy's optical depth must still be one function")
            .1
            .split_once("\n}")
            .expect("unterminated function")
            .0;
        assert!(
            depth.contains("GROUNDCOVER_CANOPY_EXTINCTION_K")
                && depth.contains("GROUNDCOVER_CANOPY_LEAF_AREA_DENSITY"),
            "the optical depth must be K x LAD x d x depth — two named physical \
             constants, not one opaque scalar (§11.9)"
        );
        for consumer in ["byroGcCanopyTransmittance", "byroGcSkyOcclusion"] {
            let body = src
                .split_once(&format!("float {consumer}("))
                .unwrap_or_else(|| panic!("{consumer} must exist"))
                .1
                .split_once("\n}")
                .expect("unterminated function")
                .0;
            assert!(
                body.contains("byroGcCanopyOpticalDepth("),
                "{consumer} must go through the shared optical depth; a second \
                 expression here is how §12.1 and §12.5 come to disagree"
            );
        }
    }

    /// §12.5's terrain receiver has to read the SAME density the sward it
    /// shadows was scattered from, off the same include. A second expression
    /// in `triangle.frag` would let the shadow's density and the grass's drift
    /// apart with nothing failing (#4057).
    #[test]
    fn the_terrain_receiver_reuses_the_shared_density_field() {
        let src = include_str!("../../shaders/triangle.frag");
        assert!(
            src.contains("#include \"include/groundcover_density.glsl\""),
            "triangle.frag must evaluate the shared field, not a local copy"
        );
        assert!(
            src.contains("gGcDGround = byroGcDensityGround(") && src.contains("byroGcLaplacian("),
            "the terrain receiver must call the shared density entry point and \
             the shared curvature stencil"
        );
        assert!(
            src.contains("terrainTile.canopyHeight > 0.0"),
            "a zero canopy height must disable the term — that is what a LOD \
             tile, an interior and an unresolved palette all arrive as"
        );
        let lighting = include_str!("../../shaders/include/lighting.glsl");
        assert!(
            lighting.contains("lightType >= 1.5 && gGcCanopyHeight > 0.0"),
            "the canopy must attenuate DIRECTIONAL light only, at \
             shadowableLightRadiance's single exit — splitting it across \
             triangle.frag's call sites breaks the #1369 cancel-bit-for-bit \
             invariant between the ReSTIR passes"
        );
    }

    /// §12.4's records are shader contracts like every other GPU struct here.
    #[test]
    fn interaction_records_match_their_std430_layout() {
        assert_eq!(std::mem::size_of::<GpuGroundCoverDisturber>(), 16);
        assert_eq!(std::mem::size_of::<GpuGroundCoverFieldState>(), 32);
        // 256² texels x 4 B x two halves = 512 KB device-local.
        assert_eq!(INTERACTION_TEXEL_COUNT * 2 * 4, 512 * 1024);
        // The field is centred on the camera and one texel must be coarse
        // enough that neighbouring blades read nearly the same value — that
        // shared read is what makes them part together rather than tip
        // individually — and fine enough to give a human footfall shape.
        assert_eq!(GROUNDCOVER_INTERACTION_TEXEL_UNITS, 8.0);
        let human = 36.0;
        assert!(
            human / GROUNDCOVER_INTERACTION_TEXEL_UNITS >= 4.0,
            "a vanilla actor capsule must span at least 4 texels, or the \
             channel it opens has no shape (§12.4)"
        );
    }

    /// §12.4's recovery is the part that is easy to get wrong: the field must
    /// **decay**, never clear, or a blade snaps upright the instant an entity
    /// passes and draws the eye straight to the boundary.
    #[test]
    fn the_interaction_field_decays_rather_than_clearing() {
        let src = include_str!("../../shaders/groundcover_interaction.comp");
        assert!(
            src.contains("value *= exp(-0.6931472 * dt"),
            "the field must decay exponentially toward zero, framerate-independently"
        );
        assert!(
            src.contains("unpackHalf2x16(gcField[byroGcFieldIndex(prevTexel, prevHalf)])"),
            "each frame must start from the PREVIOUS frame's field — a shader \
             that re-splats from scratch is the naive version §12.4 exists not \
             to be"
        );
        assert!(
            src.contains("if (dot(push, push) > dot(value, value))"),
            "disturbers must combine by per-texel maximum, not by sum: a second \
             pass over the same ground refreshes the trail rather than doubling it"
        );
        // The reprojection has to be an exact texel copy. A filter applied
        // every frame to its own output is a low-pass at frame rate.
        assert!(
            src.contains("ivec2(round(byroGcFieldCoord("),
            "the reprojection must round to a texel, which the host's snapped \
             origin makes exact"
        );
    }



    #[test]
    fn empty_stats_render_a_well_formed_row() {
        let line = GroundCoverStats::default().bench_line();
        assert!(
            line.starts_with("groundcover: chunks=0 blades=0 overflow=0 d_ground=0.0000..0.0000")
        );
        assert!(line.contains("d_ground_hist="));
        // Every factor named, so a zeroed term is legible without a lookup.
        for name in GROUNDCOVER_FACTOR_NAMES {
            assert!(line.contains(&format!("{name}:0.000")), "{line}");
        }
        // 15 histogram separators plus the four between the five factors.
        assert_eq!(line.matches(',').count(), 19);
        // #4338 — a cap-truncated frame is reported, not just shorter.
        assert!(line.ends_with(" truncated=0"), "{line}");
    }
}
#[cfg(test)]
mod memory_budget_ledger_tests {
    /// `1234567` → `"1,234,567"`.
    fn grouped(n: u64) -> String {
        let digits = n.to_string();
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push(',');
            }
            out.push(c);
        }
        out
    }

    /// #4300 — the three fixed-size owners SKYAL and EXAL added were absent
    /// from `memory-budget.md`, and the one size helper that claimed to feed
    /// the page had no caller. The page now states each owner's exact
    /// resident bytes; this holds those figures to the functions the
    /// allocation code shares.
    #[test]
    fn memory_budget_ledgers_the_sky_and_ground_cover_owners() {
        let doc = include_str!("../../../../docs/engine/memory-budget.md");
        let section = doc
            .split_once("## Sky and Ground Cover (fixed-size)")
            .expect("memory-budget.md must keep its SKYAL / EXAL section")
            .1;
        let section = &section[..section.find("\n## ").unwrap_or(section.len())];
        for (owner, bytes) in [
            ("EXAL ground cover", super::groundcover_resident_bytes()),
            (
                "EXAL ground-cover model tier",
                crate::vulkan::groundcover_models::groundcover_model_resident_bytes(),
            ),
            (
                "SKYAL sky bake",
                crate::vulkan::sky_cube::sky_bake_resident_bytes(),
            ),
            (
                "SKYAL cloud noise",
                crate::vulkan::cloud_noise::cloud_noise_bytes(),
            ),
        ] {
            let row = section
                .lines()
                .find(|line| line.starts_with(&format!("| {owner} |")))
                .unwrap_or_else(|| panic!("no `{owner}` row in the SKYAL / EXAL ledger"));
            assert!(
                row.contains(&format!("**{} B**", grouped(bytes))),
                "memory-budget.md states a stale size for {owner}; the code allocates \
                 {} B: {row}",
                grouped(bytes)
            );
        }
    }
}
