//! EXAL ground cover §11.1 — the terrain-attribute sampling bench (#4052).
//!
//! `docs/engine/exal-groundcover.md` §11.1 asks which of two paths should
//! supply terrain height, normal and splat weights at an arbitrary world
//! point, and says the answer cannot be reasoned to:
//!
//! * **Path A** reads the renderer's existing global vertex SSBO directly.
//!   Bakes nothing and stays automatically in lockstep with the terrain; the
//!   indirection cost per sample is what nobody has measured.
//! * **Path B** samples a baked per-cell attribute texture. Cheaper per fetch
//!   in principle, but costs memory, costs a bake, and can drift.
//!
//! Neither had any code to extend when this landed: splat weights reach
//! `triangle.frag` as interpolated vertex attributes, not as a lookup, so no
//! shader in this renderer had ever sampled terrain at an arbitrary point.
//! This module is therefore a purpose-built harness rather than
//! instrumentation of something that already runs.
//!
//! ## Why it measures four things and not two
//!
//! §11.1 was written as a question about the scatter pass. The scope
//! correction recorded on it makes the *raster* pass the larger consumer by a
//! wide margin — the blade vertex shader re-samples once per vertex per blade,
//! where the scatter samples once per candidate point — and the raster side is
//! the one §4's store-vs-resample trade turns on. So the bench crosses both
//! paths with both consumers:
//!
//! | Variant           | Stage   | Path    | Access pattern                       |
//! |-------------------|---------|---------|--------------------------------------|
//! | `ScatterSsbo`     | compute | A       | incoherent, one sample per candidate |
//! | `ScatterBaked`    | compute | B       | incoherent, one sample per candidate |
//! | `ScatterFloor`    | compute | control | same points, no sample               |
//! | `RasterSsbo`      | vertex  | A       | coherent, `BLADE_VERTS` per root     |
//! | `RasterBaked`     | vertex  | B       | coherent, `BLADE_VERTS` per root     |
//! | `RasterFloor`     | vertex  | control | same primitives, no sample           |
//!
//! The two controls sample nothing while doing everything else the paired
//! variants do. They are what makes the other four readable: the scatter's
//! candidate-point generation and the raster's per-vertex invocation +
//! primitive assembly land inside the same bracket, and without a control
//! "the two paths are indistinguishable" cannot be told apart from "the
//! sampling is buried under an overhead that dominates both".
//!
//! ## One variant per frame
//!
//! All four could be dispatched in one frame under four brackets. They are
//! not, for two reasons. Adjacent brackets absorb each other's queue drain
//! (see [`super::gpu_timers::GpuTimerSnapshot`]'s "upper bound, not a precise
//! attribution" note), and four back-to-back dispatches over the same terrain
//! would leave later variants sampling a cache the earlier ones warmed. So the
//! harness round-robins: one variant per frame, one bracket, and a `--bench-
//! frames N` run collects `N/4` independent samples of each.
//!
//! ## The chunk-to-instance association
//!
//! §11.1's original text put path A's locator on the terrain-tile record;
//! `GpuTerrainTile` is 24 texture indices and nothing else. The locator is
//! `GpuInstance.vertex_offset`, and the missing link is the chunk → covering-
//! terrain-instance association. The harness builds it (host-side, from the
//! resident terrain cells) and makes **both** paths go through it — path B's
//! array layer is the same cell index path A's vertex offset comes from — so
//! neither path is measured with a locator the other had to look up.
//!
//! The association is rebuilt every frame rather than cached: `MeshRegistry`
//! compacts, and a stale vertex offset would silently point into another
//! mesh's vertices.

use anyhow::{Context, Result};
use ash::vk;

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::gpu_timers::GpuPerFrameTimers;
use super::image::{GpuImage, GpuImageDesc};
use super::reflect::{validate_set_layout, ReflectedShader};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use crate::shader_constants::{
    GROUNDCOVER_BENCH_BLADE_VERTS, GROUNDCOVER_BENCH_WORKGROUP, GROUNDCOVER_CHUNKS_PER_CELL_SIDE,
    GROUNDCOVER_CHUNK_UNITS, LAND_GRID_VERTS,
};

const BENCH_COMP_SPV: &[u8] = include_bytes!("../../shaders/groundcover_bench.comp.spv");
const BENCH_VERT_SPV: &[u8] = include_bytes!("../../shaders/groundcover_bench.vert.spv");
const BENCH_FRAG_SPV: &[u8] = include_bytes!("../../shaders/groundcover_bench.frag.spv");
const BENCH_BAKE_SPV: &[u8] = include_bytes!("../../shaders/groundcover_bench_bake.comp.spv");

/// Array layers in path B's attribute textures, and therefore the cap on
/// resident terrain cells the bench measures over. `--radius 3` (the largest
/// exterior ring the smoke tests use) resides 49 cells; 128 leaves headroom
/// for `--radius 5` without sizing the bake for a ring nobody runs.
pub const MAX_BENCH_CELLS: usize = 128;

/// §4: an 8×8 grid of 512-unit chunks per exterior cell.
const CHUNKS_PER_CELL: usize =
    (GROUNDCOVER_CHUNKS_PER_CELL_SIDE * GROUNDCOVER_CHUNKS_PER_CELL_SIDE) as usize;

/// Sink slots. Power of two so the shader can mask rather than modulo.
/// Nothing reads this buffer back — it exists only so no path's loads can be
/// dead-code-eliminated.
const RESULT_SLOTS: usize = 4096;

/// Candidate points each compute thread draws. With
/// `GROUNDCOVER_BENCH_WORKGROUP` (64) threads per chunk this is 1024 samples
/// per chunk — deliberately equal to the raster half's
/// `DEFAULT_BLADES_PER_CHUNK × GROUNDCOVER_BENCH_BLADE_VERTS`, so the two
/// consumers are compared at the same sample count and the per-sample
/// numbers can be read side by side.
pub const DEFAULT_SAMPLES_PER_THREAD: u32 = 16;

/// Blades per chunk in the raster half. See [`DEFAULT_SAMPLES_PER_THREAD`].
pub const DEFAULT_BLADES_PER_CHUNK: u32 = 128;

/// Attachment extent for the raster half. The blades collapse to zero-area
/// triangles and never produce a fragment, so this is only what the render
/// pass has to be sized against; 64×64 rather than 1×1 to stay off any
/// degenerate-target fast path a driver might take.
const RASTER_EXTENT: u32 = 64;

const ATTR_FORMAT: vk::Format = vk::Format::R32G32B32A32_SFLOAT;
const SPLAT_FORMAT: vk::Format = vk::Format::R8G8B8A8_UNORM;
const RASTER_COLOR_FORMAT: vk::Format = vk::Format::R8_UNORM;

/// One resident exterior terrain cell, already resolved against the mesh
/// registry. Built fresh every frame — see the module docs on why the
/// vertex offset is not cached.
#[derive(Clone, Copy, Debug)]
pub struct BenchCellInput {
    /// Y-up world XZ of the cell's (row 0, col 0) terrain vertex.
    pub origin_xz: [f32; 2],
    /// `GpuInstance.vertex_offset` for the terrain mesh covering the cell.
    pub vertex_offset: u32,
}

/// GPU mirror of `BenchCell` in `include/groundcover_bench.glsl`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct GpuBenchCell {
    origin_xz: [f32; 2],
    vertex_offset: u32,
    pad0: u32,
}
// SAFETY: `#[repr(C)]`, all fields are plain `f32`/`u32`, and the struct's
// 16-byte size leaves no padding bytes — every byte is initialised by the
// field writes, which is exactly `NoUninit`'s contract.
unsafe impl super::buffer::NoUninit for GpuBenchCell {}

