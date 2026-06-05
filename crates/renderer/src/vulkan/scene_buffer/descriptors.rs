//! Per-frame descriptor-set updates (AO + GBuffer + cluster + TLAS).
//!
//! `write_ao_texture` / `write_geometry_buffers` / `write_cluster_buffers` /
//! `write_tlas` + `reset_ray_budget`.

use super::super::allocator::SharedAllocator;
use super::super::descriptors::{
    write_combined_image_sampler, write_storage_buffer,
};
use super::*;
use anyhow::Result;
use ash::vk;

impl super::buffers::SceneBuffers {

    /// Write the SSAO texture into the scene descriptor set for a given frame.
    pub fn write_ao_texture(
        &self,
        device: &ash::Device,
        frame_index: usize,
        ao_image_view: vk::ImageView,
        ao_sampler: vk::Sampler,
    ) {
        let image_info = [vk::DescriptorImageInfo::default()
            .sampler(ao_sampler)
            .image_view(ao_image_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let write = write_combined_image_sampler(self.descriptor_sets[frame_index], 7, &image_info);
        unsafe {
            device.update_descriptor_sets(&[write], &[]);
        }
    }

    /// Write global geometry SSBO references for RT reflection UV lookups.
    pub fn write_geometry_buffers(
        &self,
        device: &ash::Device,
        frame_index: usize,
        vertex_buffer: vk::Buffer,
        vertex_size: vk::DeviceSize,
        index_buffer: vk::Buffer,
        index_size: vk::DeviceSize,
    ) {
        let vert_info = [vk::DescriptorBufferInfo {
            buffer: vertex_buffer,
            offset: 0,
            range: vertex_size,
        }];
        let idx_info = [vk::DescriptorBufferInfo {
            buffer: index_buffer,
            offset: 0,
            range: index_size,
        }];
        let set = self.descriptor_sets[frame_index];
        let writes = [
            write_storage_buffer(set, 8, &vert_info),
            write_storage_buffer(set, 9, &idx_info),
        ];
        unsafe { device.update_descriptor_sets(&writes, &[]) }
    }

    /// Write cluster buffer references into the scene descriptor set for a given frame.
    /// Called once during init after the cluster cull pipeline is created.
    pub fn write_cluster_buffers(
        &self,
        device: &ash::Device,
        frame_index: usize,
        grid_buffer: vk::Buffer,
        grid_size: vk::DeviceSize,
        index_buffer: vk::Buffer,
        index_size: vk::DeviceSize,
    ) {
        let grid_info = [vk::DescriptorBufferInfo {
            buffer: grid_buffer,
            offset: 0,
            range: grid_size,
        }];
        let index_info = [vk::DescriptorBufferInfo {
            buffer: index_buffer,
            offset: 0,
            range: index_size,
        }];
        let set = self.descriptor_sets[frame_index];
        let writes = [
            write_storage_buffer(set, 5, &grid_info),
            write_storage_buffer(set, 6, &index_info),
        ];
        unsafe {
            device.update_descriptor_sets(&writes, &[]);
        }
    }

    /// Update the TLAS acceleration structure in the descriptor set for a given frame.
    pub fn write_tlas(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        tlas: vk::AccelerationStructureKHR,
    ) {
        self.tlas_written[frame_index] = true;
        let accel_structs = [tlas];
        let mut accel_write = vk::WriteDescriptorSetAccelerationStructureKHR::default()
            .acceleration_structures(&accel_structs);

        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.descriptor_sets[frame_index])
            .dst_binding(2)
            .descriptor_type(vk::DescriptorType::ACCELERATION_STRUCTURE_KHR)
            .descriptor_count(1)
            .push_next(&mut accel_write);

        unsafe {
            device.update_descriptor_sets(&[write], &[]);
        }
    }

    /// Zero the ray budget counter for the given frame before the render pass.
    ///
    /// Called from `draw_frame` after uploading instances and before
    /// `cmd_begin_render_pass`. The fragment shader atomically increments this
    /// counter for each Phase-3 IOR glass ray pair it fires; once the count
    /// exceeds `GLASS_RAY_BUDGET` (declared in `triangle.frag`) all further
    /// glass fragments degrade to the tier-1 cheaper path for that frame.
    pub fn reset_ray_budget(&mut self, device: &ash::Device, frame: usize) -> Result<()> {
        // #683 / MEM-2-8 — write the u32 zero at this frame's stride
        // offset within the shared buffer, then flush only that slot's
        // range on non-coherent memory. Mapped slice access bypasses
        // the from-byte-0-only `write_mapped` helper.
        let offset = (frame as vk::DeviceSize) * RAY_BUDGET_STRIDE;
        let off_usize = offset as usize;
        let mapped = self.ray_budget_buffer.mapped_slice_mut()?;
        mapped[off_usize..off_usize + 4].copy_from_slice(&0u32.to_le_bytes());
        self.ray_budget_buffer.flush_range(
            device,
            offset,
            std::mem::size_of::<u32>() as vk::DeviceSize,
        )
    }

    /// Destroy all resources.
    ///
    /// Pre-#732 LIFE-N1 the per-Vec `buf.destroy()` loops below freed
    /// every GPU allocation but never cleared the `Vec`s, so each
    /// `GpuBuffer` struct stayed alive (with `allocation: None` after
    /// `destroy`) and kept its `Arc<Mutex<Allocator>>` clone live until
    /// `SceneBuffers` itself naturally dropped — *after*
    /// `VulkanContext::Drop` had already failed `Arc::try_unwrap` and
    /// taken the warn-and-leak fall-through path. The post-fix
    /// `.clear()` calls drop each `GpuBuffer` immediately so the
    /// allocator unwrap sees a smaller strong count by the time it
    /// runs.
    ///
    /// # Safety
    ///
    /// Caller must ensure `device` and `allocator` are valid and live, the
    /// device is not lost, and that none of the scene buffers are still in
    /// use by an in-flight command buffer. The buffers must not be used
    /// after this call.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for buf in &mut self.light_buffers {
            buf.destroy(device, allocator);
        }
        self.light_buffers.clear();
        for buf in &mut self.camera_buffers {
            buf.destroy(device, allocator);
        }
        self.camera_buffers.clear();
        for buf in &mut self.bone_device_buffers {
            buf.destroy(device, allocator);
        }
        self.bone_device_buffers.clear();
        // M29.5 — bone-world + bind-inverse pairs for the palette
        // compute pass. Destroy in the same group as the palette pair.
        for buf in &mut self.bone_world_staging_buffers {
            buf.destroy(device, allocator);
        }
        self.bone_world_staging_buffers.clear();
        for buf in &mut self.bone_world_device_buffers {
            buf.destroy(device, allocator);
        }
        self.bone_world_device_buffers.clear();
        // M29.6 — single persistent SSBO + small staging.
        self.bind_inverses_persistent.destroy(device, allocator);
        self.bind_inverse_upload_staging.destroy(device, allocator);
        for buf in &mut self.instance_buffers {
            buf.destroy(device, allocator);
        }
        self.instance_buffers.clear();
        for buf in &mut self.material_buffers {
            buf.destroy(device, allocator);
        }
        self.material_buffers.clear();
        for buf in &mut self.dalc_buffers {
            buf.destroy(device, allocator);
        }
        self.dalc_buffers.clear();
        for buf in &mut self.indirect_buffers {
            buf.destroy(device, allocator);
        }
        self.indirect_buffers.clear();
        // #683 / MEM-2-8 — single shared buffer, single destroy.
        self.ray_budget_buffer.destroy(device, allocator);
        self.terrain_tile_buffer.destroy(device, allocator);
        device.destroy_descriptor_pool(self.descriptor_pool, None);
        device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
    }
}

