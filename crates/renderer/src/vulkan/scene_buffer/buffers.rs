//! `SceneBuffers` storage + lifecycle (`new` / `destroy` / accessors).
//!
//! Per-FIF descriptor sets + bindless texture array. `new()` does the full
//! allocation chain; the upload + descriptor-write methods live in their own
//! siblings.

use super::super::allocator::SharedAllocator;
use super::super::buffer::GpuBuffer;
use super::super::descriptors::{
    write_storage_buffer, write_uniform_buffer, DescriptorPoolBuilder,
};
use super::super::sync::MAX_FRAMES_IN_FLIGHT;
use super::*;
use anyhow::{Context, Result};
use ash::vk;

#[derive(Clone, Copy)]
pub(super) struct LightHeader {
    /// Light count for this frame. Rust never reads it back — the value
    /// is byte-copied straight to the SSBO via the std430-packed write
    /// in `upload::upload_lights`, where the shader consumes it as the
    /// scene-buffer header. Annotate to silence dead-code (TD2-203).
    #[allow(dead_code)]
    pub(super) count: u32,
    pub(super) _pad: [u32; 3],
}

/// Per-frame scene buffers and their descriptor sets.
pub struct SceneBuffers {
    /// One SSBO per frame-in-flight (header + light array).
    pub(super) light_buffers: Vec<GpuBuffer>,
    /// One UBO per frame-in-flight (camera data).
    pub(super) camera_buffers: Vec<GpuBuffer>,
    /// HOST_VISIBLE | TRANSFER_SRC staging buffer per frame-in-flight that the
    /// CPU writes the bone palette into each frame ([`upload_bones`]). The
    /// matching [`bone_device_buffers`] slot is the actual storage-buffer
    /// binding consumed by skinning shaders; [`record_bone_copy`] schedules
    /// the staging→device transfer + visibility barrier on the command
    /// buffer that will read the palette.
    ///
    /// Pre-#921 the descriptor pointed at this host-visible buffer
    /// directly, so every skinned vertex performed a PCIe round-trip
    /// through host-mapped storage per shader invocation (~2 MB read per
    /// frame on a single skinned mesh and 4 KB × bones across every
    /// fragment that referenced a bone — verified against AUDIT_RENDERER
    /// Dim 12 / REN-D12-NEW-04).
    pub(super) bone_staging_buffers: Vec<GpuBuffer>,
    /// DEVICE_LOCAL | STORAGE_BUFFER | TRANSFER_DST bone palette per
    /// frame-in-flight. Bound as descriptor binding 3 (current frame)
    /// and binding 12 (previous frame, the OTHER slot in the ring — see
    /// SH-3 / #641). Populated each frame by [`record_bone_copy`] via a
    /// `cmd_copy_buffer` from the matching [`bone_staging_buffers`] slot
    /// followed by a TRANSFER→COMPUTE_SHADER|VERTEX_SHADER memory barrier.
    /// Slot 0 is seeded with the identity matrix at construction
    /// (see [`seed_identity_bones`]) so rigid vertices that fall through
    /// to the palette and binding-12 reads on the very first frame see
    /// a valid transform.
    pub(super) bone_device_buffers: Vec<GpuBuffer>,
    /// Bytes most recently written into [`bone_staging_buffers[frame]`]
    /// by [`upload_bones`]. [`record_bone_copy`] copies exactly this
    /// many bytes — avoids transferring the full ~2 MB
    /// `MAX_TOTAL_BONES × mat4` slab when only a handful of bones were
    /// actually written. Reset by [`upload_bones`]; pinned at the
    /// identity-slot size by the init seed so frames without skinned
    /// content still refresh the identity row.
    pub(super) bone_upload_bytes: Vec<vk::DeviceSize>,
    /// One SSBO per frame-in-flight (per-instance data for instanced drawing).
    /// Each entry contains model matrix + texture index + bone offset.
    pub(super) instance_buffers: Vec<GpuBuffer>,
    /// One UBO per frame-in-flight holding the per-TOD-interpolated
    /// 6-axis directional ambient cube (`GpuDalcCube`). Sourced from
    /// Skyrim WTHR.DALC; the cube's `flags.x` gates the consumer so
    /// non-Skyrim cells fall back to the legacy AMBIENT_AO_FLOOR path
    /// in triangle.frag. See #993 / REN-AMBIENT-DALC.
    pub(super) dalc_buffers: Vec<GpuBuffer>,
    /// One SSBO per frame-in-flight ([`super::super::material::GpuMaterial`]
    /// table). Indexed by `GpuInstance.materialId`. Phase 4 (R1)
    /// migrates one field (`roughness`) onto this path; Phases 5–6
    /// migrate the rest and finally drop the redundant per-instance
    /// copies. Sized for [`MAX_MATERIALS`] entries.
    pub(super) material_buffers: Vec<GpuBuffer>,
    /// Per-frame-in-flight content hash of the most recent
    /// successful `upload_materials` write. The next call computes
    /// the hash of the new slice and skips the
    /// `copy_nonoverlapping + flush_if_needed` pair when it matches
    /// — a static interior cell where `build_render_data` produces
    /// a byte-identical materials slice each frame is the steady-
    /// state case. `None` until the first upload so the cold path
    /// is unconditional. See #878 / DIM8-01.
    pub(super) last_uploaded_material_hash: [Option<u64>; MAX_FRAMES_IN_FLIGHT],
    /// One `INDIRECT_BUFFER`-usage buffer per frame-in-flight for
    /// `vkCmdDrawIndexedIndirect`. Holds
    /// `VkDrawIndexedIndirectCommand` entries uploaded CPU-side each
    /// frame. The draw loop collapses consecutive batches sharing
    /// `(pipeline_key, is_decal)` into one indirect draw call reading
    /// a contiguous range of this buffer. See #309.
    pub(super) indirect_buffers: Vec<GpuBuffer>,
    /// Single DEVICE_LOCAL SSBO holding `MAX_TERRAIN_TILES`
    /// `GpuTerrainTile` entries. Rewritten only at cell transitions
    /// via [`SceneBuffers::upload_terrain_tiles`] — a staging copy
    /// into GPU memory. Pre-#497 this was double-buffered HOST_VISIBLE,
    /// which wasted 32 KB of scarce BAR heap for read-only data. All
    /// frame-in-flight descriptor sets point at the same buffer since
    /// there are no per-frame contents. Fragment shader reads
    /// `terrainTiles[tile_idx]` when `INSTANCE_FLAG_TERRAIN_SPLAT` is
    /// set on the instance. See #470 / #497.
    pub(super) terrain_tile_buffer: GpuBuffer,
    /// Single HOST_VISIBLE buffer holding `MAX_FRAMES_IN_FLIGHT` ray-budget
    /// counter slots, one per frame-in-flight, each [`RAY_BUDGET_STRIDE`]
    /// bytes apart so they satisfy `minStorageBufferOffsetAlignment` on
    /// every common device. Each frame's descriptor set writes binding 11
    /// at `offset = frame * RAY_BUDGET_STRIDE, range = 4`. The CPU zeroes
    /// the active frame's slot before each render pass; the fragment
    /// shader atomically increments it per IOR ray pair fired and skips
    /// Phase-3 glass once the budget is exhausted.
    ///
    /// Pre-fix this was `Vec<GpuBuffer>` with one allocation per frame
    /// for a single u32 — `gpu-allocator` rounded each up to the
    /// alignment-padded sub-allocation size and could reserve a fresh
    /// 64 KB host-visible block to satisfy the layout. The single shared
    /// buffer collapses both frames into one ~512 B sub-allocation.
    /// See #683 / MEM-2-8.
    pub(super) ray_budget_buffer: GpuBuffer,
    /// Size of the terrain tile buffer in bytes — stashed so upload
    /// paths don't have to recompute it from `MAX_TERRAIN_TILES`.
    pub(super) terrain_tile_buf_size: vk::DeviceSize,
    /// Descriptor pool for scene descriptor sets.
    pub(super) descriptor_pool: vk::DescriptorPool,
    /// Layout for set 1: binding 0 = SSBO (lights), binding 1 = UBO (camera),
    /// binding 2 = TLAS (RT only), binding 3 = SSBO (bone palette),
    /// binding 4 = SSBO (instance data), …, binding 12 = SSBO (previous-frame
    /// bone palette — SH-3 / #641, vertex-stage only, points at the
    /// frame-in-flight ring's other slot so motion vectors on skinned
    /// vertices reflect actual joint motion rather than zero).
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    /// One descriptor set per frame-in-flight.
    pub descriptor_sets: Vec<vk::DescriptorSet>,
    /// Tracks whether the TLAS binding has been written for each frame.
    pub tlas_written: Vec<bool>,
}

