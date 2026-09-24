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
use super::buffer::{GpuBuffer, NoUninit};
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

/// `VkDrawIndexedIndirectCommand` stride.
const DRAW_STRIDE: u64 = 20;
/// Bytes of one `GcModelPoint` in the placement slab.
const POINT_BYTES: u64 = 32;
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

fn push_bytes(push: &ModelPush) -> &[u8] {
    // SAFETY: `#[repr(C)]` over 4-byte scalars with no padding, so every byte
    // is initialised; the slice borrows `push`.
    unsafe {
        std::slice::from_raw_parts(
            (push as *const ModelPush).cast::<u8>(),
            std::mem::size_of::<ModelPush>(),
        )
    }
}

/// One template shape as the host resolved it this frame: the record it
/// belongs to and its fully built `DrawCommand` (material interned, mesh and
/// texture resolved, `model_matrix` = the shape's model-root-local transform).
pub struct GroundCoverModelShapeInput<'a> {
    pub record: u32,
    pub draw: &'a super::context::DrawCommand,
}

/// Everything the tier needs from the host for one frame.
pub struct GroundCoverModelFrame<'a> {
    pub records: &'a [GpuGroundCoverModelRecord],
    /// Record index per selection-table entry, in proportion to climate weight.
    pub record_table: &'a [u32],
    /// Sorted by record.
    pub shapes: &'a [GroundCoverModelShapeInput<'a>],
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

/// Placement totals read back one pipelined frame late.
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
                * POINT_BYTES,
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
        self.harvest(device, frame);
        self.frame_recorded = false;
        self.frame_record_count = 0;
        self.frame_shape_count = 0;
        self.frame_draws.clear();
        let record_count = input
            .records
            .len()
            .min(GROUNDCOVER_MODEL_MAX_RECORDS as usize);
        if record_count == 0 || !(input.grid_spacing > 0.0) {
            return false;
        }
        let mut records = input.records[..record_count].to_vec();
        for record in &mut records {
            record.shape_first = 0;
            record.shape_count = 0;
        }
        let mut shapes = Vec::with_capacity(input.shapes.len());
        for shape in input.shapes {
            if shapes.len() == GROUNDCOVER_MODEL_MAX_SHAPES as usize {
                break;
            }
            let Some(record) = records.get_mut(shape.record as usize) else {
                continue;
            };
            let Some(mesh) = mesh_registry.get(shape.draw.mesh_handle) else {
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
            let draw = shape.draw;
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
                record: shape.record,
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
        if shapes.is_empty() {
            self.frame_draws.clear();
            return false;
        }
        let mut table = [0u32; GROUNDCOVER_SPECIES_TABLE_SIZE as usize];
        let len = input.record_table.len().min(table.len());
        table[..len].copy_from_slice(&input.record_table[..len]);
        let uploads = self.record_buffers[frame]
            .write_mapped(device, &records)
            .and_then(|()| self.table_buffers[frame].write_mapped(device, &table))
            .and_then(|()| self.shape_buffers[frame].write_mapped(device, &shapes));
        if let Err(error) = uploads {
            log::warn!("ground-cover model tier: upload failed: {error}");
            self.frame_draws.clear();
            return false;
        }
        self.frame_record_count = records.len() as u32;
        self.frame_shape_count = shapes.len() as u32;
        self.frame_grid_spacing = input.grid_spacing;
        true
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
        let mut writes: Vec<vk::WriteDescriptorSet> = buffers
            .iter()
            .map(|(binding, info)| {
                vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(*binding)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(info)
            })
            .collect();
        writes.push(crate::vulkan::descriptors::write_acceleration_structure(
            set,
            3,
            &mut accel_write,
        ));
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
                    push_bytes(&push),
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

    #[test]
    fn host_mirrors_match_the_shader_strides() {
        assert_eq!(std::mem::size_of::<GpuGroundCoverModelRecord>(), 48);
        assert_eq!(std::mem::size_of::<GpuGroundCoverModelShape>(), 112);
        assert_eq!(std::mem::size_of::<ModelPush>(), 64);
        assert_eq!(
            DRAW_STRIDE as usize,
            std::mem::size_of::<vk::DrawIndexedIndirectCommand>()
        );
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
