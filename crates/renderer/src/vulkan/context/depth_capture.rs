//! Depth-buffer capture — copies the depth attachment to a staging buffer
//! for CPU readback (#3308).
//!
//! Sibling of [`super::screenshot`] and deliberately shaped like it: a
//! request flag the outside world sets, a copy recorded inside the frame's
//! command buffer, and a readback performed at the top of the *next* frame
//! once the fence proves the GPU is done.
//!
//! Exists to make depth precision observable. `Camera::depth_resolution_at`
//! predicts what the buffer's resolution should be; this hands the CPU what
//! it actually contains, so `Camera::analyze_depth_field` can check the two
//! against each other and — after a reversed-Z conversion — report the
//! before/after difference. That comparison is #3308's step-2 gate, and none
//! of it is otherwise observable from `cargo test`.
//!
//! Unlike the screenshot bridge this has a single consumer (the
//! `depth.stats` console command), so it carries no owner tag and no
//! capture generation: there is no second claimant for a straggler to
//! publish into, and a stale result is simply an older frame's depth, which
//! is a meaningful answer for a diagnostic rather than a correctness hazard.

use super::VulkanContext;
use ash::vk;
use byroredux_core::ecs::DepthCapture;
use gpu_allocator::vulkan as vk_alloc;
use gpu_allocator::MemoryLocation;
use std::sync::atomic::Ordering;

impl VulkanContext {
    /// If a previous frame recorded a depth copy and the GPU has completed,
    /// read the staging buffer back into `depth_capture_result`.
    ///
    /// Called at the top of `draw_frame()` after the fence wait, exactly
    /// where `screenshot_finish_readback` runs and for the same reason.
    pub(super) fn depth_capture_finish_readback(&mut self) {
        // Same REG-02 / #1634 invariant the screenshot path documents: read
        // back the extent CAPTURED at record time, never the live one. A
        // same-frame resize between record and readback would otherwise
        // decode the new dimensions against the old copy.
        let Some(extent) = self.depth_capture_pending_readback.take() else {
            return;
        };
        let Some((_, ref allocation, _)) = self.depth_capture_staging else {
            return;
        };

        let width = extent.width;
        let height = extent.height;
        let expected = width as usize * height as usize * 4;

        // #2740 / REN-D4-04 — a fence's memory dependency has device-only
        // access scope, so it makes the copy complete without making it
        // visible to the host. `GpuToCpu` prefers `HOST_CACHED`, so without
        // this invalidate the read can return stale cache lines. Same
        // reasoning and same shared range helper as the screenshot path;
        // no-op on coherent memory.
        if !allocation
            .memory_properties()
            .contains(vk::MemoryPropertyFlags::HOST_COHERENT)
        {
            let (aligned_offset, aligned_size) =
                super::super::buffer::aligned_flush_range(allocation.offset(), allocation.size());
            // SAFETY: `allocation` is live and owned by
            // `self.depth_capture_staging` for this call. `aligned_flush_range`
            // widens outward to `NON_COHERENT_ATOM_SIZE`, staying inside the
            // parent `GpuAllocatorManaged` block; invalidating a superset only
            // discards possibly-stale host cache lines, publishing nothing.
            // `Allocation::memory` names the memory object for the range and
            // is neither freed, mapped, nor bound through this call.
            let result = unsafe {
                let range = vk::MappedMemoryRange::default()
                    .memory(allocation.memory())
                    .offset(aligned_offset)
                    .size(aligned_size);
                self.device.invalidate_mapped_memory_ranges(&[range])
            };
            if let Err(e) = result {
                log::warn!("Depth capture invalidate failed: {e} — samples may be stale");
            }
        }

        let Some(slice) = allocation.mapped_slice() else {
            log::warn!("Depth capture staging buffer not mapped");
            return;
        };
        if slice.len() < expected {
            log::warn!(
                "Depth capture staging too small: {} < {expected} — dropping",
                slice.len()
            );
            return;
        }

        // D32_SFLOAT: one f32 per sample, tightly packed by the copy region.
        // #3570 / D10-01 — sound only because `depth_capture_record_copy`
        // refuses to set `depth_capture_pending_readback` (and thus reach
        // here) unless `self.depth_format == D32_SFLOAT`. Do not lift that
        // guard without also widening this decode.
        let samples: Vec<f32> = slice[..expected]
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();

        log::info!("Depth capture: {width}x{height}, {} samples", samples.len());
        // #1174 sibling — recover from poison so one panicking consumer
        // doesn't take out every subsequent capture.
        *self
            .depth_capture_result
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(DepthCapture {
            width,
            height,
            samples,
        });
    }