/// GPU mirror of `BenchChunk` in `include/groundcover_bench.glsl`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct GpuBenchChunk {
    base_xz: [f32; 2],
    cell_index: u32,
    seed: u32,
}
// SAFETY: as `GpuBenchCell` — `#[repr(C)]`, 16 bytes, no padding.
unsafe impl super::buffer::NoUninit for GpuBenchChunk {}

/// Push constants shared by both consumers. Mirrors `BenchPush` in
/// `include/groundcover_bench.glsl`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct BenchPush {
    chunk_count: u32,
    samples_per_thread: u32,
    blades_per_chunk: u32,
    sink_scale: f32,
    result_mask: u32,
}

/// Push constants for the bake. Mirrors `BakePush` in
/// `groundcover_bench_bake.comp`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct BakePush {
    cell_count: u32,
}

/// The six measured combinations: two paths plus a control, per consumer.
///
/// Ordered so the round-robin alternates path A / path B rather than running
/// both A variants back to back — any slow drift over a bench run (thermal,
/// clock ramp) then lands on both paths equally instead of on whichever ran
/// first, and each consumer's control sits next to the pair it calibrates.
///
/// The `*Floor` entries sample nothing. They are the control that makes the
/// other four readable: both consumers pay per-frame costs that are not
/// sampling — the scatter generates candidate points, the raster shades a
/// vertex and assembles a primitive per blade vertex — and both land inside
/// the same bracket. Without them, "the two paths are indistinguishable in
/// the raster" and "the raster's sampling is buried under primitive assembly"
/// produce identical numbers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BenchVariant {
    ScatterSsbo,
    ScatterBaked,
    ScatterFloor,
    RasterSsbo,
    RasterBaked,
    RasterFloor,
}

impl BenchVariant {
    pub const ALL: [BenchVariant; 6] = [
        BenchVariant::ScatterSsbo,
        BenchVariant::ScatterBaked,
        BenchVariant::ScatterFloor,
        BenchVariant::RasterSsbo,
        BenchVariant::RasterBaked,
        BenchVariant::RasterFloor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BenchVariant::ScatterSsbo => "scatter/ssbo",
            BenchVariant::ScatterBaked => "scatter/baked",
            BenchVariant::ScatterFloor => "scatter/floor",
            BenchVariant::RasterSsbo => "raster/ssbo",
            BenchVariant::RasterBaked => "raster/baked",
            BenchVariant::RasterFloor => "raster/floor",
        }
    }

    fn is_raster(self) -> bool {
        matches!(
            self,
            BenchVariant::RasterSsbo | BenchVariant::RasterBaked | BenchVariant::RasterFloor
        )
    }

    /// Whether this variant reads path B's baked textures, and therefore
    /// cannot run before the first bake.
    fn is_baked(self) -> bool {
        matches!(self, BenchVariant::ScatterBaked | BenchVariant::RasterBaked)
    }

    /// `BENCH_PATH` specialization value: 0 = SSBO, 1 = baked, 2 = control.
    fn path(self) -> usize {
        match self {
            BenchVariant::ScatterSsbo | BenchVariant::RasterSsbo => 0,
            BenchVariant::ScatterBaked | BenchVariant::RasterBaked => 1,
            BenchVariant::ScatterFloor | BenchVariant::RasterFloor => 2,
        }
    }

    fn index(self) -> usize {
        match self {
            BenchVariant::ScatterSsbo => 0,
            BenchVariant::ScatterBaked => 1,
            BenchVariant::ScatterFloor => 2,
            BenchVariant::RasterSsbo => 3,
            BenchVariant::RasterBaked => 4,
            BenchVariant::RasterFloor => 5,
        }
    }
}

/// Accumulated timings for one variant.
#[derive(Clone, Copy, Default)]
pub struct VariantStats {
    /// Frames whose bracket retired and was harvested.
    pub frames: u32,
    pub total_ms: f64,
    pub min_ms: f32,
    pub max_ms: f32,
    /// Samples the harvested frames issued in total.
    pub total_samples: u64,
}

impl VariantStats {
    fn record(&mut self, ms: f32, samples: u64) {
        if self.frames == 0 {
            self.min_ms = ms;
            self.max_ms = ms;
        } else {
            self.min_ms = self.min_ms.min(ms);
            self.max_ms = self.max_ms.max(ms);
        }
        self.frames += 1;
        self.total_ms += ms as f64;
        self.total_samples += samples;
    }

    pub fn mean_ms(&self) -> f64 {
        if self.frames == 0 {
            0.0
        } else {
            self.total_ms / self.frames as f64
        }
    }

    /// Nanoseconds per sample — the number §11.1 is actually asking for.
    pub fn ns_per_sample(&self) -> f64 {
        if self.total_samples == 0 {
            0.0
        } else {
            self.total_ms * 1.0e6 / self.total_samples as f64
        }
    }
}

/// A device-local array image plus its view and allocation.
/// #3860 — an array image is an owned image plus its view, so it is a
/// [`GpuImage`]. The struct this replaced held `image` / `view` /
/// `allocation` under those same names.
type ArrayImage = GpuImage;

fn create_array_image(
    device: &ash::Device,
    allocator: &SharedAllocator,
    format: vk::Format,
    width: u32,
    height: u32,
    layers: u32,
    name: &str,
) -> Result<ArrayImage> {
    GpuImage::create(
        device,
        allocator,
        &GpuImageDesc::color_2d_array(
            name,
            width,
            height,
            layers,
            format,
            vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::SAMPLED,
        ),
    )
}

/// The §11.1 harness. Created only when `--bench-groundcover-sampling` is
/// passed — it owns ~3.5 MB of baked textures and four pipelines that no
/// production path touches.
pub struct GroundcoverBench {
    // ── Sampling pipelines (both consumers × both paths) ───────────────
    /// Shared by every sampling pipeline: chunk + cell + vertex SSBOs, the
    /// sink, and path B's three textures. One layout, so a variant cannot
    /// win by binding less than its opposite number.
    sample_set_layout: vk::DescriptorSetLayout,
    sample_pipeline_layout: vk::PipelineLayout,
    /// `[path A, path B]` — specialized on `BENCH_PATH`.
    /// Indexed by `BenchVariant::path()`: SSBO, baked, control.
    scatter_pipelines: [vk::Pipeline; 3],
    raster_pipelines: [vk::Pipeline; 3],
    raster_render_pass: vk::RenderPass,
    raster_target: Option<RasterTarget>,

    // ── Path B's bake ──────────────────────────────────────────────────
    bake_set_layout: vk::DescriptorSetLayout,
    bake_pipeline_layout: vk::PipelineLayout,
    bake_pipeline: vk::Pipeline,

    descriptor_pool: vk::DescriptorPool,
    /// One sampling set per frame-in-flight slot (the chunk / cell buffers
    /// they point at are per-slot).
    sample_sets: Vec<vk::DescriptorSet>,
    /// One bake set per slot, for the same reason (it reads the cell buffer).
    bake_sets: Vec<vk::DescriptorSet>,

    /// Per-slot host-visible chunk + cell buffers, rewritten each frame after
    /// that slot's fence has been waited.
    cell_buffers: Vec<GpuBuffer>,
    chunk_buffers: Vec<GpuBuffer>,
    result_buffer: Option<GpuBuffer>,

    // #3860 — `Option`, not a null-handle triple. These used to be
    // constructed as `ArrayImage { image: null(), view: null(), allocation:
    // None }` before `create_images` ran, which is `Option::None` written in
    // Vulkan handles; `GpuImage` has no such state and should not gain one.
    attr_image: Option<ArrayImage>,
    splat0_image: Option<ArrayImage>,
    splat1_image: Option<ArrayImage>,
    sampler: vk::Sampler,
    /// `false` until the first bake transitions them out of `UNDEFINED`.
    images_initialised: bool,

