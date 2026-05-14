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
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        for buf in &mut self.light_buffers {
            buf.destroy(device, allocator);
        }
        self.light_buffers.clear();
        for buf in &mut self.camera_buffers {
            buf.destroy(device, allocator);
        }
        self.camera_buffers.clear();
        for buf in &mut self.bone_staging_buffers {
            buf.destroy(device, allocator);
        }
        self.bone_staging_buffers.clear();
        for buf in &mut self.bone_device_buffers {
            buf.destroy(device, allocator);
        }
        self.bone_device_buffers.clear();
        for buf in &mut self.instance_buffers {
            buf.destroy(device, allocator);
        }
        self.instance_buffers.clear();
        for buf in &mut self.material_buffers {
            buf.destroy(device, allocator);
        }
        self.material_buffers.clear();
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
/// `std::collections::hash_map::DefaultHasher` (SipHash-1-3) — its
/// state is documented stable across `new()` calls within one
/// process, so two identical slices in the same run produce the
/// same `u64` and the upload skip is byte-content-addressable.
///
/// SipHash on a 200-material slice (~52 KB) takes ~30 µs, well under
/// the per-frame budget at 60 fps. xxh3 would be ~10× faster but
/// would require a new dependency; the hash itself is well below
/// the signal floor either way.
///
/// Routed through `GpuMaterial::as_bytes`-equivalent slice cast so
/// the same byte view used by `GpuMaterial`'s `Hash`/`Eq` impls
/// (`vulkan/material.rs:280-309`) drives the slice hash too —
/// padding handling stays consistent.
pub(super) fn hash_material_slice(materials: &[super::super::material::GpuMaterial]) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let byte_size = std::mem::size_of::<super::super::material::GpuMaterial>() * materials.len();
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