    /// If a depth capture was requested, record the copy from the depth
    /// attachment into the staging buffer.
    ///
    /// Called immediately after `copy_depth_to_history`, which establishes
    /// the layout contract this relies on: the depth image is in
    /// `DEPTH_STENCIL_READ_ONLY_OPTIMAL` both before and after that helper,
    /// so this sees the same layout the SSAO / SVGF / composite consumers
    /// do and restores it identically.
    ///
    /// # Safety
    ///
    /// `cmd` must be recording and outside any render pass, and
    /// `self.depth_image` must be in `DEPTH_STENCIL_READ_ONLY_OPTIMAL` — the
    /// state `copy_depth_to_history` leaves it in. This function restores
    /// that layout before returning, so every later consumer in the frame
    /// sees what it expects whether or not a capture ran.
    pub(super) unsafe fn depth_capture_record_copy(&mut self, cmd: vk::CommandBuffer) {
        if !self.depth_capture_requested.swap(false, Ordering::AcqRel) {
            return;
        }

        // #3570 / D10-01 — `find_depth_format` is a fallback chain
        // (`D32_SFLOAT` then `D16_UNORM`); Vulkan mandates `D16_UNORM`
        // depth-attachment support but not `D32_SFLOAT`, so the D16 arm is
        // genuinely reachable on real hardware. Everything below this
        // point — the ×4 buffer sizing and the `f32`-per-sample readback
        // decode in `depth_capture_finish_readback` — hardcodes the
        // D32_SFLOAT layout. Refuse rather than silently misdecode: this
        // tool exists to give #3308 trustworthy before/after evidence, and
        // a wrong-but-confident capture is worse than none. (Zero impact on
        // the dev RTX 4070 Ti, which selects D32_SFLOAT.)
        if self.depth_format != vk::Format::D32_SFLOAT {
            log::warn!(
                "Depth capture unsupported on this device: selected depth \
                 format is {:?}, not D32_SFLOAT — refusing rather than \
                 misdecoding the readback (#3570)",
                self.depth_format
            );
            return;
        }

        let extent = self.frame_extents.render;
        let buffer_size =
            extent.width as vk::DeviceSize * extent.height as vk::DeviceSize * 4 /* D32_SFLOAT, checked above */;
        self.ensure_depth_capture_staging(buffer_size);
        let Some((staging_buffer, _, _)) = self.depth_capture_staging.as_ref() else {
            log::warn!("Depth capture staging buffer creation failed");
            return;
        };
        let staging_buffer = *staging_buffer;

        let range = vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::DEPTH,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        };
        // Source scope names the depth WRITE, not only reads — the data being
        // copied IS the render pass's depth-attachment write, and a barrier
        // whose first scope contains only reads performs no availability
        // operation for it. Same reasoning (and same both-fragment-test-stages
        // widening) as `copy_depth_to_history`'s #2484 fix next door.
        let to_src = vk::ImageMemoryBarrier::default()
            .src_access_mask(
                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                    | vk::AccessFlags::SHADER_READ,
            )
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
            .old_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.depth_image)
            .subresource_range(range);
        let restore = vk::ImageMemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::TRANSFER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_READ)
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(vk::ImageLayout::DEPTH_STENCIL_READ_ONLY_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(self.depth_image)
            .subresource_range(range);

        let region = vk::BufferImageCopy::default()
            .buffer_offset(0)
            .buffer_row_length(0) // tightly packed
            .buffer_image_height(0)
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::DEPTH,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            });

        // SAFETY: `cmd` is recording and outside any render pass (caller
        // contract). `depth_image` is live and in
        // `DEPTH_STENCIL_READ_ONLY_OPTIMAL` on entry; the barriers bracket the
        // READ_ONLY -> TRANSFER_SRC transition around the single copy and
        // restore READ_ONLY before returning, and no other access to that
        // image is recorded between them. `staging_buffer` is
        // `TRANSFER_DST`-usage, at least `buffer_size` bytes, and is not read
        // by the host until the next frame's fence wait.
        unsafe {
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[to_src],
            );
            self.device.cmd_copy_image_to_buffer(
                cmd,
                self.depth_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                staging_buffer,
                &[region],
            );
            self.device.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[restore],
            );
        }

        self.depth_capture_pending_readback = Some(extent);
    }

    /// Ensure a host-visible staging buffer exists for depth readback.
    fn ensure_depth_capture_staging(&mut self, required_size: vk::DeviceSize) {
        if let Some((_, _, existing)) = &self.depth_capture_staging {
            if *existing >= required_size {
                return;
            }
            self.destroy_depth_capture_staging();
        }
        let Some(ref alloc) = self.allocator else {
            return;
        };

        let buffer_info = vk::BufferCreateInfo::default()
            .size(required_size)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        // SAFETY: `self.device` is live; `buffer_info` is fully populated with
        // no dangling `p_next` chain.
        let buffer = unsafe {
            match self.device.create_buffer(&buffer_info, None) {
                Ok(b) => b,
                Err(e) => {
                    log::warn!("Depth capture staging buffer creation failed: {e}");
                    return;
                }
            }
        };
        // SAFETY: `buffer` was just created above by this same device.
        let requirements = unsafe { self.device.get_buffer_memory_requirements(buffer) };

        let allocation = {
            let mut allocator = alloc.lock().unwrap();
            match allocator.allocate(&vk_alloc::AllocationCreateDesc {
                name: "depth-capture-staging",
                requirements,
                location: MemoryLocation::GpuToCpu,
                linear: true,
                allocation_scheme: vk_alloc::AllocationScheme::GpuAllocatorManaged,
            }) {
                Ok(a) => a,
                Err(e) => {
                    log::warn!("Depth capture staging allocation failed: {e}");
                    // SAFETY: `buffer` was just created, has no bound memory,
                    // and is referenced by no command buffer.
                    unsafe { self.device.destroy_buffer(buffer, None) };
                    return;
                }
            }
        };

        // SAFETY: `buffer` and `allocation` were both just produced by this
        // device/allocator pair; on bind failure the buffer is unbound and
        // unreferenced, so freeing the memory and destroying it is sound.
        unsafe {
            if let Err(e) =
                self.device
                    .bind_buffer_memory(buffer, allocation.memory(), allocation.offset())
            {
                log::warn!("Depth capture staging bind failed: {e}");
                let mut allocator = alloc.lock().unwrap();
                let _ = allocator.free(allocation);
                self.device.destroy_buffer(buffer, None);
                return;
            }
        }

        self.depth_capture_staging = Some((buffer, allocation, required_size));
    }

    pub(super) fn destroy_depth_capture_staging(&mut self) {
        if let Some((buffer, allocation, _)) = self.depth_capture_staging.take() {
            // SAFETY: the only caller of `ensure_depth_capture_staging` is
            // `depth_capture_record_copy` (its grow branch), which runs
            // DURING command-buffer recording (`draw.rs`), not between
            // frames — there is no resize call site for depth-capture
            // staging. Sound anyway: `draw_frame` waits BOTH FIF fences
            // before any recording, so no submitted copy can still target
            // the buffer being freed here. The other caller is shutdown
            // teardown, after `device_wait_idle`.
            unsafe { self.device.destroy_buffer(buffer, None) };
            if let Some(ref alloc) = self.allocator {
                let mut allocator = alloc.lock().unwrap();
                let _ = allocator.free(allocation);
            }
        }
    }
}

