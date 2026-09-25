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
use super::reflect::{validate_set_layout, ReflectedShader};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use crate::shader_constants::{
    GROUNDCOVER_BLADES_PER_POINT, GROUNDCOVER_BLADE_SEGMENTS_MID, GROUNDCOVER_BLADE_SEGMENTS_NEAR,
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
    /// Padding to the 64 B record. Two scalars rather than `[f32; 2]` so the
    /// declaration matches the GLSL `pad1, pad2` field for field, which is what
    /// lets `name_diverging_glsl_rust_mirrors_stay_in_lockstep` guard it (#4849).
    pub pad1: f32,
    pub pad2: f32,
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
    pub pad: [u32; 2],
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

impl GroundCoverPipeline {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        render_pass: vk::RenderPass,
        pipeline_cache: vk::PipelineCache,
        texture_set_layout: vk::DescriptorSetLayout,
        scene_set_layout: vk::DescriptorSetLayout,
    ) -> Result<Self> {
        let mut this = Self {
            scatter_set_layout: vk::DescriptorSetLayout::null(),
            scatter_pipeline_layout: vk::PipelineLayout::null(),
            scatter_pipeline: vk::Pipeline::null(),
            interaction_set_layout: vk::DescriptorSetLayout::null(),
            interaction_pipeline_layout: vk::PipelineLayout::null(),
            interaction_pipeline: vk::Pipeline::null(),
            draw_set_layout: vk::DescriptorSetLayout::null(),
            draw_pipeline_layout: vk::PipelineLayout::null(),
            blade_pipeline: vk::Pipeline::null(),
            debug_pipeline: vk::Pipeline::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            scatter_sets: Vec::new(),
            draw_sets: Vec::new(),
            chunk_buffers: Vec::new(),
            cell_buffers: Vec::new(),
            species_buffers: Vec::new(),
            species_table_buffers: Vec::new(),
            blade_buffer: None,
            indirect_buffer: None,
            counter_buffer: None,
            counter_readback: Vec::new(),
            field_buffer: None,
            field_state_buffers: Vec::new(),
            disturber_buffers: Vec::new(),
            interaction_sets: Vec::new(),
            field_previous: None,
            field_write_half: 0,
            frame_field_state: GpuGroundCoverFieldState::default(),
            frame_disturber_count: 0,
            bound_vertex_buffer: vk::Buffer::null(),
            pending_chunks: [0; MAX_FRAMES_IN_FLIGHT],
            pending_chunk_slots: [0; MAX_FRAMES_IN_FLIGHT],
            pending_truncated: [0; MAX_FRAMES_IN_FLIGHT],
            stats: GroundCoverStats::default(),
            frame_chunk_count: 0,
            frame_active_chunk_count: 0,
            frame_chunks_truncated: 0,
            frame_debug_points: false,
            frame_cover_reach: 0.0,
            frame_push: BladePush::default(),
        };
        macro_rules! try_or_cleanup {
            ($expr:expr) => {
                match $expr {
                    Ok(v) => v,
                    Err(e) => {
                        // SAFETY: `this` is still under construction — nothing
                        // in it has reached a queue, so `destroy`'s "not in use
                        // by an in-flight command buffer" contract is trivial.
                        unsafe { this.destroy(device, allocator) };
                        return Err(e.into());
                    }
                }
            };
        }
        try_or_cleanup!(this.create_buffers(device, allocator));
        try_or_cleanup!(this.create_layouts(device, texture_set_layout, scene_set_layout));
        try_or_cleanup!(this.create_descriptors(device));
        try_or_cleanup!(this.create_pipelines(device, pipeline_cache, render_pass));
        Ok(this)
    }

    fn create_buffers(&mut self, device: &ash::Device, allocator: &SharedAllocator) -> Result<()> {
        let bytes = GroundCoverBufferBytes::new();
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            self.chunk_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                bytes.chunks,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
            self.cell_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                bytes.cells,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
            self.species_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                bytes.species,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
            self.species_table_buffers
                .push(GpuBuffer::create_host_visible(
                    device,
                    allocator,
                    bytes.species_table,
                    vk::BufferUsageFlags::STORAGE_BUFFER,
                )?);
            self.counter_readback.push(GpuBuffer::create_host_readback(
                device,
                allocator,
                bytes.counters,
                vk::BufferUsageFlags::TRANSFER_DST,
            )?);
            self.field_state_buffers
                .push(GpuBuffer::create_host_visible(
                    device,
                    allocator,
                    bytes.field_state,
                    vk::BufferUsageFlags::STORAGE_BUFFER,
                )?);
            self.disturber_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                bytes.disturbers,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
        }
        // §12.4's field: two halves, one `uint` (packHalf2x16 XZ) per texel.
        // Not per-frame-in-flight: the field is *state*, and a per-slot copy
        // would give a 2-frame pipeline two independent trails that alternate.
        self.field_buffer = Some(GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            bytes.field,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
        )?);
        // One `GpuGroundCoverBlade` per slot × the cap. See the module docs on
        // why the cap is sized for a chunk-size sweep rather than today's
        // visible set.
        self.blade_buffer = Some(GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            bytes.blades,
            vk::BufferUsageFlags::STORAGE_BUFFER,
        )?);
        self.indirect_buffer = Some(GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            bytes.indirect,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::INDIRECT_BUFFER,
        )?);
        self.counter_buffer = Some(GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            bytes.counters,
            vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::TRANSFER_DST
                | vk::BufferUsageFlags::TRANSFER_SRC,
        )?);
        Ok(())
    }

    fn create_layouts(
        &mut self,
        device: &ash::Device,
        texture_set_layout: vk::DescriptorSetLayout,
        scene_set_layout: vk::DescriptorSetLayout,
    ) -> Result<()> {
        let compute = vk::ShaderStageFlags::COMPUTE;
        let [scatter, interaction, draw] = set_layout_contracts();
        for contract in [&scatter, &interaction, &draw] {
            contract
                .validate()
                .expect("ground-cover descriptor layout drifted against its shaders (#4110)");
        }
        let scatter_bindings = scatter.bindings;
        // SAFETY: `scatter_bindings` outlives the call; `device` is live and
        // the layout is owned here until `destroy`.
        self.scatter_set_layout = unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&scatter_bindings),
                    None,
                )
                .context("create ground-cover scatter set layout")?
        };
        let scatter_range = [vk::PushConstantRange::default()
            .stage_flags(compute)
            .offset(0)
            .size(std::mem::size_of::<ScatterPush>() as u32)];
        let scatter_sets = [self.scatter_set_layout];
        // SAFETY: both slices outlive the call.
        self.scatter_pipeline_layout = unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(&scatter_sets)
                        .push_constant_ranges(&scatter_range),
                    None,
                )
                .context("create ground-cover scatter pipeline layout")?
        };

        let interaction_bindings = interaction.bindings;
        // SAFETY: `interaction_bindings` outlives the call; the layout is
        // owned here until `destroy`.
        self.interaction_set_layout = unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&interaction_bindings),
                    None,
                )
                .context("create ground-cover interaction set layout")?
        };
        let interaction_sets = [self.interaction_set_layout];
        // SAFETY: the slice outlives the call. No push constants: everything
        // the field update needs is in its header buffer, which the blade
        // vertex shader also reads.
        self.interaction_pipeline_layout = unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default().set_layouts(&interaction_sets),
                    None,
                )
                .context("create ground-cover interaction pipeline layout")?
        };

        let vertex = vk::ShaderStageFlags::VERTEX;
        let draw_bindings = draw.bindings;
        // SAFETY: as above.
        self.draw_set_layout = unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&draw_bindings),
                    None,
                )
                .context("create ground-cover draw set layout")?
        };
        let draw_range = [vk::PushConstantRange::default()
            .stage_flags(vertex)
            .offset(0)
            .size(std::mem::size_of::<BladePush>() as u32)];
        let draw_sets = [texture_set_layout, scene_set_layout, self.draw_set_layout];
        // SAFETY: as above.
        self.draw_pipeline_layout = unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(&draw_sets)
                        .push_constant_ranges(&draw_range),
                    None,
                )
                .context("create ground-cover draw pipeline layout")?
        };
        Ok(())
    }
}

/// Requested size of every buffer `GroundCoverPipeline::create_buffers`
/// allocates, in bytes. The allocation code reads its sizes from here, so
/// [`groundcover_resident_bytes`] — the figure `memory-budget.md` ledgers —
/// cannot drift from what is actually allocated (#4300).
struct GroundCoverBufferBytes {
    // Per frame in flight, host-visible.
    chunks: u64,
    cells: u64,
    species: u64,
    species_table: u64,
    field_state: u64,
    disturbers: u64,
    /// Per-slot host readback; the device-local counter buffer is the same
    /// size.
    counters: u64,
    // Shared, device-local.
    field: u64,
    blades: u64,
    indirect: u64,
}