/// Build the scene descriptor-set-layout bindings (set=1) consumed by
/// the main raster pipeline. Pure data — no Vulkan device required —
/// so this is the seam that `cargo test` reflection tests can call
/// against the include_bytes!'d `triangle.vert.spv` / `triangle.frag.spv`
/// without spinning up a real device. Production `SceneBuffers::new`
/// routes through the same function so test and runtime can't drift.
///
/// `rt_enabled = false` drops binding 2 (TLAS); the shader still
/// declares it because `rayQuery` calls are guarded by a uniform flag
/// at runtime, so the validator must list `[2]` in
/// `optional_shader_bindings` for the no-RT case. See #427 / #950.
pub(crate) fn build_scene_descriptor_bindings(
    rt_enabled: bool,
) -> Vec<vk::DescriptorSetLayoutBinding<'static>> {
    let mut bindings = vec![
        vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        // Camera UBO is read by both vertex (viewProj) and fragment (cameraPos, sceneFlags).
        vk::DescriptorSetLayoutBinding::default()
            .binding(1)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
    ];
    if rt_enabled {
        bindings.push(
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        );
    }
    // Binding 3: bone palette SSBO (vertex shader — skinning).
    bindings.push(
        vk::DescriptorSetLayoutBinding::default()
            .binding(3)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX),
    );
    // Binding 4: instance data SSBO (vertex + fragment — instanced drawing + PBR materials).
    bindings.push(
        vk::DescriptorSetLayoutBinding::default()
            .binding(4)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT),
    );
    // Binding 5: cluster grid SSBO (fragment shader — clustered lighting).
    bindings.push(
        vk::DescriptorSetLayoutBinding::default()
            .binding(5)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    );
    // Binding 6: cluster light indices SSBO (fragment shader — clustered lighting).
    bindings.push(
        vk::DescriptorSetLayoutBinding::default()
            .binding(6)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    );
    // Binding 7: SSAO texture (fragment shader — ambient occlusion).
    bindings.push(
        vk::DescriptorSetLayoutBinding::default()
            .binding(7)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    );
    // Binding 8: global vertex SSBO (fragment shader — RT reflection UV lookup).
    bindings.push(
        vk::DescriptorSetLayoutBinding::default()
            .binding(8)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    );
    // Binding 9: global index SSBO (fragment shader — RT reflection UV lookup).
    bindings.push(
        vk::DescriptorSetLayoutBinding::default()
            .binding(9)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    );
    // Binding 10: terrain tile SSBO (fragment shader — LAND splat layer
    // texture indices, one entry per terrain entity). #470.
    bindings.push(
        vk::DescriptorSetLayoutBinding::default()
            .binding(10)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    );
    // Binding 11: RT mipmap ray budget counter (fragment shader — u32 atomic).
    // The CPU zeroes this before each render pass; Phase-3 glass fragments
    // atomicAdd to claim a budget slot and skip IOR refraction once exhausted.
    bindings.push(
        vk::DescriptorSetLayoutBinding::default()
            .binding(11)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    );
    // Binding 12: previous-frame bone palette SSBO (vertex shader —
    // skinned-vertex motion vectors, SH-3 / #641). Bound to the OTHER
    // slot in the `bone_buffers` frame-in-flight ring, so reading it
    // yields the palette uploaded last frame. Pre-#641 the vertex
    // shader composed `fragPrevClipPos = prevViewProj * worldPos`
    // using the CURRENT-frame skinned worldPos — every actor pixel
    // had a motion vector encoding only camera + rigid motion, and
    // SVGF / TAA reprojected the wrong source pixel on intra-mesh
    // disocclusions (forearm crossing torso). Same indices/weights as
    // binding 3, so the per-mesh bone offset stamped on the
    // `GpuInstance` still resolves correctly as long as the offset
    // assignment is stable across the two frames.
    bindings.push(
        vk::DescriptorSetLayoutBinding::default()
            .binding(12)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX),
    );
    // Binding 13: material table SSBO (fragment — R1 Phase 4).
    // The fragment stage reads `materials[instance.materialId]`
    // for migrated per-material fields. Vertex stage isn't a
    // consumer today; widen the stage mask if a future migration
    // (e.g. M29.5 GPU skinning) needs material data in the vertex
    // pipeline.
    bindings.push(
        vk::DescriptorSetLayoutBinding::default()
            .binding(13)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    );
    // Binding 14: DALC ambient cube UBO (fragment — #993 / REN-AMBIENT-DALC).
    // 6-axis directional ambient sourced from Skyrim WTHR.DALC,
    // axis-swapped + per-TOD-lerped engine-side. `GpuDalcCube.flags.x`
    // gates use; when zero, triangle.frag falls back to the legacy
    // AMBIENT_AO_FLOOR path so non-Skyrim cells render unchanged.
    bindings.push(
        vk::DescriptorSetLayoutBinding::default()
            .binding(14)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT),
    );
    bindings
}