/// Regression for #3570 / D10-01. `find_depth_format` can select
/// `D16_UNORM` (Vulkan mandates it; `D32_SFLOAT` is not guaranteed), but
/// the buffer sizing in `depth_capture_record_copy` and the readback
/// decode in `depth_capture_finish_readback` both hardcode 4 bytes /
/// f32-per-sample. A live `VulkanContext` test is impractical (needs a
/// Vulkan device); a source-scan test pins that the format check exists
/// and runs before the pending-readback handoff, mirroring this crate's
/// other guard-ordering regressions.
#[cfg(test)]
mod depth_format_guard_tests {
    #[test]
    fn record_copy_refuses_non_d32_sfloat_before_arming_the_pending_readback() {
        let src = include_str!("depth_capture.rs");

        let format_check_pos = src
            .find("if self.depth_format != vk::Format::D32_SFLOAT {")
            .expect(
                "depth_capture_record_copy must refuse a non-D32_SFLOAT \
                 depth format rather than silently misdecoding it (#3570)",
            );
        let pending_readback_pos = src
            .find("self.depth_capture_pending_readback = Some(extent);")
            .expect("depth_capture_record_copy must arm the pending readback");

        assert!(
            format_check_pos < pending_readback_pos,
            "the D32_SFLOAT format guard must run BEFORE \
             depth_capture_pending_readback is armed, or \
             depth_capture_finish_readback would decode a D16_UNORM \
             buffer as f32 samples. (#3570)"
        );
    }
}