impl GroundCoverBufferBytes {
    const fn new() -> Self {
        Self {
            chunks: GROUNDCOVER_MAX_CHUNKS as u64
                * std::mem::size_of::<GpuGroundCoverChunk>() as u64,
            cells: (MAX_GROUNDCOVER_CELLS * std::mem::size_of::<GpuGroundCoverCell>()) as u64,
            species: (MAX_GROUNDCOVER_SPECIES * std::mem::size_of::<GpuGroundCoverSpecies>())
                as u64,
            species_table: GROUNDCOVER_SPECIES_TABLE_SIZE as u64 * 4,
            field_state: std::mem::size_of::<GpuGroundCoverFieldState>() as u64,
            disturbers: GROUNDCOVER_INTERACTION_MAX_DISTURBERS as u64
                * std::mem::size_of::<GpuGroundCoverDisturber>() as u64,
            counters: (COUNTER_SLOTS * 4) as u64,
            // 256² texels × one `uint` × two halves.
            field: INTERACTION_TEXEL_COUNT * 2 * 4,
            // The stride comes from the Rust mirror, never a literal (#4335).
            blades: GROUNDCOVER_MAX_CHUNKS as u64
                * GROUNDCOVER_MAX_BLADES_PER_CHUNK as u64
                * std::mem::size_of::<GpuGroundCoverBlade>() as u64,
            // `sizeof(VkDrawIndirectCommand)` per chunk per stream.
            indirect: GROUNDCOVER_MAX_CHUNKS as u64 * 16 * GROUNDCOVER_INDIRECT_STREAMS,
        }
    }
}

/// Every buffer EXAL ground cover allocates, in bytes: the per-slot
/// host-visible set × `MAX_FRAMES_IN_FLIGHT` plus the shared device-local
/// field, blade arena, indirect and counter buffers. Allocated on every RT
/// device at renderer init, whether or not the scene has ground cover.
/// Ledgered in `docs/engine/memory-budget.md` (#4300).
pub const fn groundcover_resident_bytes() -> u64 {
    let b = GroundCoverBufferBytes::new();
    let per_slot = b.chunks
        + b.cells
        + b.species
        + b.species_table
        + b.field_state
        + b.disturbers
        + b.counters;
    MAX_FRAMES_IN_FLIGHT as u64 * per_slot + b.field + b.blades + b.indirect + b.counters
}

/// One ground-cover descriptor-set layout and the shaders that must agree
/// with it.
struct SetLayoutContract {
    name: &'static str,
    set: u32,
    bindings: Vec<vk::DescriptorSetLayoutBinding<'static>>,
    shaders: Vec<ReflectedShader<'static>>,
}

impl SetLayoutContract {
    fn validate(&self) -> Result<()> {
        validate_set_layout(self.set, &self.bindings, &self.shaders, self.name, &[])
    }
}

/// The scatter, interaction and draw set layouts, in that order, each paired
/// with the shaders that declare it. Built once here so construction and
/// `cargo test` validate the same lists (#4110).
fn set_layout_contracts() -> [SetLayoutContract; 3] {
    let compute = vk::ShaderStageFlags::COMPUTE;
    let vertex = vk::ShaderStageFlags::VERTEX;
    // The scatter's own set 0: chunks, cells, the global vertex SSBO
    // (§11.1 path A), blades, the indirect draws it writes, and the counters
    // it appends through; then §7's species selection table, and the TLAS
    // for the placed-geometry cover test (ground cover must not grow through
    // roads, flagstones or rock bases).
    let mut scatter: Vec<_> = (0..6)
        .map(|binding| storage_binding(binding, compute))
        .collect();
    scatter.push(storage_binding(7, compute));
    scatter.push(
        vk::DescriptorSetLayoutBinding::default()
            .binding(6)
            .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
            .descriptor_count(1)
            .stage_flags(compute),
    );
    // §12.4's own set: the field, its header, and the frame's disturbers.
    let interaction = (0..3)
        .map(|binding| storage_binding(binding, compute))
        .collect();
    // Set 2 for the draw pipelines. Bindings 4 and 5 (indirect, counters) are
    // deliberately absent: the draw reads neither, and declaring them would
    // let a future edit sample the scatter's scratch from a fragment shader
    // without anything failing. 7 and 8 are §12.4's field and header,
    // read-only in the vertex shader.
    let draw = vec![
        storage_binding(0, vertex),
        storage_binding(1, vertex),
        storage_binding(2, vertex),
        storage_binding(3, vertex),
        storage_binding(6, vertex | vk::ShaderStageFlags::FRAGMENT),
        storage_binding(7, vertex),
        storage_binding(8, vertex),
    ];
    let shader = |name, spirv| ReflectedShader { name, spirv };
    [
        SetLayoutContract {
            name: "ground-cover scatter",
            set: 0,
            bindings: scatter,
            shaders: vec![shader("groundcover_scatter.comp", SCATTER_SPV)],
        },
        SetLayoutContract {
            name: "ground-cover interaction",
            set: 0,
            bindings: interaction,
            shaders: vec![shader("groundcover_interaction.comp", INTERACTION_SPV)],
        },
        SetLayoutContract {
            name: "ground-cover draw",
            set: 2,
            bindings: draw,
            shaders: vec![
                shader("groundcover_blade.vert", BLADE_VERT_SPV),
                shader("groundcover_blade.frag", BLADE_FRAG_SPV),
                shader("groundcover_debug.frag", DEBUG_FRAG_SPV),
            ],
        },
    ]
}

fn storage_binding(
    binding: u32,
    stages: vk::ShaderStageFlags,
) -> vk::DescriptorSetLayoutBinding<'static> {
    vk::DescriptorSetLayoutBinding::default()
        .binding(binding)
        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
        .descriptor_count(1)
        .stage_flags(stages)
}

