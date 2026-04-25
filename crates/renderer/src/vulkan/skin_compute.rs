//! M29 Phase 1 — GPU pre-skinning compute pipeline.
//!
//! Reads bind-pose vertices from the shared global vertex SSBO + the
//! per-frame bone palette, multiplies each vertex through its weighted
//! bone transform, and writes the world-space skinned vertices to a
//! per-mesh dedicated output buffer. Phase 2 will refit a per-mesh BLAS
//! against this output so RT shadow / reflection / GI ray queries see
//! the animated geometry; the rasterized pipeline keeps its existing
//! inline-skinning path (`triangle.vert:147-204`) until Phase 3
//! (optional) migrates raster to read from the same buffer.
//!
//! Phase 1 ships the pipeline + slot manager + dispatch helper.
//! It does NOT yet schedule per-frame dispatches (Phase 1.5) and the
//! output buffer is unused by raster + RT (Phase 2 wires AcceleratedManager).
//!
//! See `shaders/skin_vertices.comp` for the shader-side contract +
//! per-vertex layout (matches `vertex.rs::Vertex`, 21 floats / 84 B).

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::reflect::{validate_set_layout, ReflectedShader};
use super::sync::MAX_FRAMES_IN_FLIGHT;
use anyhow::{Context, Result};
use ash::vk;

const SKIN_VERTICES_COMP_SPV: &[u8] = include_bytes!("../../shaders/skin_vertices.comp.spv");

/// Per-vertex stride in floats (matches `Vertex` Rust struct: 21 × 4 B
/// = 84 B). Cross-checked against the shader's `VERTEX_STRIDE_FLOATS`
/// constant — drift here means the per-vertex bone-index unpack would
/// read random bytes.
pub const VERTEX_STRIDE_FLOATS: u32 = 21;
pub const VERTEX_STRIDE_BYTES: u64 = (VERTEX_STRIDE_FLOATS as u64) * 4;

/// Compute workgroup size (local_size_x in skin_vertices.comp).
const WORKGROUP_SIZE: u32 = 64;

/// Push constant payload — matches `skin_vertices.comp::PushConstants`.
/// 16 bytes (4 × u32) keeps the layout in the safe portion of every
/// device's `maxPushConstantsSize` (128 B minimum).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SkinPushConstants {
    /// Where this mesh's bind-pose vertices start in the input SSBO
    /// (in vertices, not floats).
    pub vertex_offset: u32,
    pub vertex_count: u32,
    /// Where this mesh's bone palette starts in the BoneBuffer (in
    /// mat4 entries). Must match the value the inline-skinning vertex
    /// shader reads from `GpuInstance.boneOffset`.
    pub bone_offset: u32,
    pub _pad: u32,
}

const PUSH_CONSTANTS_SIZE: u32 = std::mem::size_of::<SkinPushConstants>() as u32;

/// Per-skinned-mesh output buffer + descriptor sets. Allocated lazily
/// on first dispatch and held until the owning entity is destroyed.
/// The descriptor sets bind the shared input + per-frame bone palette
/// against this slot's dedicated output buffer; one set per
/// frame-in-flight so the bone-palette buffer rotation stays correct.
pub struct SkinSlot {
    /// Skinned-vertex output buffer. Sized for `vertex_count` ×
    /// `VERTEX_STRIDE_BYTES`. Layout-identical to the input SSBO so
    /// Phase 3 can swap raster to read from it directly.
    pub output_buffer: GpuBuffer,
    /// Capacity of the output buffer in bytes (equal to the active
    /// vertex_count × VERTEX_STRIDE_BYTES at allocation time).
    pub output_size: vk::DeviceSize,
    /// One descriptor set per frame-in-flight — each binds (input,
    /// bone palette for that frame, this slot's output).
    descriptor_sets: [vk::DescriptorSet; MAX_FRAMES_IN_FLIGHT],
    /// Vertex count this slot was sized for. If the underlying mesh's
    /// vertex_count changes (NIF re-import, mod swap), the slot must
    /// be destroyed + recreated rather than reused.
    vertex_count: u32,
}

impl SkinSlot {
    /// Number of vertices this slot was sized for.
    pub fn vertex_count(&self) -> u32 {
        self.vertex_count
    }
}