/// Regression for #3628 (REN-2026-08-30-D20-03). Two cross-file ordering
/// invariants keep the depth capture trustworthy as ground truth against
/// `Camera::depth_resolution_at`; both held only by comment before this
/// test, with `draw_frame`'s tail restructured three times (#1748, #2258,
/// #3426) and nothing to stop a fourth from moving either call silently:
///
/// (a) [`VulkanContext::depth_capture_finish_readback`] must run only
///     after this frame-in-flight slot's fence proves the GPU is done —
///     otherwise the readback races the copy and returns stale or
///     torn data with no error. Lives in `sync_and_acquire_frame.rs`
///     since the #3282 `draw_frame` split (not `draw.rs`, where #3628
///     was originally filed — anchored on symbol names instead of the
///     file, per that issue's own drift warning).
/// (b) [`VulkanContext::depth_capture_record_copy`] must run immediately
///     after [`VulkanContext::copy_depth_to_history`] in `draw.rs`,
///     which is what leaves the depth image in the
///     `DEPTH_STENCIL_READ_ONLY_OPTIMAL` layout `depth_capture_record_copy`
///     documents as its precondition — a barrier or layout transition
///     landing between them would silently invalidate that precondition
///     (a validation-layer error or corrupt samples, not a compile or
///     `cargo test` failure).
///
/// A live `VulkanContext` test is impractical for both (70+ Vulkan-loader
/// fields, no safe defaults); source-scan assertions pin the ordering
/// instead, the same shape as `depth_format_guard_tests` above and
/// `egui_pass.rs::dependency_chain_tests` / `resize.rs`'s swapchain-format
/// scanners / `post_passes.rs`'s SVGF-latch scanner.
#[cfg(test)]
mod capture_ordering_tests {
    /// Invariant (a): `depth_capture_finish_readback` (and its
    /// `screenshot_finish_readback` sibling, which documents the same
    /// fence-proven-timing rationale) must be called after the
    /// both-frames-in-flight `wait_for_fences` in
    /// `sync_and_acquire_frame.rs`.
    #[test]
    fn finish_readback_runs_after_the_in_flight_fence_wait() {
        let src = include_str!("sync_and_acquire_frame.rs");

        let wait_pos = src
            .find(".wait_for_fences(")
            .expect("sync_and_acquire_frame must wait for the in-flight fences (#282)");
        let screenshot_pos = src.find("self.screenshot_finish_readback();").expect(
            "sync_and_acquire_frame must call screenshot_finish_readback \
             once the fence proves the GPU is done",
        );
        let depth_pos = src.find("self.depth_capture_finish_readback();").expect(
            "sync_and_acquire_frame must call depth_capture_finish_readback \
             once the fence proves the GPU is done (#3308)",
        );

        assert!(
            wait_pos < screenshot_pos,
            "screenshot_finish_readback must run AFTER wait_for_fences — \
             called any earlier, it would race the GPU's copy and return \
             stale or torn pixels with no error. (#3628 sibling)"
        );
        assert!(
            wait_pos < depth_pos,
            "depth_capture_finish_readback must run AFTER wait_for_fences \
             — the same race, on the depth staging buffer instead of the \
             screenshot one. (#3628)"
        );
    }

    /// Invariant (b): `depth_capture_record_copy(cmd)` must immediately
    /// follow `copy_depth_to_history(cmd)` in `draw.rs`, with no
    /// image-layout-affecting call between them. The GPU-timer wrapper
    /// around `copy_depth_to_history` (`cmd_depth_history_copy_start`/
    /// `_end`) and the explanatory comment between the two calls are the
    /// only things allowed to sit there — neither touches the depth
    /// image's layout, unlike a barrier, copy, or blit would.
    #[test]
    fn record_copy_runs_immediately_after_the_depth_history_copy() {
        let src = include_str!("draw.rs");

        let history_copy_pos = src
            .find("self.copy_depth_to_history(cmd);")
            .expect("draw_frame must call copy_depth_to_history (#2484)");
        let record_copy_pos = src
            .find("self.depth_capture_record_copy(cmd);")
            .expect("draw_frame must call depth_capture_record_copy (#3308)");

        assert!(
            history_copy_pos < record_copy_pos,
            "depth_capture_record_copy must come AFTER copy_depth_to_history \
             — it documents DEPTH_STENCIL_READ_ONLY_OPTIMAL as its \
             precondition, and that layout is only guaranteed once the \
             history copy's own barriers have run. (#3628)"
        );

        let between = &src[history_copy_pos..record_copy_pos];
        for hazard in [
            "cmd_pipeline_barrier",
            "cmd_copy_image(",
            "cmd_copy_image_to_buffer(",
            "cmd_blit_image(",
        ] {
            assert!(
                !between.contains(hazard),
                "found `{hazard}` between copy_depth_to_history and \
                 depth_capture_record_copy — a layout transition or copy \
                 there could invalidate the DEPTH_STENCIL_READ_ONLY_OPTIMAL \
                 precondition depth_capture_record_copy documents and \
                 relies on. (#3628)"
            );
        }
    }
}