    /// Global vertex SSBO the sampling + bake sets currently point at.
    /// `MeshRegistry` recreates this buffer when it grows, so the sets are
    /// rewritten whenever the handle changes.
    bound_vertex_buffer: vk::Buffer,

    // ── Per-run state ──────────────────────────────────────────────────
    /// Which variant each in-flight slot's bracket is measuring, and how many
    /// samples it issued. Harvested one full pipelined cycle later.
    pending: [Option<(BenchVariant, u64)>; MAX_FRAMES_IN_FLIGHT],
    /// Set when the slot's bracket wrapped a path-B bake instead of a
    /// sampling dispatch.
    pending_bake: [bool; MAX_FRAMES_IN_FLIGHT],
    stats: [VariantStats; BenchVariant::ALL.len()],
    bake_stats: VariantStats,
    /// Round-robin cursor over [`BenchVariant::ALL`].
    next_variant: usize,
    /// Hash of the resident (origin, vertex_offset) set the textures were
    /// baked from. A change means the bake is stale — see the module docs on
    /// why it is not cached across a `MeshRegistry` compaction.
    baked_signature: Option<u64>,
    cells: Vec<GpuBenchCell>,
    chunks: Vec<GpuBenchChunk>,
    samples_per_thread: u32,
    blades_per_chunk: u32,
    /// Cells skipped because the resident ring exceeded [`MAX_BENCH_CELLS`],
    /// reported in the summary so a truncated run cannot read as a full one.
    dropped_cells: usize,
}

/// The raster half's throwaway render target. Never read; see the module
/// docs and `groundcover_bench.frag`.
/// #3860 — the image/view/allocation triple is a [`GpuImage`]; the
/// framebuffer built over it stays a separate field, since `GpuImage`
/// deliberately owns no render-pass state.
struct RasterTarget {
    gpu: GpuImage,
    framebuffer: vk::Framebuffer,
}

impl GroundcoverBench {
    /// `global_vertex_buffer` may be `vk::Buffer::null()` at creation — the
    /// mesh registry's SSBO does not exist until the first upload. The
    /// descriptor sets are (re)written on the first frame that sees a real
    /// handle, and again whenever it changes.
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
        samples_per_thread: u32,
        blades_per_chunk: u32,
    ) -> Result<Self> {
        let mut this = Self {
            sample_set_layout: vk::DescriptorSetLayout::null(),
            sample_pipeline_layout: vk::PipelineLayout::null(),
            scatter_pipelines: [vk::Pipeline::null(); 3],
            raster_pipelines: [vk::Pipeline::null(); 3],
            raster_render_pass: vk::RenderPass::null(),
            raster_target: None,
            bake_set_layout: vk::DescriptorSetLayout::null(),
            bake_pipeline_layout: vk::PipelineLayout::null(),
            bake_pipeline: vk::Pipeline::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            sample_sets: Vec::new(),
            bake_sets: Vec::new(),
            cell_buffers: Vec::new(),
            chunk_buffers: Vec::new(),
            result_buffer: None,
            attr_image: None,
            splat0_image: None,
            splat1_image: None,
            sampler: vk::Sampler::null(),
            images_initialised: false,
            bound_vertex_buffer: vk::Buffer::null(),
            pending: [None; MAX_FRAMES_IN_FLIGHT],
            pending_bake: [false; MAX_FRAMES_IN_FLIGHT],
            stats: [VariantStats::default(); BenchVariant::ALL.len()],
            bake_stats: VariantStats::default(),
            next_variant: 0,
            baked_signature: None,
            cells: Vec::new(),
            chunks: Vec::new(),
            samples_per_thread: samples_per_thread.max(1),
            blades_per_chunk: blades_per_chunk.max(1),
            dropped_cells: 0,
        };

        macro_rules! try_or_cleanup {
            ($expr:expr) => {
                match $expr {
                    Ok(v) => v,
                    Err(e) => {
                        // SAFETY: `this` is still under construction — nothing
                        // in it has been submitted to any queue, so `destroy`'s
                        // "not in use by an in-flight command buffer" contract
                        // holds trivially.
                        unsafe { this.destroy(device, allocator) };
                        return Err(e.into());
                    }
                }
            };
        }

        try_or_cleanup!(this.create_buffers(device, allocator));
        try_or_cleanup!(this.create_images(device, allocator));
        try_or_cleanup!(this.create_layouts(device));
        try_or_cleanup!(this.create_descriptors(device));
        try_or_cleanup!(this.create_pipelines(device, pipeline_cache));
        try_or_cleanup!(this.create_raster_target(device, allocator));
        Ok(this)
    }

    fn create_buffers(&mut self, device: &ash::Device, allocator: &SharedAllocator) -> Result<()> {
        let cell_bytes = (MAX_BENCH_CELLS * std::mem::size_of::<GpuBenchCell>()) as vk::DeviceSize;
        let chunk_bytes = (MAX_BENCH_CELLS * CHUNKS_PER_CELL * std::mem::size_of::<GpuBenchChunk>())
            as vk::DeviceSize;
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            self.cell_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                cell_bytes,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
            self.chunk_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                chunk_bytes,
                vk::BufferUsageFlags::STORAGE_BUFFER,
            )?);
        }
        self.result_buffer = Some(GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            (RESULT_SLOTS * std::mem::size_of::<u32>()) as vk::DeviceSize,
            vk::BufferUsageFlags::STORAGE_BUFFER,
        )?);
        Ok(())
    }

    fn create_images(&mut self, device: &ash::Device, allocator: &SharedAllocator) -> Result<()> {
        let layers = MAX_BENCH_CELLS as u32;
        self.attr_image = Some(create_array_image(
            device,
            allocator,
            ATTR_FORMAT,
            LAND_GRID_VERTS,
            LAND_GRID_VERTS,
            layers,
            "groundcover_bench_attr",
        )?);
        self.splat0_image = Some(create_array_image(
            device,
            allocator,
            SPLAT_FORMAT,
            LAND_GRID_VERTS,
            LAND_GRID_VERTS,
            layers,
            "groundcover_bench_splat0",
        )?);
        self.splat1_image = Some(create_array_image(
            device,
            allocator,
            SPLAT_FORMAT,
            LAND_GRID_VERTS,
            LAND_GRID_VERTS,
            layers,
            "groundcover_bench_splat1",
        )?);
        // LINEAR + CLAMP_TO_EDGE: linear filtering over texel centres is what
        // makes path B reproduce path A's bilinear blend exactly, and the
        // clamp never engages because both paths reject out-of-cell queries
        // before they get here.
        let info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE);
        // SAFETY: `info` is fully populated; the sampler is owned by this
        // struct and destroyed in `destroy`.
        self.sampler = unsafe {
            device
                .create_sampler(&info, None)
                .context("create groundcover bench sampler")?
        };
        Ok(())
    }
}

/// Sampling-set bindings, in the order `include/groundcover_bench.glsl`
/// declares them. Both stage flags on every binding: the two consumers share
/// one layout so a variant cannot win by binding less than its opposite.
fn sample_bindings() -> [vk::DescriptorSetLayoutBinding<'static>; 7] {
    let both = vk::ShaderStageFlags::COMPUTE | vk::ShaderStageFlags::VERTEX;
    let ssbo = |binding: u32| {
        vk::DescriptorSetLayoutBinding::default()
            .binding(binding)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(both)
    };
    let tex = |binding: u32| {
        vk::DescriptorSetLayoutBinding::default()
            .binding(binding)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(both)
    };
    [ssbo(0), ssbo(1), ssbo(2), ssbo(3), tex(4), tex(5), tex(6)]
}

