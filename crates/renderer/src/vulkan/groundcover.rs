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
//! [`GROUNDCOVER_MAX_CHUNKS`] × [`GROUNDCOVER_MAX_BLADES_PER_CHUNK`] × 16 B =
//! 16 MB device-local, against the 4 GB total budget. The visible set at the
//! shipped 512-unit chunk and 2000-unit draw distance is ~50 chunks, so this
//! is roughly 20× headroom — deliberately, because §11.2 lists chunk size as
//! an open question wanting a sweep, and halving it quadruples the chunk
//! count. Sizing to today's number would make the sweep a code change.
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
use super::sync::MAX_FRAMES_IN_FLIGHT;
use crate::shader_constants::{
    GROUNDCOVER_BLADE_SEGMENTS_NEAR, GROUNDCOVER_CHUNKS_PER_CELL_SIDE,
    GROUNDCOVER_HISTOGRAM_BUCKETS, GROUNDCOVER_MAX_BLADES_PER_CHUNK, GROUNDCOVER_MAX_CHUNKS,
    GROUNDCOVER_VERTS_PER_SEGMENT,
};

const SCATTER_SPV: &[u8] = include_bytes!("../../shaders/groundcover_scatter.comp.spv");
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
    pub pad1: [f32; 3],
}
// SAFETY: `#[repr(C)]` over `f32`/`u32` only, explicitly padded to 64 bytes
// with named fields, so every byte is initialised by a field write.
unsafe impl NoUninit for GpuGroundCoverCell {}

/// One 512-unit chunk. Mirrors `GroundCoverChunk` in the shared GLSL header.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct GpuGroundCoverChunk {
    pub base_xz: [f32; 2],
    pub cell_index: u32,
    pub seed: u32,
}
// SAFETY: as `GpuGroundCoverCell` — 16 bytes, no padding.
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
}
// SAFETY: 48 bytes of `f32`, no padding.
unsafe impl NoUninit for GpuGroundCoverSpecies {}

/// Push constants for the scatter dispatch.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ScatterPush {
    camera_pos: [f32; 4],
    chunk_count: u32,
    blades_per_chunk: u32,
    verts_per_blade: u32,
    species_count: u32,
}

/// Push constants for the blade / debug draw. Exactly 128 bytes — Vulkan's
/// guaranteed `maxPushConstantsSize` floor, which is why the fields are packed
/// rather than given a `vec4` each: this device allows 256, so a layout that
/// only fits there is a portability bug no test on this machine can see.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct BladePush {
    /// Render-origin-RELATIVE, as `triangle.vert` uses it (#1496).
    view_proj: [f32; 16],
    /// xyz = absolute camera position, w = pixels per world unit at unit depth.
    camera_pixels: [f32; 4],
    /// xyz = render origin to subtract, w = seconds.
    origin_time: [f32; 4],
    /// xy = unit wind direction, z = speed, w = gust amplitude.
    wind: [f32; 4],
    /// x = gust frequency, yzw = blades-per-chunk / segments / species count,
    /// carried as floats because they share a `vec4` with the frequency.
    gust_and_counts: [f32; 4],
}

/// Everything the host publishes for one frame of ground cover.
pub struct GroundCoverFrame<'a> {
    pub cells: &'a [GpuGroundCoverCell],
    pub chunks: &'a [GpuGroundCoverChunk],
    pub species: &'a [GpuGroundCoverSpecies],
    pub view_proj: [f32; 16],
    pub camera_pos: [f32; 3],
    /// Cell-grid-snapped render origin the projection expects to have been
    /// subtracted (#1496). Ground cover positions terrain vertices in absolute
    /// world space, so this is what closes the gap.
    pub render_origin: [f32; 3],
    /// `[dir.x, dir.y, speed, gust_amplitude]`.
    pub wind: [f32; 4],
    pub gust_frequency: f32,
    pub time_seconds: f32,
    /// Pixels per world unit at one unit of depth: `render_height * 0.5 /
    /// tan(fov_y * 0.5)`. §6 keys blade widening to projected pixel size
    /// rather than distance, so this has to come from the live projection —
    /// a constant here would make the widening correct at exactly one
    /// resolution and shimmer at every other.
    pub pixels_per_unit_at_unit_depth: f32,
    /// Render the accepted candidate points instead of blades (§9 Phase 1).
    pub debug_points: bool,
}

