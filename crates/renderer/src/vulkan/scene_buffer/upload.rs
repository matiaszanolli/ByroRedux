//! Per-frame upload-and-flush routines for every SSBO.
//!
//! `upload_lights` / `upload_camera` / `upload_bones` / `upload_instances` /
//! `upload_materials` / `upload_indirect_draws` / `upload_terrain_tiles` +
//! the `record_bone_copy` GPU-to-GPU shortcut.

use byroredux_core::ecs::components::MAX_BONES_PER_MESH;

use super::super::allocator::SharedAllocator;
use super::super::buffer::GpuBuffer;
use super::buffers::LightHeader;
use super::descriptors::{
    hash_indirect_slice, hash_instance_slice, hash_light_slice, hash_material_slice,
    hash_previous_model_slice,
};
use super::super::sync::MAX_FRAMES_IN_FLIGHT;
use super::*;
use anyhow::Result;
use ash::vk;

const BONE_WORLD_SLOT_PENDING: u8 = 0x80;
const BONE_WORLD_FRAME_MASK: u8 = (1u8 << MAX_FRAMES_IN_FLIGHT) - 1;

#[inline]
fn mark_bone_world_slot_dirty(state: &mut u8) {
    // A new pose invalidates every per-frame device copy, not just the
    // current one. The low bits are repopulated as each FIF slot is refreshed.
    *state = BONE_WORLD_SLOT_PENDING;
}

#[cfg(test)]
mod bone_world_slot_state_tests {
    use super::{
        bone_world_slot_needs_copy, mark_bone_world_slot_dirty,
        mark_bone_world_slot_written,
    };

    #[test]
    fn dirty_slot_is_refreshed_once_into_each_frame_buffer() {
        let mut state = 0;
        mark_bone_world_slot_dirty(&mut state);
        assert!(bone_world_slot_needs_copy(state, 0));
        assert!(bone_world_slot_needs_copy(state, 1));

        mark_bone_world_slot_written(&mut state, 0);
        assert!(!bone_world_slot_needs_copy(state, 0));
        assert!(bone_world_slot_needs_copy(state, 1));

        mark_bone_world_slot_written(&mut state, 1);
        assert!(!bone_world_slot_needs_copy(state, 0));
        assert!(!bone_world_slot_needs_copy(state, 1));
    }

    #[test]
    fn a_new_pose_rearms_both_frame_buffers() {
        let mut state = 0;
        mark_bone_world_slot_dirty(&mut state);
        mark_bone_world_slot_written(&mut state, 0);
        mark_bone_world_slot_written(&mut state, 1);
        mark_bone_world_slot_dirty(&mut state);

        assert!(bone_world_slot_needs_copy(state, 0));
        assert!(bone_world_slot_needs_copy(state, 1));
    }
}

#[inline]
fn bone_world_slot_needs_copy(state: u8, frame_index: usize) -> bool {
    debug_assert!(frame_index < MAX_FRAMES_IN_FLIGHT);
    state & BONE_WORLD_SLOT_PENDING != 0 && state & (1u8 << frame_index) == 0
}

#[inline]
fn mark_bone_world_slot_written(state: &mut u8, frame_index: usize) {
    debug_assert!(frame_index < MAX_FRAMES_IN_FLIGHT);
    *state |= 1u8 << frame_index;
    if *state & BONE_WORLD_FRAME_MASK == BONE_WORLD_FRAME_MASK {
        *state = 0;
    }
}