/// All per-frame-in-flight and shared scene render buffers, plus their sizes.
///
/// Allocated by [`allocate_scene_render_buffers`]; consumed by
/// [`SceneBuffers::new`] for descriptor writes and seeding before being
/// decomposed into the final [`SceneBuffers`] struct fields.
struct SceneRenderBuffers {
    light_buffers: Vec<GpuBuffer>,
    light_buf_size: vk::DeviceSize,
    camera_buffers: Vec<GpuBuffer>,
    camera_buf_size: vk::DeviceSize,
    bone_staging_buffers: Vec<GpuBuffer>,
    bone_device_buffers: Vec<GpuBuffer>,
    bone_buf_size: vk::DeviceSize,
    instance_buffers: Vec<GpuBuffer>,
    instance_buf_size: vk::DeviceSize,
    indirect_buffers: Vec<GpuBuffer>,
    material_buffers: Vec<GpuBuffer>,
    material_buf_size: vk::DeviceSize,
    dalc_buffers: Vec<GpuBuffer>,
    dalc_buf_size: vk::DeviceSize,
    terrain_tile_buffer: GpuBuffer,
    terrain_tile_buf_size: vk::DeviceSize,
    // #683 / MEM-2-8 — single shared buffer with per-frame stride slots.
    ray_budget_buffer: GpuBuffer,
}