/// Per-frame scatter telemetry, harvested one pipelined cycle late.
#[derive(Clone, Copy, Default, Debug)]
pub struct GroundCoverStats {
    pub chunks_dispatched: u32,
    pub blades_accepted: u32,
    /// Candidates dropped because their chunk's slice was already full.
    /// Non-zero is not a bug — §4 designs for it — but a large fraction means
    /// the cap is below what the density field is asking for.
    pub blades_overflowed: u32,
    /// §11.3's `d_ground` histogram over every candidate the field was
    /// evaluated at, accepted or not.
    pub histogram: [u32; GROUNDCOVER_HISTOGRAM_BUCKETS as usize],
    /// Range of `d_ground` over the frame's candidates.
    pub d_ground_min: f32,
    pub d_ground_max: f32,
    /// Range of the view distance the fade was evaluated at. Reported because
    /// an all-bucket-0 histogram with an empty blade count has two completely
    /// different causes — a field that is genuinely near zero, or a field that
    /// is fine with every candidate past `GROUNDCOVER_DRAW_DISTANCE` — and
    /// these two numbers are what tell them apart.
    pub view_dist_min: f32,
    pub view_dist_max: f32,
    /// Largest value each of the five §3 factors reached this frame, in
    /// [`GROUNDCOVER_FACTOR_NAMES`] order. A zero here names the term that
    /// annihilated the product — which is the only way a pure product fails,
    /// and the one thing a histogram of the product cannot tell you.
    pub factor_max: [f32; 5],
}

impl GroundCoverStats {
    /// `groundcover:` summary row, in the same `key=value` shape as the rest
    /// of the bench output.
    pub fn bench_line(&self) -> String {
        let hist = self
            .histogram
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "groundcover: chunks={} blades={} overflow={} d_ground={:.4}..{:.4} \
             view_dist={:.0}..{:.0} factor_max={} d_ground_hist={}",
            self.chunks_dispatched,
            self.blades_accepted,
            self.blades_overflowed,
            self.d_ground_min,
            self.d_ground_max,
            self.view_dist_min,
            self.view_dist_max,
            GROUNDCOVER_FACTOR_NAMES
                .iter()
                .zip(self.factor_max)
                .map(|(name, value)| format!("{name}:{value:.3}"))
                .collect::<Vec<_>>()
                .join(","),
            hist
        )
    }
}

/// Counter-buffer layout. `[0 .. MAX_CHUNKS)` are the per-chunk atomic append
/// cursors; then the histogram buckets; then one overflow tally.
const COUNTER_HIST_BASE: usize = GROUNDCOVER_MAX_CHUNKS as usize;
const COUNTER_OVERFLOW: usize = COUNTER_HIST_BASE + GROUNDCOVER_HISTOGRAM_BUCKETS as usize;
/// Four extrema slots after the overflow tally: `d_ground` min/max (fixed
/// point ×1e6) and view-distance min/max (world units). See the scatter's own
/// comment on why a histogram alone cannot separate "the field is uniformly
/// low" from "the field is fine but everything is past the fade".
const COUNTER_EXTREMA_BASE: usize = COUNTER_OVERFLOW + 1;
/// Per-factor maxima, ×1e6, in `GroundCoverFactors` order.
const COUNTER_FACTOR_BASE: usize = COUNTER_EXTREMA_BASE + 4;
pub const GROUNDCOVER_FACTOR_NAMES: [&str; 5] =
    ["affinity", "slope", "moisture", "shelter", "clump"];
const COUNTER_SLOTS: usize = COUNTER_FACTOR_BASE + GROUNDCOVER_FACTOR_NAMES.len();
/// `atomicMin` seed. The clear fills the buffer with zero, which is the wrong
/// identity for a minimum — so the host seeds the two `min` slots after the
/// fill and before the dispatch.
const EXTREMA_MIN_SEED: u32 = u32::MAX;

/// Vertices one tier-0 blade emits.
pub const VERTS_PER_BLADE_NEAR: u32 =
    GROUNDCOVER_BLADE_SEGMENTS_NEAR * GROUNDCOVER_VERTS_PER_SEGMENT;