/// Bake-set bindings, in `groundcover_bench_bake.comp`'s order.
fn bake_bindings() -> [vk::DescriptorSetLayoutBinding<'static>; 5] {
    let stage = vk::ShaderStageFlags::COMPUTE;
    let entry = |binding: u32, ty: vk::DescriptorType| {
        vk::DescriptorSetLayoutBinding::default()
            .binding(binding)
            .descriptor_type(ty)
            .descriptor_count(1)
            .stage_flags(stage)
    };
    [
        entry(0, vk::DescriptorType::STORAGE_BUFFER),
        entry(1, vk::DescriptorType::STORAGE_BUFFER),
        entry(2, vk::DescriptorType::STORAGE_IMAGE),
        entry(3, vk::DescriptorType::STORAGE_IMAGE),
        entry(4, vk::DescriptorType::STORAGE_IMAGE),
    ]
}

impl GroundcoverBench {
    fn create_layouts(&mut self, device: &ash::Device) -> Result<()> {
        let sample = sample_bindings();
        validate_set_layout(
            0,
            &sample,
            &[
                ReflectedShader {
                    name: "groundcover_bench.comp",
                    spirv: BENCH_COMP_SPV,
                },
                ReflectedShader {
                    name: "groundcover_bench.vert",
                    spirv: BENCH_VERT_SPV,
                },
            ],
            "groundcover_bench_sample",
            &[],
        )
        .expect("groundcover bench sampling layout drifted against its shaders (see #427)");
        // SAFETY: `sample` outlives the call; `device` is live.
        self.sample_set_layout = unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&sample),
                    None,
                )
                .context("create groundcover bench sampling set layout")?
        };

        let bake = bake_bindings();
        validate_set_layout(
            0,
            &bake,
            &[ReflectedShader {
                name: "groundcover_bench_bake.comp",
                spirv: BENCH_BAKE_SPV,
            }],
            "groundcover_bench_bake",
            &[],
        )
        .expect("groundcover bench bake layout drifted against its shader (see #427)");
        // SAFETY: as above.
        self.bake_set_layout = unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bake),
                    None,
                )
                .context("create groundcover bench bake set layout")?
        };

        // One pipeline layout for all four sampling pipelines, compute and
        // graphics alike. The push range names both stages because the
        // graphics half reads it from the vertex stage; a compute pipeline
        // built against a layout whose range also covers VERTEX is legal and
        // keeps the two consumers provably identical in what they can see.
        let sample_range = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE | vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size(std::mem::size_of::<BenchPush>() as u32)];
        let sample_layouts = [self.sample_set_layout];
        // SAFETY: both slices outlive the call; `device` is live.
        self.sample_pipeline_layout = unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(&sample_layouts)
                        .push_constant_ranges(&sample_range),
                    None,
                )
                .context("create groundcover bench sampling pipeline layout")?
        };

        let bake_range = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .offset(0)
            .size(std::mem::size_of::<BakePush>() as u32)];
        let bake_layouts = [self.bake_set_layout];
        // SAFETY: as above.
        self.bake_pipeline_layout = unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(&bake_layouts)
                        .push_constant_ranges(&bake_range),
                    None,
                )
                .context("create groundcover bench bake pipeline layout")?
        };
        Ok(())
    }

    fn create_descriptors(&mut self, device: &ash::Device) -> Result<()> {
        let frames = MAX_FRAMES_IN_FLIGHT as u32;
        let sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(4 * frames + 2 * frames),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(3 * frames),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(3 * frames),
        ];
        // SAFETY: `sizes` outlives the call; `device` is live and the pool is
        // owned by this struct until `destroy`.
        self.descriptor_pool = unsafe {
            device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .pool_sizes(&sizes)
                        .max_sets(2 * frames),
                    None,
                )
                .context("create groundcover bench descriptor pool")?
        };

        let sample_layouts = vec![self.sample_set_layout; MAX_FRAMES_IN_FLIGHT];
        // SAFETY: the pool was created with capacity for these sets and the
        // layout slice outlives the call.
        self.sample_sets = unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(self.descriptor_pool)
                        .set_layouts(&sample_layouts),
                )
                .context("allocate groundcover bench sampling sets")?
        };
        let bake_layouts = vec![self.bake_set_layout; MAX_FRAMES_IN_FLIGHT];
        // SAFETY: as above.
        self.bake_sets = unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(self.descriptor_pool)
                        .set_layouts(&bake_layouts),
                )
                .context("allocate groundcover bench bake sets")?
        };
        Ok(())
    }

    fn create_pipelines(
        &mut self,
        device: &ash::Device,
        pipeline_cache: vk::PipelineCache,
    ) -> Result<()> {
        let entry = std::ffi::CString::new("main").expect("static literal");

        let comp_module = create_shader_module(device, BENCH_COMP_SPV, "groundcover_bench.comp")?;
        let vert_module = create_shader_module(device, BENCH_VERT_SPV, "groundcover_bench.vert")?;
        let frag_module = create_shader_module(device, BENCH_FRAG_SPV, "groundcover_bench.frag")?;
        let bake_module =
            create_shader_module(device, BENCH_BAKE_SPV, "groundcover_bench_bake.comp")?;

        // Destroy the modules however this function exits — pipelines keep
        // their own copies of the code.
        let result = self.build_pipelines(
            device,
            pipeline_cache,
            &entry,
            comp_module,
            vert_module,
            frag_module,
            bake_module,
        );
        for module in [comp_module, vert_module, frag_module, bake_module] {
            // SAFETY: every pipeline created above has already consumed the
            // module; a module may be destroyed as soon as pipeline creation
            // returns (Vulkan spec).
            unsafe { device.destroy_shader_module(module, None) };
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn build_pipelines(
        &mut self,
        device: &ash::Device,
        pipeline_cache: vk::PipelineCache,
        entry: &std::ffi::CStr,
        comp_module: vk::ShaderModule,
        vert_module: vk::ShaderModule,
        frag_module: vk::ShaderModule,
        bake_module: vk::ShaderModule,
    ) -> Result<()> {
        // `BENCH_PATH` — specialization constant 0 in both sampling shaders.
        // 0 = path A, 1 = path B, 2 = the no-sampling control.
        let path_values: [u32; 3] = [0, 1, 2];
        let map = [vk::SpecializationMapEntry::default()
            .constant_id(0)
            .offset(0)
            .size(std::mem::size_of::<u32>())];

        for (i, value) in path_values.iter().enumerate() {
            let data = value.to_ne_bytes();
            let spec = vk::SpecializationInfo::default()
                .map_entries(&map)
                .data(&data);
            let stage = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::COMPUTE)
                .module(comp_module)
                .name(entry)
                .specialization_info(&spec);
            let info = vk::ComputePipelineCreateInfo::default()
                .stage(stage)
                .layout(self.sample_pipeline_layout);
            // SAFETY: the create-info borrows `stage`/`spec`/`data`, all of
            // which outlive the call; the layout and module are live.
            self.scatter_pipelines[i] = unsafe {
                device
                    .create_compute_pipelines(pipeline_cache, &[info], None)
                    .map_err(|(_, e)| e)
                    .context("create groundcover bench scatter pipeline")?[0]
            };
        }

        let bake_stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(bake_module)
            .name(entry);
        let bake_info = vk::ComputePipelineCreateInfo::default()
            .stage(bake_stage)
            .layout(self.bake_pipeline_layout);
        // SAFETY: as above.
        self.bake_pipeline = unsafe {
            device
                .create_compute_pipelines(pipeline_cache, &[bake_info], None)
                .map_err(|(_, e)| e)
                .context("create groundcover bench bake pipeline")?[0]
        };

        self.create_raster_render_pass(device)?;

        for (i, value) in path_values.iter().enumerate() {
            let data = value.to_ne_bytes();
            let spec = vk::SpecializationInfo::default()
                .map_entries(&map)
                .data(&data);
            let stages = [
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .module(vert_module)
                    .name(entry)
                    .specialization_info(&spec),
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(frag_module)
                    .name(entry),
            ];
            // No vertex input at all: §4's blade geometry is generated from a
            // seed in the vertex shader, so the real pass will have no vertex
            // fetch either. Adding one here would price a cost the design
            // does not pay.
            let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
            let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
                .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
            let viewports = [vk::Viewport::default()
                .width(RASTER_EXTENT as f32)
                .height(RASTER_EXTENT as f32)
                .max_depth(1.0)];
            let scissors = [vk::Rect2D::default().extent(vk::Extent2D {
                width: RASTER_EXTENT,
                height: RASTER_EXTENT,
            })];
            let viewport_state = vk::PipelineViewportStateCreateInfo::default()
                .viewports(&viewports)
                .scissors(&scissors);
            // Rasterization is deliberately ENABLED. `rasterizerDiscardEnable`
            // would remove the render pass and target entirely, but it also
            // permits a driver to elide a vertex shader with no side effects —
            // which is precisely the shader being timed. Zero-area triangles
            // get the same "no fragment work" outcome while keeping
            // `gl_Position` a live consumer of every sample.
            let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
                .polygon_mode(vk::PolygonMode::FILL)
                .cull_mode(vk::CullModeFlags::NONE)
                .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                .line_width(1.0);
            let multisample = vk::PipelineMultisampleStateCreateInfo::default()
                .rasterization_samples(vk::SampleCountFlags::TYPE_1);
            let blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::R)];
            let blend =
                vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);
            let info = vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .rasterization_state(&rasterization)
                .multisample_state(&multisample)
                .color_blend_state(&blend)
                .layout(self.sample_pipeline_layout)
                .render_pass(self.raster_render_pass)
                .subpass(0);
            // SAFETY: every borrowed state struct above outlives the call;
            // the layout, render pass and modules are live.
            self.raster_pipelines[i] = unsafe {
                device
                    .create_graphics_pipelines(pipeline_cache, &[info], None)
                    .map_err(|(_, e)| e)
                    .context("create groundcover bench raster pipeline")?[0]
            };
        }
        Ok(())
    }

    fn create_raster_render_pass(&mut self, device: &ash::Device) -> Result<()> {
        let attachments = [vk::AttachmentDescription::default()
            .format(RASTER_COLOR_FORMAT)
            .samples(vk::SampleCountFlags::TYPE_1)
            // Nothing reads this target, so neither endpoint of the pass
            // should pay for it: DONT_CARE both ways keeps the bracket on the
            // vertex stage rather than on a clear the real pass would not do.
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
        let color_refs = [vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
        let subpasses = [vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_refs)];
        // SAFETY: all three slices outlive the call; `device` is live and the
        // render pass is owned by this struct until `destroy`.
        self.raster_render_pass = unsafe {
            device
                .create_render_pass(
                    &vk::RenderPassCreateInfo::default()
                        .attachments(&attachments)
                        .subpasses(&subpasses),
                    None,
                )
                .context("create groundcover bench render pass")?
        };
        Ok(())
    }

    fn create_raster_target(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
    ) -> Result<()> {
        // #3860 — was ~85 lines of create → allocate → bind → view with its
        // own three-arm cleanup.
        let mut target = RasterTarget {
            gpu: GpuImage::create(
                device,
                allocator,
                &GpuImageDesc::color_2d(
                    "groundcover_bench_raster_target",
                    RASTER_EXTENT,
                    RASTER_EXTENT,
                    RASTER_COLOR_FORMAT,
                    vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC,
                ),
            )?,
            framebuffer: vk::Framebuffer::null(),
        };
        let views = [target.gpu.view];
        // SAFETY: `views` outlives the call; the render pass is live and its
        // single attachment matches this view's format.
        target.framebuffer = unsafe {
            device
                .create_framebuffer(
                    &vk::FramebufferCreateInfo::default()
                        .render_pass(self.raster_render_pass)
                        .attachments(&views)
                        .width(RASTER_EXTENT)
                        .height(RASTER_EXTENT)
                        .layers(1),
                    None,
                )
                .context("create groundcover bench framebuffer")?
        };
        self.raster_target = Some(target);
        Ok(())
    }
}