impl GroundCoverPipeline {
    fn create_descriptors(&mut self, device: &ash::Device) -> Result<()> {
        let frames = MAX_FRAMES_IN_FLIGHT as u32;
        // 7 scatter + 7 draw + 3 interaction storage bindings per
        // frame-in-flight, plus the scatter's TLAS.
        let sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(17 * frames),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                .descriptor_count(frames),
        ];
        // SAFETY: `sizes` outlives the call; the pool is owned here.
        self.descriptor_pool = unsafe {
            device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .pool_sizes(&sizes)
                        .max_sets(3 * frames),
                    None,
                )
                .context("create ground-cover descriptor pool")?
        };
        let scatter_layouts = vec![self.scatter_set_layout; MAX_FRAMES_IN_FLIGHT];
        // SAFETY: the pool was sized for these sets; the slice outlives the call.
        self.scatter_sets = unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(self.descriptor_pool)
                        .set_layouts(&scatter_layouts),
                )
                .context("allocate ground-cover scatter sets")?
        };
        let draw_layouts = vec![self.draw_set_layout; MAX_FRAMES_IN_FLIGHT];
        // SAFETY: as above.
        self.draw_sets = unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(self.descriptor_pool)
                        .set_layouts(&draw_layouts),
                )
                .context("allocate ground-cover draw sets")?
        };
        let interaction_layouts = vec![self.interaction_set_layout; MAX_FRAMES_IN_FLIGHT];
        // SAFETY: as above.
        self.interaction_sets = unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(self.descriptor_pool)
                        .set_layouts(&interaction_layouts),
                )
                .context("allocate ground-cover interaction sets")?
        };
        Ok(())
    }

    /// Rebuild the render-pass-dependent graphics pipelines against
    /// `render_pass` (#4307).
    ///
    /// `recreate_swapchain_core` rebuilds the main render pass on a surface-
    /// format change and recreated the triangle and water pipelines against
    /// it, but not these — they kept pointing at the destroyed pass. It is not
    /// a spec violation while the new pass stays compatible with the old one,
    /// and today it does, since every main-pass attachment format is a
    /// compile-time constant plus a device-stable depth format. It stops being
    /// true the moment one of those attachments is derived from the swapchain
    /// surface format, and the failure then is
    /// VUID-vkCmdDraw-renderPass-02684 on a code path — an HDR toggle or a
    /// display change mid-session — that no test exercises.
    ///
    /// Only the two graphics pipelines are rebuilt. The water pipeline's
    /// equivalent path drops and recreates the whole object, which is right
    /// for it; here that would also throw away the blade arena, the chunk
    /// residency and the §12.4 displacement field — megabytes of state with no
    /// render-pass dependency — and make a window resize restart the sward.
    ///
    /// # Safety contract
    ///
    /// The caller must have idled the device: the two pipelines being
    /// destroyed must have no in-flight command buffer referencing them.
    pub fn recreate_draw_pipelines(
        &mut self,
        device: &ash::Device,
        pipeline_cache: vk::PipelineCache,
        render_pass: vk::RenderPass,
    ) -> Result<()> {
        // SAFETY: the caller idled the device (see the contract above), so
        // these two pipelines — created by `device` and not yet destroyed —
        // have no in-flight references. They are nulled immediately so a
        // later `destroy` or an early return cannot double-free them.
        unsafe {
            for pipeline in [&mut self.blade_pipeline, &mut self.debug_pipeline] {
                if *pipeline != vk::Pipeline::null() {
                    device.destroy_pipeline(*pipeline, None);
                    *pipeline = vk::Pipeline::null();
                }
            }
        }

        let entry = std::ffi::CString::new("main").expect("static literal");
        let vert = shader_module(device, BLADE_VERT_SPV, "groundcover_blade.vert")?;
        let blade_frag = shader_module(device, BLADE_FRAG_SPV, "groundcover_blade.frag")?;
        let debug_frag = shader_module(device, DEBUG_FRAG_SPV, "groundcover_debug.frag")?;
        let result = self.build_draw_pipelines(
            device,
            pipeline_cache,
            render_pass,
            &entry,
            vert,
            blade_frag,
            debug_frag,
        );
        // SAFETY: pipeline creation has completed, so the modules are no
        // longer referenced; they are owned here and destroyed exactly once.
        unsafe {
            for module in [vert, blade_frag, debug_frag] {
                device.destroy_shader_module(module, None);
            }
        }
        result
    }

    fn create_pipelines(
        &mut self,
        device: &ash::Device,
        pipeline_cache: vk::PipelineCache,
        render_pass: vk::RenderPass,
    ) -> Result<()> {
        let entry = std::ffi::CString::new("main").expect("static literal");
        let scatter = shader_module(device, SCATTER_SPV, "groundcover_scatter.comp")?;
        let interaction = shader_module(device, INTERACTION_SPV, "groundcover_interaction.comp")?;
        let vert = shader_module(device, BLADE_VERT_SPV, "groundcover_blade.vert")?;
        let blade_frag = shader_module(device, BLADE_FRAG_SPV, "groundcover_blade.frag")?;
        let debug_frag = shader_module(device, DEBUG_FRAG_SPV, "groundcover_debug.frag")?;
        let result = self
            .build_interaction_pipeline(device, pipeline_cache, &entry, interaction)
            .and_then(|()| {
                self.build_pipelines(
                    device,
                    pipeline_cache,
                    render_pass,
                    &entry,
                    scatter,
                    vert,
                    blade_frag,
                    debug_frag,
                )
            });
        for module in [scatter, interaction, vert, blade_frag, debug_frag] {
            // SAFETY: pipeline creation has returned, and the spec allows a
            // module to be destroyed as soon as it has.
            unsafe { device.destroy_shader_module(module, None) };
        }
        result
    }

    fn build_interaction_pipeline(
        &mut self,
        device: &ash::Device,
        pipeline_cache: vk::PipelineCache,
        entry: &std::ffi::CStr,
        module: vk::ShaderModule,
    ) -> Result<()> {
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(module)
            .name(entry);
        let info = vk::ComputePipelineCreateInfo::default()
            .stage(stage)
            .layout(self.interaction_pipeline_layout);
        // SAFETY: the create-info's borrows outlive the call; layout and
        // module are live.
        self.interaction_pipeline = unsafe {
            device
                .create_compute_pipelines(pipeline_cache, &[info], None)
                .map_err(|(_, e)| e)
                .context("create ground-cover interaction pipeline")?[0]
        };
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn build_pipelines(
        &mut self,
        device: &ash::Device,
        pipeline_cache: vk::PipelineCache,
        render_pass: vk::RenderPass,
        entry: &std::ffi::CStr,
        scatter: vk::ShaderModule,
        vert: vk::ShaderModule,
        blade_frag: vk::ShaderModule,
        debug_frag: vk::ShaderModule,
    ) -> Result<()> {
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(scatter)
            .name(entry);
        let info = vk::ComputePipelineCreateInfo::default()
            .stage(stage)
            .layout(self.scatter_pipeline_layout);
        // SAFETY: the create-info's borrows outlive the call; layout and
        // module are live.
        self.scatter_pipeline = unsafe {
            device
                .create_compute_pipelines(pipeline_cache, &[info], None)
                .map_err(|(_, e)| e)
                .context("create ground-cover scatter pipeline")?[0]
        };

        self.build_draw_pipelines(
            device,
            pipeline_cache,
            render_pass,
            entry,
            vert,
            blade_frag,
            debug_frag,
        )
    }

    /// Build the two render-pass-dependent graphics pipelines.
    ///
    /// Split out of `build_pipelines` for #4307: these are the only two
    /// objects here bound to a render pass, so a surface-format change has to
    /// rebuild exactly them — not the scatter/interaction compute pipelines,
    /// and not the blade arena or the §12.4 displacement field, which are
    /// several megabytes of residency with no dependency on the pass at all.
    #[allow(clippy::too_many_arguments)]
    fn build_draw_pipelines(
        &mut self,
        device: &ash::Device,
        pipeline_cache: vk::PipelineCache,
        render_pass: vk::RenderPass,
        entry: &std::ffi::CStr,
        vert: vk::ShaderModule,
        blade_frag: vk::ShaderModule,
        debug_frag: vk::ShaderModule,
    ) -> Result<()> {
        // `GC_DEBUG_POINTS` — specialization constant 0 in the shared vertex
        // shader. One shader, two pipelines: the debug view renders points at
        // exactly the positions the blades will occupy, so it cannot drift
        // from the thing it is validating.
        let map = [vk::SpecializationMapEntry::default()
            .constant_id(0)
            .offset(0)
            .size(std::mem::size_of::<u32>())];
        for (index, (frag, debug)) in [(blade_frag, 0u32), (debug_frag, 1u32)]
            .into_iter()
            .enumerate()
        {
            let data = debug.to_ne_bytes();
            let spec = vk::SpecializationInfo::default()
                .map_entries(&map)
                .data(&data);
            let stages = [
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .module(vert)
                    .name(entry)
                    .specialization_info(&spec),
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(frag)
                    .name(entry),
            ];
            // No vertex input: §4's blade geometry is generated from a seed,
            // so there is nothing to fetch.
            let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
            let topology = if debug == 1 {
                vk::PrimitiveTopology::POINT_LIST
            } else {
                vk::PrimitiveTopology::TRIANGLE_LIST
            };
            let input_assembly =
                vk::PipelineInputAssemblyStateCreateInfo::default().topology(topology);
            let viewport_state = vk::PipelineViewportStateCreateInfo::default()
                .viewport_count(1)
                .scissor_count(1);
            // Two-sided by construction: a blade is a ribbon with no inside,
            // and back-face culling would blank every blade the wind turns
            // away from the camera.
            let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
                .polygon_mode(vk::PolygonMode::FILL)
                .cull_mode(vk::CullModeFlags::NONE)
                .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                .line_width(1.0);
            let multisample = vk::PipelineMultisampleStateCreateInfo::default()
                .rasterization_samples(vk::SampleCountFlags::TYPE_1);
            // Opaque: blades write depth so they occlude each other correctly.
            let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
                .depth_test_enable(true)
                .depth_write_enable(true)
                .depth_compare_op(crate::vulkan::pipeline::default_depth_compare_op());
            let blend_attachments = draw_color_write_masks(debug == 1).map(|mask| {
                vk::PipelineColorBlendAttachmentState::default().color_write_mask(mask)
            });
            let blend =
                vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);
            let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
            let dynamic =
                vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
            let info = vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .rasterization_state(&rasterization)
                .multisample_state(&multisample)
                .depth_stencil_state(&depth_stencil)
                .color_blend_state(&blend)
                .dynamic_state(&dynamic)
                .layout(self.draw_pipeline_layout)
                .render_pass(render_pass)
                .subpass(0);
            // SAFETY: every borrowed state struct outlives the call; layout,
            // render pass and modules are live.
            let pipeline = unsafe {
                device
                    .create_graphics_pipelines(pipeline_cache, &[info], None)
                    .map_err(|(_, e)| e)
                    .context("create ground-cover draw pipeline")?[0]
            };
            if index == 0 {
                self.blade_pipeline = pipeline;
            } else {
                self.debug_pipeline = pipeline;
            }
        }
        Ok(())
    }
}

/// Per-attachment colour write masks for the blade (`debug_points == false`)
/// or debug-point pipeline, in main-pass attachment order.
///
/// Eight attachments to match the main pass. 0 (HDR colour), 5 (albedo) and
/// 6/7 (the FSR masks) are written by both pipelines; 2 (motion) only by the
/// blade pipeline, whose diagnostic sibling has no motion output. Normal /
/// mesh-ID / raw-indirect remain masked: raw indirect is left holding the
/// ground's GI on purpose, and albedo is written so composite's
/// `indirect * albedo` lights the blade with it rather than re-adding the
/// terrain's own reflectance on top of the blade (the debug view writes black
/// there, keeping its points purely emissive).
///
/// A masked-on attachment the fragment shader does not write receives
/// undefined values (#4295, the #3977 class), so the set of non-empty masks
/// must equal each shader's declared output locations — pinned by
/// `draw_write_masks_match_each_fragment_shaders_outputs`.
fn draw_color_write_masks(debug_points: bool) -> [vk::ColorComponentFlags; 8] {
    use vk::ColorComponentFlags as C;
    let mut masks = [C::empty(); 8];
    masks[0] = C::R | C::G | C::B | C::A;
    if !debug_points {
        masks[2] = C::R | C::G;
    }
    // `B10G11R11_UFLOAT` carries no alpha channel.
    masks[5] = C::R | C::G | C::B;
    masks[6] = C::R;
    masks[7] = C::R;
    masks
}

fn shader_module(device: &ash::Device, spirv: &[u8], name: &str) -> Result<vk::ShaderModule> {
    let mut cursor = std::io::Cursor::new(spirv);
    let code = ash::util::read_spv(&mut cursor).with_context(|| format!("read {name} SPIR-V"))?;
    // SAFETY: `code` is a validated SPIR-V word stream and outlives the call.
    unsafe {
        device
            .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)
            .with_context(|| format!("create {name} shader module"))
    }
}