pub struct GroundCoverPipeline {
    scatter_set_layout: vk::DescriptorSetLayout,
    scatter_pipeline_layout: vk::PipelineLayout,
    scatter_pipeline: vk::Pipeline,

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
    blade_buffer: Option<GpuBuffer>,
    indirect_buffer: Option<GpuBuffer>,
    counter_buffer: Option<GpuBuffer>,
    /// Per-slot readback of `counter_buffer`, copied at the end of the scatter
    /// so the histogram (§11.3) and the overflow tally can be reported without
    /// stalling the frame that produced them.
    counter_readback: Vec<GpuBuffer>,

    bound_vertex_buffer: vk::Buffer,
    /// Chunk count each in-flight slot dispatched, so the readback taken one
    /// cycle later is attributed to the right frame.
    pending_chunks: [u32; MAX_FRAMES_IN_FLIGHT],
    stats: GroundCoverStats,
    /// Chunks uploaded for the frame currently being recorded.
    frame_chunk_count: u32,
    frame_debug_points: bool,
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
            blade_buffer: None,
            indirect_buffer: None,
            counter_buffer: None,
            counter_readback: Vec::new(),
            bound_vertex_buffer: vk::Buffer::null(),
            pending_chunks: [0; MAX_FRAMES_IN_FLIGHT],
            stats: GroundCoverStats::default(),
            frame_chunk_count: 0,
            frame_debug_points: false,
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
        let chunk_bytes = (GROUNDCOVER_MAX_CHUNKS as usize
            * std::mem::size_of::<GpuGroundCoverChunk>())
            as vk::DeviceSize;
        let cell_bytes =
            (MAX_GROUNDCOVER_CELLS * std::mem::size_of::<GpuGroundCoverCell>()) as vk::DeviceSize;
        let species_bytes = (MAX_GROUNDCOVER_SPECIES * std::mem::size_of::<GpuGroundCoverSpecies>())
            as vk::DeviceSize;
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            self.chunk_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                chunk_bytes,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
            self.cell_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                cell_bytes,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
            self.species_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                species_bytes,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
            self.counter_readback.push(GpuBuffer::create_host_readback(
                device,
                allocator,
                (COUNTER_SLOTS * 4) as vk::DeviceSize,
                vk::BufferUsageFlags::TRANSFER_DST,
            )?);
        }
        // 16 B per blade × the cap. See the module docs on why the cap is
        // sized for a chunk-size sweep rather than for today's visible set.
        let blade_bytes =
            (GROUNDCOVER_MAX_CHUNKS as u64) * (GROUNDCOVER_MAX_BLADES_PER_CHUNK as u64) * 16;
        self.blade_buffer = Some(GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            blade_bytes,
            vk::BufferUsageFlags::STORAGE_BUFFER,
        )?);
        self.indirect_buffer = Some(GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            (GROUNDCOVER_MAX_CHUNKS as u64) * 16,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::INDIRECT_BUFFER,
        )?);
        self.counter_buffer = Some(GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            (COUNTER_SLOTS * 4) as vk::DeviceSize,
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
        // The scatter's own set 0: chunks, cells, the global vertex SSBO
        // (§11.1 path A), blades, the indirect draws it writes, and the
        // counters it appends through.
        let compute = vk::ShaderStageFlags::COMPUTE;
        let scatter_bindings: Vec<vk::DescriptorSetLayoutBinding> = (0..6)
            .map(|binding| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(binding)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .descriptor_count(1)
                    .stage_flags(compute)
            })
            .collect();
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

        // Set 2 for the draw pipelines. Bindings 4 and 5 (indirect, counters)
        // are deliberately absent: the draw reads neither, and declaring them
        // would let a future edit sample the scatter's scratch from a fragment
        // shader without anything failing.
        let vertex = vk::ShaderStageFlags::VERTEX;
        let draw_bindings = [
            storage_binding(0, vertex),
            storage_binding(1, vertex),
            storage_binding(2, vertex),
            storage_binding(3, vertex),
            storage_binding(6, vertex | vk::ShaderStageFlags::FRAGMENT),
        ];
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
        let sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(11 * frames)];
        // SAFETY: `sizes` outlives the call; the pool is owned here.
        self.descriptor_pool = unsafe {
            device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .pool_sizes(&sizes)
                        .max_sets(2 * frames),
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
        Ok(())
    }

    fn create_pipelines(
        &mut self,
        device: &ash::Device,
        pipeline_cache: vk::PipelineCache,
        render_pass: vk::RenderPass,
    ) -> Result<()> {
        let entry = std::ffi::CString::new("main").expect("static literal");
        let scatter = shader_module(device, SCATTER_SPV, "groundcover_scatter.comp")?;
        let vert = shader_module(device, BLADE_VERT_SPV, "groundcover_blade.vert")?;
        let blade_frag = shader_module(device, BLADE_FRAG_SPV, "groundcover_blade.frag")?;
        let debug_frag = shader_module(device, DEBUG_FRAG_SPV, "groundcover_debug.frag")?;
        let result = self.build_pipelines(
            device,
            pipeline_cache,
            render_pass,
            &entry,
            scatter,
            vert,
            blade_frag,
            debug_frag,
        );
        for module in [scatter, vert, blade_frag, debug_frag] {
            // SAFETY: pipeline creation has returned, and the spec allows a
            // module to be destroyed as soon as it has.
            unsafe { device.destroy_shader_module(module, None) };
        }
        result
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
                .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);
            // Eight attachments to match the main pass. Only 0 (HDR colour)
            // and 6/7 (the FSR masks) are written; the G-buffer's normal /
            // motion / mesh-ID attachments stay masked off, exactly as water
            // leaves them and for the same reason — see the fragment shader's
            // header on why procedural wind-animated geometry has no motion
            // vector this pass could honestly write.
            let mut blend_attachments = [vk::PipelineColorBlendAttachmentState::default(); 8];
            blend_attachments[0] = blend_attachments[0].color_write_mask(
                vk::ColorComponentFlags::R
                    | vk::ColorComponentFlags::G
                    | vk::ColorComponentFlags::B
                    | vk::ColorComponentFlags::A,
            );
            blend_attachments[6] =
                blend_attachments[6].color_write_mask(vk::ColorComponentFlags::R);
            blend_attachments[7] =
                blend_attachments[7].color_write_mask(vk::ColorComponentFlags::R);
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

        if input.chunks.is_empty() || input.cells.is_empty() || vertex_buffer == vk::Buffer::null()
        {
            self.frame_chunk_count = 0;
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
            return false;
        }

        let uploads = self.chunk_buffers[frame]
            .write_mapped(device, chunks)
            .and_then(|()| self.cell_buffers[frame].write_mapped(device, cells))
            .and_then(|()| self.species_buffers[frame].write_mapped(device, species));
        if let Err(error) = uploads {
            log::warn!("ground cover: record upload failed: {error}");
            self.frame_chunk_count = 0;
            return false;
        }
        self.write_descriptor_sets(device, frame, vertex_buffer);
        self.bound_vertex_buffer = vertex_buffer;

        self.frame_chunk_count = chunks.len() as u32;
        self.frame_debug_points = input.debug_points;
        let segments = if input.debug_points {
            1
        } else {
            GROUNDCOVER_BLADE_SEGMENTS_NEAR
        };
        self.frame_push = BladePush {
            view_proj: input.view_proj,
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
            gust_and_counts: [
                input.gust_frequency,
                GROUNDCOVER_MAX_BLADES_PER_CHUNK as f32,
                segments as f32,
                species.len() as f32,
            ],
        };
        true
    }

    fn harvest(&mut self, device: &ash::Device, frame: usize) {
        let dispatched = std::mem::take(&mut self.pending_chunks[frame]);
        if dispatched == 0 {
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
            ..Default::default()
        };
        // Clamped, not summed raw: the append cursor deliberately runs past
        // the cap (§4's saturating overflow), so the raw value is "candidates
        // that tried", not "blades that exist".
        stats.blades_accepted = counters[..dispatched as usize]
            .iter()
            .map(|c| c.min(&GROUNDCOVER_MAX_BLADES_PER_CHUNK))
            .sum();
        stats.blades_overflowed = counters[COUNTER_OVERFLOW];
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
            write(draw, 0, &chunk_info),
            write(draw, 1, &cell_info),
            write(draw, 2, &vertex_info),
            write(draw, 3, &blade_info),
            write(draw, 6, &species_info),
        ];
        // SAFETY: every `*_info` slice outlives the call, and only slot
        // `frame`'s sets are touched — the caller has waited that slot's
        // fence, so nothing in flight reads them.
        unsafe { device.update_descriptor_sets(&writes, &[]) };
    }

    /// Record the scatter dispatch. Must be OUTSIDE a render pass and before
    /// the main geometry pass that draws the result.
    pub fn record_scatter(&mut self, device: &ash::Device, cmd: vk::CommandBuffer, frame: usize) {
        if self.frame_chunk_count == 0 {
            return;
        }
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
                    0.0,
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
                    GROUNDCOVER_BLADE_SEGMENTS_NEAR * GROUNDCOVER_VERTS_PER_SEGMENT
                },
                species_count: self.frame_push.gust_and_counts[3] as u32,
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
            buffer_barrier(
                device,
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::DRAW_INDIRECT | vk::PipelineStageFlags::VERTEX_SHADER,
                vk::AccessFlags::INDIRECT_COMMAND_READ | vk::AccessFlags::SHADER_READ,
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
        self.pending_chunks[frame] = self.frame_chunk_count;
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
            device.cmd_push_constants(
                cmd,
                self.draw_pipeline_layout,
                vk::ShaderStageFlags::VERTEX,
                0,
                blade_push_bytes(&self.frame_push),
            );
            // One `VkDrawIndirectCommand` per chunk, written by the scatter.
            // `firstVertex` carries the chunk's blade-slice base, so the
            // vertex shader indexes the global blade buffer directly with
            // `gl_VertexIndex` — no `firstInstance`, and therefore no
            // dependency on `drawIndirectFirstInstance`.
            device.cmd_draw_indirect(cmd, indirect.buffer, 0, self.frame_chunk_count, 16);
        }
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
        }
        for layout in [&mut self.scatter_set_layout, &mut self.draw_set_layout] {
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
            .chain(self.counter_readback.iter_mut())
            .chain(self.blade_buffer.iter_mut())
            .chain(self.indirect_buffer.iter_mut())
            .chain(self.counter_buffer.iter_mut())
        {
            buffer.destroy(device, allocator);
        }
        self.chunk_buffers.clear();
        self.cell_buffers.clear();
        self.species_buffers.clear();
        self.counter_readback.clear();
        self.blade_buffer = None;
        self.indirect_buffer = None;
        self.counter_buffer = None;
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

    /// The GPU records are the shader contract. std430 rounds a `vec4` to a
    /// 16-byte boundary, so a record whose Rust side lost its explicit padding
    /// would read the next field's bytes as an affinity weight — plausible
    /// numbers, wrong ground.
    #[test]
    fn gpu_records_match_their_std430_layout() {
        assert_eq!(std::mem::size_of::<GpuGroundCoverCell>(), 64);
        assert_eq!(std::mem::size_of::<GpuGroundCoverChunk>(), 16);
        assert_eq!(std::mem::size_of::<GpuGroundCoverSpecies>(), 48);
        // §4's blade record is "~16 bytes", and the blade buffer is sized by
        // that number in `create_buffers`.
        assert_eq!(
            (GROUNDCOVER_MAX_CHUNKS as u64) * (GROUNDCOVER_MAX_BLADES_PER_CHUNK as u64) * 16,
            16 * 1024 * 1024,
            "the blade buffer's documented 16 MB is derived from these two caps"
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
        assert_eq!(std::mem::size_of::<BladePush>(), 128);
        assert!(std::mem::size_of::<ScatterPush>() <= 128);
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
        assert_eq!(COUNTER_SLOTS, COUNTER_FACTOR_BASE + 5);
        let src = include_str!("../../shaders/groundcover_scatter.comp");
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
        assert_eq!(crate::shader_constants::GROUNDCOVER_SCATTER_WORKGROUP, 64);
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
            .find("if (accept >= dDraw)")
            .expect("the accept test must read d_draw");
        let fade = src
            .find("byroGcDistanceFade(")
            .expect("d_draw must come from the distance fade");
        assert!(fade < accept, "d_draw must be computed before it is tested");
    }

    /// §4's overflow policy: the atomic append saturates, it does not wrap,
    /// and a losing thread drops its candidate rather than writing outside its
    /// chunk's slice.
    #[test]
    fn the_atomic_append_saturates() {
        let src = include_str!("../../shaders/groundcover_scatter.comp");
        assert!(
            src.contains("uint slot = atomicAdd(gcCounters[chunkIdx], 1u);")
                && src.contains("if (slot >= pc.bladesPerChunk) {"),
            "the append must bail on a slot past the cap rather than wrapping \
             into the next chunk's slice (#4054 / §4)"
        );
        assert!(
            src.contains("uint accepted = min(gcCounters[chunkIdx], pc.bladesPerChunk);"),
            "the published draw must clamp the cursor: it deliberately runs \
             past the cap, so the raw value is candidates-that-tried"
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
    }
}