fn create_shader_module(
    device: &ash::Device,
    spirv: &[u8],
    name: &str,
) -> Result<vk::ShaderModule> {
    let mut cursor = std::io::Cursor::new(spirv);
    let code = ash::util::read_spv(&mut cursor).with_context(|| format!("read {name} SPIR-V"))?;
    // SAFETY: `code` is a validated SPIR-V word stream produced by
    // `read_spv`, and it outlives the create call.
    unsafe {
        device
            .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)
            .with_context(|| format!("create {name} shader module"))
    }
}

impl GroundcoverBench {
    /// Rebuild the per-frame chunk/cell records from the resident terrain
    /// cells. Returns the signature path B's bake is keyed on.
    fn build_records(&mut self, cells: &[BenchCellInput]) -> u64 {
        self.cells.clear();
        self.chunks.clear();
        self.dropped_cells = cells.len().saturating_sub(MAX_BENCH_CELLS);

        let mut signature: u64 = 0xcbf2_9ce4_8422_2325;
        for (cell_index, cell) in cells.iter().take(MAX_BENCH_CELLS).enumerate() {
            self.cells.push(GpuBenchCell {
                origin_xz: cell.origin_xz,
                vertex_offset: cell.vertex_offset,
                pad0: 0,
            });
            // FNV-1a over the bytes that make a bake stale: the cell's origin
            // (which cell it is) and its vertex offset (where the mesh
            // registry currently keeps it).
            for word in [
                cell.origin_xz[0].to_bits(),
                cell.origin_xz[1].to_bits(),
                cell.vertex_offset,
            ] {
                signature ^= word as u64;
                signature = signature.wrapping_mul(0x0000_0100_0000_01b3);
            }

            for cz in 0..GROUNDCOVER_CHUNKS_PER_CELL_SIDE {
                for cx in 0..GROUNDCOVER_CHUNKS_PER_CELL_SIDE {
                    self.chunks.push(GpuBenchChunk {
                        // +X and −Z from the cell origin, matching the terrain
                        // grid's row direction (see `terrain_sample.glsl`).
                        base_xz: [
                            cell.origin_xz[0] + cx as f32 * GROUNDCOVER_CHUNK_UNITS,
                            cell.origin_xz[1] - cz as f32 * GROUNDCOVER_CHUNK_UNITS,
                        ],
                        cell_index: cell_index as u32,
                        // §4 requires blade placement stable frame to frame and
                        // across sessions, so the scramble seed is derived from
                        // the chunk's world position, never from a frame index.
                        seed: (cell.origin_xz[0].to_bits() ^ cell.origin_xz[1].to_bits())
                            .wrapping_mul(0x9E37_79B9)
                            ^ (cz * GROUNDCOVER_CHUNKS_PER_CELL_SIDE + cx),
                    });
                }
            }
        }
        signature
    }