impl GroundCoverPipeline {
    /// Upload the frame's chunk / cell / species records and rebind the
    /// descriptors. Returns `false` when there is nothing to scatter, which is
    /// every interior frame and every exterior frame before the first terrain
    /// mesh has uploaded.
    ///
    /// Must be called after slot `frame`'s fence has been waited — it writes
    /// that slot's host-visible buffers and rewrites its descriptor sets.
    pub fn prepare(
        &mut self,
        device: &ash::Device,
        frame: usize,
        vertex_buffer: vk::Buffer,
        input: &GroundCoverFrame<'_>,
    ) -> bool {
        // Harvest the readback this slot took one pipelined cycle ago, before
        // the buffers it describes are overwritten.
        self.harvest(device, frame);

        // #4338 — the host has already cut its farthest chunks to the cap;
        // anything still over it is clamped just below, and both count.
        self.frame_chunks_truncated = input.chunks_truncated.saturating_add(
            input
                .chunks
                .len()
                .saturating_sub(GROUNDCOVER_MAX_CHUNKS as usize) as u32,
        );
        if input.chunks.is_empty() || input.cells.is_empty() || vertex_buffer == vk::Buffer::null()
        {
            self.frame_chunk_count = 0;
            self.frame_active_chunk_count = 0;
            return false;
        }
        let chunks = &input.chunks[..input.chunks.len().min(GROUNDCOVER_MAX_CHUNKS as usize)];
        let cells = &input.cells[..input.cells.len().min(MAX_GROUNDCOVER_CELLS)];
        let species = &input.species[..input.species.len().min(MAX_GROUNDCOVER_SPECIES)];
        if species.is_empty() {
            // `GroundCoverPalette::resolve` guarantees a non-empty palette, so
            // reaching here means the host published one it had not resolved.
            // Drawing anyway would index `gcSpecies[0]` out of bounds.
            self.frame_chunk_count = 0;
            self.frame_active_chunk_count = 0;
            return false;
        }

        // §12.4 — advance the field's header before the descriptor write, so
        // this slot's state buffer and its descriptor go up together.
        let disturbers = &input.disturbers[..input
            .disturbers
            .len()
            .min(GROUNDCOVER_INTERACTION_MAX_DISTURBERS as usize)];
        let field_state =
            self.advance_field_state(input.camera_pos, input.delta_seconds, disturbers.len());

        // §7's selection table, padded to its fixed size with species 0 so
        // every entry the scatter's 8 hash bits can reach is initialised.
        let mut species_table = [0u32; GROUNDCOVER_SPECIES_TABLE_SIZE as usize];
        let table_len = input.species_table.len().min(species_table.len());
        species_table[..table_len].copy_from_slice(&input.species_table[..table_len]);

        let uploads = self.chunk_buffers[frame]
            .write_mapped(device, chunks)
            .and_then(|()| self.cell_buffers[frame].write_mapped(device, cells))
            .and_then(|()| self.species_buffers[frame].write_mapped(device, species))
            .and_then(|()| self.species_table_buffers[frame].write_mapped(device, &species_table))
            .and_then(|()| self.field_state_buffers[frame].write_mapped(device, &[field_state]))
            .and_then(|()| {
                if disturbers.is_empty() {
                    Ok(())
                } else {
                    self.disturber_buffers[frame].write_mapped(device, disturbers)
                }
            });
        if let Err(error) = uploads {
            log::warn!("ground cover: record upload failed: {error}");
            self.frame_chunk_count = 0;
            self.frame_active_chunk_count = 0;
            return false;
        }
        self.frame_field_state = field_state;
        self.frame_disturber_count = disturbers.len() as u32;
        self.write_descriptor_sets(device, frame, vertex_buffer);
        self.bound_vertex_buffer = vertex_buffer;

        self.frame_chunk_count = chunks.len() as u32;
        self.frame_active_chunk_count =
            chunks.iter().filter(|chunk| chunk.active != 0).count() as u32;
        self.frame_debug_points = input.debug_points;
        // `size_range[1]` is each species' height ceiling. Non-finite entries
        // cannot reach here (`GroundCoverSpecies::is_well_formed` rejects
        // them at palette resolve), and the palette is non-empty.
        self.frame_cover_reach = species
            .iter()
            .map(|s| s.size_range[1])
            .fold(0.0_f32, f32::max);
        self.frame_push = BladePush {
            camera_pixels: [
                input.camera_pos[0],
                input.camera_pos[1],
                input.camera_pos[2],
                input.pixels_per_unit_at_unit_depth,
            ],
            origin_time: [
                input.render_origin[0],
                input.render_origin[1],
                input.render_origin[2],
                input.time_seconds,
            ],
            wind: input.wind,
            gust_and_timing: [
                input.gust_frequency,
                input.time_seconds - input.delta_seconds.max(0.0),
                species.len() as f32,
                // Packed `(frame serial << 2) | lod tier`. It keeps the push
                // block compact and makes the blue-noise transition
                // repeat under a replayed simulation clock. The shift is two
                // bits, not one, because `GROUNDCOVER_INDIRECT_STREAMS` is 3:
                // a one-bit tier field cannot encode the clump-card tier, and
                // its value carried into the serial instead.
                //
                // The serial is masked to `GROUNDCOVER_FRAME_SERIAL_BITS`
                // before the ×stride so the word stays f32-exact for the life
                // of the process: past 2^24 the f32 spacing widens to 2 and
                // `record_draw`'s odd-tier `+ 1.0` rounds away (#4498). The
                // serial only seeds a wrapping tile rotation, so the 22-bit
                // period costs nothing.
                (((input.time_seconds.max(0.0) * 60.0).floor() as u64)
                    & GROUNDCOVER_FRAME_SERIAL_MASK) as f32
                    * GROUNDCOVER_LOD_TIER_STRIDE as f32,
            ],
        };
        true
    }

    /// Advance §12.4's field header for this frame and flip the ping-pong.
    ///
    /// **The origin is snapped to the texel grid**, which is what makes the
    /// compute pass's reprojection a copy rather than a filtered fetch. A
    /// fractional offset would need bilinear resampling, and a filter applied
    /// every frame to its own output is a low-pass running at frame rate: a
    /// crisp channel smears into nothing within a second or two.
    ///
    /// The previous half is only declared usable once one update has actually
    /// run, so the first frame starts from an empty field rather than from
    /// whatever `create_device_local_uninit` left in the allocation.
    fn advance_field_state(
        &mut self,
        camera_pos: [f32; 3],
        delta_seconds: f32,
        disturbers: usize,
    ) -> GpuGroundCoverFieldState {
        advance_field_state(
            &mut self.field_previous,
            &mut self.field_write_half,
            camera_pos,
            delta_seconds,
            disturbers,
        )
    }

    fn harvest(&mut self, device: &ash::Device, frame: usize) {
        let truncated = std::mem::take(&mut self.pending_truncated[frame]);
        let dispatched = std::mem::take(&mut self.pending_chunks[frame]);
        let slots = std::mem::take(&mut self.pending_chunk_slots[frame]);
        if slots == 0 {
            return;
        }
        let buffer = &mut self.counter_readback[frame];
        if buffer.invalidate_if_needed(device).is_err() {
            return;
        }
        let Ok(bytes) = buffer.mapped_slice_mut() else {
            return;
        };
        let counters: Vec<u32> = bytes
            .chunks_exact(4)
            .take(COUNTER_SLOTS)
            .map(|w| u32::from_ne_bytes([w[0], w[1], w[2], w[3]]))
            .collect();
        if counters.len() < COUNTER_SLOTS {
            return;
        }
        let mut stats = GroundCoverStats {
            chunks_dispatched: dispatched,
            chunks_truncated: truncated,
            ..Default::default()
        };
        // Clamped, not summed raw: the append cursor deliberately runs past
        // the cap (§4's saturating overflow), so the raw value is "candidates
        // that tried", not "blades that exist".
        stats.blades_accepted = counters[..slots as usize]
            .iter()
            .map(|c| c.min(&GROUNDCOVER_MAX_BLADES_PER_CHUNK))
            .sum();
        stats.blades_overflowed = counters[COUNTER_OVERFLOW];
        stats.blades_covered = counters[COUNTER_COVERED];
        stats
            .histogram
            .copy_from_slice(&counters[COUNTER_HIST_BASE..COUNTER_OVERFLOW]);
        // A frame where the field was never evaluated leaves the `min` slots
        // at their seed; reporting `u32::MAX / 1e6` there would read as a
        // density of 4295, so an untouched pair collapses to zero.
        let untouched = counters[COUNTER_EXTREMA_BASE] == EXTREMA_MIN_SEED;
        if !untouched {
            stats.d_ground_min = counters[COUNTER_EXTREMA_BASE] as f32 / 1.0e6;
            stats.d_ground_max = counters[COUNTER_EXTREMA_BASE + 1] as f32 / 1.0e6;
            stats.view_dist_min = counters[COUNTER_EXTREMA_BASE + 2] as f32;
            stats.view_dist_max = counters[COUNTER_EXTREMA_BASE + 3] as f32;
            for (i, slot) in stats.factor_max.iter_mut().enumerate() {
                *slot = counters[COUNTER_FACTOR_BASE + i] as f32 / 1.0e6;
            }
        }
        self.stats = stats;
    }

