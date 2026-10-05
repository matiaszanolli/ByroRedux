//! EXAL ground cover §12.12 Phase C — the authored-model tier (#4413).
//!
//! `docs/engine/exal-groundcover.md` §12.12. Every `GRAS` record draws its
//! own model — grass cards, rocks, ferns, leaf decals, kelp — at points the
//! §3 density field accepts on the game's own grass grid, weighted by the
//! worldspace climate and filtered by each record's authored water rule.
//!
//! ## Drawn by the main pass
//!
//! The models are ordinary meshes with ordinary materials, so they are drawn
//! by the main geometry pipeline rather than a pipeline of their own: the
//! `groundcover_models.comp` dispatch writes each placed shape as a
//! `GpuInstance` into the **tail of the main instance buffer**, after the
//! frame's own instance list, and the geometry pass issues one indexed
//! indirect draw per shape. Cards alpha-test and rocks shade as rock through
//! the same `triangle.frag` material path every placed object uses, and the
//! instance indices stay unique, which the fragment shader's self-hit test
//! and temporal surface identity rely on. Receive-only, like the blades: no
//! TLAS entry.
//!
//! Tail slots cannot be known when the blade scatter is recorded — the main
//! instance count is fixed later, by `build_and_upload_instances` — so this
//! tier dispatches after that upload and before the geometry pass.
//!
//! ## Frame
//!
//! 1. [`GroundCoverModelTier::prepare`] uploads the records, the selection
//!    table and the shapes (each resolved from its template's `DrawCommand`,
//!    re-read every frame because `MeshRegistry` compacts).
//! 2. [`GroundCoverModelTier::record`] runs the three compute phases (see the
//!    shader's header) against the blade scatter's chunk and cell records.
//! 3. The geometry pass draws [`GroundCoverModelTier::shape_draws`].

use anyhow::{Context, Result};
use ash::vk;

use super::allocator::SharedAllocator;
use super::buffer::{GpuBuffer, NoUninit, byte_view};
use super::reflect::{validate_set_layout, ReflectedShader};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use crate::shader_constants::{
    GROUNDCOVER_MAX_CHUNKS, GROUNDCOVER_MODEL_MAX_INSTANCES, GROUNDCOVER_MODEL_MAX_RECORDS,
    GROUNDCOVER_MODEL_MAX_SHAPES, GROUNDCOVER_MODEL_PHASE_EMIT, GROUNDCOVER_MODEL_PHASE_LAYOUT,
    GROUNDCOVER_MODEL_PHASE_PLACE, GROUNDCOVER_MODEL_POINTS_PER_CHUNK,
    GROUNDCOVER_MODEL_SHAPE_FLAG_NON_UNIFORM, GROUNDCOVER_MODEL_STATS_REGION,
    GROUNDCOVER_MODEL_STATS_WORDS, GROUNDCOVER_SPECIES_TABLE_SIZE,
};

const MODELS_SPV: &[u8] = include_bytes!("../../shaders/groundcover_models.comp.spv");

/// One authored record. Mirrors `GcModelRecord` in `groundcover_models.comp`.
/// The host fills everything but `shape_first` / `shape_count`, which
/// [`GroundCoverModelTier::prepare`] derives from the shape list.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct GpuGroundCoverModelRecord {
    pub density: f32,
    pub water_rule: u32,
    pub water_distance: f32,
    pub height_range: f32,
    pub position_range: f32,
    /// `GROUNDCOVER_MODEL_RECORD_FLAG_*` bits.
    pub flags: u32,
    pub shape_first: u32,
    pub shape_count: u32,
    /// Height of the placed-geometry cover test's span.
    pub cover_reach: f32,
    pub pad0: u32,
    pub pad1: u32,
    pub pad2: u32,
}
// SAFETY: `#[repr(C)]` over 4-byte scalars, 48 bytes, padding named and
// initialised by every constructor.
unsafe impl NoUninit for GpuGroundCoverModelRecord {}

/// One shape of a record's model. Mirrors `GcModelShape`.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
struct GpuGroundCoverModelShape {
    local: [[f32; 4]; 4],
    record: u32,
    material_id: u32,
    texture_index: u32,
    vertex_offset: u32,
    index_offset: u32,
    index_count: u32,
    vertex_count: u32,
    flags: u32,
    avg_albedo_r: f32,
    avg_albedo_g: f32,
    avg_albedo_b: f32,
    ior: f32,
}
// SAFETY: `#[repr(C)]` over 4-byte scalars, 112 bytes (a multiple of the
// struct's 16-byte std430 alignment), no implicit padding.
unsafe impl NoUninit for GpuGroundCoverModelShape {}

/// One placement point in the slab. Mirrors `GcModelPoint` in
/// `groundcover_models.comp`.
///
/// No host code reads or writes one — the shader fills the slab and reads it
/// back. The mirror exists so the slab is sized from `size_of` rather than from
/// a literal stride that had to match the GLSL record by hand (#4849), the same
/// arrangement `GpuGroundCoverBlade` has for the blade buffer (#4335):
/// `name_diverging_glsl_rust_mirrors_stay_in_lockstep` fails until both sides
/// agree, instead of the placement pass writing past the slab.
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct GpuGroundCoverModelPoint {
    /// xyz = absolute Y-up world position of the root.
    pub position: [f32; 4],
    /// x = record | (candidate << `GROUNDCOVER_MODEL_RECORD_BITS`), y = rank
    /// among the chunk's points of that record, z = packSnorm2x16(terrain
    /// normal .xz), w = candidate hash.
    pub meta: [u32; 4],
}

/// Bytes of one indirect draw. The geometry pass strides by the same
/// `size_of`, and `name_diverging_glsl_rust_mirrors_stay_in_lockstep` pins the
/// shader's `GcDrawIndexed` to it.
const DRAW_STRIDE: u64 = std::mem::size_of::<vk::DrawIndexedIndirectCommand>() as u64;
/// `gcCounts` length — the shared region layout plus the stats words.
const STATS_WORDS: u64 = GROUNDCOVER_MODEL_STATS_WORDS as u64;
const COUNT_WORDS: u64 = GROUNDCOVER_MODEL_STATS_REGION as u64 + STATS_WORDS;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ModelPush {
    camera_pos: [f32; 4],
    render_origin_enable: [f32; 4],
    phase: u32,
    chunk_count: u32,
    record_count: u32,
    shape_count: u32,
    tail_base: u32,
    tail_capacity: u32,
    grid_spacing: f32,
    pad: u32,
}
// SAFETY: `#[repr(C)]` over 4-byte scalars, 64 bytes, the one padding word
// named (`pad`) and initialised by every constructor; `host_mirrors_match_
// the_shader_strides` pins the size. Adding a field that breaks the tiling
// now fails the `NoUninit` gate here instead of pushing uninitialised bytes
// into `vkCmdPushConstants` (#5122).
unsafe impl NoUninit for ModelPush {}