    fn write_descriptor_sets(&self, device: &ash::Device, frame: usize, vertex_buffer: vk::Buffer) {
        let result_buffer = self
            .result_buffer
            .as_ref()
            .expect("result buffer created in new()");
        let chunk_info = [vk::DescriptorBufferInfo::default()
            .buffer(self.chunk_buffers[frame].buffer)
            .range(vk::WHOLE_SIZE)];
        let cell_info = [vk::DescriptorBufferInfo::default()
            .buffer(self.cell_buffers[frame].buffer)
            .range(vk::WHOLE_SIZE)];
        let vertex_info = [vk::DescriptorBufferInfo::default()
            .buffer(vertex_buffer)
            .range(vk::WHOLE_SIZE)];
        let result_info = [vk::DescriptorBufferInfo::default()
            .buffer(result_buffer.buffer)
            .range(vk::WHOLE_SIZE)];
        let sampled = |view: vk::ImageView| {
            [vk::DescriptorImageInfo::default()
                .sampler(self.sampler)
                .image_view(view)
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)]
        };
        let attr_info = sampled(
            self.attr_image
                .as_ref()
                .expect("groundcover bench images read before create_images")
                .view,
        );
        let splat0_info = sampled(
            self.splat0_image
                .as_ref()
                .expect("groundcover bench images read before create_images")
                .view,
        );
        let splat1_info = sampled(
            self.splat1_image
                .as_ref()
                .expect("groundcover bench images read before create_images")
                .view,
        );
        let storage = |view: vk::ImageView| {
            [vk::DescriptorImageInfo::default()
                .image_view(view)
                .image_layout(vk::ImageLayout::GENERAL)]
        };
        let attr_storage = storage(
            self.attr_image
                .as_ref()
                .expect("groundcover bench images read before create_images")
                .view,
        );
        let splat0_storage = storage(
            self.splat0_image
                .as_ref()
                .expect("groundcover bench images read before create_images")
                .view,
        );
        let splat1_storage = storage(
            self.splat1_image
                .as_ref()
                .expect("groundcover bench images read before create_images")
                .view,
        );

        let set = self.sample_sets[frame];
        let bake_set = self.bake_sets[frame];
        fn buffer_write<'a>(
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
        let writes = [
            buffer_write(set, 0, &chunk_info),
            buffer_write(set, 1, &cell_info),
            buffer_write(set, 2, &vertex_info),
            buffer_write(set, 3, &result_info),
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(4)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&attr_info),
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(5)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&splat0_info),
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(6)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&splat1_info),
            buffer_write(bake_set, 0, &cell_info),
            buffer_write(bake_set, 1, &vertex_info),
            vk::WriteDescriptorSet::default()
                .dst_set(bake_set)
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .image_info(&attr_storage),
            vk::WriteDescriptorSet::default()
                .dst_set(bake_set)
                .dst_binding(3)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .image_info(&splat0_storage),
            vk::WriteDescriptorSet::default()
                .dst_set(bake_set)
                .dst_binding(4)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .image_info(&splat1_storage),
        ];
        // SAFETY: every `*_info` slice outlives the call. Only slot `frame`'s
        // sets are touched, and the caller has already waited that slot's
        // fence, so no in-flight command buffer is reading them.
        unsafe { device.update_descriptor_sets(&writes, &[]) };
    }

    fn image_barrier(
        &self,
        image: vk::Image,
        old: vk::ImageLayout,
        new: vk::ImageLayout,
        src_access: vk::AccessFlags,
        dst_access: vk::AccessFlags,
    ) -> vk::ImageMemoryBarrier<'static> {
        vk::ImageMemoryBarrier::default()
            .src_access_mask(src_access)
            .dst_access_mask(dst_access)
            .old_layout(old)
            .new_layout(new)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(MAX_BENCH_CELLS as u32),
            )
    }

    /// Record this frame's measurement. Returns the variant that was timed,
    /// or `None` when the frame had nothing to measure (no resident terrain,
    /// no vertex SSBO yet, or no timestamp support).
    ///
    /// Call sites must have already waited slot `frame`'s fence — this writes
    /// that slot's host-visible chunk/cell buffers and rewrites its descriptor
    /// sets.
    pub fn record(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        vertex_buffer: vk::Buffer,
        cells: &[BenchCellInput],
        timers: Option<&mut GpuPerFrameTimers>,
    ) -> Option<BenchVariant> {
        let Some(timers) = timers else {
            // Without TIMESTAMP support there is nothing to read back, and
            // dispatching anyway would only burn frames.
            return None;
        };
        // Harvest the measurement this slot took one pipelined cycle ago.
        // `read_and_reset` has already run for this slot at the top of
        // `draw_frame`, so `last_snapshot` is exactly that frame's bracket.
        if let Some((variant, samples)) = self.pending[frame].take() {
            let snap = timers.last_snapshot();
            if snap.groundcover_bench_active {
                if self.pending_bake[frame] {
                    self.bake_stats.record(snap.groundcover_bench_ms, samples);
                } else {
                    self.stats[variant.index()].record(snap.groundcover_bench_ms, samples);
                }
            }
        }
        self.pending_bake[frame] = false;

        if cells.is_empty() || vertex_buffer == vk::Buffer::null() {
            return None;
        }

        let signature = self.build_records(cells);
        if self.cells.is_empty() {
            return None;
        }
        if let Err(error) = self.upload_records(device, frame) {
            log::warn!("groundcover bench: record upload failed: {error}");
            return None;
        }
        self.write_descriptor_sets(device, frame, vertex_buffer);
        self.bound_vertex_buffer = vertex_buffer;

        let chunk_count = self.chunks.len() as u32;

        // A stale bake would sample a moved mesh, so it takes priority over
        // the frame's sampling variant — and it gets the bracket to itself, so
        // path B's setup cost is reported rather than smeared into whichever
        // variant happened to share the frame.
        if self.baked_signature != Some(signature) {
            timers.cmd_groundcover_bench_start(device, cmd, frame);
            self.record_bake(device, cmd, frame);
            timers.cmd_groundcover_bench_end(device, cmd, frame);
            self.baked_signature = Some(signature);
            self.images_initialised = true;
            let texels =
                self.cells.len() as u64 * (LAND_GRID_VERTS as u64) * (LAND_GRID_VERTS as u64);
            self.pending[frame] = Some((BenchVariant::ScatterBaked, texels));
            self.pending_bake[frame] = true;
            return None;
        }

        let variant = BenchVariant::ALL[self.next_variant % BenchVariant::ALL.len()];
        self.next_variant = self.next_variant.wrapping_add(1);
        // Path B cannot be sampled before its first bake has run.
        if variant.is_baked() && !self.images_initialised {
            return None;
        }

        let push = BenchPush {
            chunk_count,
            samples_per_thread: self.samples_per_thread,
            blades_per_chunk: self.blades_per_chunk,
            // Zero at runtime, unknown at compile time — see `BenchPush`'s
            // GLSL twin. This is what keeps the sampling loop alive under
            // optimisation while leaving the sink's stored value constant.
            sink_scale: 0.0,
            result_mask: (RESULT_SLOTS - 1) as u32,
        };
        let path = variant.path();

        timers.cmd_groundcover_bench_start(device, cmd, frame);
        if variant.is_raster() {
            self.record_raster(device, cmd, frame, path, &push);
        } else {
            self.record_scatter(device, cmd, frame, path, &push, chunk_count);
        }
        timers.cmd_groundcover_bench_end(device, cmd, frame);

        let samples = if variant.is_raster() {
            chunk_count as u64 * self.blades_per_chunk as u64 * GROUNDCOVER_BENCH_BLADE_VERTS as u64
        } else {
            chunk_count as u64 * GROUNDCOVER_BENCH_WORKGROUP as u64 * self.samples_per_thread as u64
        };
        self.pending[frame] = Some((variant, samples));
        Some(variant)
    }

    fn upload_records(&mut self, device: &ash::Device, frame: usize) -> Result<()> {
        self.cell_buffers[frame].write_mapped(device, &self.cells)?;
        self.chunk_buffers[frame].write_mapped(device, &self.chunks)?;
        Ok(())
    }

    fn record_bake(&mut self, device: &ash::Device, cmd: vk::CommandBuffer, frame: usize) {
        let old = if self.images_initialised {
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        } else {
            vk::ImageLayout::UNDEFINED
        };
        let to_general = [
            self.image_barrier(
                self.attr_image
                    .as_ref()
                    .expect("groundcover bench images read before create_images")
                    .image,
                old,
                vk::ImageLayout::GENERAL,
                vk::AccessFlags::SHADER_READ,
                vk::AccessFlags::SHADER_WRITE,
            ),
            self.image_barrier(
                self.splat0_image
                    .as_ref()
                    .expect("groundcover bench images read before create_images")
                    .image,
                old,
                vk::ImageLayout::GENERAL,
                vk::AccessFlags::SHADER_READ,
                vk::AccessFlags::SHADER_WRITE,
            ),
            self.image_barrier(
                self.splat1_image
                    .as_ref()
                    .expect("groundcover bench images read before create_images")
                    .image,
                old,
                vk::ImageLayout::GENERAL,
                vk::AccessFlags::SHADER_READ,
                vk::AccessFlags::SHADER_WRITE,
            ),
        ];
        // SAFETY: `cmd` is recording. The source stage set covers every stage
        // that can be sampling these images — the graphics queue is the only
        // queue that touches them, and submissions on it execute in order, so
        // this also orders the bake against a *previous* frame's raster
        // variant that may still be in flight.
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::VERTEX_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &to_general,
            );
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.bake_pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.bake_pipeline_layout,
                0,
                &[self.bake_sets[frame]],
                &[],
            );
            let push = BakePush {
                cell_count: self.cells.len() as u32,
            };
            device.cmd_push_constants(
                cmd,
                self.bake_pipeline_layout,
                vk::ShaderStageFlags::COMPUTE,
                0,
                &push.cell_count.to_ne_bytes(),
            );
            // 8×8 local size over the LAND_GRID_VERTS² texel grid, one array
            // layer per resident cell.
            device.cmd_dispatch(
                cmd,
                LAND_GRID_VERTS.div_ceil(8),
                LAND_GRID_VERTS.div_ceil(8),
                self.cells.len() as u32,
            );
        }

        let to_read = [
            self.image_barrier(
                self.attr_image
                    .as_ref()
                    .expect("groundcover bench images read before create_images")
                    .image,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::AccessFlags::SHADER_WRITE,
                vk::AccessFlags::SHADER_READ,
            ),
            self.image_barrier(
                self.splat0_image
                    .as_ref()
                    .expect("groundcover bench images read before create_images")
                    .image,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::AccessFlags::SHADER_WRITE,
                vk::AccessFlags::SHADER_READ,
            ),
            self.image_barrier(
                self.splat1_image
                    .as_ref()
                    .expect("groundcover bench images read before create_images")
                    .image,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::AccessFlags::SHADER_WRITE,
                vk::AccessFlags::SHADER_READ,
            ),
        ];
        // SAFETY: `cmd` is recording; the barrier makes the bake's writes
        // visible to both consumers' reads.
        unsafe {
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::VERTEX_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &to_read,
            );
        }
    }

    fn record_scatter(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        path: usize,
        push: &BenchPush,
        chunk_count: u32,
    ) {
        // SAFETY: `cmd` is recording; the pipeline, layout and set are live
        // and the push range covers `BenchPush`'s size.
        unsafe {
            device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.scatter_pipelines[path],
            );
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.sample_pipeline_layout,
                0,
                &[self.sample_sets[frame]],
                &[],
            );
            device.cmd_push_constants(
                cmd,
                self.sample_pipeline_layout,
                vk::ShaderStageFlags::COMPUTE | vk::ShaderStageFlags::VERTEX,
                0,
                push_bytes(push),
            );
            // §4: one workgroup per chunk, mirroring `cluster_cull.comp`.
            device.cmd_dispatch(cmd, chunk_count, 1, 1);
        }
    }

    fn record_raster(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        path: usize,
        push: &BenchPush,
    ) {
        let Some(target) = self.raster_target.as_ref() else {
            return;
        };
        let vertex_count = push.chunk_count * push.blades_per_chunk * GROUNDCOVER_BENCH_BLADE_VERTS;
        let begin = vk::RenderPassBeginInfo::default()
            .render_pass(self.raster_render_pass)
            .framebuffer(target.framebuffer)
            .render_area(vk::Rect2D::default().extent(vk::Extent2D {
                width: RASTER_EXTENT,
                height: RASTER_EXTENT,
            }));
        // SAFETY: `cmd` is recording; the render pass, framebuffer, pipeline,
        // layout and descriptor set are all live, and the pass has no
        // attachment that needs a clear value (both ops are DONT_CARE).
        unsafe {
            device.cmd_begin_render_pass(cmd, &begin, vk::SubpassContents::INLINE);
            device.cmd_bind_pipeline(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.raster_pipelines[path],
            );
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.sample_pipeline_layout,
                0,
                &[self.sample_sets[frame]],
                &[],
            );
            device.cmd_push_constants(
                cmd,
                self.sample_pipeline_layout,
                vk::ShaderStageFlags::COMPUTE | vk::ShaderStageFlags::VERTEX,
                0,
                push_bytes(push),
            );
            device.cmd_draw(cmd, vertex_count, 1, 0, 0);
            device.cmd_end_render_pass(cmd);
        }
    }
}