    fn write_descriptor_sets(&self, device: &ash::Device, frame: usize, vertex_buffer: vk::Buffer) {
        let blade = self.blade_buffer.as_ref().expect("created in new()");
        let indirect = self.indirect_buffer.as_ref().expect("created in new()");
        let counters = self.counter_buffer.as_ref().expect("created in new()");
        let info = |buffer: vk::Buffer| {
            [vk::DescriptorBufferInfo::default()
                .buffer(buffer)
                .range(vk::WHOLE_SIZE)]
        };
        let chunk_info = info(self.chunk_buffers[frame].buffer);
        let cell_info = info(self.cell_buffers[frame].buffer);
        let vertex_info = info(vertex_buffer);
        let blade_info = info(blade.buffer);
        let indirect_info = info(indirect.buffer);
        let counter_info = info(counters.buffer);
        let species_info = info(self.species_buffers[frame].buffer);
        let species_table_info = info(self.species_table_buffers[frame].buffer);
        let field = self.field_buffer.as_ref().expect("created in new()");
        let field_info = info(field.buffer);
        let field_state_info = info(self.field_state_buffers[frame].buffer);
        let disturber_info = info(self.disturber_buffers[frame].buffer);

        fn write<'a>(
            set: vk::DescriptorSet,
            binding: u32,
            info: &'a [vk::DescriptorBufferInfo],
        ) -> vk::WriteDescriptorSet<'a> {
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(binding)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(info)
        }
        let scatter = self.scatter_sets[frame];
        let draw = self.draw_sets[frame];
        let writes = [
            write(scatter, 0, &chunk_info),
            write(scatter, 1, &cell_info),
            write(scatter, 2, &vertex_info),
            write(scatter, 3, &blade_info),
            write(scatter, 4, &indirect_info),
            write(scatter, 5, &counter_info),
            write(scatter, 7, &species_table_info),
            write(draw, 0, &chunk_info),
            write(draw, 1, &cell_info),
            write(draw, 2, &vertex_info),
            write(draw, 3, &blade_info),
            write(draw, 6, &species_info),
            write(draw, 7, &field_info),
            write(draw, 8, &field_state_info),
            write(self.interaction_sets[frame], 0, &field_info),
            write(self.interaction_sets[frame], 1, &field_state_info),
            write(self.interaction_sets[frame], 2, &disturber_info),
        ];
        // SAFETY: every `*_info` slice outlives the call, and only slot
        // `frame`'s sets are touched — the caller has waited that slot's
        // fence, so nothing in flight reads them.
        unsafe { device.update_descriptor_sets(&writes, &[]) };
    }

    /// Record the scatter dispatch. Must be OUTSIDE a render pass and before
    /// the main geometry pass that draws the result.
    /// `timers` brackets the whole compute half — interaction dispatch,
    /// counter clears, scatter dispatch and the trailing publish barrier
    /// (#4315). Passed in rather than bracketed at the call site because both
    /// early returns below are real skips: an interior frame has no chunks,
    /// and a frame without a TLAS handle abandons the dispatch. Bracketing
    /// outside would mark those frames active with a ~0 ms reading, which is
    /// exactly the "ran and took no time" / "did not run" confusion the
    /// `_active` flags exist to prevent.
    pub fn record_scatter(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        tlas: Option<vk::AccelerationStructureKHR>,
        mut timers: Option<&mut super::gpu_timers::GpuPerFrameTimers>,
    ) {
        if self.frame_chunk_count == 0 {
            return;
        }
        // The scatter statically uses binding 6, so it cannot run against an
        // unwritten one. Ground cover is created only on ray-query devices,
        // where init builds an (empty) TLAS for every slot, so `None` means
        // the acceleration manager itself is gone — skip the frame rather
        // than draw last frame's indirect list over this frame's scene.
        let Some(tlas) = tlas else {
            self.frame_chunk_count = 0;
            self.frame_active_chunk_count = 0;
            return;
        };
        let accel_structs = [tlas];
        let mut accel_write = vk::WriteDescriptorSetAccelerationStructureKHR::default()
            .acceleration_structures(&accel_structs);
        let write = crate::vulkan::descriptors::write_acceleration_structure(
            self.scatter_sets[frame],
            6,
            &mut accel_write,
        );
        // SAFETY: `accel_write` borrows `accel_structs`, both live for the
        // call; only slot `frame`'s set is touched and its fence has been
        // waited, and the set is not yet bound in `cmd`.
        unsafe { device.update_descriptor_sets(&[write], &[]) };
        if let Some(timers) = timers.as_deref_mut() {
            timers.cmd_groundcover_scatter_start(device, cmd, frame);
        }
        self.record_interaction(device, cmd, frame);
        let counters = self.counter_buffer.as_ref().expect("created in new()");
        // SAFETY: `cmd` is recording; every handle below is live and owned by
        // this pipeline. The barriers order the clear against the dispatch and
        // the dispatch against both its readers (the indirect draw and the
        // vertex shader reading the blade buffer).
        unsafe {
            // Counters carry per-chunk append cursors and the histogram, both
            // of which are per-frame quantities. Clearing here rather than at
            // the end of the previous frame keeps the whole lifetime inside
            // one command buffer.
            device.cmd_fill_buffer(cmd, counters.buffer, 0, counters.size, 0);
            // #4293 — the seed fills below write ranges this zero fill already
            // wrote, and nothing orders the two: under the sync model this
            // crate follows (#4177/#4179/#4181) their relative order is
            // undefined, so sync validation reports each seed as a
            // WRITE_AFTER_WRITE hazard. If the zero fill landed last, both
            // `atomicMin` extrema would start at 0 and
            // `GroundCoverStats::d_ground_min` / `view_dist_min` would read 0 —
            // telemetry EXAL tuning reads directly. A TRANSFER → TRANSFER
            // barrier is the device edge that sequences them. Confirmed as the
            // source of the ten `vkCmdFillBuffer` WAW hazards on a
            // `BYRO_VALIDATION=1` Skyrim tundra capture, and cleared by this.
            buffer_barrier(
                device,
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::TRANSFER_WRITE,
            );
            // Zero is the identity for every counter here except the two
            // `atomicMin` extrema, which need the opposite. Two four-byte
            // fills rather than a staging upload: this is 8 bytes.
            device.cmd_fill_buffer(
                cmd,
                counters.buffer,
                (COUNTER_EXTREMA_BASE * 4) as vk::DeviceSize,
                4,
                EXTREMA_MIN_SEED,
            );
            device.cmd_fill_buffer(
                cmd,
                counters.buffer,
                ((COUNTER_EXTREMA_BASE + 2) * 4) as vk::DeviceSize,
                4,
                EXTREMA_MIN_SEED,
            );
            buffer_barrier(
                device,
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
            );
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.scatter_pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.scatter_pipeline_layout,
                0,
                &[self.scatter_sets[frame]],
                &[],
            );
            let push = ScatterPush {
                camera_pos: [
                    self.frame_push.camera_pixels[0],
                    self.frame_push.camera_pixels[1],
                    self.frame_push.camera_pixels[2],
                    self.frame_cover_reach,
                ],
                render_origin: [
                    self.frame_push.origin_time[0],
                    self.frame_push.origin_time[1],
                    self.frame_push.origin_time[2],
                    1.0,
                ],
                chunk_count: self.frame_chunk_count,
                blades_per_chunk: GROUNDCOVER_MAX_BLADES_PER_CHUNK,
                // The debug view is one point per blade; the blade tier is
                // `segments * 6`. The scatter writes this into the indirect
                // `vertexCount`, so it has to agree with whatever the vertex
                // shader will divide `gl_VertexIndex` by.
                verts_per_blade: if self.frame_debug_points {
                    1
                } else {
                    // A point grows GROUNDCOVER_BLADES_PER_POINT blades,
                    // and the vertex shader divides `gl_VertexIndex` by
                    // exactly this product to find the point.
                    VERTS_PER_BLADE_NEAR * GROUNDCOVER_BLADES_PER_POINT
                },
                species_count: self.frame_push.gust_and_timing[2] as u32,
            };
            device.cmd_push_constants(
                cmd,
                self.scatter_pipeline_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                scatter_push_bytes(&push),
            );
            // §4: one workgroup per chunk, mirroring `cluster_cull.comp`.
            device.cmd_dispatch(cmd, self.frame_chunk_count, 1, 1);
            // #4181 / CONC-D2-01 — `TRANSFER` / `TRANSFER_READ` in the dst
            // scope is for the `cmd_copy_buffer` immediately below, not for
            // the draw.
            //
            // The scatter writes `counters.buffer` via atomics; this is the
            // single barrier publishing those writes, and it used to name
            // only the draw's consumers (`DRAW_INDIRECT | VERTEX_SHADER` /
            // `INDIRECT_COMMAND_READ | SHADER_READ`). The readback copy
            // reads the same buffer with no dependency on the compute write
            // at all — the two sibling edges in this file
            // (`TRANSFER→COMPUTE` above the dispatch, `COMPUTE→COMPUTE |
            // VERTEX` in `record_interaction`) are both correct by contrast.
            //
            // Rendering was never affected: the draw path reads through the
            // correctly-published half of this same barrier. What was
            // undefined is the readback `harvest` decodes into
            // `GroundCoverStats` — blade counts, density histogram, extrema
            // — which EXAL ground-cover tuning reads directly (#4054), so
            // the failure mode is silently wrong telemetry driving tuning
            // decisions rather than a visible artefact.
            //
            // Widening the existing edge rather than adding a second one:
            // same class of purely-additive change as #2403's skinned-vertex
            // publish mask.
            buffer_barrier(
                device,
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::DRAW_INDIRECT
                    | vk::PipelineStageFlags::VERTEX_SHADER
                    | vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::INDIRECT_COMMAND_READ
                    | vk::AccessFlags::SHADER_READ
                    | vk::AccessFlags::TRANSFER_READ,
            );
            // Snapshot the counters for the §11.3 histogram. Copied rather
            // than read in place so the scatter never touches host-visible
            // memory: it issues one atomic per candidate, and 3 M atomics a
            // frame across PCIe is not a diagnostic, it is a stall.
            device.cmd_copy_buffer(
                cmd,
                counters.buffer,
                self.counter_readback[frame].buffer,
                &[vk::BufferCopy::default().size(counters.size)],
            );
        }
        if let Some(timers) = timers {
            timers.cmd_groundcover_scatter_end(device, cmd, frame);
        }
        self.pending_chunks[frame] = self.frame_active_chunk_count;
        self.pending_chunk_slots[frame] = self.frame_chunk_count;
        self.pending_truncated[frame] = self.frame_chunks_truncated;
    }

    /// Record §12.4's field update. Runs from `record_scatter`, before the
    /// scatter's own dispatch, so ordering against the blade draw is the
    /// scatter's existing trailing barrier and there is no second edge to get
    /// wrong.
    ///
    /// It shares the scatter's "only when there is ground cover" gate on
    /// purpose. Nothing reads the field on a frame with no chunks, and the
    /// staleness that skipping introduces resolves itself: the origin the next
    /// live frame snaps to either matches (the camera did not move, and the
    /// trail is genuinely still there) or does not (every reprojection falls
    /// outside the previous field and reads zero).
    fn record_interaction(&self, device: &ash::Device, cmd: vk::CommandBuffer, frame: usize) {
        if self.interaction_pipeline == vk::Pipeline::null() {
            return;
        }
        // SAFETY: `cmd` is recording; the pipeline, layout and set are live and
        // owned here, and the caller has waited slot `frame`'s fence so the
        // host-visible header and disturber buffers this set points at are not
        // in flight. The trailing barrier orders the field's writes against
        // the blade vertex shader that reads them.
        unsafe {
            // The field is one persistent allocation shared by every
            // frame-in-flight, and this dispatch READS the half the *previous*
            // frame's dispatch wrote. A barrier's first synchronization scope
            // covers everything submitted earlier on the queue, so this one
            // edge sequences that read against that write.
            //
            // #4293 — this comment used to justify the edge by saying "the
            // per-slot fence only serialises frames MAX_FRAMES_IN_FLIGHT
            // apart, so consecutive frames can overlap on the queue". The
            // host does not do that: `sync_and_acquire_frame` waits on every
            // frame-in-flight fence, so the previous frame has finished before
            // this one records. The barrier is still required, for the reason
            // #4177/#4179 settled: a host-side fence wait is not a device
            // edge, and the queue's synchronization model — which is what sync
            // validation checks — only sees barriers.
            buffer_barrier(
                device,
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
            );
            device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.interaction_pipeline,
            );
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.interaction_pipeline_layout,
                0,
                &[self.interaction_sets[frame]],
                &[],
            );
            let groups = GROUNDCOVER_INTERACTION_TEXELS.div_ceil(GROUNDCOVER_INTERACTION_WORKGROUP);
            device.cmd_dispatch(cmd, groups, groups, 1);
            // The field's two halves alternate, so this frame's writes are the
            // next frame's reads as well as this frame's vertex-shader reads.
            buffer_barrier(
                device,
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::VERTEX_SHADER,
                vk::AccessFlags::SHADER_READ,
            );
        }
    }

    /// Record the blade (or debug-point) draw. Must be INSIDE the main
    /// geometry render pass, after opaque geometry.
    pub fn record_draw(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        texture_set: vk::DescriptorSet,
        scene_set: vk::DescriptorSet,
    ) {
        if self.frame_chunk_count == 0 {
            return;
        }
        let indirect = self.indirect_buffer.as_ref().expect("created in new()");
        let pipeline = if self.frame_debug_points {
            self.debug_pipeline
        } else {
            self.blade_pipeline
        };
        // #4307 — a failed `recreate_draw_pipelines` leaves these null rather
        // than dangling, and the ground cover stops drawing until the next
        // successful rebuild. The object itself stays alive so its arena,
        // field images and descriptor pool are still torn down by `destroy`;
        // dropping it here to signal the failure would leak every one of them.
        if pipeline == vk::Pipeline::null() {
            return;
        }
        // SAFETY: `cmd` is recording inside the render pass this pipeline was
        // created against; the descriptor sets and the indirect buffer are
        // live, and the scatter's trailing barrier has made its writes visible
        // to `DRAW_INDIRECT` and `VERTEX_SHADER`.
        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.draw_pipeline_layout,
                0,
                &[texture_set, scene_set, self.draw_sets[frame]],
                &[],
            );
            let mut push = self.frame_push;
            // Stream 0 is the three-segment tier and stream 1 the one-segment
            // tier. They share every residency slab, while their separate
            // first-vertex strides prevent an LOD change from re-addressing a
            // neighbouring chunk's blade records.
            let streams = if self.frame_debug_points {
                1
            } else {
                GROUNDCOVER_INDIRECT_STREAMS
            };
            for tier in 0..streams {
                push.gust_and_timing[3] = self.frame_push.gust_and_timing[3] + tier as f32;
                device.cmd_push_constants(
                    cmd,
                    self.draw_pipeline_layout,
                    vk::ShaderStageFlags::VERTEX,
                    0,
                    blade_push_bytes(&push),
                );
                device.cmd_draw_indirect(
                    cmd,
                    indirect.buffer,
                    tier * GROUNDCOVER_MAX_CHUNKS as u64 * 16,
                    self.frame_chunk_count,
                    16,
                );
            }
        }
    }

    /// The frame's chunk and cell records, the terrain vertex buffer and the
    /// camera, for the authored-model tier's placement (#4413) — the same
    /// chunks the blades scattered over. `None` when the scatter did not run
    /// this frame (an interior, or no TLAS to trace).
    pub fn model_scatter_inputs(
        &self,
        frame: usize,
        tlas: Option<vk::AccelerationStructureKHR>,
    ) -> Option<super::groundcover_models::ModelScatterInputs> {
        (self.frame_chunk_count > 0).then(|| super::groundcover_models::ModelScatterInputs {
            chunk_buffer: self.chunk_buffers[frame].buffer,
            cell_buffer: self.cell_buffers[frame].buffer,
            vertex_buffer: self.bound_vertex_buffer,
            chunk_count: self.frame_chunk_count,
            camera_pos: [
                self.frame_push.camera_pixels[0],
                self.frame_push.camera_pixels[1],
                self.frame_push.camera_pixels[2],
            ],
            render_origin: [
                self.frame_push.origin_time[0],
                self.frame_push.origin_time[1],
                self.frame_push.origin_time[2],
            ],
            tlas,
        })
    }

    pub fn stats(&self) -> GroundCoverStats {
        self.stats
    }

    /// # Safety
    ///
    /// No in-flight command buffer or descriptor set may still reference any
    /// object owned here — the caller must have idled the device.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for pipeline in [
            &mut self.scatter_pipeline,
            &mut self.interaction_pipeline,
            &mut self.blade_pipeline,
            &mut self.debug_pipeline,
        ] {
            if *pipeline != vk::Pipeline::null() {
                // SAFETY: caller's contract — nothing in flight uses it.
                unsafe { device.destroy_pipeline(*pipeline, None) };
                *pipeline = vk::Pipeline::null();
            }
        }
        for layout in [
            &mut self.scatter_pipeline_layout,
            &mut self.interaction_pipeline_layout,
            &mut self.draw_pipeline_layout,
        ] {
            if *layout != vk::PipelineLayout::null() {
                // SAFETY: every pipeline built against it is destroyed above.
                unsafe { device.destroy_pipeline_layout(*layout, None) };
                *layout = vk::PipelineLayout::null();
            }
        }
        if self.descriptor_pool != vk::DescriptorPool::null() {
            // SAFETY: destroying the pool frees its sets; caller guarantees no
            // in-flight command buffer still binds them.
            unsafe { device.destroy_descriptor_pool(self.descriptor_pool, None) };
            self.descriptor_pool = vk::DescriptorPool::null();
            self.scatter_sets.clear();
            self.draw_sets.clear();
            self.interaction_sets.clear();
        }
        for layout in [
            &mut self.scatter_set_layout,
            &mut self.interaction_set_layout,
            &mut self.draw_set_layout,
        ] {
            if *layout != vk::DescriptorSetLayout::null() {
                // SAFETY: the pipeline layouts and sets referencing it are gone.
                unsafe { device.destroy_descriptor_set_layout(*layout, None) };
                *layout = vk::DescriptorSetLayout::null();
            }
        }
        for buffer in self
            .chunk_buffers
            .iter_mut()
            .chain(self.cell_buffers.iter_mut())
            .chain(self.species_buffers.iter_mut())
            .chain(self.species_table_buffers.iter_mut())
            .chain(self.counter_readback.iter_mut())
            .chain(self.field_state_buffers.iter_mut())
            .chain(self.disturber_buffers.iter_mut())
            .chain(self.blade_buffer.iter_mut())
            .chain(self.indirect_buffer.iter_mut())
            .chain(self.counter_buffer.iter_mut())
            .chain(self.field_buffer.iter_mut())
        {
            buffer.destroy(device, allocator);
        }
        self.chunk_buffers.clear();
        self.cell_buffers.clear();
        self.species_buffers.clear();
        self.species_table_buffers.clear();
        self.counter_readback.clear();
        self.field_state_buffers.clear();
        self.disturber_buffers.clear();
        self.blade_buffer = None;
        self.indirect_buffer = None;
        self.counter_buffer = None;
        self.field_buffer = None;
    }
}