/// Allocate all Vulkan buffers needed by the scene render pipeline.
///
/// Creates per-frame-in-flight SSBO/UBO buffers (lights, camera, bones,
/// instances, materials, indirect draw, DALC cube) and shared buffers
/// (terrain tile SSBO, ray-budget counter). Separated from
/// [`SceneBuffers::new`] so the allocation phase can be read independently
/// from the descriptor-setup phase. See #1052 / TD9-009.
fn allocate_scene_render_buffers(
    device: &ash::Device,
    allocator: &SharedAllocator,
) -> Result<SceneRenderBuffers> {
    // ── Buffer sizes ──────────────────────────────────────────────────────
    let light_buf_size = (std::mem::size_of::<LightHeader>()
        + std::mem::size_of::<GpuLight>() * MAX_LIGHTS)
        as vk::DeviceSize;
    let camera_buf_size = std::mem::size_of::<GpuCamera>() as vk::DeviceSize;
    let dalc_buf_size = std::mem::size_of::<GpuDalcCube>() as vk::DeviceSize;
    // Bone palette: 4 × vec4 (mat4) per slot, std430 layout.
    let bone_buf_size =
        (std::mem::size_of::<[[f32; 4]; 4]>() * MAX_TOTAL_BONES) as vk::DeviceSize;
    // Instance SSBO: per-instance model matrix + texture index + bone offset.
    let instance_buf_size =
        (std::mem::size_of::<GpuInstance>() * MAX_INSTANCES) as vk::DeviceSize;
    // Material SSBO: deduplicated `GpuMaterial` table (R1 Phase 4).
    let material_buf_size =
        (std::mem::size_of::<super::super::material::GpuMaterial>() * MAX_MATERIALS)
            as vk::DeviceSize;
    // Indirect buffer: one VkDrawIndexedIndirectCommand (20 B) per batch. #309.
    let indirect_buf_size = (std::mem::size_of::<vk::DrawIndexedIndirectCommand>()
        * MAX_INDIRECT_DRAWS) as vk::DeviceSize;
    // Terrain tile SSBO: 32 B per slot × MAX_TERRAIN_TILES. #470.
    let terrain_tile_buf_size =
        (std::mem::size_of::<GpuTerrainTile>() * MAX_TERRAIN_TILES) as vk::DeviceSize;

    // ── Per-frame-in-flight buffers ───────────────────────────────────────
    let mut light_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    let mut camera_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    // #921 / REN-D12-NEW-04 — split bone palette into HOST_VISIBLE staging
    // + DEVICE_LOCAL storage. Descriptors bind the device buffers; the
    // staging buffer is written by `upload_bones` and transferred each
    // frame by `record_bone_copy`.
    let mut bone_staging_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    let mut bone_device_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    let mut instance_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    let mut indirect_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    let mut material_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    let mut dalc_buffers = Vec::with_capacity(MAX_FRAMES_IN_FLIGHT);
    for _ in 0..MAX_FRAMES_IN_FLIGHT {
        light_buffers.push(GpuBuffer::create_host_visible(
            device,
            allocator,
            light_buf_size,
            vk::BufferUsageFlags::STORAGE_BUFFER,
        )?);
        camera_buffers.push(GpuBuffer::create_host_visible(
            device,
            allocator,
            camera_buf_size,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
        )?);
        bone_staging_buffers.push(GpuBuffer::create_host_visible(
            device,
            allocator,
            bone_buf_size,
            vk::BufferUsageFlags::TRANSFER_SRC,
        )?);
        bone_device_buffers.push(GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            bone_buf_size,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
        )?);
        instance_buffers.push(GpuBuffer::create_host_visible(
            device,
            allocator,
            instance_buf_size,
            vk::BufferUsageFlags::STORAGE_BUFFER,
        )?);
        indirect_buffers.push(GpuBuffer::create_host_visible(
            device,
            allocator,
            indirect_buf_size,
            vk::BufferUsageFlags::INDIRECT_BUFFER,
        )?);
        material_buffers.push(GpuBuffer::create_host_visible(
            device,
            allocator,
            material_buf_size,
            vk::BufferUsageFlags::STORAGE_BUFFER,
        )?);
        dalc_buffers.push(GpuBuffer::create_host_visible(
            device,
            allocator,
            dalc_buf_size,
            vk::BufferUsageFlags::UNIFORM_BUFFER,
        )?);
        // Ray budget counter is a SINGLE shared buffer (created below)
        // with per-frame slots at `frame * RAY_BUDGET_STRIDE`. #683 / MEM-2-8.
    }

    // ── Shared (single-allocation) buffers ────────────────────────────────
    // #683 / MEM-2-8 — single shared buffer covering all frame slots.
    let ray_budget_buffer = GpuBuffer::create_host_visible(
        device,
        allocator,
        RAY_BUDGET_STRIDE * MAX_FRAMES_IN_FLIGHT as vk::DeviceSize,
        vk::BufferUsageFlags::STORAGE_BUFFER,
    )?;
    // Terrain tile SSBO: single DEVICE_LOCAL buffer, uploaded via staging at
    // cell load. Shared across all FIF descriptor sets since contents are
    // static until the next cell transition. See #497.
    let terrain_tile_buffer = GpuBuffer::create_device_local_uninit(
        device,
        allocator,
        terrain_tile_buf_size,
        vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
    )?;

    Ok(SceneRenderBuffers {
        light_buffers,
        light_buf_size,
        camera_buffers,
        camera_buf_size,
        bone_staging_buffers,
        bone_device_buffers,
        bone_buf_size,
        instance_buffers,
        instance_buf_size,
        indirect_buffers,
        material_buffers,
        material_buf_size,
        dalc_buffers,
        dalc_buf_size,
        terrain_tile_buffer,
        terrain_tile_buf_size,
        ray_budget_buffer,
    })
}