fn push_bytes(push: &BenchPush) -> &[u8] {
    // SAFETY: `BenchPush` is `#[repr(C)]` over five 4-byte scalars with no
    // padding, so every byte of its 20-byte footprint is initialised. The
    // slice borrows `push` and cannot outlive it.
    unsafe {
        std::slice::from_raw_parts(
            (push as *const BenchPush).cast::<u8>(),
            std::mem::size_of::<BenchPush>(),
        )
    }
}

impl GroundcoverBench {
    pub fn stats(&self, variant: BenchVariant) -> VariantStats {
        self.stats[variant.index()]
    }

    pub fn bake_stats(&self) -> VariantStats {
        self.bake_stats
    }

    pub fn dropped_cells(&self) -> usize {
        self.dropped_cells
    }

    /// Has any variant produced at least one harvested frame?
    pub fn has_samples(&self) -> bool {
        self.stats.iter().any(|s| s.frames > 0)
    }

    /// One `groundcover-bench:` line per variant plus a bake line, in the
    /// same key=value shape the existing `bench:` summary uses so the rows
    /// can be grepped out of a `--bench-hold` log.
    ///
    /// `ns_sample` is the number §11.1 asks for, and `ns_sample_net` is that
    /// number with the consumer's own control subtracted — the cost of the
    /// *sampling*, with the candidate-point generation (scatter) and the
    /// per-vertex invocation + primitive assembly (raster) taken out. The
    /// control rows report a `ns_sample_net` of zero by construction.
    ///
    /// The `scatter/*` and `raster/*` rows are directly comparable to each
    /// other because both consumers are configured to issue the same number
    /// of samples per chunk (see [`DEFAULT_SAMPLES_PER_THREAD`]).
    pub fn report_lines(&self) -> Vec<String> {
        let scatter_floor = self.stats[BenchVariant::ScatterFloor.index()].ns_per_sample();
        let raster_floor = self.stats[BenchVariant::RasterFloor.index()].ns_per_sample();
        let mut lines = Vec::new();
        for variant in BenchVariant::ALL {
            let s = self.stats[variant.index()];
            let floor = if variant.path() == 2 {
                s.ns_per_sample()
            } else if variant.is_raster() {
                raster_floor
            } else {
                scatter_floor
            };
            lines.push(format!(
                "groundcover-bench: variant={} frames={} samples={} mean_ms={:.4} \
                 min_ms={:.4} max_ms={:.4} ns_sample={:.4} ns_sample_net={:.4}",
                variant.label(),
                s.frames,
                s.total_samples,
                s.mean_ms(),
                s.min_ms,
                s.max_ms,
                s.ns_per_sample(),
                (s.ns_per_sample() - floor).max(0.0),
            ));
        }
        let b = self.bake_stats;
        lines.push(format!(
            "groundcover-bench: variant=bake bakes={} texels={} mean_ms={:.4} \
             max_ms={:.4} ns_texel={:.3}",
            b.frames,
            b.total_samples,
            b.mean_ms(),
            b.max_ms,
            b.ns_per_sample(),
        ));
        if self.dropped_cells > 0 {
            lines.push(format!(
                "groundcover-bench: WARNING dropped_cells={} (resident ring exceeds \
                 MAX_BENCH_CELLS={MAX_BENCH_CELLS}); the numbers above cover a truncated ring",
                self.dropped_cells,
            ));
        }
        lines
    }