/// GPU pre-skinning compute pipeline.
///
/// Owns the pipeline + descriptor-set layout + descriptor pool. Slot
/// allocation (per skinned mesh) goes through `create_slot` and
/// returns an opaque `SkinSlot` the caller hands back to `dispatch`.
/// Lifecycle: pipeline lives for the renderer's lifetime; slots live
/// for the skinned mesh's lifetime.
pub struct SkinComputePipeline {
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    /// Cached input vertex SSBO handle — same buffer for every slot.
    /// Captured at pipeline creation; rebuilds (e.g. cell transition
    /// growing the global vertex buffer) require notifying the
    /// pipeline so existing slots' descriptor sets get rewritten.
    /// Phase 1 doesn't address this (no per-frame dispatch yet).
    input_buffer: vk::Buffer,
    input_buffer_size: vk::DeviceSize,
    /// Per-frame bone palette buffer handles. Slot descriptor sets
    /// reference these on creation.
    bone_buffers: [vk::Buffer; MAX_FRAMES_IN_FLIGHT],
    bone_buffer_size: vk::DeviceSize,
}

impl SkinComputePipeline {
    /// Create the pipeline. `input_buffer` is the global vertex SSBO
    /// (same buffer the rasterized vertex shader reads via location
    /// 0..7 attributes); `bone_buffers` are the per-frame palette
    /// SSBOs (same buffers `triangle.vert` binds at set=1, binding=3).
    pub fn new(
        device: &ash::Device,
        pipeline_cache: vk::PipelineCache,
        input_buffer: vk::Buffer,
        input_buffer_size: vk::DeviceSize,
        bone_buffers: [vk::Buffer; MAX_FRAMES_IN_FLIGHT],
        bone_buffer_size: vk::DeviceSize,
        max_slots: u32,
    ) -> Result<Self> {
        // Descriptor set layout — 3 storage buffers (input, palette,
        // output). One set per (slot × frame_in_flight); the pool
        // sizes for `max_slots × MAX_FRAMES_IN_FLIGHT × 3` storage
        // buffer descriptors total.
        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
            vk::DescriptorSetLayoutBinding::default()
                .binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE),
        ];
        // Cross-check the layout against SPIR-V reflection so a future
        // shader edit that reorders / renames bindings fails the build
        // instead of silently mis-binding. See cluster_cull's same
        // pattern + #427.
        validate_set_layout(
            0,
            &bindings,
            &[ReflectedShader {
                name: "skin_vertices.comp",
                spirv: SKIN_VERTICES_COMP_SPV,
            }],
            "skin_compute",
            &[],
        )
        .expect("skin_compute layout drifted against skin_vertices.comp");

        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(
                    &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                    None,
                )
                .context("create skin_compute descriptor set layout")?
        };

        // Pipeline layout with a single push constant range covering
        // SkinPushConstants. Compute-only access.
        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::COMPUTE)
            .offset(0)
            .size(PUSH_CONSTANTS_SIZE);
        let pipeline_layout = unsafe {
            device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::default()
                        .set_layouts(std::slice::from_ref(&descriptor_set_layout))
                        .push_constant_ranges(std::slice::from_ref(&push_range)),
                    None,
                )
                .context("create skin_compute pipeline layout")?
        };

        // Compile the compute pipeline.
        let shader_module = super::pipeline::load_shader_module(device, SKIN_VERTICES_COMP_SPV)?;
        let pipeline_result = unsafe {
            device.create_compute_pipelines(
                pipeline_cache,
                &[vk::ComputePipelineCreateInfo::default()
                    .stage(
                        vk::PipelineShaderStageCreateInfo::default()
                            .stage(vk::ShaderStageFlags::COMPUTE)
                            .module(shader_module)
                            .name(c"main"),
                    )
                    .layout(pipeline_layout)],
                None,
            )
        };
        unsafe { device.destroy_shader_module(shader_module, None) };
        let pipeline = match pipeline_result {
            Ok(pipelines) => pipelines[0],
            Err((_, e)) => {
                // Roll back what we already created — the partial
                // struct path used in cluster_cull is overkill for
                // three resources; explicit cleanup is clearer here.
                unsafe {
                    device.destroy_pipeline_layout(pipeline_layout, None);
                    device.destroy_descriptor_set_layout(descriptor_set_layout, None);
                }
                return Err(e).context("create skin_compute pipeline");
            }
        };

        // Descriptor pool — sized for max_slots × MAX_FRAMES_IN_FLIGHT
        // descriptor sets, each consuming 3 storage buffers (input,
        // palette, output). max_slots == 32 (matches MAX_TOTAL_BONES /
        // MAX_BONES_PER_MESH) covers every realistic interior cell.
        let pool_total = max_slots * (MAX_FRAMES_IN_FLIGHT as u32);
        let pool_sizes = [vk::DescriptorPoolSize {
            ty: vk::DescriptorType::STORAGE_BUFFER,
            descriptor_count: pool_total * 3,
        }];
        let descriptor_pool = unsafe {
            device
                .create_descriptor_pool(
                    &vk::DescriptorPoolCreateInfo::default()
                        .pool_sizes(&pool_sizes)
                        .max_sets(pool_total)
                        // Slots are freed on entity destruction (cell
                        // unload); FREE_DESCRIPTOR_SET allows
                        // `vkFreeDescriptorSets` rather than only
                        // `vkResetDescriptorPool`.
                        .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET),
                    None,
                )
                .context("create skin_compute descriptor pool")?
        };

        log::info!(
            "Skin compute pipeline created (max_slots={}, sets={}, push={} B)",
            max_slots,
            pool_total,
            PUSH_CONSTANTS_SIZE,
        );

        Ok(Self {
            pipeline,
            pipeline_layout,
            descriptor_set_layout,
            descriptor_pool,
            input_buffer,
            input_buffer_size,
            bone_buffers,
            bone_buffer_size,
        })
    }

    /// Allocate a per-mesh slot. The caller owns the returned
    /// `SkinSlot` and must hand it back to [`Self::destroy_slot`]
    /// before this pipeline is destroyed.
    pub fn create_slot(
        &self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        vertex_count: u32,
    ) -> Result<SkinSlot> {
        let output_size = (vertex_count as u64) * VERTEX_STRIDE_BYTES;
        // Phase 2 will set this with VERTEX_BUFFER + ACCELERATION_STRUCTURE_BUILD_INPUT
        // so the BLAS refit can read from it directly. STORAGE_BUFFER
        // is sufficient for Phase 1 (compute write only).
        let output_buffer = GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            output_size,
            vk::BufferUsageFlags::STORAGE_BUFFER,
        )
        .context("allocate skin slot output buffer")?;

        // One descriptor set per frame-in-flight.
        let layouts = [self.descriptor_set_layout; MAX_FRAMES_IN_FLIGHT];
        let allocated = unsafe {
            device
                .allocate_descriptor_sets(
                    &vk::DescriptorSetAllocateInfo::default()
                        .descriptor_pool(self.descriptor_pool)
                        .set_layouts(&layouts),
                )
                .context("allocate skin slot descriptor sets")?
        };
        let mut descriptor_sets = [vk::DescriptorSet::null(); MAX_FRAMES_IN_FLIGHT];
        for (i, set) in allocated.iter().enumerate() {
            descriptor_sets[i] = *set;
        }

        // Wire each frame-in-flight set: shared input, per-frame
        // bone palette, this slot's dedicated output.
        for frame in 0..MAX_FRAMES_IN_FLIGHT {
            let input_info = [vk::DescriptorBufferInfo {
                buffer: self.input_buffer,
                offset: 0,
                range: self.input_buffer_size,
            }];
            let bone_info = [vk::DescriptorBufferInfo {
                buffer: self.bone_buffers[frame],
                offset: 0,
                range: self.bone_buffer_size,
            }];
            let output_info = [vk::DescriptorBufferInfo {
                buffer: output_buffer.buffer,
                offset: 0,
                range: output_size,
            }];
            let writes = [
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[frame])
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&input_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[frame])
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&bone_info),
                vk::WriteDescriptorSet::default()
                    .dst_set(descriptor_sets[frame])
                    .dst_binding(2)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&output_info),
            ];
            unsafe { device.update_descriptor_sets(&writes, &[]) };
        }

        Ok(SkinSlot {
            output_buffer,
            output_size,
            descriptor_sets,
            vertex_count,
        })
    }

    /// Free the GPU resources behind a slot. Caller must ensure the
    /// slot's output buffer isn't referenced by an in-flight command
    /// buffer (typical pattern: defer via the `MeshRegistry`'s
    /// `deferred_destroy` slot, or call after a `device_wait_idle`).
    pub fn destroy_slot(
        &self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        mut slot: SkinSlot,
    ) {
        unsafe {
            // Free the descriptor sets back to the pool. Required
            // because the pool was created with FREE_DESCRIPTOR_SET;
            // without this call the pool would leak descriptor slots
            // until pool reset / destruction.
            let _ = device.free_descriptor_sets(self.descriptor_pool, &slot.descriptor_sets);
        }
        slot.output_buffer.destroy(device, allocator);
    }

    /// Record a dispatch into `cmd` that pre-skins this slot's
    /// vertices. Must be called between the bone-palette upload for
    /// `frame_index` and any consumer of the output buffer. Phase 1
    /// has no consumers; Phase 2 inserts a COMPUTE→ACCELERATION_STRUCTURE_BUILD
    /// barrier on the output buffer before BLAS refit.
    pub unsafe fn dispatch(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        slot: &SkinSlot,
        frame_index: usize,
        push: SkinPushConstants,
    ) {
        device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[slot.descriptor_sets[frame_index]],
            &[],
        );
        // SAFETY: `SkinPushConstants` is `repr(C)` with all u32 fields,
        // 16 bytes, no padding. The slice is contiguous + aligned.
        let bytes = std::slice::from_raw_parts(
            (&push as *const SkinPushConstants) as *const u8,
            PUSH_CONSTANTS_SIZE as usize,
        );
        device.cmd_push_constants(
            cmd,
            self.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            bytes,
        );
        let groups = push.vertex_count.div_ceil(WORKGROUP_SIZE);
        device.cmd_dispatch(cmd, groups, 1, 1);
    }

    /// Tear down the pipeline + descriptor pool. Caller must
    /// destroy every outstanding slot first (slots hold descriptor
    /// sets from the pool; destroying the pool while sets are
    /// outstanding triggers a Vulkan validation error). The renderer
    /// pairs this with `device_wait_idle` in the Drop chain.
    pub unsafe fn destroy(&mut self, device: &ash::Device) {
        device.destroy_pipeline(self.pipeline, None);
        device.destroy_pipeline_layout(self.pipeline_layout, None);
        device.destroy_descriptor_pool(self.descriptor_pool, None);
        device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pin the per-vertex stride against the Rust `Vertex` size — the
    /// shader hardcodes 21 floats / 84 bytes per vertex; if a vertex
    /// field is added without bumping `VERTEX_STRIDE_FLOATS` here AND
    /// `VERTEX_STRIDE_FLOATS` in the shader, the compute pass would
    /// read past the end of each vertex and write the wrong target
    /// vertex. Phase 1 catch — the renderer crate has no Vulkan-free
    /// test path for the rest of the pipeline.
    #[test]
    fn vertex_stride_matches_rust_vertex_size() {
        use crate::vertex::Vertex;
        assert_eq!(
            std::mem::size_of::<Vertex>(),
            (VERTEX_STRIDE_FLOATS * 4) as usize,
            "VERTEX_STRIDE_FLOATS ({}) × 4 must equal size_of::<Vertex>() — \
             skin_vertices.comp will read garbage if these drift",
            VERTEX_STRIDE_FLOATS,
        );
        assert_eq!(VERTEX_STRIDE_BYTES, 84);
    }

    /// Push constant payload size must fit in the conservative 128 B
    /// minimum every Vulkan implementation guarantees, AND must match
    /// the shader's declared `PushConstants` block. A second runtime
    /// check inside `new()` would verify alignment, but a static-size
    /// assert here catches the common drift case (adding a field
    /// without updating both sides).
    #[test]
    fn push_constants_size_is_16_bytes() {
        assert_eq!(PUSH_CONSTANTS_SIZE, 16);
        assert_eq!(std::mem::size_of::<SkinPushConstants>(), 16);
    }
}