/// Everything the tier needs from the host for one frame.
pub struct GroundCoverModelFrame<'a> {
    pub records: &'a [GpuGroundCoverModelRecord],
    /// Record index per selection-table entry, in proportion to climate weight.
    pub record_table: &'a [u32],
    /// One entry per template shape, sorted by record: the record it belongs
    /// to and its fully built `DrawCommand` (material interned, mesh and
    /// texture resolved, `model_matrix` = the shape's model-root-local
    /// transform). The host's own template list, passed as is (#4922).
    pub shapes: &'a [(u32, super::context::DrawCommand)],
    pub grid_spacing: f32,
}

/// Raster state the geometry pass applies before a shape's indirect draw —
/// the same state a `DrawBatch` carries for an ordinary draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelShapeDraw {
    pub two_sided: bool,
    pub z_test: bool,
    pub z_write: bool,
    pub z_function: u8,
    pub render_layer: byroredux_core::ecs::components::RenderLayer,
}

/// Placement totals of the most recent frame that dispatched the tier, read
/// back one pipelined frame late. Any frame whose `prepare` finds nothing to
/// place (no records, cover off, upload failure) zeroes the struct, so the
/// numbers never outlive the placement that produced them (#5220) — an
/// interior or a cover-less frame reports 0/0 rather than the last
/// exterior's counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GroundCoverModelStats {
    /// Instances the placed plants asked for.
    pub demanded: u32,
    /// Instances written — `demanded` past the tail budget is truncated.
    pub emitted: u32,
}

pub struct GroundCoverModelTier {
    set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    descriptor_pool: vk::DescriptorPool,
    sets: Vec<vk::DescriptorSet>,
    record_buffers: Vec<GpuBuffer>,
    table_buffers: Vec<GpuBuffer>,
    shape_buffers: Vec<GpuBuffer>,
    stats_readback: Vec<GpuBuffer>,
    point_buffer: Option<GpuBuffer>,
    count_buffer: Option<GpuBuffer>,
    draw_buffer: Option<GpuBuffer>,
    frame_record_count: u32,
    frame_shape_count: u32,
    frame_grid_spacing: f32,
    frame_draws: Vec<ModelShapeDraw>,
    /// Whether the frame's dispatch was recorded, so the geometry pass draws
    /// only what this frame wrote.
    frame_recorded: bool,
    pending_stats: [bool; MAX_FRAMES_IN_FLIGHT],
    stats: GroundCoverModelStats,
    /// #4920 — latched while placement is over the tail budget, so the
    /// truncation is logged once per episode rather than every frame.
    overflow_logged: bool,
    /// #4922 — `prepare`'s per-frame record and shape lists, kept across
    /// frames rather than rebuilt from fresh allocations.
    records_scratch: Vec<GpuGroundCoverModelRecord>,
    shapes_scratch: Vec<GpuGroundCoverModelShape>,
}

/// Bytes every buffer the tier allocates, for `memory-budget.md`.
struct ModelBufferBytes {
    records: u64,
    table: u64,
    shapes: u64,
    stats: u64,
    points: u64,
    counts: u64,
    draws: u64,
}

impl ModelBufferBytes {
    const fn new() -> Self {
        Self {
            records: GROUNDCOVER_MODEL_MAX_RECORDS as u64
                * std::mem::size_of::<GpuGroundCoverModelRecord>() as u64,
            table: GROUNDCOVER_SPECIES_TABLE_SIZE as u64 * 4,
            shapes: GROUNDCOVER_MODEL_MAX_SHAPES as u64
                * std::mem::size_of::<GpuGroundCoverModelShape>() as u64,
            stats: STATS_WORDS * 4,
            points: GROUNDCOVER_MAX_CHUNKS as u64
                * GROUNDCOVER_MODEL_POINTS_PER_CHUNK as u64
                * std::mem::size_of::<GpuGroundCoverModelPoint>() as u64,
            counts: COUNT_WORDS * 4,
            draws: GROUNDCOVER_MODEL_MAX_SHAPES as u64 * DRAW_STRIDE,
        }
    }
}

/// Every buffer the authored-model tier allocates, in bytes: the per-slot
/// host-visible record / table / shape uploads and stats readback ×
/// `MAX_FRAMES_IN_FLIGHT`, plus the shared device-local placement slab,
/// counters and indirect draws. The instance-tail growth it causes in the
/// main instance SSBOs is ledgered with those. Ledgered in
/// `docs/engine/memory-budget.md`.
pub const fn groundcover_model_resident_bytes() -> u64 {
    let b = ModelBufferBytes::new();
    MAX_FRAMES_IN_FLIGHT as u64 * (b.records + b.table + b.shapes + b.stats)
        + b.points
        + b.counts
        + b.draws
}

/// Instance slots the tier asks the main instance buffers to hold beyond the
/// frame's own list.
pub const fn groundcover_model_tail_budget() -> usize {
    GROUNDCOVER_MODEL_MAX_INSTANCES as usize
}

fn set_layout_bindings() -> Vec<vk::DescriptorSetLayoutBinding<'static>> {
    let compute = vk::ShaderStageFlags::COMPUTE;
    (0..12)
        .map(|binding| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(binding)
                .descriptor_type(if binding == 3 {
                    vk::DescriptorType::ACCELERATION_STRUCTURE_KHR
                } else {
                    vk::DescriptorType::STORAGE_BUFFER
                })
                .descriptor_count(1)
                .stage_flags(compute)
        })
        .collect()
}