/// Content hash of a `GpuMaterial` slice for the dirty-gate in
/// [`SceneBuffers::upload_materials`] (#878 / DIM8-01). Uses
/// `rustc_hash::FxHasher` (#1368) — seedless + deterministic, so two
/// identical slices in the same run produce the same `u64` and the
/// upload skip is byte-content-addressable.
///
/// FxHash is ~5-10× faster than the SipHash it replaced, well under
/// the per-frame budget; collision resistance is irrelevant for a
/// same-frame content gate.
///
/// Routed through `GpuMaterial::as_bytes`-equivalent slice cast so
/// the same byte view used by `GpuMaterial`'s `Hash`/`Eq` impls
/// (`vulkan/material.rs:280-309`) drives the slice hash too —
/// padding handling stays consistent.
pub(super) fn hash_material_slice(materials: &[super::super::material::GpuMaterial]) -> u64 {
    use std::hash::Hasher;
    let mut hasher = rustc_hash::FxHasher::default();
    let byte_size = std::mem::size_of_val(materials);
    // SAFETY: `GpuMaterial` is `#[repr(C)]` with f32/u32 fields and
    // explicit padding fields the producer always initialises (see
    // `GpuMaterial::as_bytes` doc at vulkan/material.rs:281-294).
    // The slice view is contiguous because `[T]` storage is too;
    // `byte_size` matches the slice's footprint exactly.
    let bytes: &[u8] =
        unsafe { std::slice::from_raw_parts(materials.as_ptr() as *const u8, byte_size) };
    hasher.write(bytes);
    hasher.finish()
}

/// Sibling of [`hash_material_slice`] for the [`SceneBuffers::upload_instances`]
/// dirty-gate (#1134 / PERF-D8-NEW-01). MedTek ships 7359 draws at 72 B
/// per `GpuInstance` ≈ 530 KB/frame; static interiors produce
/// byte-identical slices in steady state so the copy + flush skip
/// saves ~32 MB/s sustained PCIe at 60 fps.
///
/// `GpuInstance` is `#[repr(C)]` with f32 / u32 / packed-vec4 fields
/// and zero implicit padding (`gpu_instance_layout_tests` pins this);
/// the slice-byte cast is sound for the same reason `GpuMaterial`'s
/// is.
pub(super) fn hash_instance_slice(instances: &[super::gpu_types::GpuInstance]) -> u64 {
    use std::hash::Hasher;
    let mut hasher = rustc_hash::FxHasher::default();
    let byte_size = std::mem::size_of_val(instances);
    // SAFETY: see hash_material_slice — same invariant on the producer
    // side. The layout test pins the byte-size against a known constant
    // so an unintended padding insert would surface there first.
    let bytes: &[u8] =
        unsafe { std::slice::from_raw_parts(instances.as_ptr() as *const u8, byte_size) };
    hasher.write(bytes);
    hasher.finish()
}
