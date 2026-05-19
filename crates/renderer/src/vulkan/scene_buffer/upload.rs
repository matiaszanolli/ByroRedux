//! Per-frame upload-and-flush routines for every SSBO.
//!
//! `upload_lights` / `upload_camera` / `upload_bones` / `upload_instances` /
//! `upload_materials` / `upload_indirect_draws` / `upload_terrain_tiles` +
//! the `record_bone_copy` GPU-to-GPU shortcut.

use super::super::allocator::SharedAllocator;
use super::super::buffer::GpuBuffer;
use super::*;
use super::descriptors::{hash_instance_slice, hash_material_slice};
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

    /// Upload the 6-axis directional ambient cube for the current
    /// frame-in-flight. Consumed by `triangle.frag` at descriptor set 1
    /// binding 14. The cube's `flags.x` field gates the consumer: when
    /// the host writes a disabled cube (the `GpuDalcCube::default()` —
    /// all zeros + `flags = 0`), the shader falls back to the legacy
    /// AMBIENT_AO_FLOOR path. See #993 / REN-AMBIENT-DALC.
    pub fn upload_dalc(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        dalc: &GpuDalcCube,
    ) -> Result<()> {
        self.dalc_buffers[frame_index].write_mapped(device, std::slice::from_ref(dalc))
    }

    // M29.5 cleanup — `upload_bones` + `record_bone_copy` (the legacy
    // pre-multiplied palette upload + staging→device transfer path)
    // removed here. `build_render_data` now produces the two input
    // arrays `bone_world` + `bind_inverses` rather than the
    // multiplied palette; `upload_bone_inputs` + `record_bone_inputs_copy`
    // (below) ship them to the device, and the new
    // `skin_palette.comp` dispatch in `draw_frame` does the per-slot
    // multiply and writes the palette buffer directly. The historical
    // staging buffer + transfer-copy code is preserved in git history
    // up to the M29.5-cleanup commit.

    /// M29.5 — write the per-frame bone-world + bind-inverses input
    /// pair into the staging buffers consumed by `skin_palette.comp`.
    ///
    /// Replaces the host-side `bone_world × bind_inverses` multiply
    /// that [`upload_bones`] used to receive pre-multiplied. The two
    /// slices must be parallel (same length, same per-slot meaning) —
    /// each `bone_world[i]` corresponds to its `bind_inverses[i]`, and
    /// the compute pass writes `palette[i] = bone_world[i] *
    /// bind_inverses[i]` into the existing `bone_device_buffers[frame]`
    /// slot. Both slices are clamped at `MAX_TOTAL_BONES` to match the
    /// SSBO sizing — the caller's overflow path (`render/skinned.rs`)
    /// already gates upstream of this so the clamp is defensive only.
    ///
    /// Records the byte-count into `bone_input_upload_bytes[frame]`
    /// for [`record_bone_inputs_copy`] to consume.
    pub fn upload_bone_inputs(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        bone_world: &[[[f32; 4]; 4]],
        bind_inverses: &[[[f32; 4]; 4]],
    ) -> Result<()> {
        debug_assert_eq!(
            bone_world.len(),
            bind_inverses.len(),
            "bone_world and bind_inverses must be parallel slices (M29.5 \
             per-slot palette[i] = world[i] * bind_inv[i])"
        );
        let count = bone_world.len().min(bind_inverses.len()).min(MAX_TOTAL_BONES);
        if count == 0 {
            self.bone_input_upload_bytes[frame_index] = 0;
            return Ok(());
        }

        let byte_size = (std::mem::size_of::<[[f32; 4]; 4]>() * count) as vk::DeviceSize;
        // bone_world
        let world_buf = &mut self.bone_world_staging_buffers[frame_index];
        let world_mapped = world_buf.mapped_slice_mut()?;
        // SAFETY: [[f32;4];4] is repr(C)-compatible with std430 mat4.
        // bone_world_staging_buffers are sized for MAX_TOTAL_BONES; count
        // is clamped above.
        unsafe {
            std::ptr::copy_nonoverlapping(
                bone_world.as_ptr() as *const u8,
                world_mapped.as_mut_ptr(),
                byte_size as usize,
            );
        }
        world_buf.flush_if_needed(device)?;
        // bind_inverses
        let bind_buf = &mut self.bind_inverse_staging_buffers[frame_index];
        let bind_mapped = bind_buf.mapped_slice_mut()?;
        // SAFETY: same shape + size invariant as above. The two staging
        // buffers were created back-to-back in `allocate_scene_render_buffers`
        // with identical `bone_buf_size`.
        unsafe {
            std::ptr::copy_nonoverlapping(
                bind_inverses.as_ptr() as *const u8,
                bind_mapped.as_mut_ptr(),
                byte_size as usize,
            );
        }
        bind_buf.flush_if_needed(device)?;
        self.bone_input_upload_bytes[frame_index] = byte_size;
        Ok(())
    }

    /// M29.5 — record the staging→device copies for the bone_world +
    /// bind_inverses pair and the visibility barrier that makes them
    /// readable by `skin_palette.comp`.
    ///
    /// Layout: two `cmd_copy_buffer` calls followed by ONE buffer
    /// barrier (TRANSFER_WRITE → COMPUTE_SHADER_READ) covering both
    /// device buffers. The shader reads both at the same dispatch so
    /// merging the barrier saves one pipeline-barrier emission per
    /// frame without affecting visibility semantics.
    ///
    /// Caller (draw_frame) must invoke this AFTER [`upload_bone_inputs`]
    /// for the same frame and BEFORE the `skin_palette` dispatch.
    pub fn record_bone_inputs_copy(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame_index: usize,
    ) {
        let byte_size = self.bone_input_upload_bytes[frame_index];
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
                self.bone_world_staging_buffers[frame_index].buffer,
                self.bone_world_device_buffers[frame_index].buffer,
                &[copy],
            );
            device.cmd_copy_buffer(
                cmd,
                self.bind_inverse_staging_buffers[frame_index].buffer,
                self.bind_inverse_device_buffers[frame_index].buffer,
                &[copy],
            );
            // One barrier covering both device buffers — the next
            // consumer (`skin_palette.comp`) reads both at the same
            // dispatch so merging is correctness-equivalent.
            let barriers = [
                vk::BufferMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .buffer(self.bone_world_device_buffers[frame_index].buffer)
                    .offset(0)
                    .size(byte_size),
                vk::BufferMemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .buffer(self.bind_inverse_device_buffers[frame_index].buffer)
                    .offset(0)
                    .size(byte_size),
            ];
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &barriers,
                &[],
            );
        }
    }

    /// M29.5 — bytes most recently written by [`upload_bone_inputs`]
    /// for `frame_index`. Used by `draw.rs` to size the skin_palette
    /// dispatch (bone_count = byte_size / mat4_size). Returns 0 when
    /// the frame had no skinned content, in which case the caller
    /// should skip the dispatch entirely.
    pub fn bone_input_upload_bytes(&self, frame_index: usize) -> vk::DeviceSize {
        self.bone_input_upload_bytes[frame_index]
    }

    // M29.5 cleanup — `seed_identity_bones` removed here. The one-time
    // startup transfer used to seed slot 0 of every frame-in-flight
    // palette with the identity matrix so the first-frame binding-12
    // read (previous-frame palette) saw a valid transform. Post-M29.5
    // the per-frame `skin_palette.comp` dispatch writes slot 0 of the
    // palette every frame (the CPU seeds slot 0 of both input arrays
    // with identity in `build_render_data`, and `identity × identity =
    // identity`), so the startup seed is redundant.

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

        // #1134 / PERF-D8-NEW-01 — dirty-gate via content hash, mirror
        // of #878's upload_materials gate. MedTek ships ~530 KB/frame
        // (7359 × 72 B); static interiors produce byte-identical
        // slices each frame, so skipping the copy + flush in steady
        // state saves ~32 MB/s sustained PCIe at 60 fps. Hash is
        // computed over the clamped prefix actually written, so an
        // overflow that drops trailing instances still re-uploads when
        // the kept prefix changes.
        let hash = hash_instance_slice(&instances[..count]);
        if self.last_uploaded_instance_hash[frame_index] == Some(hash) {
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
        buf.flush_if_needed(device)?;
        self.last_uploaded_instance_hash[frame_index] = Some(hash);
        Ok(())
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
        // intern() in MaterialTable already caps at MAX_MATERIALS before
        // building this slice — materials.len() > MAX_MATERIALS is
        // unreachable (#1064 / REN-D14-NEW-04). The debug_assert documents
        // the invariant so a future refactor that breaks the cap surfaces
        // immediately rather than silently truncating uploads.
        debug_assert!(
            materials.len() <= MAX_MATERIALS,
            "upload_materials: len {} > MAX_MATERIALS {}; intern() should have capped",
            materials.len(),
            MAX_MATERIALS,
        );
        let count = materials.len().min(MAX_MATERIALS);
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

    /// M29.5 — per-frame DEVICE_LOCAL bone-world matrices buffers.
    /// `skin_palette.comp` reads these as `BoneWorldBuffer` (set 0
    /// binding 0). Populated each frame by [`upload_bone_inputs`] +
    /// [`record_bone_inputs_copy`].
    pub fn bone_world_buffers(&self) -> &[GpuBuffer] {
        &self.bone_world_device_buffers
    }

    /// M29.5 — per-frame DEVICE_LOCAL inverse-bind-pose matrices.
    /// `skin_palette.comp` reads these as `BindInverseBuffer` (set 0
    /// binding 1). Same per-frame upload model as
    /// [`bone_world_buffers`]; follow-on M29.6 will promote this to a
    /// write-once SSBO once the per-skinned-mesh slot lifecycle is in
    /// place.
    pub fn bind_inverse_buffers(&self) -> &[GpuBuffer] {
        &self.bind_inverse_device_buffers
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