fn validate_layout(bindings: &[vk::DescriptorSetLayoutBinding<'_>]) -> Result<()> {
    validate_set_layout(
        0,
        bindings,
        &[ReflectedShader {
            name: "groundcover_models.comp",
            spirv: MODELS_SPV,
        }],
        "ground-cover model tier",
        &[],
    )
}

impl GroundCoverModelTier {
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
    ) -> Result<Self> {
        let mut this = Self {
            set_layout: vk::DescriptorSetLayout::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            sets: Vec::new(),
            record_buffers: Vec::new(),
            table_buffers: Vec::new(),
            shape_buffers: Vec::new(),
            stats_readback: Vec::new(),
            point_buffer: None,
            count_buffer: None,
            draw_buffer: None,
            frame_record_count: 0,
            frame_shape_count: 0,
            frame_grid_spacing: 0.0,
            frame_draws: Vec::new(),
            frame_recorded: false,
            pending_stats: [false; MAX_FRAMES_IN_FLIGHT],
            stats: GroundCoverModelStats::default(),
            overflow_logged: false,
            records_scratch: Vec::new(),
            shapes_scratch: Vec::new(),
        };
        if let Err(error) = this.create(device, allocator, pipeline_cache) {
            // SAFETY: nothing created so far has reached a queue.
            unsafe { this.destroy(device, allocator) };
            return Err(error);
        }
        Ok(this)
    }

    fn create(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        pipeline_cache: vk::PipelineCache,
    ) -> Result<()> {
        let bytes = ModelBufferBytes::new();
        let host = vk::BufferUsageFlags::STORAGE_BUFFER;
        for _ in 0..MAX_FRAMES_IN_FLIGHT {
            self.record_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                bytes.records,
                host,
            )?);
            self.table_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                bytes.table,
                host,
            )?);
            self.shape_buffers.push(GpuBuffer::create_host_visible(
                device,
                allocator,
                bytes.shapes,
                host,
            )?);
            self.stats_readback.push(GpuBuffer::create_host_readback(
                device,
                allocator,
                bytes.stats,
                vk::BufferUsageFlags::TRANSFER_DST,
            )?);
        }
        self.point_buffer = Some(GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            bytes.points,
            vk::BufferUsageFlags::STORAGE_BUFFER,
        )?);
        self.count_buffer = Some(GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            bytes.counts,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_SRC,
        )?);
        self.draw_buffer = Some(GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            bytes.draws,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::INDIRECT_BUFFER,
        )?);

        let bindings = set_layout_bindings();
        validate_layout(&bindings)
            .expect("ground-cover model tier layout drifted against its shader");
        // SAFETY: `bindings` outlives the call; the layout is owned here.
        self.set_layout = unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("create ground-cover model set layout")?
        };
        let ranges = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .offset(0)
            .size(std::mem::size_of::<ModelPush>() as u32)];
        let layouts = [self.set_layout];
        // SAFETY: both slices outlive the call.
        self.pipeline_layout = unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(&layouts)
                        .push_constant_ranges(&ranges),
                    None,
                )
                .context("create ground-cover model pipeline layout")?
        };

        let frames = MAX_FRAMES_IN_FLIGHT as u32;
        let sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(11 * frames),
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
                        .max_sets(frames),
                    None,
                )
                .context("create ground-cover model descriptor pool")?
        };
        let set_layouts = vec![self.set_layout; MAX_FRAMES_IN_FLIGHT];
        // SAFETY: the pool was sized for these sets; the slice outlives the call.
        self.sets = unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(self.descriptor_pool)
                        .set_layouts(&set_layouts),
                )
                .context("allocate ground-cover model sets")?
        };

        // SAFETY: `MODELS_SPV` is a checked-in SPIR-V blob; the module is
        // destroyed right after pipeline creation below.
        let module = unsafe {
            device
                .create_shader_module(
                    &vk::ShaderModuleCreateInfo::default()
                        .code(&ash::util::read_spv(&mut std::io::Cursor::new(MODELS_SPV))?),
                    None,
                )
                .context("create ground-cover model shader module")?
        };
        let entry = c"main";
        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(module)
            .name(entry);
        let info = vk::ComputePipelineCreateInfo::default()
            .stage(stage)
            .layout(self.pipeline_layout);
        // SAFETY: the create-info's borrows outlive the call; layout and
        // module are live.
        let result = unsafe {
            device
                .create_compute_pipelines(pipeline_cache, &[info], None)
                .map_err(|(_, e)| e)
                .context("create ground-cover model pipeline")
        };
        // SAFETY: pipeline creation has returned; the module may go.
        unsafe { device.destroy_shader_module(module, None) };
        self.pipeline = result?[0];
        Ok(())
    }

    /// Upload the frame's records, table and shapes, resolving each shape's
    /// mesh through the live registry. Returns whether the tier has anything
    /// to place this frame.
    pub fn prepare(
        &mut self,
        device: &ash::Device,
        frame: usize,
        mesh_registry: &crate::mesh::MeshRegistry,
        texture_registry: &crate::texture_registry::TextureRegistry,
        input: &GroundCoverModelFrame<'_>,
    ) -> bool {
        // # fence contract (#4851): this runs before draw_frame's all-slots
        // wait, so the prior draw_frame's wait must have retired every slot
        // before harvest reads stats_readback[frame] or these host-visible
        // buffers are rewritten. Do not narrow that wait until this tier is
        // made per-FIF or its old resources are deferred-destroyed.
        self.harvest(device, frame);
        self.frame_recorded = false;
        self.frame_record_count = 0;
        self.frame_shape_count = 0;
        self.frame_draws.clear();
        let record_count = input
            .records
            .len()
            .min(GROUNDCOVER_MODEL_MAX_RECORDS as usize);
        // NaN-rejecting: a NaN spacing fails `> 0.0` and must bail too.
        if record_count == 0 || input.grid_spacing.is_nan() || input.grid_spacing <= 0.0 {
            // #5220 — nothing to place this frame: drop the last placement's
            // counts so a `stats` reader sees this frame's truth (0/0 in an
            // interior or with cover off) rather than a latched exterior
            // number. AFTER `harvest`, so a dispatch that did fire two frames
            // ago is still consumed first.
            self.stats = GroundCoverModelStats::default();
            return false;
        }
        let mut records = std::mem::take(&mut self.records_scratch);
        records.clear();
        records.extend_from_slice(&input.records[..record_count]);
        for record in &mut records {
            record.shape_first = 0;
            record.shape_count = 0;
        }
        let mut shapes = std::mem::take(&mut self.shapes_scratch);
        shapes.clear();
        for (shape_record, draw) in input.shapes {
            if shapes.len() == GROUNDCOVER_MODEL_MAX_SHAPES as usize {
                break;
            }
            let Some(record) = records.get_mut(*shape_record as usize) else {
                continue;
            };
            let Some(mesh) = mesh_registry.get(draw.mesh_handle) else {
                continue;
            };
            // Shapes arrive sorted by record, so a record's shapes are
            // contiguous and its first shape fixes the run's start.
            if record.shape_count == 0 {
                record.shape_first = shapes.len() as u32;
            } else if record.shape_first + record.shape_count != shapes.len() as u32 {
                continue;
            }
            record.shape_count += 1;
            let m = draw.model_matrix;
            let column_len_sq = |c: usize| m[c] * m[c] + m[c + 1] * m[c + 1] + m[c + 2] * m[c + 2];
            let non_uniform = (column_len_sq(0) - column_len_sq(4)).abs() > 0.001
                || (column_len_sq(0) - column_len_sq(8)).abs() > 0.001;
            let mean = texture_registry.handle_avg_rgb(draw.texture_handle);
            let albedo = match mean {
                Some(mean) => [
                    draw.avg_albedo[0] * mean[0],
                    draw.avg_albedo[1] * mean[1],
                    draw.avg_albedo[2] * mean[2],
                ],
                None => draw.avg_albedo,
            };
            shapes.push(GpuGroundCoverModelShape {
                // Column-major, as `DrawCommand::model_matrix` and a GLSL
                // `mat4` both store it.
                local: [
                    [m[0], m[1], m[2], m[3]],
                    [m[4], m[5], m[6], m[7]],
                    [m[8], m[9], m[10], m[11]],
                    [m[12], m[13], m[14], m[15]],
                ],
                record: *shape_record,
                material_id: draw.material_id,
                texture_index: draw.texture_handle,
                vertex_offset: mesh.global_vertex_offset,
                index_offset: mesh.global_index_offset,
                index_count: mesh.index_count,
                vertex_count: mesh.vertex_count,
                flags: if non_uniform {
                    GROUNDCOVER_MODEL_SHAPE_FLAG_NON_UNIFORM
                } else {
                    0
                } | shape_instance_flags(draw.render_layer, draw.flat_shading),
                avg_albedo_r: albedo[0],
                avg_albedo_g: albedo[1],
                avg_albedo_b: albedo[2],
                ior: draw.ior,
            });
            self.frame_draws.push(ModelShapeDraw {
                two_sided: draw.two_sided,
                z_test: draw.z_test,
                z_write: draw.z_write,
                z_function: draw.z_function,
                render_layer: draw.render_layer,
            });
        }
        let ready = !shapes.is_empty() && {
            let mut table = [0u32; GROUNDCOVER_SPECIES_TABLE_SIZE as usize];
            let len = input.record_table.len().min(table.len());
            table[..len].copy_from_slice(&input.record_table[..len]);
            let uploads = self.record_buffers[frame]
                .write_mapped(device, &records)
                .and_then(|()| self.table_buffers[frame].write_mapped(device, &table))
                .and_then(|()| self.shape_buffers[frame].write_mapped(device, &shapes));
            if let Err(error) = &uploads {
                log::warn!("ground-cover model tier: upload failed: {error}");
            }
            uploads.is_ok()
        };
        if ready {
            self.frame_record_count = records.len() as u32;
            self.frame_shape_count = shapes.len() as u32;
            self.frame_grid_spacing = input.grid_spacing;
        } else {
            self.frame_draws.clear();
            // #5220 — shapes-empty or upload-failed is also "nothing placed
            // this frame": same un-latch as the early return above.
            self.stats = GroundCoverModelStats::default();
        }
        self.records_scratch = records;
        self.shapes_scratch = shapes;
        ready
    }

    fn harvest(&mut self, device: &ash::Device, frame: usize) {
        if !std::mem::take(&mut self.pending_stats[frame]) {
            return;
        }
        let buffer = &mut self.stats_readback[frame];
        if buffer.invalidate_if_needed(device).is_err() {
            return;
        }
        let Ok(bytes) = buffer.mapped_slice_mut() else {
            return;
        };
        let word = |i: usize| {
            u32::from_ne_bytes([
                bytes[4 * i],
                bytes[4 * i + 1],
                bytes[4 * i + 2],
                bytes[4 * i + 3],
            ])
        };
        self.stats = GroundCoverModelStats {
            demanded: word(0),
            emitted: word(1),
        };
        // #4920 — a truncated placement is visible without `--bench-*`.
        let truncated = self.stats.demanded > self.stats.emitted;
        if truncated && !self.overflow_logged {
            log::warn!(
                "ground-cover model tier: placed plants need {} instances but the tail \
                 budget is {} — every record keeps the same share of whole plants \
                 ({} emitted; logged once per episode)",
                self.stats.demanded,
                groundcover_model_tail_budget(),
                self.stats.emitted,
            );
        }
        self.overflow_logged = truncated;
    }

    /// Forget the previous frame's recording. Called at the top of every
    /// frame's tier recording, so a frame that records nothing — an interior,
    /// a skipped scatter — cannot draw the last frame's indirect commands
    /// against this frame's instance buffer.
    pub fn clear_frame(&mut self) {
        self.frame_recorded = false;
    }

    /// Instance slots the frame's main instance buffers must hold beyond
    /// their own list: the tail budget while there is anything to place.
    pub fn tail_request(&self) -> usize {
        if self.frame_shape_count > 0 {
            groundcover_model_tail_budget()
        } else {
            0
        }
    }

    pub fn stats(&self) -> GroundCoverModelStats {
        self.stats
    }

    /// `(len, capacity, element bytes)` of the records and shapes scratch
    /// (#4922), for the `ctx.scratch` telemetry rows (#4610).
    pub fn scratch_telemetry(&self) -> [(usize, usize, usize); 2] {
        [
            (
                self.records_scratch.len(),
                self.records_scratch.capacity(),
                std::mem::size_of::<GpuGroundCoverModelRecord>(),
            ),
            (
                self.shapes_scratch.len(),
                self.shapes_scratch.capacity(),
                std::mem::size_of::<GpuGroundCoverModelShape>(),
            ),
        ]
    }

    /// Record the three phases. Must be outside a render pass, after the
    /// frame's instance upload fixed `tail_base`, and before the geometry
    /// pass. `instance_buffer` / `previous_model_buffer` are this frame
    /// slot's main-pass buffers, holding `tail_base + tail_capacity`
    /// entries at least.
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame: usize,
        scatter: ModelScatterInputs,
        instance_buffer: vk::Buffer,
        previous_model_buffer: vk::Buffer,
        tail_base: u32,
        tail_capacity: u32,
        mut timers: Option<&mut super::gpu_timers::GpuPerFrameTimers>,
    ) {
        if self.frame_shape_count == 0 || scatter.chunk_count == 0 || tail_capacity == 0 {
            return;
        }
        let Some(tlas) = scatter.tlas else {
            return;
        };
        let points = self.point_buffer.as_ref().expect("created in new()");
        let counts = self.count_buffer.as_ref().expect("created in new()");
        let draws = self.draw_buffer.as_ref().expect("created in new()");
        let info = |buffer: vk::Buffer| {
            [vk::DescriptorBufferInfo::default()
                .buffer(buffer)
                .range(vk::WHOLE_SIZE)]
        };
        let buffers = [
            (0, info(scatter.chunk_buffer)),
            (1, info(scatter.cell_buffer)),
            (2, info(scatter.vertex_buffer)),
            (4, info(self.record_buffers[frame].buffer)),
            (5, info(self.table_buffers[frame].buffer)),
            (6, info(self.shape_buffers[frame].buffer)),
            (7, info(points.buffer)),
            (8, info(counts.buffer)),
            (9, info(draws.buffer)),
            (10, info(instance_buffer)),
            (11, info(previous_model_buffer)),
        ];
        let set = self.sets[frame];
        let accel_structs = [tlas];
        let mut accel_write = vk::WriteDescriptorSetAccelerationStructureKHR::default()
            .acceleration_structures(&accel_structs);
        let tlas_write =
            crate::vulkan::descriptors::write_acceleration_structure(set, 3, &mut accel_write);
        // #4922 — a fixed array: this runs every frame the tier draws.
        let writes: [vk::WriteDescriptorSet; 12] = std::array::from_fn(|i| match buffers.get(i) {
            Some((binding, info)) => vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(*binding)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(info),
            None => tlas_write,
        });
        // SAFETY: every info outlives the call, and only slot `frame`'s set is
        // written — the caller has waited that slot's fence, so nothing in
        // flight reads it. Rewritten every frame because the main instance
        // buffers are replaced when they grow.
        unsafe { device.update_descriptor_sets(&writes, &[]) };

        let mut push = ModelPush {
            camera_pos: [
                scatter.camera_pos[0],
                scatter.camera_pos[1],
                scatter.camera_pos[2],
                0.0,
            ],
            render_origin_enable: [
                scatter.render_origin[0],
                scatter.render_origin[1],
                scatter.render_origin[2],
                1.0,
            ],
            phase: GROUNDCOVER_MODEL_PHASE_PLACE,
            chunk_count: scatter.chunk_count,
            record_count: self.frame_record_count,
            shape_count: self.frame_shape_count,
            tail_base,
            tail_capacity: tail_capacity.min(GROUNDCOVER_MODEL_MAX_INSTANCES),
            grid_spacing: self.frame_grid_spacing,
            pad: 0,
        };
        if let Some(timers) = timers.as_deref_mut() {
            timers.cmd_groundcover_models_start(device, cmd, frame);
        }
        // SAFETY: `cmd` is recording outside a render pass; the pipeline,
        // layout, set and every bound buffer are live for the frame.
        unsafe {
            // The slab, counters and draws are shared across frames in
            // flight: order this frame's writes after every earlier frame's
            // reads of them (the indirect draw and the emit phase).
            barrier(
                device,
                cmd,
                vk::PipelineStageFlags::DRAW_INDIRECT
                    | vk::PipelineStageFlags::COMPUTE_SHADER
                    | vk::PipelineStageFlags::TRANSFER,
                vk::AccessFlags::INDIRECT_COMMAND_READ
                    | vk::AccessFlags::SHADER_READ
                    | vk::AccessFlags::TRANSFER_READ,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::AccessFlags::SHADER_WRITE | vk::AccessFlags::SHADER_READ,
            );
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline_layout,
                0,
                &[set],
                &[],
            );
            for (phase, groups) in [
                (GROUNDCOVER_MODEL_PHASE_PLACE, scatter.chunk_count),
                (GROUNDCOVER_MODEL_PHASE_LAYOUT, 1),
                (GROUNDCOVER_MODEL_PHASE_EMIT, scatter.chunk_count),
            ] {
                push.phase = phase;
                device.cmd_push_constants(
                    cmd,
                    self.pipeline_layout,
                    vk::ShaderStageFlags::COMPUTE,
                    0,
                    // #5122 — the sanctioned `T → &[u8]` path: `ModelPush:
                    // NoUninit` is the audited padding proof.
                    byte_view(std::slice::from_ref(&push)),
                );
                device.cmd_dispatch(cmd, groups, 1, 1);
                barrier(
                    device,
                    cmd,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::AccessFlags::SHADER_WRITE,
                    if phase == GROUNDCOVER_MODEL_PHASE_EMIT {
                        vk::PipelineStageFlags::DRAW_INDIRECT
                            | vk::PipelineStageFlags::VERTEX_SHADER
                            | vk::PipelineStageFlags::FRAGMENT_SHADER
                            | vk::PipelineStageFlags::TRANSFER
                    } else {
                        vk::PipelineStageFlags::COMPUTE_SHADER
                    },
                    if phase == GROUNDCOVER_MODEL_PHASE_EMIT {
                        vk::AccessFlags::INDIRECT_COMMAND_READ
                            | vk::AccessFlags::SHADER_READ
                            | vk::AccessFlags::TRANSFER_READ
                    } else {
                        vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE
                    },
                );
            }
            device.cmd_copy_buffer(
                cmd,
                counts.buffer,
                self.stats_readback[frame].buffer,
                &[vk::BufferCopy::default()
                    .src_offset(GROUNDCOVER_MODEL_STATS_REGION as u64 * 4)
                    .size(STATS_WORDS * 4)],
            );
        }
        if let Some(timers) = timers {
            timers.cmd_groundcover_models_end(device, cmd, frame);
        }
        self.pending_stats[frame] = true;
        self.frame_recorded = true;
    }

    /// The indirect buffer and, per shape, its raster state — empty when this
    /// frame recorded no placement. Draw `i` is at byte `i * 20`.
    pub fn shape_draws(&self) -> Option<(vk::Buffer, &[ModelShapeDraw])> {
        if !self.frame_recorded || self.frame_draws.is_empty() {
            return None;
        }
        let draws = self.draw_buffer.as_ref()?;
        Some((draws.buffer, &self.frame_draws))
    }

    /// # Safety
    ///
    /// No in-flight command buffer may reference any object owned here.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        if self.pipeline != vk::Pipeline::null() {
            // SAFETY: caller's contract — nothing in flight uses it.
            unsafe { device.destroy_pipeline(self.pipeline, None) };
            self.pipeline = vk::Pipeline::null();
        }
        if self.pipeline_layout != vk::PipelineLayout::null() {
            // SAFETY: the pipeline built against it is destroyed above.
            unsafe { device.destroy_pipeline_layout(self.pipeline_layout, None) };
            self.pipeline_layout = vk::PipelineLayout::null();
        }
        if self.descriptor_pool != vk::DescriptorPool::null() {
            // SAFETY: destroying the pool frees its sets; nothing binds them.
            unsafe { device.destroy_descriptor_pool(self.descriptor_pool, None) };
            self.descriptor_pool = vk::DescriptorPool::null();
            self.sets.clear();
        }
        if self.set_layout != vk::DescriptorSetLayout::null() {
            // SAFETY: the pipeline layout and sets referencing it are gone.
            unsafe { device.destroy_descriptor_set_layout(self.set_layout, None) };
            self.set_layout = vk::DescriptorSetLayout::null();
        }
        for buffer in self
            .record_buffers
            .iter_mut()
            .chain(self.table_buffers.iter_mut())
            .chain(self.shape_buffers.iter_mut())
            .chain(self.stats_readback.iter_mut())
            .chain(self.point_buffer.iter_mut())
            .chain(self.count_buffer.iter_mut())
            .chain(self.draw_buffer.iter_mut())
        {
            buffer.destroy(device, allocator);
        }
        self.record_buffers.clear();
        self.table_buffers.clear();
        self.shape_buffers.clear();
        self.stats_readback.clear();
        self.point_buffer = None;
        self.count_buffer = None;
        self.draw_buffer = None;
    }
}

