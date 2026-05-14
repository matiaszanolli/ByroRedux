//! Per-frame upload-and-flush routines for every SSBO.
//!
//! `upload_lights` / `upload_camera` / `upload_bones` / `upload_instances` /
//! `upload_materials` / `upload_indirect_draws` / `upload_terrain_tiles` +
//! the `record_bone_copy` GPU-to-GPU shortcut.

use super::super::allocator::SharedAllocator;
use super::super::buffer::GpuBuffer;
use super::super::sync::MAX_FRAMES_IN_FLIGHT;
use super::*;
use super::descriptors::hash_material_slice;
use super::buffers::LightHeader;
use anyhow::{Context, Result};
use ash::vk;

impl super::buffers::SceneBuffers {

    /// Upload light data for the current frame-in-flight.
    pub fn upload_lights(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        lights: &[GpuLight],
    ) -> Result<()> {
        let count = lights.len().min(MAX_LIGHTS);
        let header = LightHeader {
            count: count as u32,
            _pad: [0; 3],
        };

        let header_size = std::mem::size_of::<LightHeader>();
        let light_size = std::mem::size_of::<GpuLight>();

        // Write directly to mapped GPU memory — no intermediate Vec allocation.
        let buf = &mut self.light_buffers[frame_index];
        let mapped = buf.mapped_slice_mut()?;

        // SAFETY: LightHeader and GpuLight are #[repr(C)] with plain f32/u32 fields.
        // mapped buffer is sized for MAX_LIGHTS. No overlap between header and light regions.
        unsafe {
            std::ptr::copy_nonoverlapping(
                &header as *const LightHeader as *const u8,
                mapped.as_mut_ptr(),
                header_size,
            );
            if count > 0 {
                std::ptr::copy_nonoverlapping(
                    lights.as_ptr() as *const u8,
                    mapped.as_mut_ptr().add(header_size),
                    light_size * count,
                );
            }
        }

        buf.flush_if_needed(device)
    }

    /// Upload camera data for the current frame-in-flight.
    pub fn upload_camera(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        camera: &GpuCamera,
    ) -> Result<()> {
        self.camera_buffers[frame_index].write_mapped(device, std::slice::from_ref(camera))
    }

    /// Upload the bone palette for the current frame-in-flight into the
    /// HOST_VISIBLE staging buffer. The matching DEVICE_LOCAL slot is
    /// populated by [`record_bone_copy`] once a recording command buffer
    /// is available — until then the shader still sees last frame's
    /// device contents.
    ///
    /// `palette` is packed contiguous mat4 entries in column-major glam
    /// layout. Slot 0 is always the identity matrix — callers that
    /// assemble multiple meshes into one palette should keep slot 0 as
    /// identity and start writing mesh bones at slot 1.
    ///
    /// Writes at most `MAX_TOTAL_BONES` entries; extra are silently
    /// clamped and logged once per session by the caller.
    pub fn upload_bones(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        palette: &[[[f32; 4]; 4]],
    ) -> Result<()> {
        let count = palette.len().min(MAX_TOTAL_BONES);
        if count == 0 {
            return Ok(());
        }

        let byte_size = (std::mem::size_of::<[[f32; 4]; 4]>() * count) as vk::DeviceSize;
        let buf = &mut self.bone_staging_buffers[frame_index];
        let mapped = buf.mapped_slice_mut()?;
        // SAFETY: [[f32; 4]; 4] is #[repr(C)]-compatible with std430 mat4.
        // bone_staging_buffers are sized for MAX_TOTAL_BONES slots; count is clamped.
        unsafe {
            std::ptr::copy_nonoverlapping(
                palette.as_ptr() as *const u8,
                mapped.as_mut_ptr(),
                byte_size as usize,
            );
        }
        buf.flush_if_needed(device)?;
        // Record exactly how many bytes need to ride the staging→device
        // copy for this frame so `record_bone_copy` doesn't transfer the
        // full ~2 MB slab when only a few bones were written.
        self.bone_upload_bytes[frame_index] = byte_size;
        Ok(())
    }

