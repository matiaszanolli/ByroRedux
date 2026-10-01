//! Construct-time half of [`GroundCoverPipeline`] (#5089, split out of
//! `groundcover.rs`, which retains the struct, the GPU record types and
//! every constant): buffer allocation, the descriptor-set layout contracts,
//! and pipeline building. The per-frame half lives in `frame.rs`.
use super::*;

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
            // One `VkDrawIndirectCommand` per chunk per stream.
            indirect: GROUNDCOVER_MAX_CHUNKS as u64
                * GC_DRAW_INDIRECT_STRIDE
                * GROUNDCOVER_INDIRECT_STREAMS,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// #4297 / LOD tiers 1–2 — the opaque endpoints retain real
    /// wind/displacement velocity and zero FSR masks; only either stochastic
    /// projected-size handoff raises a bounded reactive contribution. Shader
    /// text catches a future pipeline mask or output regression without a GPU.
    #[test]
    fn blade_motion_and_fsr_mask_contract_stay_material_driven() {
        let vert = include_str!("../../../shaders/groundcover_blade.vert");
        let frag = include_str!("../../../shaders/groundcover_blade.frag");
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
        let production =
            crate::source_scan::production_text(include_str!("construct.rs"));
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
}