/// The blade scatter's per-frame state the placement phase reads.
#[derive(Clone, Copy)]
pub struct ModelScatterInputs {
    pub chunk_buffer: vk::Buffer,
    pub cell_buffer: vk::Buffer,
    pub vertex_buffer: vk::Buffer,
    pub chunk_count: u32,
    pub camera_pos: [f32; 3],
    pub render_origin: [f32; 3],
    pub tlas: Option<vk::AccelerationStructureKHR>,
}

/// The rasterizer flags a main-pass instance of `draw` would carry that do
/// not depend on its transform (see `build_and_upload_instances`): flat
/// shading, and the render layer's debug bits. Alpha blending is never set:
/// the tier draws with the opaque pipeline, alpha-testing its cards.
fn shape_instance_flags(
    render_layer: byroredux_core::ecs::components::RenderLayer,
    flat_shading: bool,
) -> u32 {
    use super::scene_buffer::{
        INSTANCE_FLAG_FLAT_SHADING, INSTANCE_RENDER_LAYER_MASK, INSTANCE_RENDER_LAYER_SHIFT,
    };
    let mut flags =
        (render_layer as u32 & INSTANCE_RENDER_LAYER_MASK) << INSTANCE_RENDER_LAYER_SHIFT;
    if flat_shading {
        flags |= INSTANCE_FLAG_FLAT_SHADING;
    }
    flags
}