/// Create scene descriptor set layout, pool, allocate per-FIF sets, and
/// write the initial buffer-backed descriptors into them.
///
/// Returns `(layout, pool, sets)`. Separated from [`SceneBuffers::new`] so
/// the descriptor-wiring phase can be read independently from the allocation
/// phase. See #1052 / TD9-009.
///
/// # Safety
/// Calls `vkCreateDescriptorSetLayout`, `vkAllocateDescriptorSets`, and
/// `vkUpdateDescriptorSets` — all safe as long as `device` is valid and the
/// buffer handles in `bufs` are live for the duration of the sets' lifetimes.
/// [`SceneBuffers::destroy`] drops the pool before the layout and before the
/// device, satisfying the invariant.
fn create_scene_descriptors(
    device: &ash::Device,
    rt_enabled: bool,
    bufs: &SceneRenderBuffers,
) -> Result<(vk::DescriptorSetLayout, vk::DescriptorPool, Vec<vk::DescriptorSet>)> {
    // ── Descriptor set layout ─────────────────────────────────────────────
    let bindings = build_scene_descriptor_bindings(rt_enabled);
    // Mark bindings 5+6 (cluster data) as PARTIALLY_BOUND so they are
    // valid even when unwritten (cluster cull pipeline may fail to create).
    let binding_flags: Vec<vk::DescriptorBindingFlags> = bindings
        .iter()
        .map(|b| {
            if b.binding >= 5 {
                vk::DescriptorBindingFlags::PARTIALLY_BOUND
            } else {
                vk::DescriptorBindingFlags::empty()
            }
        })
        .collect();
    let mut binding_flags_info =
        vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&binding_flags);
    // Validate against triangle.vert/frag SPIR-V before creating the layout (#427).
    let optional_bindings: &[u32] = if rt_enabled { &[] } else { &[2] };
    super::super::reflect::validate_set_layout(
        1,
        &bindings,
        &[
            super::super::reflect::ReflectedShader {
                name: "triangle.vert",
                spirv: super::super::pipeline::TRIANGLE_VERT_SPV,
            },
            super::super::reflect::ReflectedShader {
                name: "triangle.frag",
                spirv: super::super::pipeline::TRIANGLE_FRAG_SPV,
            },
        ],
        "scene (set=1)",
        optional_bindings,
    )
    .expect("scene descriptor layout drifted against triangle.vert/frag (see #427)");
    let layout_info = vk::DescriptorSetLayoutCreateInfo::default()
        .bindings(&bindings)
        .push_next(&mut binding_flags_info);
    let descriptor_set_layout = unsafe {
        device
            .create_descriptor_set_layout(&layout_info, None)
            .context("Failed to create scene descriptor set layout")?
    };

    // ── Descriptor pool + set allocation ─────────────────────────────────
    // Pool sizes derived from `bindings` so conditional TLAS slot flows
    // through automatically (#1030 / REN-D10-NEW-09).
    let descriptor_pool =
        DescriptorPoolBuilder::from_layout_bindings(&bindings, MAX_FRAMES_IN_FLIGHT as u32)
            .max_sets(MAX_FRAMES_IN_FLIGHT as u32)
            .build(device, "Failed to create scene descriptor pool")?;

    let layouts = vec![descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
    let alloc_info = vk::DescriptorSetAllocateInfo::default()
        .descriptor_pool(descriptor_pool)
        .set_layouts(&layouts);
    let descriptor_sets = unsafe {
        device
            .allocate_descriptor_sets(&alloc_info)
            .context("Failed to allocate scene descriptor sets")?
    };

    // ── Write descriptors ─────────────────────────────────────────────────
    for i in 0..MAX_FRAMES_IN_FLIGHT {
        let light_buf_info = [vk::DescriptorBufferInfo {
            buffer: bufs.light_buffers[i].buffer,
            offset: 0,
            range: bufs.light_buf_size,
        }];
        let camera_buf_info = [vk::DescriptorBufferInfo {
            buffer: bufs.camera_buffers[i].buffer,
            offset: 0,
            range: bufs.camera_buf_size,
        }];
        // #921 — descriptors point at the DEVICE buffers; staging copies
        // happen on the recording command buffer via `record_bone_copy`.
        let bone_buf_info = [vk::DescriptorBufferInfo {
            buffer: bufs.bone_device_buffers[i].buffer,
            offset: 0,
            range: bufs.bone_buf_size,
        }];
        // Previous-frame bone palette (SH-3 / #641): the OTHER ring slot.
        let bone_prev_idx = (i + MAX_FRAMES_IN_FLIGHT - 1) % MAX_FRAMES_IN_FLIGHT;
        let bone_prev_buf_info = [vk::DescriptorBufferInfo {
            buffer: bufs.bone_device_buffers[bone_prev_idx].buffer,
            offset: 0,
            range: bufs.bone_buf_size,
        }];
        let instance_buf_info = [vk::DescriptorBufferInfo {
            buffer: bufs.instance_buffers[i].buffer,
            offset: 0,
            range: bufs.instance_buf_size,
        }];
        let terrain_tile_buf_info = [vk::DescriptorBufferInfo {
            buffer: bufs.terrain_tile_buffer.buffer,
            offset: 0,
            range: bufs.terrain_tile_buf_size,
        }];
        // #683 / MEM-2-8 — slot into the shared buffer at this frame's stride.
        let ray_budget_buf_info = [vk::DescriptorBufferInfo {
            buffer: bufs.ray_budget_buffer.buffer,
            offset: (i as vk::DeviceSize) * RAY_BUDGET_STRIDE,
            range: std::mem::size_of::<u32>() as vk::DeviceSize,
        }];
        let material_buf_info = [vk::DescriptorBufferInfo {
            buffer: bufs.material_buffers[i].buffer,
            offset: 0,
            range: bufs.material_buf_size,
        }];
        let dalc_buf_info = [vk::DescriptorBufferInfo {
            buffer: bufs.dalc_buffers[i].buffer,
            offset: 0,
            range: bufs.dalc_buf_size,
        }];
        let set = descriptor_sets[i];
        let writes = [
            write_storage_buffer(set, 0, &light_buf_info),
            write_uniform_buffer(set, 1, &camera_buf_info),
            write_storage_buffer(set, 3, &bone_buf_info),
            write_storage_buffer(set, 4, &instance_buf_info),
            write_storage_buffer(set, 10, &terrain_tile_buf_info),
            write_storage_buffer(set, 11, &ray_budget_buf_info),
            write_storage_buffer(set, 12, &bone_prev_buf_info),
            write_storage_buffer(set, 13, &material_buf_info),
            write_uniform_buffer(set, 14, &dalc_buf_info),
        ];
        unsafe {
            device.update_descriptor_sets(&writes, &[]);
        }
    }

    Ok((descriptor_set_layout, descriptor_pool, descriptor_sets))
}