    /// # Safety
    ///
    /// No in-flight command buffer or descriptor set may still reference any
    /// object owned by this bench — the caller must have idled the device (or
    /// otherwise proven no outstanding GPU work touches them). `device` and
    /// `allocator` must be the ones every object here was created against.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        // Reverse creation order: pipelines and the framebuffer/render pass
        // reference the layouts, and the descriptor sets reference the images
        // and buffers.
        for pipeline in self
            .scatter_pipelines
            .iter_mut()
            .chain(self.raster_pipelines.iter_mut())
            .chain(std::iter::once(&mut self.bake_pipeline))
        {
            if *pipeline != vk::Pipeline::null() {
                // SAFETY: caller's contract — nothing in flight uses it.
                unsafe { device.destroy_pipeline(*pipeline, None) };
                *pipeline = vk::Pipeline::null();
            }
        }
        if let Some(mut target) = self.raster_target.take() {
            if target.framebuffer != vk::Framebuffer::null() {
                // SAFETY: caller's contract.
                unsafe { device.destroy_framebuffer(target.framebuffer, None) };
            }
            // #3860 — view, image and slab in one call, in that order.
            target.gpu.destroy(device, allocator);
        }
        if self.raster_render_pass != vk::RenderPass::null() {
            // SAFETY: the pipelines and framebuffer that referenced it are
            // already destroyed.
            unsafe { device.destroy_render_pass(self.raster_render_pass, None) };
            self.raster_render_pass = vk::RenderPass::null();
        }
        for layout in [
            &mut self.sample_pipeline_layout,
            &mut self.bake_pipeline_layout,
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
            self.sample_sets.clear();
            self.bake_sets.clear();
        }
        for layout in [&mut self.sample_set_layout, &mut self.bake_set_layout] {
            if *layout != vk::DescriptorSetLayout::null() {
                // SAFETY: the pipeline layouts and sets referencing it are gone.
                unsafe { device.destroy_descriptor_set_layout(*layout, None) };
                *layout = vk::DescriptorSetLayout::null();
            }
        }
        if self.sampler != vk::Sampler::null() {
            // SAFETY: the descriptor sets that held it are freed above.
            unsafe { device.destroy_sampler(self.sampler, None) };
            self.sampler = vk::Sampler::null();
        }
        // Caller's contract — no in-flight work references the images.
        // #3860 — no `unsafe` needed any more: `GpuImage::destroy` is a safe
        // fn whose own contract is the same one this caller already upholds.
        for image in [
            self.attr_image.as_mut(),
            self.splat0_image.as_mut(),
            self.splat1_image.as_mut(),
        ]
        .into_iter()
        .flatten()
        {
            image.destroy(device, allocator);
        }
        for buffer in self
            .cell_buffers
            .iter_mut()
            .chain(self.chunk_buffers.iter_mut())
            .chain(self.result_buffer.iter_mut())
        {
            buffer.destroy(device, allocator);
        }
        self.cell_buffers.clear();
        self.chunk_buffers.clear();
        self.result_buffer = None;
        self.images_initialised = false;
        self.baked_signature = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two consumers must issue the same number of samples per chunk, or
    /// the `ns_sample` columns of a `groundcover-bench:` report are not
    /// comparable and §4's store-vs-resample trade cannot be read off them.
    #[test]
    fn both_consumers_issue_the_same_samples_per_chunk() {
        let scatter = GROUNDCOVER_BENCH_WORKGROUP * DEFAULT_SAMPLES_PER_THREAD;
        let raster = DEFAULT_BLADES_PER_CHUNK * GROUNDCOVER_BENCH_BLADE_VERTS;
        assert_eq!(
            scatter, raster,
            "scatter issues {scatter} samples/chunk and raster {raster}; the defaults must \
             match so the per-sample numbers can be read side by side (#4052)"
        );
    }

    /// `RESULT_SLOTS - 1` is used as a mask in the shader, which is only a
    /// valid modulo for a power of two. A non-power-of-two would silently
    /// fold half the sink writes onto the same slots — harmless for the
    /// numbers, but the mask would stop meaning what its name says.
    #[test]
    fn result_slots_is_a_power_of_two() {
        assert!(RESULT_SLOTS.is_power_of_two());
    }

    /// The GPU structs are the shader contract. Both are 16 bytes with no
    /// padding, which is what makes the `NoUninit` impls sound and what
    /// std430 expects for a `vec2 + uint + uint` record.
    #[test]
    fn gpu_records_are_sixteen_bytes() {
        assert_eq!(std::mem::size_of::<GpuBenchCell>(), 16);
        assert_eq!(std::mem::size_of::<GpuBenchChunk>(), 16);
        assert_eq!(std::mem::align_of::<GpuBenchCell>(), 4);
        assert_eq!(std::mem::align_of::<GpuBenchChunk>(), 4);
    }

    /// `push_bytes` hands the driver `size_of::<BenchPush>()` bytes, and the
    /// push range is declared with the same number. Five 4-byte scalars, no
    /// padding — if a field were ever added at a different alignment the
    /// unsafe slice would start reading uninitialised padding.
    #[test]
    fn push_constants_have_no_padding() {
        assert_eq!(std::mem::size_of::<BenchPush>(), 20);
        assert_eq!(std::mem::size_of::<BakePush>(), 4);
    }

    /// §4 puts an 8×8 grid of 512-unit chunks in a 4096-unit exterior cell.
    /// A drift in either constant silently changes how much terrain the bench
    /// covers, which changes the sample count without changing the label.
    #[test]
    fn chunk_grid_tiles_the_exterior_cell_exactly() {
        let side = GROUNDCOVER_CHUNKS_PER_CELL_SIDE as f32 * GROUNDCOVER_CHUNK_UNITS;
        assert_eq!(side, crate::shader_constants::EXTERIOR_CELL_UNITS);
        assert_eq!(CHUNKS_PER_CELL, 64);
    }

    /// The round-robin has to visit every variant, or a `--bench-frames` run
    /// would silently report zero frames for whichever one it skipped.
    #[test]
    fn variant_indices_are_unique_and_dense() {
        let mut seen = [false; BenchVariant::ALL.len()];
        for variant in BenchVariant::ALL {
            let i = variant.index();
            assert!(!seen[i], "{} reuses index {i}", variant.label());
            seen[i] = true;
        }
        assert!(seen.iter().all(|&s| s));
    }

    /// Both stats are meaningless without frames; `mean_ms` / `ns_per_sample`
    /// must not divide by zero into a NaN that then poisons a report line.
    #[test]
    fn empty_stats_report_zero_not_nan() {
        let s = VariantStats::default();
        assert_eq!(s.mean_ms(), 0.0);
        assert_eq!(s.ns_per_sample(), 0.0);
    }

    #[test]
    fn stats_track_extremes_from_the_first_sample() {
        let mut s = VariantStats::default();
        s.record(2.0, 100);
        assert_eq!(s.min_ms, 2.0);
        assert_eq!(s.max_ms, 2.0);
        s.record(1.0, 100);
        s.record(3.0, 100);
        assert_eq!(s.min_ms, 1.0);
        assert_eq!(s.max_ms, 3.0);
        assert_eq!(s.frames, 3);
        assert!((s.mean_ms() - 2.0).abs() < 1e-9);
        // 6 ms over 300 samples = 20 µs = 20000 ns each.
        assert!((s.ns_per_sample() - 20_000.0).abs() < 1e-6);
    }
}