fn barrier(
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
    // SAFETY: `cmd` is recording and `barrier` outlives the call.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// #4847 — the model emitter must preserve every host-packed draw flag
    /// except the non-uniform bit, which is recomputed from placement scale.
    #[test]
    fn model_emission_preserves_flat_shading_and_render_layer_flags() {
        let shader = include_str!("../../shaders/groundcover_models.comp");
        let emit = shader
            .split_once("void emit(")
            .expect("groundcover model emit function")
            .1
            .split_once("void main()")
            .expect("emit must end before main")
            .0;
        assert!(emit.contains("shape.flags & ~GROUNDCOVER_MODEL_SHAPE_FLAG_NON_UNIFORM"));
        assert!(emit.contains("INSTANCE_FLAG_NON_UNIFORM_SCALE"));
        // #4923 — the flags must be built FROM the masked shape flags, not
        // assigned from the scale bit alone (a third assert here used to be
        // `!a || b` with `b` already asserted above: always true).
        assert!(
            emit.contains("inst.flags = (shape.flags & ~GROUNDCOVER_MODEL_SHAPE_FLAG_NON_UNIFORM)"),
            "the emitted flags must start from the host-packed shape flags"
        );
    }

    /// #4923 — `gcWaterAdmits` switches on generated constants, and those
    /// constants are the translated rule's own `gpu_code`s: a reordered
    /// `CoverWaterRule` or a hand-edited literal now fails here instead of
    /// silently swapping "above" for "below".
    #[test]
    fn water_rule_codes_match_the_translated_rule() {
        use crate::shader_constants::{
            GROUNDCOVER_WATER_RULE_ABOVE_AT_LEAST, GROUNDCOVER_WATER_RULE_ABOVE_AT_MOST,
            GROUNDCOVER_WATER_RULE_BELOW_AT_LEAST, GROUNDCOVER_WATER_RULE_BELOW_AT_MOST,
            GROUNDCOVER_WATER_RULE_EITHER_AT_LEAST, GROUNDCOVER_WATER_RULE_EITHER_AT_MOST,
        };
        use byroredux_core::ecs::components::groundcover::CoverWaterRule;
        let pairs = [
            (CoverWaterRule::AboveAtLeast, GROUNDCOVER_WATER_RULE_ABOVE_AT_LEAST, "ABOVE_AT_LEAST"),
            (CoverWaterRule::AboveAtMost, GROUNDCOVER_WATER_RULE_ABOVE_AT_MOST, "ABOVE_AT_MOST"),
            (CoverWaterRule::BelowAtLeast, GROUNDCOVER_WATER_RULE_BELOW_AT_LEAST, "BELOW_AT_LEAST"),
            (CoverWaterRule::BelowAtMost, GROUNDCOVER_WATER_RULE_BELOW_AT_MOST, "BELOW_AT_MOST"),
            (CoverWaterRule::EitherAtLeast, GROUNDCOVER_WATER_RULE_EITHER_AT_LEAST, "EITHER_AT_LEAST"),
            (CoverWaterRule::EitherAtMost, GROUNDCOVER_WATER_RULE_EITHER_AT_MOST, "EITHER_AT_MOST"),
        ];
        let shader = include_str!("../../shaders/groundcover_models.comp");
        let admits = shader
            .split_once("bool gcWaterAdmits(")
            .expect("gcWaterAdmits")
            .1
            .split_once("\n}\n")
            .expect("gcWaterAdmits closes")
            .0;
        for (rule, code, name) in pairs {
            assert_eq!(rule.gpu_code(), code, "{rule:?}");
            assert!(
                admits.contains(&format!("case GROUNDCOVER_WATER_RULE_{name}:")),
                "gcWaterAdmits must switch on GROUNDCOVER_WATER_RULE_{name}"
            );
        }
        assert!(
            !(0..6).any(|code| admits.contains(&format!("case {code}u:"))),
            "gcWaterAdmits must not switch on bare literals"
        );
    }

    /// Mirror of PLACE's per-chunk lattice span (#4919): the first world
    /// lattice index along one axis and how many lattice points the chunk's
    /// half-open footprint `[base, base + CHUNK)` holds.
    fn lattice_span(base: f32, spacing: f32) -> (i32, u32) {
        let per_side_max = (crate::shader_constants::GROUNDCOVER_CHUNK_UNITS / spacing).ceil();
        let first = (base / spacing - 0.5).ceil() as i32;
        let end = ((base + crate::shader_constants::GROUNDCOVER_CHUNK_UNITS) / spacing - 0.5)
            .ceil() as i32;
        (first, ((end - first).max(0) as u32).min(per_side_max as u32))
    }

    /// #4919 — candidates sit on one world-space lattice, so a row of
    /// chunks tiles it with no gap, duplicate or crowded column at a border.
    /// The per-chunk `ceil(512 / 80) = 7` grid put 49 candidates where 40.96
    /// belong and spaced the border columns 80, 64, 48, 80.
    #[test]
    fn model_candidates_tile_one_world_lattice_across_chunk_borders() {
        let chunk = crate::shader_constants::GROUNDCOVER_CHUNK_UNITS;
        for spacing in [20.0_f32, 48.0, 64.0, 80.0, 100.0, 128.0] {
            let mut positions = Vec::new();
            let mut total = 0u32;
            for c in -8..8 {
                let base = c as f32 * chunk;
                let (first, count) = lattice_span(base, spacing);
                total += count;
                for i in 0..count as i32 {
                    let at = (f64::from(first + i) + 0.5) * f64::from(spacing);
                    assert!(
                        at >= f64::from(base) && at < f64::from(base + chunk),
                        "spacing {spacing}: point {at} left chunk [{base}, {})",
                        base + chunk
                    );
                    positions.push(at);
                }
            }
            for pair in positions.windows(2) {
                let gap = pair[1] - pair[0];
                assert!(
                    (gap - f64::from(spacing)).abs() < 1e-3,
                    "spacing {spacing}: gap {gap} between {} and {} — the lattice broke \
                     at a chunk border",
                    pair[0],
                    pair[1]
                );
            }
            let expected = 16.0 * chunk / spacing;
            assert!(
                (f64::from(total) - f64::from(expected)).abs() <= 1.0,
                "spacing {spacing}: {total} candidates across 16 chunks, want {expected}"
            );
        }
        let shader = include_str!("../../shaders/groundcover_models.comp");
        for pinned in [
            "ivec2 latticeFirst = ivec2(ceil(chunkUV / spacing - 0.5));",
            "ivec2 latticeEnd = ivec2(ceil((chunkUV + GROUNDCOVER_CHUNK_UNITS) / spacing - 0.5));",
            "vec2 lattice = (vec2(latticeFirst + ivec2(gx, gz)) + 0.5) * spacing;",
        ] {
            assert!(
                shader.contains(pinned),
                "PLACE must keep the world-anchored lattice this test mirrors: {pinned}"
            );
        }
    }

    /// Mirror of LAYOUT's grants (#4920): instances per shape for per-record
    /// plant totals, shapes in layout order naming their record.
    fn layout_grants(record_totals: &[u32], shape_records: &[u32], capacity: u32) -> Vec<u32> {
        let demand: u32 = shape_records
            .iter()
            .map(|&r| record_totals[r as usize])
            .sum();
        let granted: Vec<u32> = if demand > capacity {
            record_totals
                .iter()
                .map(|&t| (u64::from(t) * u64::from(capacity) / u64::from(demand)) as u32)
                .collect()
        } else {
            record_totals.to_vec()
        };
        let mut cursor = 0u32;
        shape_records
            .iter()
            .map(|&r| {
                let count = granted[r as usize].min(capacity - cursor);
                cursor += count;
                count
            })
            .collect()
    }

    /// #4920 — over the tail budget every record keeps the same share of
    /// whole plants. The old per-shape clamp in FormID order gave the last
    /// record nothing, and could keep a plant's first shape without its
    /// second.
    #[test]
    fn over_budget_layout_grants_every_record_whole_plants() {
        let capacity = 1000;
        // Record 2 is the late (DLC / mod) record, with a two-shape model.
        let totals = [600, 300, 400];
        let shape_records = [0, 1, 2, 2];
        let counts = layout_grants(&totals, &shape_records, capacity);
        assert!(counts.iter().sum::<u32>() <= capacity);
        assert!(counts.iter().all(|&c| c > 0), "a record was starved: {counts:?}");
        assert_eq!(counts[2], counts[3], "a plant's shapes must survive together");
        // Proportional: every record keeps the same fraction, within a plant.
        let demand: u32 = shape_records.iter().map(|&r| totals[r as usize]).sum();
        for (shape, &r) in shape_records.iter().enumerate() {
            let exact = f64::from(totals[r as usize]) * f64::from(capacity) / f64::from(demand);
            assert!((f64::from(counts[shape]) - exact).abs() < 1.0);
        }
        // Under the budget nothing changes.
        assert_eq!(layout_grants(&[3, 4], &[0, 1, 1], 100), vec![3, 4, 4]);

        let shader = include_str!("../../shaders/groundcover_models.comp");
        assert!(
            shader.contains(
                "uint64_t(sRecordTotal[r]) * uint64_t(pc.tailCapacity) / uint64_t(demand)"
            ),
            "LAYOUT must keep the proportional whole-plant grant this test mirrors"
        );
    }

    /// #5176 — LAYOUT's rank base for one record, mirrored: chunks are
    /// visited in the host's nearest-first permutation, so the grant's
    /// `rank < count` keeps the nearest chunks' plants. Returns, per chunk
    /// (slot order), how many of its plants survive a grant of `granted`.
    fn surviving_per_chunk(counts: &[u32], order: &[u32], granted: u32) -> Vec<u32> {
        let mut base = vec![0u32; counts.len()];
        let mut running = 0;
        for &chunk in order {
            base[chunk as usize] = running;
            running += counts[chunk as usize];
        }
        counts
            .iter()
            .zip(&base)
            .map(|(&count, &base)| granted.saturating_sub(base).min(count))
            .collect()
    }

    /// #5176 — over budget, the farthest chunks thin first. Slot order put
    /// the camera's own chunk last, so a half grant left it bare while a
    /// distant one kept every plant.
    #[test]
    fn over_budget_grant_keeps_the_nearest_chunks_plants() {
        // Slots 0..3; slot 2 is nearest the camera, slot 0 farthest.
        let counts = [10, 10, 10];
        let nearest_first = [2, 1, 0];
        assert_eq!(
            surviving_per_chunk(&counts, &nearest_first, 15),
            vec![0, 5, 10]
        );
        // Slot order — the pre-fix walk — starved the nearest chunk.
        assert_eq!(surviving_per_chunk(&counts, &[0, 1, 2], 15), vec![10, 5, 0]);

        let shader = include_str!("../../shaders/groundcover_models.comp");
        let layout = shader
            .split("void layoutShapes(uint lane) {")
            .nth(1)
            .expect("LAYOUT phase present");
        let walk = layout
            .find("uint c = gcChunks[o].layoutOrder;")
            .expect("LAYOUT must walk chunks in the host's nearest-first order (#5176)");
        let base = layout
            .find("uint slot = c * GROUNDCOVER_MODEL_MAX_RECORDS + r;")
            .expect("LAYOUT indexes the per-chunk record counts");
        assert!(walk < base, "the permuted chunk must be the one LAYOUT bases");
    }

    /// #4866: decoding a timer pair alone does not prove the GPU work writes it.
    #[test]
    fn model_timer_encloses_all_phases_and_stats_copy() {
        let source = crate::source_scan::production_text(include_str!("groundcover_models.rs"));
        let start = source
            .find("timers.cmd_groundcover_models_start(device, cmd, frame)")
            .expect("model tier must begin its GPU bracket");
        let end = source
            .find("timers.cmd_groundcover_models_end(device, cmd, frame)")
            .expect("model tier must end its GPU bracket");
        let bracket = &source[start..end];
        for command in [
            "(GROUNDCOVER_MODEL_PHASE_PLACE, scatter.chunk_count)",
            "(GROUNDCOVER_MODEL_PHASE_LAYOUT, 1)",
            "(GROUNDCOVER_MODEL_PHASE_EMIT, scatter.chunk_count)",
            "device.cmd_dispatch(cmd, groups, 1, 1)",
            "device.cmd_copy_buffer(",
        ] {
            assert!(
                bracket.contains(command),
                "GPU bracket must include {command}"
            );
        }
        let timers = crate::source_scan::production_text(include_str!("gpu_timers.rs"));
        let start_method = timers
            .split("pub fn cmd_groundcover_models_start(")
            .nth(1)
            .unwrap()
            .split("pub fn cmd_groundcover_models_end(")
            .next()
            .unwrap();
        assert!(start_method.contains("vk::PipelineStageFlags::COMPUTE_SHADER"));
        let end_method = timers
            .split("pub fn cmd_groundcover_models_end(")
            .nth(1)
            .unwrap()
            .split("pub fn cmd_volumetrics_inject_start(")
            .next()
            .unwrap();
        assert!(end_method.contains("vk::PipelineStageFlags::BOTTOM_OF_PIPE"));
        assert!(end_method.contains("self.active_bits[frame] |= BIT_GROUNDCOVER_MODELS"));
    }

    /// #5220 — the tier's placement totals must not outlive the frame that
    /// produced them: `DebugStats::groundcover_model_{demanded,emitted}` are
    /// mirrored from `stats()` every frame, and a `prepare` that finds
    /// nothing to place has to zero the latch so an interior / cover-off
    /// frame reads 0/0 instead of the last exterior's counts. `prepare`
    /// needs a device and a registry, so this pins the un-latch at source
    /// level: both nothing-to-place paths must carry the reset, after the
    /// `harvest` call that consumes any pending readback.
    #[test]
    fn prepare_unlatches_the_stats_when_nothing_is_placed() {
        let source = crate::source_scan::production_text(include_str!("groundcover_models.rs"));
        // Slice prepare's own text: the fn ends where `harvest` is declared.
        let prepare = source
            .split("pub fn prepare(")
            .nth(1)
            .expect("prepare must exist")
            .split("    fn harvest(")
            .next()
            .expect("prepare's text must end at harvest");
        let reset = "self.stats = GroundCoverModelStats::default();";
        assert_eq!(
            prepare.matches(reset).count(),
            2,
            "both nothing-to-place paths in `prepare` (the no-records early \
             return and the not-ready tail) must reset the stats latch (#5220)"
        );
        let harvest = prepare
            .find("self.harvest(device, frame);")
            .expect("prepare must start with harvest");
        for (at, _) in prepare.match_indices(reset) {
            assert!(
                at > harvest,
                "the stats reset must come AFTER `harvest` — a dispatch that \
                 fired two frames ago is still consumed by it (#5220)"
            );
        }
    }

    #[test]
    fn host_mirrors_match_the_shader_strides() {
        assert_eq!(std::mem::size_of::<GpuGroundCoverModelRecord>(), 48);
        assert_eq!(std::mem::size_of::<GpuGroundCoverModelShape>(), 112);
        // Sizes the placement slab, which `docs/engine/memory-budget.md`
        // ledgers — a change here is a budget change.
        assert_eq!(std::mem::size_of::<GpuGroundCoverModelPoint>(), 32);
        assert_eq!(std::mem::size_of::<ModelPush>(), 64);
    }

    #[test]
    fn descriptor_layout_matches_the_shader() {
        validate_layout(&set_layout_bindings()).expect("layout contract");
    }

    /// The shape's per-draw flags land in the same bits the main instance
    /// path writes, and never claim alpha blending.
    #[test]
    fn shape_flags_carry_flat_shading_and_the_render_layer_only() {
        use super::super::scene_buffer::{
            INSTANCE_FLAG_ALPHA_BLEND, INSTANCE_FLAG_FLAT_SHADING, INSTANCE_RENDER_LAYER_SHIFT,
        };
        let flags =
            shape_instance_flags(byroredux_core::ecs::components::RenderLayer::Clutter, true);
        assert_ne!(flags & INSTANCE_FLAG_FLAT_SHADING, 0);
        assert_eq!(flags & INSTANCE_FLAG_ALPHA_BLEND, 0);
        assert_eq!(flags >> INSTANCE_RENDER_LAYER_SHIFT & 0x3, 1);
    }
}