impl SceneBuffers {
    /// Create scene buffers and descriptor infrastructure.
    ///
    /// Delegates to [`allocate_scene_render_buffers`] (buffer allocation) and
    /// [`create_scene_descriptors`] (layout + pool + writes), then seeds the
    /// bone-palette identity slot and the DALC UBO default before returning the
    /// fully-initialised [`SceneBuffers`]. See #1052 / TD9-009.
    pub fn new(
        device: &ash::Device,
        allocator: &SharedAllocator,
        rt_enabled: bool,
    ) -> Result<Self> {
        let mut bufs = allocate_scene_render_buffers(device, allocator)?;
        let (descriptor_set_layout, descriptor_pool, descriptor_sets) =
            create_scene_descriptors(device, rt_enabled, &bufs)?;

        // ── Seed initial buffer data ──────────────────────────────────────
        // Seed slot 0 of each STAGING bone buffer with the identity matrix so
        // rigid vertices that fall through to the palette produce correct
        // positions rather than collapsing to origin. The matching device
        // buffer is populated by `seed_identity_bones` once the caller can
        // supply a queue + command pool for the initial transfer copy.
        let identity: [[f32; 4]; 4] = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        for buf in &mut bufs.bone_staging_buffers {
            buf.write_mapped(device, std::slice::from_ref(&identity))?;
        }
        // Seed DALC UBOs with the disabled-default cube so frame 0's
        // fragment read on binding 14 sees a valid `flags.x = 0`
        // (shader's fallback path). Overwritten via `upload_dalc` when
        // a Skyrim WTHR.DALC becomes active. See #993.
        let default_dalc = GpuDalcCube::default();
        for buf in &mut bufs.dalc_buffers {
            buf.write_mapped(device, std::slice::from_ref(&default_dalc))?;
        }
        // Track the seeded identity so `record_bone_copy` on frame 0
        // still propagates slot 0 to device memory even without skinned
        // content.
        let identity_bytes = std::mem::size_of::<[[f32; 4]; 4]>() as vk::DeviceSize;
        let bone_upload_bytes = vec![identity_bytes; MAX_FRAMES_IN_FLIGHT];

        log::info!(
            "Scene buffers created: {} frames, {} max lights ({} bytes/frame)",
            MAX_FRAMES_IN_FLIGHT,
            MAX_LIGHTS,
            bufs.light_buf_size,
        );

        Ok(Self {
            light_buffers: bufs.light_buffers,
            camera_buffers: bufs.camera_buffers,
            bone_staging_buffers: bufs.bone_staging_buffers,
            bone_device_buffers: bufs.bone_device_buffers,
            bone_upload_bytes,
            instance_buffers: bufs.instance_buffers,
            dalc_buffers: bufs.dalc_buffers,
            material_buffers: bufs.material_buffers,
            indirect_buffers: bufs.indirect_buffers,
            terrain_tile_buffer: bufs.terrain_tile_buffer,
            terrain_tile_buf_size: bufs.terrain_tile_buf_size,
            ray_budget_buffer: bufs.ray_budget_buffer,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_sets,
            tlas_written: vec![false; MAX_FRAMES_IN_FLIGHT],
            last_uploaded_material_hash: [None; MAX_FRAMES_IN_FLIGHT],
        })
    }
}