    /// Record the staging→device bone-palette copy and the visibility
    /// barrier on `cmd`, scoped to the bytes most recently written by
    /// [`upload_bones`] for this frame.
    ///
    /// The barrier widens the dst stage mask to cover every consumer of
    /// the device buffer:
    ///   * `COMPUTE_SHADER` — M29 GPU pre-skin pass (`SkinComputePipeline`)
    ///     reads the palette before issuing per-vertex skinning into the
    ///     entity's output buffer.
    ///   * `VERTEX_SHADER` — fallback CPU-feeds + raster vertex skinning
    ///     read binding 3 (current frame) and binding 12 (previous frame).
    ///
    /// Callers MUST invoke this on every command buffer that consumes the
    /// palette — both the main per-frame command buffer (steady-state
    /// dispatch + raster vertex stage) and the one-time "prime" command
    /// buffers used for first-sight skinned BLAS builds. The copy is
    /// idempotent: a redundant call on the main cmd buffer after the prime
    /// finished copies the same bytes again, which is harmless.
    pub fn record_bone_copy(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame_index: usize,
    ) {
        let byte_size = self.bone_upload_bytes[frame_index];
        if byte_size == 0 {
            return;
        }
        let copy = vk::BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size: byte_size,
        };
        unsafe {
            device.cmd_copy_buffer(
                cmd,
                self.bone_staging_buffers[frame_index].buffer,
                self.bone_device_buffers[frame_index].buffer,
                &[copy],
            );
            // Make the copied range visible to every shader stage that
            // reads the palette. Buffer barrier (not global) so we don't
            // perturb unrelated cache state on the same submission.
            let barrier = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(self.bone_device_buffers[frame_index].buffer)
                .offset(0)
                .size(byte_size);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::VERTEX_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[barrier],
                &[],
            );
        }
    }

    /// Copy the identity-matrix seed in slot 0 of every staging buffer
    /// to its matching DEVICE_LOCAL slot via a one-time command buffer,
    /// so the first frame's binding-12 read (previous-frame palette) and
    /// the rigid-vertex fallback path see a valid transform in slot 0
    /// from frame 0. Mirrors the pre-#921 invariant where the
    /// host-visible bone buffers were directly mapped and slot 0 was
    /// seeded with the identity by `write_mapped` in `new()`.
    pub fn seed_identity_bones(
        &self,
        device: &ash::Device,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
    ) -> Result<()> {
        let identity_bytes = std::mem::size_of::<[[f32; 4]; 4]>() as vk::DeviceSize;
        super::super::texture::with_one_time_commands(device, queue, command_pool, |cmd| {
            for i in 0..MAX_FRAMES_IN_FLIGHT {
                let copy = vk::BufferCopy {
                    src_offset: 0,
                    dst_offset: 0,
                    size: identity_bytes,
                };
                unsafe {
                    device.cmd_copy_buffer(
                        cmd,
                        self.bone_staging_buffers[i].buffer,
                        self.bone_device_buffers[i].buffer,
                        &[copy],
                    );
                }
            }
            Ok(())
        })
    }

    /// Upload per-instance data for the current frame-in-flight.
    ///
    /// Called once per frame before the render pass. The vertex shader reads
    /// `instances[gl_InstanceIndex]` for model matrix, texture index, and bone offset.
    pub fn upload_instances(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        instances: &[GpuInstance],
    ) -> Result<()> {
        let count = instances.len().min(MAX_INSTANCES);
        if instances.len() > MAX_INSTANCES {
            log::warn!(
                "Instance SSBO overflow: {} instances submitted, capped at {} — excess draws silently dropped. #279 P2-12",
                instances.len(),
                MAX_INSTANCES,
            );
        }
        if count == 0 {
            return Ok(());
        }
        let buf = &mut self.instance_buffers[frame_index];
        let mapped = buf.mapped_slice_mut()?;
        let byte_size = std::mem::size_of::<GpuInstance>() * count;
        // SAFETY: GpuInstance is #[repr(C)] with plain f32/u32 fields.
        // instance_buffers are sized for MAX_INSTANCES; count is clamped.
        unsafe {
            std::ptr::copy_nonoverlapping(
                instances.as_ptr() as *const u8,
                mapped.as_mut_ptr(),
                byte_size,
            );
        }
        buf.flush_if_needed(device)
    }

    /// Get a mutable reference to the mapped instance buffer for direct writes.
    /// Used by the UI overlay to append a single instance after the bulk upload.
    pub fn instance_buffer_mapped_mut(&mut self, frame_index: usize) -> Result<&mut [u8]> {
        self.instance_buffers[frame_index].mapped_slice_mut()
    }

    /// Upload the deduplicated material table for the current
    /// frame-in-flight (R1 Phase 4). Called once per frame after
    /// `build_render_data` has populated the table; the fragment
    /// shader reads `materials[instance.materialId]` for migrated
    /// fields. Empty table is a no-op (no draws → no material reads).
    pub fn upload_materials(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        materials: &[super::super::material::GpuMaterial],
    ) -> Result<()> {
        let count = materials.len().min(MAX_MATERIALS);
        if materials.len() > MAX_MATERIALS {
            log::warn!(
                "Material table overflow: {} materials submitted, capped at {} \
                 — instances pointing past the cap silently default to material 0",
                materials.len(),
                MAX_MATERIALS,
            );
        }
        if count == 0 {
            return Ok(());
        }

        // #878 / DIM8-01 — dirty-gate via content hash. Static
        // interior cells produce a byte-identical materials slice
        // every frame; skipping the copy + flush in steady state
        // saves ~3 MB/s sustained PCIe traffic at 60 fps with 200
        // unique materials. The hash is computed over the clamped
        // prefix actually written to the buffer (`materials[..count]`)
        // so an overflow that drops trailing materials still
        // re-uploads when the kept prefix changes.
        let hash = hash_material_slice(&materials[..count]);
        if self.last_uploaded_material_hash[frame_index] == Some(hash) {
            return Ok(());
        }

        let buf = &mut self.material_buffers[frame_index];
        let mapped = buf.mapped_slice_mut()?;
        let byte_size = std::mem::size_of::<super::super::material::GpuMaterial>() * count;
        // SAFETY: GpuMaterial is #[repr(C)] with f32/u32 fields and
        // explicit padding (no implicit Drop, no uninitialised bytes).
        // material_buffers are sized for MAX_MATERIALS; count is clamped.
        unsafe {
            std::ptr::copy_nonoverlapping(
                materials.as_ptr() as *const u8,
                mapped.as_mut_ptr(),
                byte_size,
            );
        }
        buf.flush_if_needed(device)?;
        // Stamp the hash AFTER a successful flush — a flush failure
        // leaves the buffer in an indeterminate state, so we want
        // the next call to re-upload rather than skip.
        self.last_uploaded_material_hash[frame_index] = Some(hash);
        Ok(())
    }

    /// Upload `VkDrawIndexedIndirectCommand` entries for the current
    /// frame-in-flight. The draw loop issues one
    /// `vkCmdDrawIndexedIndirect` per pipeline group, reading a
    /// contiguous range of this buffer. See #309.
    ///
    /// Clamps at [`MAX_INDIRECT_DRAWS`] and logs a warn on overflow —
    /// real scenes with the #272 instanced batching rarely emit more
    /// than a few hundred batches per frame, so the clamp is a
    /// defense-in-depth against unbounded-growth bugs.
    pub fn upload_indirect_draws(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        draws: &[vk::DrawIndexedIndirectCommand],
    ) -> Result<()> {
        let count = draws.len().min(MAX_INDIRECT_DRAWS);
        if draws.len() > MAX_INDIRECT_DRAWS {
            log::warn!(
                "Indirect draw overflow: {} commands submitted, capped at {} — excess draws silently dropped",
                draws.len(),
                MAX_INDIRECT_DRAWS,
            );
        }
        if count == 0 {
            return Ok(());
        }
        let buf = &mut self.indirect_buffers[frame_index];
        let mapped = buf.mapped_slice_mut()?;
        let byte_size = std::mem::size_of::<vk::DrawIndexedIndirectCommand>() * count;
        // SAFETY: VkDrawIndexedIndirectCommand is a Vulkan-defined C struct
        // with the exact layout expected by the device. `indirect_buffers`
        // are sized for MAX_INDIRECT_DRAWS; count is clamped above.
        unsafe {
            std::ptr::copy_nonoverlapping(
                draws.as_ptr() as *const u8,
                mapped.as_mut_ptr(),
                byte_size,
            );
        }
        buf.flush_if_needed(device)
    }

    /// Return the `VkBuffer` handle for the current frame's indirect
    /// buffer. The draw loop passes this to `cmd_draw_indexed_indirect`.
    pub fn indirect_buffer(&self, frame_index: usize) -> vk::Buffer {
        self.indirect_buffers[frame_index].buffer
    }

    /// Upload terrain tile data into the single DEVICE_LOCAL SSBO via
    /// a staging buffer + one-time `vkCmdCopyBuffer`. Called from the
    /// cell loader path after `spawn_terrain_mesh` packs per-tile layer
    /// texture indices. The data is static until the next cell
    /// transition, so exactly one upload per dirty transition is
    /// enough — no per-frame double-buffering. See #470 / #497.
    pub fn upload_terrain_tiles(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
        tiles: &[GpuTerrainTile],
    ) -> Result<()> {
        let count = tiles.len().min(MAX_TERRAIN_TILES);
        if tiles.len() > MAX_TERRAIN_TILES {
            log::warn!(
                "Terrain tile SSBO overflow: {} tiles submitted, capped at {} — excess slots silently dropped. #470",
                tiles.len(),
                MAX_TERRAIN_TILES,
            );
        }
        if count == 0 {
            return Ok(());
        }

        let byte_size = (std::mem::size_of::<GpuTerrainTile>() * count) as vk::DeviceSize;

        // Create a transient staging buffer. Terrain tile uploads run
        // at cell-transition frequency (a few times a minute at most),
        // so skip the StagingPool reuse overhead — a one-shot 32 KB
        // CpuToGpu allocation is cheap and the buffer vanishes cleanly
        // via the guard below on any exit path.
        let staging_info = vk::BufferCreateInfo::default()
            .size(byte_size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let staging_buffer = unsafe {
            device
                .create_buffer(&staging_info, None)
                .context("Failed to create terrain tile staging buffer")?
        };
        let reqs = unsafe { device.get_buffer_memory_requirements(staging_buffer) };
        let mut staging_alloc = allocator
            .lock()
            .expect("allocator lock poisoned")
            .allocate(&gpu_allocator::vulkan::AllocationCreateDesc {
                name: "terrain_tile_staging",
                requirements: reqs,
                location: gpu_allocator::MemoryLocation::CpuToGpu,
                linear: true,
                allocation_scheme: gpu_allocator::vulkan::AllocationScheme::GpuAllocatorManaged,
            })
            .context("Failed to allocate terrain tile staging memory")?;
        super::super::buffer::debug_assert_cpu_to_gpu_mapped(&staging_alloc, "terrain_tile_staging");
        unsafe {
            device
                .bind_buffer_memory(
                    staging_buffer,
                    staging_alloc.memory(),
                    staging_alloc.offset(),
                )
                .context("Failed to bind terrain tile staging buffer")?;
        }

        // SAFETY: GpuTerrainTile is #[repr(C)] with u32-only fields
        // matching std430. Staging was sized to `byte_size` above.
        let mapped = staging_alloc
            .mapped_slice_mut()
            .context("Terrain tile staging not mapped")?;
        unsafe {
            std::ptr::copy_nonoverlapping(
                tiles.as_ptr() as *const u8,
                mapped.as_mut_ptr(),
                byte_size as usize,
            );
        }

        let copy = vk::BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size: byte_size,
        };
        let dst = self.terrain_tile_buffer.buffer;
        let result = super::super::texture::with_one_time_commands(device, queue, command_pool, |cmd| {
            unsafe {
                device.cmd_copy_buffer(cmd, staging_buffer, dst, &[copy]);
            }
            Ok(())
        });

        // Tear down staging regardless of copy outcome.
        unsafe {
            device.destroy_buffer(staging_buffer, None);
        }
        allocator
            .lock()
            .expect("allocator lock poisoned")
            .free(staging_alloc)
            .ok();

        // Suppress "field never read" on the cached size — kept for
        // future layout changes / debugging introspection.
        let _ = self.terrain_tile_buf_size;

        result
    }

    /// Get the light buffers (for compute pipeline descriptor writes).
    pub fn light_buffers(&self) -> &[GpuBuffer] {
        &self.light_buffers
    }

    /// Get the camera buffers (for compute pipeline descriptor writes).
    pub fn camera_buffers(&self) -> &[GpuBuffer] {
        &self.camera_buffers
    }

    /// Get the instance buffers (for the caustic pipeline's descriptor writes).
    pub fn instance_buffers(&self) -> &[GpuBuffer] {
        &self.instance_buffers
    }

    /// Get the per-frame DEVICE_LOCAL bone palette buffers (M29 — skin
    /// compute reads them as the bone-matrix source per-dispatch). After
    /// #921 these are the device-side targets of the staging copy
    /// scheduled by [`record_bone_copy`]; the host-visible staging
    /// buffers are private.
    pub fn bone_buffers(&self) -> &[GpuBuffer] {
        &self.bone_device_buffers
    }

    /// Bone palette buffer size in bytes (`MAX_TOTAL_BONES × mat4`).
    pub fn bone_buffer_size(&self) -> vk::DeviceSize {
        (std::mem::size_of::<[[f32; 4]; 4]>() * MAX_TOTAL_BONES) as vk::DeviceSize
    }

    /// Light buffer size in bytes.
    pub fn light_buffer_size(&self) -> vk::DeviceSize {
        (std::mem::size_of::<LightHeader>() + std::mem::size_of::<GpuLight>() * MAX_LIGHTS)
            as vk::DeviceSize
    }

    /// Camera buffer size in bytes.
    pub fn camera_buffer_size(&self) -> vk::DeviceSize {
        std::mem::size_of::<GpuCamera>() as vk::DeviceSize
    }

    /// Instance buffer size in bytes.
    pub fn instance_buffer_size(&self) -> vk::DeviceSize {
        (std::mem::size_of::<GpuInstance>() * MAX_INSTANCES) as vk::DeviceSize
    }

    /// Get the descriptor set for the current frame-in-flight.
    pub fn descriptor_set(&self, frame_index: usize) -> vk::DescriptorSet {
        self.descriptor_sets[frame_index]
    }
}