impl super::buffers::SceneBuffers {
    /// Upload light data for the current frame-in-flight.
    pub fn upload_lights(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        lights: &[GpuLight],
    ) -> Result<()> {
        let count = lights.len().min(MAX_LIGHTS);
        self.last_light_counts = (
            lights.len().min(u32::MAX as usize) as u32,
            count.min(u32::MAX as usize) as u32,
        );
        if lights.len() > MAX_LIGHTS {
            log::warn!(
                "Light SSBO overflow: {} lights submitted, capped at {} — excess lights silently dropped in storage-iteration order (no proximity priority). PERF-D4-NEW-02 / #1808",
                lights.len(),
                MAX_LIGHTS,
            );
        }
        // #2036 / PERF-D4-01 — dirty-gate via content hash, mirror of
        // #1134's upload_instances / #878's upload_materials / #1809's
        // upload_indirect_draws gates. Light buffers are only a few KB/
        // frame so the win is small, and the gate will often miss on
        // flickering-light content (a torch's color/radius changing
        // every frame defeats it by design) — but static interiors with
        // no dynamic lights produce a byte-identical slice each frame,
        // and this closes the one per-frame SSBO upload that hadn't
        // gained the sibling gate yet. Hash is computed over the clamped
        // prefix actually written (`lights[..count]`), so an overflow
        // that drops trailing lights still re-uploads when the kept
        // prefix changes, and a count change (including to/from zero)
        // always changes the hashed byte length.
        let hash = hash_light_slice(&lights[..count]);
        if self.last_uploaded_light_hash[frame_index] == Some(hash) {
            return Ok(());
        }

        let header = LightHeader {
            count: count as u32,
            _pad: [0; 3],
        };

        let header_size = std::mem::size_of::<LightHeader>();
        let light_size = std::mem::size_of::<GpuLight>();

        // Write directly to mapped GPU memory — no intermediate Vec allocation.
        let buf = &mut self.light_buffers[frame_index];
        let mapped = buf.mapped_slice_mut()?;

        // In-bounds invariant: light_buffers are allocated as
        // sizeof::<LightHeader>() + sizeof::<GpuLight>() * MAX_LIGHTS bytes
        // (see buffers.rs `light_buf_size`), and `count` is clamped to MAX_LIGHTS
        // above, so `header_size + light_size * count <= mapped.len()`.
        debug_assert!(
            header_size + light_size * count <= mapped.len(),
            "upload_lights: header_size({}) + light_size({}) * count({}) = {} > mapped.len()({}); \
             buffer must be sized for LightHeader + MAX_LIGHTS * GpuLight",
            header_size,
            light_size,
            count,
            header_size + light_size * count,
            mapped.len(),
        );

        // SAFETY: LightHeader and GpuLight are #[repr(C)] with plain f32/u32 fields.
        // The debug_assert above proves the write range [0 .. header_size + light_size*count]
        // fits within `mapped`. `.add(header_size)` is valid because `header_size <
        // mapped.len()` (at minimum the header always fits). The header and light regions
        // are disjoint: header occupies [0, header_size), lights occupy
        // [header_size, header_size + light_size * count). No overlap.
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

        // F8 (#1587) — flush only the written prefix (header + live lights),
        // not the whole allocation, on non-coherent host-visible memory.
        buf.flush_range(
            device,
            0,
            (header_size + light_size * count) as vk::DeviceSize,
        )?;
        // Stamp the hash AFTER a successful flush — a flush failure
        // leaves the buffer in an indeterminate state, so we want the
        // next call to re-upload rather than skip.
        self.last_uploaded_light_hash[frame_index] = Some(hash);
        Ok(())
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

    /// Patch `GpuCamera.flags[0]` (the `rt_flag` slot) in place for the
    /// current frame-in-flight, without re-uploading the rest of the UBO.
    ///
    /// Used by `draw_frame` to flip `rt_flag` from `0.0 -> 1.0` on the
    /// first frame each FIF slot's TLAS becomes valid (`write_tlas` ran
    /// successfully this frame). Pre-#1227 the `rt_flag` value was
    /// computed when `upload_camera` ran near the top of the frame —
    /// before `build_tlas` / `write_tlas` — so on the first frame in
    /// each FIF slot, the value was `0.0` even though the TLAS would
    /// build successfully that same frame. Shaders saw `rt_flag = 0.0`
    /// and skipped every ray query for frames 0 + 1, producing a brief
    /// "RT off" flash that TAA had to dissolve across ~5 frames on
    /// every cell-load.
    ///
    /// Safe to call between `vkBeginCommandBuffer` and the
    /// `vkCmdBeginRenderPass` that consumes the camera UBO:
    /// `camera_buffers` are HOST_VISIBLE (and typically HOST_COHERENT;
    /// `write_mapped` flushes if not), so the patched f32 reaches the
    /// shader at `vkQueueSubmit` time. The only descriptor-set read of
    /// `GpuCamera.flags` happens inside the main render pass at
    /// `triangle.frag` / RT consumers, which start after the TLAS
    /// barrier — so this patch is causally ordered with respect to
    /// every consumer.
    ///
    /// `flags[0]` lives at byte offset 208 within `GpuCamera` (after
    /// 3 × mat4 + position vec4). See [`GpuCamera`] for the layout —
    /// any field reorder there must update the offset constant below.
    pub fn patch_camera_rt_flag(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        rt_flag: f32,
    ) -> Result<()> {
        // Compile-time check that the offset hasn't drifted from the
        // GpuCamera layout. If a future field reorder shifts `flags`,
        // this will fail to compile and the offset has to be revisited.
        const FLAGS_OFFSET: usize = std::mem::offset_of!(GpuCamera, flags);
        const _: () = assert!(FLAGS_OFFSET == 208);

        let buf = &mut self.camera_buffers[frame_index];
        let mapped = buf.mapped_slice_mut()?;
        let bytes = rt_flag.to_le_bytes();
        // `flags[0]` is the first f32 of `[f32; 4]` at FLAGS_OFFSET.
        mapped[FLAGS_OFFSET..FLAGS_OFFSET + 4].copy_from_slice(&bytes);
        buf.flush_if_needed(device)
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
    /// M29.6 — write the per-frame bone-world matrices into the
    /// staging buffer consumed by `skin_palette.comp`. (M29.5's
    /// sibling bind_inverses path moved to a persistent SSBO with
    /// first-sight uploads — see [`upload_pending_bind_inverses`].)
    ///
    /// `bone_world` is a packed contiguous mat4 array indexed by
    /// `skin_slot_id × MAX_BONES_PER_MESH`. Slot 0 is always identity
    /// (`build_render_data` pushes IDENTITY at slot 0); slots
    /// 1..=`max_used_slot` hold per-entity bone transforms. Slots
    /// that the [`SkinSlotPool`] did not allocate this frame contain
    /// stale data — the skin_palette dispatch writes their palettes,
    /// but no entity references them, so the staleness is invisible.
    ///
    /// #3665 — copy only dirty slot strides, plus any slot that has not yet
    /// been copied into the current frame-in-flight buffer. The high-water
    /// `bone_world` length is retained separately for the dense palette
    /// dispatch; it no longer determines the transfer width. The per-mesh
    /// padding tail remains a full-slot copy here and is the separate #1794
    /// remainder.
    ///
    /// `dirty_slot_offsets` contains sparse `bone_world` offsets for entities
    /// whose CPU pose changed. The per-slot state stamps make a changed slot
    /// persist until every frame-in-flight device buffer has received it.
    ///
    /// Records sparse regions into `bone_world_copy_regions[frame]` for
    /// [`record_bone_world_copy`] to consume.
    pub fn upload_bone_worlds<I>(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        bone_world: &[[[f32; 4]; 4]],
        dirty_slot_offsets: I,
    ) -> Result<()>
    where
        I: IntoIterator<Item = u32>,
    {
        let count = bone_world.len().min(MAX_TOTAL_BONES);
        let mat4_size = std::mem::size_of::<[[f32; 4]; 4]>();
        self.bone_world_dispatch_bytes[frame_index] =
            (mat4_size * count) as vk::DeviceSize;
        self.bone_input_upload_bytes[frame_index] = 0;
        self.bone_world_copy_regions[frame_index].clear();

        let slot_stride = MAX_BONES_PER_MESH as u32;
        for offset in dirty_slot_offsets {
            debug_assert_eq!(
                offset % slot_stride,
                0,
                "bone-world dirty offset must identify a slot boundary"
            );
            let slot = (offset / slot_stride) as usize;
            if let Some(state) = self.bone_world_slot_states.get_mut(slot) {
                mark_bone_world_slot_dirty(state);
            }
        }

        if count == 0 {
            return Ok(());
        }

        let copy_regions = &mut self.bone_world_copy_regions[frame_index];
        for (slot, &state) in self.bone_world_slot_states.iter().enumerate() {
            if !bone_world_slot_needs_copy(state, frame_index) {
                continue;
            }
            let start = slot * MAX_BONES_PER_MESH;
            if start >= count {
                continue;
            }
            let slot_count = (count - start).min(MAX_BONES_PER_MESH);
            let byte_offset = (start * mat4_size) as vk::DeviceSize;
            let byte_size = (slot_count * mat4_size) as vk::DeviceSize;
            copy_regions.push(vk::BufferCopy {
                src_offset: byte_offset,
                dst_offset: byte_offset,
                size: byte_size,
            });
        }
        if copy_regions.is_empty() {
            return Ok(());
        }

        let world_buf = &mut self.bone_world_staging_buffers[frame_index];
        let world_mapped = world_buf.mapped_slice_mut()?;
        // SAFETY: [[f32;4];4] is repr(C)-compatible with std430 mat4.
        // bone_world_staging_buffers are sized for MAX_TOTAL_BONES; count
        // is clamped above. Each copied region has the same source/destination
        // offset within the MAX_TOTAL_BONES-sized staging allocation.
        unsafe {
            for region in copy_regions.iter() {
                debug_assert!(
                    region.dst_offset as usize + region.size as usize <= world_mapped.len(),
                    "bone-world staging region exceeds the mapped allocation"
                );
                std::ptr::copy_nonoverlapping(
                    bone_world
                        .as_ptr()
                        .add(region.src_offset as usize / mat4_size)
                        as *const u8,
                    world_mapped
                        .as_mut_ptr()
                        .add(region.dst_offset as usize),
                    region.size as usize,
                );
            }
        }
        // Flush one envelope around the sparse writes. This avoids one Vulkan
        // flush call per dirty slot while excluding the untouched tail above
        // the highest region.
        let first = copy_regions
            .first()
            .expect("non-empty copy region list")
            .dst_offset;
        let last_region = copy_regions
            .last()
            .expect("non-empty copy region list");
        let last = last_region.dst_offset + last_region.size;
        world_buf.flush_range(device, first, last - first)?;
        self.bone_input_upload_bytes[frame_index] = last;
        Ok(())
    }

    /// M29.6 — record the bone-world staging→device copy and the
    /// visibility barrier that makes it readable by `skin_palette.comp`.
    ///
    /// One sparse `cmd_copy_buffer` call with all dirty-slot regions plus one
    /// buffer barrier (TRANSFER_WRITE → COMPUTE_SHADER_READ). M29.5's
    /// sibling bind_inverses copy is gone (persistent SSBO is uploaded
    /// separately at first-sight via [`record_pending_bind_inverse_copies`]).
    pub fn record_bone_world_copy(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        frame_index: usize,
    ) {
        let barrier_size = self.bone_input_upload_bytes[frame_index];
        let region_count = self.bone_world_copy_regions[frame_index].len();
        if barrier_size == 0 || region_count == 0 {
            return;
        }
        unsafe {
            // SAFETY: `cmd` is in the recording state (the caller's frame command
            // buffer). Both `bone_world_staging_buffers[frame_index]` and
            // `bone_world_device_buffers[frame_index]` are device-owned and live for
            // the renderer's lifetime; every sparse region is within the
            // MAX_TOTAL_BONES-sized allocations, and the following barrier's
            // [0, barrier_size) envelope covers every region.
            device.cmd_copy_buffer(
                cmd,
                self.bone_world_staging_buffers[frame_index].buffer,
                self.bone_world_device_buffers[frame_index].buffer,
                &self.bone_world_copy_regions[frame_index],
            );
            let barrier = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(self.bone_world_device_buffers[frame_index].buffer)
                .offset(0)
                .size(barrier_size);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[barrier],
                &[],
            );
        }
        for index in 0..region_count {
            let offset = self.bone_world_copy_regions[frame_index][index].dst_offset;
            let slot = (offset as usize
                / (std::mem::size_of::<[[f32; 4]; 4]>() * MAX_BONES_PER_MESH))
                as usize;
            if let Some(state) = self.bone_world_slot_states.get_mut(slot) {
                mark_bone_world_slot_written(state, frame_index);
            }
        }
    }

    /// M29.6 — write pending first-sight `bind_inverses` uploads into
    /// the small HOST_VISIBLE staging buffer. Returns the number of
    /// slots actually written (clamped at
    /// `MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME`; the caller defers
    /// the excess to the next frame's pending queue).
    ///
    /// Layout in the staging buffer: pending upload `i` (0-indexed)
    /// occupies `[i × MBPM × 64 B .. (i+1) × MBPM × 64 B)`. The
    /// matching [`record_pending_bind_inverse_copies`] consumes the
    /// same `pending` list and issues one `cmd_copy_buffer` per
    /// upload, targeting the slot's offset in
    /// `bind_inverses_persistent`.
    pub fn upload_pending_bind_inverses(
        &mut self,
        device: &ash::Device,
        pending: &[(u32, Vec<[[f32; 4]; 4]>)],
    ) -> Result<usize> {
        let capped = pending
            .len()
            .min(MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME);
        if capped == 0 {
            return Ok(0);
        }
        let slot_byte_stride = (MAX_BONES_PER_MESH * std::mem::size_of::<[[f32; 4]; 4]>()) as usize;
        let staging = &mut self.bind_inverse_upload_staging;
        let mapped = staging.mapped_slice_mut()?;
        for (i, (_slot_id, bind_inverses)) in pending.iter().take(capped).enumerate() {
            let bytes_this_mesh =
                std::mem::size_of::<[[f32; 4]; 4]>() * bind_inverses.len().min(MAX_BONES_PER_MESH);
            let offset = i * slot_byte_stride;
            // SAFETY: bytes_this_mesh ≤ MBPM × 64 = slot_byte_stride;
            // `i < capped ≤ MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME`
            // so offset + slot_byte_stride ≤ staging size.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    bind_inverses.as_ptr() as *const u8,
                    mapped.as_mut_ptr().add(offset),
                    bytes_this_mesh,
                );
            }
        }
        staging.flush_if_needed(device)?;
        Ok(capped)
    }

    /// M29.6 — record per-pending-upload staging→persistent copies and
    /// the visibility barrier that makes the new slot data readable
    /// by `skin_palette.comp`.
    ///
    /// Issues one `cmd_copy_buffer` per pending upload (each copy is
    /// MBPM × 64 B = ~9 KB targeting `bind_inverses_persistent` at
    /// the slot's byte offset). A single combined buffer barrier
    /// covers the full persistent SSBO range — over-conservative for
    /// non-touched slots but cheaper than emitting N per-slot barriers
    /// and correct.
    ///
    /// `capped` must equal what [`upload_pending_bind_inverses`]
    /// returned for this same frame (so the staging layout aligns).
    pub fn record_pending_bind_inverse_copies(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        pending_slots: &[u32],
        capped: usize,
    ) {
        if capped == 0 {
            return;
        }
        let slot_byte_stride =
            (MAX_BONES_PER_MESH * std::mem::size_of::<[[f32; 4]; 4]>()) as vk::DeviceSize;
        let mut copies: Vec<vk::BufferCopy> = Vec::with_capacity(capped);
        for (i, &slot_id) in pending_slots.iter().take(capped).enumerate() {
            // M29.6 hotfix (#1193 / SAFE-D7-NEW-03) — defend against
            // a SkinSlotPool constructed with capacity past
            // `(MAX_TOTAL_BONES / MBPM) - 1`. The cmd_copy_buffer
            // below would otherwise write past the persistent SSBO's
            // 2 MB end (VUID-vkCmdCopyBuffer-dstOffset-00114) — UB on
            // drivers without validation, device-lost otherwise.
            // The pool's max_slot is enforced at construction, so a
            // pool that respects the (MAX_TOTAL_BONES / MBPM) - 1
            // ceiling will never trip this. The assert is the
            // tripwire if a future constructor / test forgets the
            // -1.
            debug_assert!(
                ((slot_id as usize) + 1) * MAX_BONES_PER_MESH <= MAX_TOTAL_BONES,
                "M29.6 contract: slot_id {} would write past \
                 bind_inverses_persistent end (MAX_TOTAL_BONES = {}). \
                 SkinSlotPool capacity must be \
                 ≤ (MAX_TOTAL_BONES / MAX_BONES_PER_MESH) - 1",
                slot_id,
                MAX_TOTAL_BONES,
            );
            copies.push(vk::BufferCopy {
                src_offset: (i as vk::DeviceSize) * slot_byte_stride,
                dst_offset: (slot_id as vk::DeviceSize) * slot_byte_stride,
                size: slot_byte_stride,
            });
        }
        unsafe {
            // SAFETY: `cmd` is a recording-state command buffer; `bind_inverse_upload_staging`
            // and `bind_inverses_persistent` are device-owned and live. Every `copies`
            // region's dst_offset+size stays within the persistent SSBO (the debug_assert
            // above proves each slot_id write ends ≤ MAX_TOTAL_BONES), and the src regions
            // index the staging slots written by upload_pending_bind_inverses.
            // Single cmd_copy_buffer with N regions — one Vulkan call
            // even when several pending uploads land in the same
            // frame. Spec: all regions target the same dst buffer
            // (bind_inverses_persistent), so this is the standard
            // batched form.
            device.cmd_copy_buffer(
                cmd,
                self.bind_inverse_upload_staging.buffer,
                self.bind_inverses_persistent.buffer,
                &copies,
            );
            // Cover the whole persistent SSBO. Per-slot barriers
            // would be tighter but require N entries; the dst stage
            // mask is the same (COMPUTE_SHADER_READ) so a single
            // whole-buffer barrier is correctness-equivalent.
            let barrier = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(self.bind_inverses_persistent.buffer)
                .offset(0)
                .size(vk::WHOLE_SIZE);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[barrier],
                &[],
            );
        }
    }

    /// M29.5/M29.6 — highest byte offset covered by the sparse regions most
    /// recently prepared by [`upload_bone_worlds`] for `frame_index`.
    /// Returns 0 when this frame has no bone-world regions to transfer; the
    /// caller uses that to skip the copy and, when no bind-inverse upload is
    /// pending, the palette dispatch too.
    pub fn bone_input_upload_bytes(&self, frame_index: usize) -> vk::DeviceSize {
        self.bone_input_upload_bytes[frame_index]
    }

    /// Full dense bone-world span required by `skin_palette.comp` for this
    /// frame. This remains high-water-sized even when the transfer itself is
    /// sparse, because the current compute shader dispatch is dense.
    pub fn bone_world_dispatch_bytes(&self, frame_index: usize) -> vk::DeviceSize {
        self.bone_world_dispatch_bytes[frame_index]
    }

    /// M29.6 hotfix (#1191 / SAFE-D7-NEW-01) — write identity matrices
    /// into `bind_inverses_persistent` slot 0 (bytes
    /// `0..MAX_BONES_PER_MESH × 64`). Required because the slot pool
    /// reserves slot 0 for the "global identity slot" but never
    /// pushes a pending upload for it, leaving the persistent SSBO's
    /// slot 0 range as `create_device_local_uninit` garbage.
    ///
    /// Without this seed, pool-overflowed skinned entities (which
    /// fall through to `bone_offset = 0`) would compute
    /// `palette[0..MBPM] = identity × UNDEFINED` via the
    /// `skin_palette.comp` dispatch — UB. With the seed,
    /// `palette[0..MBPM] = identity × identity = identity` and the
    /// fallback renders the entity in bind pose (the pre-M29.6
    /// behaviour).
    ///
    /// Uses `cmd_update_buffer` for an inline write (no staging
    /// needed since MBPM × 64 B = 9216 B is well under the
    /// Vulkan-guaranteed 65536 B `vkCmdUpdateBuffer` payload limit).
    /// Runs once at `VulkanContext::new` after `SceneBuffers` is
    /// constructed; the data is persistent for the renderer's
    /// lifetime.
    pub fn seed_persistent_bind_inverses_identity(
        &self,
        device: &ash::Device,
        queue: &std::sync::Mutex<vk::Queue>,
        command_pool: vk::CommandPool,
    ) -> Result<()> {
        // 144 identity mat4s packed contiguously. cmd_update_buffer
        // takes the data slice inline; the slice goes through the
        // ash binding straight to vkCmdUpdateBuffer.
        let identity = [
            [1.0_f32, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let mut payload = Vec::with_capacity(MAX_BONES_PER_MESH);
        for _ in 0..MAX_BONES_PER_MESH {
            payload.push(identity);
        }
        // SAFETY: `[[f32; 4]; 4]` is repr(C); the byte slice has the
        // same layout the GPU reads as `mat4`. Size pinned at
        // MBPM × 64 = 9216 B, well within the 65536 B
        // `vkCmdUpdateBuffer` limit. Inline-written from the
        // command buffer; no staging buffer to manage.
        let payload_bytes: &[u8] = unsafe {
            std::slice::from_raw_parts(
                payload.as_ptr() as *const u8,
                payload.len() * std::mem::size_of::<[[f32; 4]; 4]>(),
            )
        };
        let dst = self.bind_inverses_persistent.buffer;
        super::super::texture::with_one_time_commands(device, queue, command_pool, |cmd| {
            unsafe {
                // SAFETY: `cmd` is the recording-state one-time command buffer from
                // with_one_time_commands; `dst` (bind_inverses_persistent) is
                // device-owned and live; `payload_bytes` is 9216 B (< the 65536 B
                // vkCmdUpdateBuffer limit) and stays borrowed for the whole call.
                device.cmd_update_buffer(cmd, dst, 0, payload_bytes);
            }
            Ok(())
        })
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
        // of #878's upload_materials gate. MedTek ships ~920 KB/frame
        // (7359 × 128 B); static interiors produce byte-identical
        // slices each frame, so skipping the copy + flush in steady
        // state saves ~54 MB/s sustained PCIe at 60 fps. (#2692 — was
        // stated as 112 B / 805 KB / 48 MB/s, pre-#2219.) Hash is
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
        // F8 (#1587) — flush only the written byte range, not the full allocation.
        buf.flush_range(device, 0, byte_size as vk::DeviceSize)?;
        self.last_uploaded_instance_hash[frame_index] = Some(hash);
        Ok(())
    }

    /// Upload previous rigid transforms aligned with the current instance
    /// array. New entities carry their current transform, producing zero
    /// object velocity on first sight.
    pub fn upload_previous_models(
        &mut self,
        device: &ash::Device,
        frame_index: usize,
        models: &[GpuPreviousModel],
    ) -> Result<()> {
        let count = models.len().min(MAX_INSTANCES);
        if models.len() > MAX_INSTANCES {
            log::warn!(
                "Previous-model SSBO overflow: {} transforms submitted, capped at {}",
                models.len(),
                MAX_INSTANCES,
            );
        }
        if count == 0 {
            return Ok(());
        }

        let hash = hash_previous_model_slice(&models[..count]);
        if self.last_uploaded_previous_model_hash[frame_index] == Some(hash) {
            return Ok(());
        }

        let buf = &mut self.previous_model_buffers[frame_index];
        let mapped = buf.mapped_slice_mut()?;
        let byte_size = std::mem::size_of::<GpuPreviousModel>() * count;
        // SAFETY: `GpuPreviousModel` is a tightly packed nested f32 array;
        // the destination is sized for MAX_INSTANCES and count is clamped.
        unsafe {
            std::ptr::copy_nonoverlapping(
                models.as_ptr().cast::<u8>(),
                mapped.as_mut_ptr(),
                byte_size,
            );
        }
        buf.flush_range(device, 0, byte_size as vk::DeviceSize)?;
        self.last_uploaded_previous_model_hash[frame_index] = Some(hash);
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
        // unreachable (#1064 / REN-D14-NEW-04). Hard assert so a future
        // refactor that breaks the cap surfaces in release builds too,
        // rather than silently truncating uploads.
        assert!(
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
        // F8 (#1587) — flush only the written byte range, not the full allocation.
        buf.flush_range(device, 0, byte_size as vk::DeviceSize)?;
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

        // #1809 / PERF-D4-NEW-03 — dirty-gate via content hash, mirror
        // of #1134's upload_instances / #878's upload_materials gates.
        // The indirect list is derived from the same per-frame batches
        // as instances and materials, so it's byte-identical under the
        // exact same "static interior" conditions those two already
        // skip the copy + flush for. Hash is computed over the clamped
        // prefix actually written, so an overflow that drops trailing
        // commands still re-uploads when the kept prefix changes.
        let hash = hash_indirect_slice(&draws[..count]);
        if self.last_uploaded_indirect_hash[frame_index] == Some(hash) {
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
        // F8 (#1587) — flush only the written byte range, not the full allocation.
        buf.flush_range(device, 0, byte_size as vk::DeviceSize)?;
        // Stamp the hash AFTER a successful flush — a flush failure
        // leaves the buffer in an indeterminate state, so we want the
        // next call to re-upload rather than skip.
        self.last_uploaded_indirect_hash[frame_index] = Some(hash);
        Ok(())
    }

    /// Return the `VkBuffer` handle for the current frame's indirect
    /// buffer. The draw loop passes this to `cmd_draw_indexed_indirect`.
    pub fn indirect_buffer(&self, frame_index: usize) -> vk::Buffer {
        self.indirect_buffers[frame_index].buffer
    }

    /// Record a terrain tile update into the current frame command buffer.
    /// Called from the cell-loader path after `spawn_terrain_mesh` packs
    /// per-tile layer texture indices. The CPU writes only the high-water
    /// prefix supplied by `fill_terrain_tiles`; the shared DEVICE_LOCAL SSBO
    /// remains unchanged beyond that prefix, where no live instance can
    /// legally index it. See #470 / #497 / #3664.
    ///
    /// Each frame-in-flight owns a reusable HOST_VISIBLE staging buffer. The
    /// frame fence has already retired that slot before `draw_frame` records
    /// into it, and the copy is ordered before the geometry pass by the
    /// transfer-to-fragment barrier below. This avoids both transient Vulkan
    /// allocations and the blocking one-time-submit fence that used to sit in
    /// the render hot path.
    pub fn upload_terrain_tiles(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
        cmd: vk::CommandBuffer,
        frame_index: usize,
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

        // The previous guard for this frame slot is safe to return now:
        // `draw_frame` waited that slot's fence before opening this command
        // buffer. Keep the newly acquired guard in the slot until the same
        // fence retires, so the staging source cannot be overwritten while
        // the GPU is still consuming it.
        if let Some(previous) = self.terrain_tile_staging_buffers[frame_index].take() {
            let capacity = previous
                .allocation
                .as_ref()
                .map(|allocation| allocation.size())
                .unwrap_or(byte_size);
            previous.release_to(&mut self.terrain_tile_staging_pool, capacity);
        }
        let (staging_buffer, staging_alloc) = self
            .terrain_tile_staging_pool
            .acquire(byte_size)?;
        let mut staging = super::super::buffer::StagingGuard::new(
            staging_buffer,
            staging_alloc,
            device.clone(),
            allocator.clone(),
        );

        // SAFETY: GpuTerrainTile is #[repr(C)] with u32-only fields
        // matching std430. The pool acquisition is sized to the high-water
        // prefix, so the mapped range below always fits even when only a
        // small number of terrain slots is live.
        let mapped = staging.mapped_slice_mut()?;
        unsafe {
            // SAFETY: `mapped` is the host-visible staging slice fetched just
            // above, valid for `byte_size` bytes; `tiles` is a distinct
            // `#[repr(C)]` source of the same length, so the regions do not
            // overlap.
            std::ptr::copy_nonoverlapping(
                tiles.as_ptr() as *const u8,
                mapped.as_mut_ptr(),
                byte_size as usize,
            );
        }
        staging.flush_range(device, 0, byte_size)?;

        let copy = vk::BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size: byte_size,
        };
        let dst = self.terrain_tile_buffer.buffer;
        unsafe {
            // SAFETY: `cmd` is the recording frame command buffer. The
            // per-frame staging buffer and shared terrain destination are
            // live TRANSFER_SRC/DST buffers, and `copy` stays within both
            // allocations. The transfer is sequenced before fragment reads
            // by the barrier immediately below.
            device.cmd_copy_buffer(cmd, staging.buffer, dst, &[copy]);
            let barrier = vk::BufferMemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .buffer(dst)
                .offset(0)
                .size(byte_size);
            device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[barrier],
                &[],
            );
        }

        self.terrain_tile_staging_buffers[frame_index] = Some(staging);

        Ok(())
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

    /// M29.5/M29.6 — per-frame DEVICE_LOCAL bone-world matrices
    /// buffers. `skin_palette.comp` reads these as `BoneWorldBuffer`
    /// (set 0 binding 0). Populated each frame by
    /// [`upload_bone_worlds`] + [`record_bone_world_copy`].
    pub fn bone_world_buffers(&self) -> &[GpuBuffer] {
        &self.bone_world_device_buffers
    }

    /// M29.6 — persistent DEVICE_LOCAL inverse-bind-pose matrices
    /// SSBO. `skin_palette.comp` reads it as `BindInverseBuffer` (set
    /// 0 binding 1). Written once per skinned-mesh first-sight via
    /// [`upload_pending_bind_inverses`] +
    /// [`record_pending_bind_inverse_copies`]; the slot pool
    /// guarantees the bytes uploaded at slot S × MBPM stay correct
    /// for the lifetime of the entity that owns slot S.
    pub fn bind_inverses_persistent(&self) -> &GpuBuffer {
        &self.bind_inverses_persistent
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