/// Free-function core of [`GroundCoverPipeline::advance_field_state`], so the
/// snapping and clamping rules can be tested without a Vulkan device.
fn advance_field_state(
    field_previous: &mut Option<[f32; 2]>,
    field_write_half: &mut u32,
    camera_pos: [f32; 3],
    delta_seconds: f32,
    disturbers: usize,
) -> GpuGroundCoverFieldState {
    let texel = GROUNDCOVER_INTERACTION_TEXEL_UNITS;
    let half_extent = GROUNDCOVER_INTERACTION_UNITS * 0.5;
    let snap = |v: f32| ((v - half_extent) / texel).floor() * texel;
    let origin = [snap(camera_pos[0]), snap(camera_pos[2])];
    // Clamped, not trusted. A long hitch must not wipe a trail outright (the
    // decay is exponential, so a quarter-second step already removes ~11% of
    // it), and a negative or non-finite dt would *amplify* one — `exp(-k·dt)`
    // with dt < 0 grows without bound, and the field is fed back into itself
    // every frame, so one bad value would not decay away.
    let dt = if delta_seconds.is_finite() {
        delta_seconds.clamp(0.0, 0.25)
    } else {
        0.0
    };
    let write_half = *field_write_half;
    let previous = *field_previous;
    *field_write_half = 1 - write_half;
    *field_previous = Some(origin);
    GpuGroundCoverFieldState {
        current: [origin[0], origin[1], dt, write_half as f32],
        previous: [
            previous.map_or(0.0, |p| p[0]),
            previous.map_or(0.0, |p| p[1]),
            disturbers as f32,
            if previous.is_some() { 1.0 } else { 0.0 },
        ],
    }
}

fn buffer_barrier(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    src_stage: vk::PipelineStageFlags,
    src_access: vk::AccessFlags,
    dst_stage: vk::PipelineStageFlags,
    dst_access: vk::AccessFlags,
) {
    let barrier = [vk::MemoryBarrier::default()
        .src_access_mask(src_access)
        .dst_access_mask(dst_access)];
    // SAFETY: `cmd` is recording and `barrier` outlives the call. A global
    // memory barrier rather than per-buffer ones: the scatter's four written
    // buffers are all consumed at the same two stages, so naming them
    // individually would be four times the API traffic for the same edge.
    unsafe {
        device.cmd_pipeline_barrier(
            cmd,
            src_stage,
            dst_stage,
            vk::DependencyFlags::empty(),
            &barrier,
            &[],
            &[],
        );
    }
}

fn scatter_push_bytes(push: &ScatterPush) -> &[u8] {
    // SAFETY: `#[repr(C)]` over 4-byte scalars with no padding, so every byte
    // of the struct is initialised; the slice borrows `push`.
    unsafe {
        std::slice::from_raw_parts(
            (push as *const ScatterPush).cast::<u8>(),
            std::mem::size_of::<ScatterPush>(),
        )
    }
}

fn blade_push_bytes(push: &BladePush) -> &[u8] {
    // SAFETY: as `scatter_push_bytes`.
    unsafe {
        std::slice::from_raw_parts(
            (push as *const BladePush).cast::<u8>(),
            std::mem::size_of::<BladePush>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shader_constants::GROUNDCOVER_HISTOGRAM_BUCKETS;
    use crate::vulkan::groundcover_stats::GROUNDCOVER_FACTOR_NAMES;

    /// Regression: #4181 / CONC-D2-01 — the scatter's publish barrier must
    /// cover the counter readback copy, not just the draw.
    ///
    /// `groundcover_scatter.comp` writes `counter_buffer` via atomics; the
    /// single barrier after the dispatch used to name only the draw's
    /// consumers (`DRAW_INDIRECT | VERTEX_SHADER` /
    /// `INDIRECT_COMMAND_READ | SHADER_READ`), and the `cmd_copy_buffer`
    /// immediately below it read the same buffer with no dependency on the
    /// compute write at all.
    ///
    /// Rendering was never affected — the draw reads through the
    /// correctly-published half of this same barrier. What was undefined is
    /// the readback `harvest` decodes into `GroundCoverStats`, which EXAL
    /// ground-cover tuning reads directly (#4054): silently wrong telemetry
    /// driving tuning decisions, which no rendering assertion would catch.
    #[test]
    fn scatter_publish_barrier_covers_the_counter_readback_copy() {
        let src = include_str!("groundcover.rs");
        // Scoped to the production portion so this test's own literals
        // cannot satisfy it.
        let module_start = src
            .find("mod tests {")
            .expect("this test module must still exist");
        let src = &src[..module_start];

        let dispatch_at = src
            .find("device.cmd_dispatch(cmd, self.frame_chunk_count, 1, 1);")
            .expect("the scatter dispatch must still exist under this spelling");
        let copy_at = src[dispatch_at..]
            .find("device.cmd_copy_buffer(")
            .expect("the counter readback copy must still follow the scatter dispatch")
            + dispatch_at;
        let between = &src[dispatch_at..copy_at];

        assert!(
            between.contains("vk::PipelineStageFlags::TRANSFER"),
            "the barrier between the scatter dispatch and the counter readback must \
             name TRANSFER in its dst stage mask (#4181)"
        );
        assert!(
            between.contains("vk::AccessFlags::TRANSFER_READ"),
            "the barrier between the scatter dispatch and the counter readback must \
             name TRANSFER_READ in its dst access mask (#4181)"
        );
        // The draw's half must survive too — this is a widening, not a swap.
        assert!(
            between.contains("vk::PipelineStageFlags::DRAW_INDIRECT")
                && between.contains("vk::AccessFlags::INDIRECT_COMMAND_READ"),
            "widening the dst scope must not drop the draw's own edge"
        );
    }

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

    /// #4338 — slot indices are blade-arena ownership, not a compact draw
    /// list.  A vacant ring slot must remain an explicit 32-byte record and
    /// produce a no-op indirect command; otherwise the next resident shifts
    /// into its slab even though the host-side ring says it did not move.
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

    #[test]
    fn inactive_residency_slots_remain_explicit_and_are_skipped_by_scatter() {
        let inactive = GpuGroundCoverChunk::default();
        assert_eq!(inactive.active, 0);
        assert_eq!(inactive.entry_progress, 0.0);
        assert_eq!(inactive.pad, [0; 2]);

        let scene = include_str!("../../shaders/include/groundcover_scene.glsl");
        let scatter = include_str!("../../shaders/groundcover_scatter.comp");
        assert!(
            scene.contains("uint slotActive;")
                && scene.contains("float entryProgress;")
                && scene.contains("uvec2 pad;"),
            "the GLSL record must retain the host's explicit inactive-slot and grow-in lanes"
        );
        assert!(
            scatter.contains("if (chunk.slotActive == 0u)")
                && scatter.contains("gcDraws[chunkIdx].instanceCount = 0u;"),
            "scatter must turn a vacant slot into a no-op indirect draw before reading its cell"
        );
    }

    /// `cmd_draw_indirect` is issued with a hard-coded stride of 16, which is
    /// `sizeof(VkDrawIndirectCommand)`. A mismatch would read every command
    /// but the first from the wrong offset.
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

    /// #4297 / LOD tiers 1–2 — the opaque endpoints retain real
    /// wind/displacement velocity and zero FSR masks; only either stochastic
    /// projected-size handoff raises a bounded reactive contribution. Shader
    /// text catches a future pipeline mask or output regression without a GPU.
    #[test]
    fn blade_motion_and_fsr_mask_contract_stay_material_driven() {
        let vert = include_str!("../../shaders/groundcover_blade.vert");
        let frag = include_str!("../../shaders/groundcover_blade.frag");
        assert!(
            vert.contains("#define GC_PREV_TIME        pc.gustAndCounts.y")
                && vert.contains("byroGcSampleField(base.xz, gcFieldPrevious)")
                && vert.contains("gcPrevViewProj"),
            "the vertex shader must evaluate the preceding shared wind clock, \
             previous displacement field, and previous camera projection"
        );
        assert!(
            frag.contains("layout(location = 2) out vec2 outMotion;")
                && frag.contains("outMotion = (currNDC - prevNDC) * 0.5;")
                && frag.contains("blueNoiseRankAt(")
                && frag.contains("vLodMidWeight * (1.0 - vCardWeight)")
                && frag
                    .contains("float midTransition = 4.0 * vLodMidWeight * (1.0 - vLodMidWeight);")
                && frag.contains("float cardTransition = 4.0 * vCardWeight * (1.0 - vCardWeight);")
                && frag.contains("outFsrReactive = 0.9 * max(midTransition, cardTransition);")
                && frag.contains("outFsrTransparency = 0.0;"),
            "blade ribbons must write real velocity, use complementary blue-noise \
             coverage for both LOD bands, and keep reactive output bounded"
        );
        let module = include_str!("groundcover.rs");
        let production = module
            .split_once("mod tests {")
            .expect("groundcover production module must precede tests")
            .0;
        assert!(
            production.contains("masks[2] = C::R | C::G;")
                && production.contains("if !debug_points {"),
            "the blade pipeline must enable RG motion writes while the debug \
             point pipeline keeps its unwritten attachment masked"
        );
    }

    /// #4110 — every ground-cover set layout is validated against the SPIR-V
    /// under `cargo test`, not only when a device builds the pipelines.
    #[test]
    fn set_layouts_match_their_shaders() {
        for contract in set_layout_contracts() {
            contract
                .validate()
                .unwrap_or_else(|e| panic!("{} drifted: {e:#}", contract.name));
        }
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

    /// #4295 — every attachment a ground-cover draw pipeline enables must be
    /// a declared output of that pipeline's fragment shader, and vice versa.
    /// The debug-point pipeline once shared the blade's albedo mask without
    /// writing albedo, leaving attachment 5 undefined in the debug view.
    #[test]
    fn draw_write_masks_match_each_fragment_shaders_outputs() {
        use crate::vulkan::reflect::reflect_output_locations;
        for (debug_points, spv, name) in [
            (false, BLADE_FRAG_SPV, "groundcover_blade.frag"),
            (true, DEBUG_FRAG_SPV, "groundcover_debug.frag"),
        ] {
            let written: Vec<u32> = draw_color_write_masks(debug_points)
                .iter()
                .enumerate()
                .filter(|(_, mask)| !mask.is_empty())
                .map(|(location, _)| location as u32)
                .collect();
            let declared = reflect_output_locations(spv).expect("reflect fragment outputs");
            assert_eq!(
                written, declared,
                "{name}: the pipeline enables writes on {written:?} but the shader \
                 declares outputs at {declared:?} (#4295 / #3977)"
            );
        }
    }

    /// #4297 / Step 3 — every stochastic ground-cover seed is derived in the
    /// integer domain. A vendor-dependent float-trig hash makes a dithered LOD
    /// transition shear on AMD; a frame/time-derived seed instead becomes
    /// full-screen temporal noise. Keep the three live seed paths pinned here
    /// until their GLSL can share one common include.
    #[test]
    fn groundcover_seed_hashes_are_integer_and_frame_invariant() {
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

        for (name, source) in [
            ("scatter candidate", scatter_hash),
            ("density noise", density_hash),
            ("blade attribute", blade_seed),
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
    }

    #[test]
    fn indirect_stride_matches_the_command() {
        assert_eq!(std::mem::size_of::<vk::DrawIndirectCommand>(), 16);
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

    /// Tier changes are draw representations, not residency changes. Pin both
    /// streams to the one arena and make the shader publish a no-op command
    /// for both of an inactive ring slot.
    #[test]
    fn tiered_indirect_streams_preserve_fixed_blade_slabs() {
        let scatter = include_str!("../../shaders/groundcover_scatter.comp");
        let blade = include_str!("../../shaders/groundcover_blade.vert");
        let module = include_str!("groundcover.rs");
        assert_eq!(GROUNDCOVER_INDIRECT_STREAMS, 3);
        assert!(scatter.contains("uint midIndex = chunkIdx + GROUNDCOVER_MAX_CHUNKS;"));
        assert!(scatter.contains("gcDraws[midIndex].firstVertex = sliceBase * midVertsPerPoint;"));
        assert!(scatter.contains("gcDraws[midIndex].instanceCount = 0u;"));
        assert!(scatter.contains("uint cardIndex = chunkIdx + 2u * GROUNDCOVER_MAX_CHUNKS;"));
        assert!(scatter.contains("gcDraws[cardIndex].firstVertex = (sliceBase / cardCluster)"));
        assert!(scatter.contains("uint midVertsPerPoint = GROUNDCOVER_BLADE_SEGMENTS_MID\n                * GROUNDCOVER_VERTS_PER_SEGMENT;"));
        assert!(blade.contains("GC_DEBUG_POINTS == 1u || GC_LOD_TIER == 1u || cardTier"));
        assert!(blade.contains("halfWidth * float(GROUNDCOVER_BLADES_PER_POINT)"));
        assert!(blade.contains("width * GROUNDCOVER_MAX_WIDTH_MULTIPLIER"));
        assert!(blade.contains("bool cardTier = GC_LOD_TIER == 2u;"));
        assert!(module.contains("tier * GROUNDCOVER_MAX_CHUNKS as u64 * 16"));
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

    /// A negative or non-finite `dt` turns the decay into growth without
    /// bound (`exp(-k·dt)`, dt < 0), and a hitch must not wipe a trail.
    #[test]
    fn the_field_clamps_the_frame_delta() {
        for (input, expected) in [
            (-1.0f32, 0.0f32),
            (f32::NAN, 0.0),
            (10.0, 0.25),
            (0.016, 0.016),
        ] {
            let (mut prev, mut half) = (None, 0u32);
            let state = advance_field_state(&mut prev, &mut half, [0.0; 3], input, 0);
            assert_eq!(state.current[2], expected, "dt {input} clamped wrong");
        }
    }

    /// The origin must land on the texel grid, or the compute pass's
    /// reprojection needs a filtered fetch and the field smears into itself.
    #[test]
    fn the_field_origin_snaps_to_the_texel_grid() {
        let (mut prev, mut half) = (None, 0u32);
        // First update has no previous half to reproject from.
        let first = advance_field_state(&mut prev, &mut half, [1234.5, 0.0, -987.25], 0.016, 3);
        assert_eq!(first.previous[3], 0.0);
        assert_eq!(first.current[3], 0.0, "first frame writes half 0");
        for axis in [first.current[0], first.current[1]] {
            assert_eq!(
                axis % GROUNDCOVER_INTERACTION_TEXEL_UNITS,
                0.0,
                "origin {axis} is off the texel grid"
            );
        }
        // Second update flips the half and can reproject.
        let second = advance_field_state(&mut prev, &mut half, [1234.5, 0.0, -987.25], 0.016, 3);
        assert_eq!(second.current[3], 1.0);
        assert_eq!(second.previous[3], 1.0);
        assert_eq!(second.previous[0], first.current[0]);
        assert_eq!(second.previous[1], first.current[1]);
        assert_eq!(second.previous[2], 3.0, "disturber count rides previous.z");
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
    /// #4293 — the counter clear's seed fills must be sequenced after the zero
    /// fill by a device edge.
    ///
    /// Three `vkCmdFillBuffer`s hit the one shared counter buffer: a
    /// whole-buffer zero, then two 4-byte `EXTREMA_MIN_SEED` writes into
    /// ranges the zero already covered. With nothing between them their order
    /// is undefined under the sync model this crate follows, and sync
    /// validation reports each seed as WRITE_AFTER_WRITE — the ten standing
    /// hazards a `BYRO_VALIDATION=1` Skyrim tundra capture showed, and which a
    /// TRANSFER → TRANSFER barrier took to zero. If the zero landed last, both
    /// `atomicMin` extrema would start at 0 and the `d_ground` / `view_dist`
    /// minima EXAL tuning reads would report 0.
    ///
    /// `cargo test` cannot run sync validation, so this pins the shape that
    /// validation accepted: a TRANSFER-to-TRANSFER barrier between the zero
    /// fill and the first seed fill.
    #[test]
    fn counter_seed_fills_are_sequenced_after_the_zero_fill() {
        let src = include_str!("groundcover.rs");
        let production = &src[..src.find("\nmod tests {").expect("test module")];
        let zero = production
            .find("device.cmd_fill_buffer(cmd, counters.buffer, 0, counters.size, 0);")
            .expect("the whole-buffer zero fill");
        let after_zero = &production[zero..];
        let first_seed = after_zero
            .find("EXTREMA_MIN_SEED")
            .expect("the extrema seed fills follow the zero fill");
        let between = &after_zero[..first_seed];
        assert!(
            between.contains("buffer_barrier(")
                && between.contains(
                    "vk::PipelineStageFlags::TRANSFER,\n                vk::AccessFlags::TRANSFER_WRITE,\n                vk::PipelineStageFlags::TRANSFER,"
                ),
            "a TRANSFER -> TRANSFER barrier must separate the zero fill from the seed fills \
             (#4293); without it sync validation reports two WRITE_AFTER_WRITE hazards per \
             scatter frame"
        );
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
