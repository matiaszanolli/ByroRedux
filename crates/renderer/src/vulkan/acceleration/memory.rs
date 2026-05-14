//! Memory housekeeping: shrink-to-fit on BLAS / TLAS scratch and
//! instance buffers, plus the telemetry getters that surface state to
//! the debug console.

use super::super::allocator::SharedAllocator;
use super::super::buffer::GpuBuffer;
use super::constants::WORKING_SET_FLOOR;
use super::predicates::{scratch_should_shrink, tlas_instance_should_shrink};
use super::AccelerationManager;
use ash::vk;

impl AccelerationManager {
    /// Shrink `blas_scratch_buffer` down to the size required by the
    /// current surviving BLAS set, if the high-water mark has grown
    /// disproportionately vs the current peak (see
    /// [`scratch_should_shrink`] for the threshold).
    ///
    /// Call at cell-unload boundaries — **not** from inside a BLAS
    /// build path. The method assumes no BLAS build is in flight (the
    /// shared scratch buffer is only referenced during one-time build
    /// command buffers that the per-build fence already waits on, but
    /// we also don't recreate it mid-build because that would invalidate
    /// a live device address).
    ///
    /// Per-frame TLAS `scratch_buffers[i]` are NOT touched here: they
    /// can be in flight on the GPU at this point and dropping them
    /// without the pending-destroy pattern would be a use-after-free.
    /// Shrinking TLAS scratch needs a follow-up that mirrors
    /// [`pending_destroy_blas`]. Issue #495 tracks this gap.
    ///
    /// # Safety
    ///
    /// - Caller must guarantee no BLAS build command buffer is
    ///   currently referencing `blas_scratch_buffer`. The two build
    ///   paths use one-time command buffers with synchronous fence
    ///   waits, so any call site that is NOT inside a BLAS build is
    ///   safe by construction.
    /// - The `device` and `allocator` must be the same ones that
    ///   allocated the current scratch buffer.
    pub unsafe fn shrink_blas_scratch_to_fit(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
    ) {
        let current = match self.blas_scratch_buffer.as_ref().map(|b| b.size) {
            Some(c) => c,
            None => return, // nothing to shrink
        };

        let peak: vk::DeviceSize = self
            .blas_entries
            .iter()
            .flatten()
            .map(|e| e.build_scratch_size)
            .max()
            .unwrap_or(0);

        if peak == 0 {
            // No BLAS survives — drop the scratch entirely. Next build
            // will allocate fresh (via `scratch_needs_growth`'s None
            // arm) at whatever the new build's peak is.
            if let Some(mut old) = self.blas_scratch_buffer.take() {
                old.destroy(device, allocator);
                log::debug!(
                    "BLAS scratch dropped: {:.1} MB → 0 (no BLAS survives)",
                    current as f64 / (1024.0 * 1024.0),
                );
            }
            return;
        }

        if !scratch_should_shrink(current, peak) {
            return;
        }

        // Reallocate to the current peak size. A future build that
        // exceeds the new capacity will grow via `scratch_needs_growth`.
        if let Some(mut old) = self.blas_scratch_buffer.take() {
            old.destroy(device, allocator);
        }
        match GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            peak,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        ) {
            Ok(new_buf) => {
                log::debug!(
                    "BLAS scratch shrunk: {:.1} MB → {:.1} MB (peak survivor)",
                    current as f64 / (1024.0 * 1024.0),
                    peak as f64 / (1024.0 * 1024.0),
                );
                self.blas_scratch_buffer = Some(new_buf);
            }
            Err(e) => {
                // Allocation failed — leave `blas_scratch_buffer` as
                // `None` and let the next build allocate fresh. This is
                // a degraded but correct state.
                log::warn!("BLAS scratch shrink realloc failed: {e}; next build will re-allocate");
            }
        }
    }

    /// Drop the TLAS instance buffer pair on `slot_index` when its
    /// capacity has grown out of proportion to the current `working_set`
    /// instance count. Mirror of [`shrink_blas_scratch_to_fit`] for the
    /// TLAS staging side (`#645` / MEM-2-3) — `instance_buffer` and
    /// `instance_buffer_device` are grow-only via the existing resize
    /// path at line 1804 (`max_instances < instance_count` triggers a
    /// rebuild), so a 32 K-instance exterior peak pinned ~2 MB of
    /// host-visible BAR + ~2 MB DEVICE_LOCAL stage residue for the
    /// rest of the session even after the player walked into a
    /// small interior.
    ///
    /// Hysteresis matches the BLAS-scratch policy ([`scratch_should_shrink`])
    /// in shape: `2×` ratio + slack (calibrated for TLAS scale via
    /// [`tlas_instance_should_shrink`]). The slot is destroyed
    /// outright; the next [`Self::build_tlas`] call sees
    /// `tlas[slot_index].is_none()` and recreates the slot at the
    /// fresh-build padded size (which the existing `*2 .max(8192)`
    /// padding still honours).
    ///
    /// Returns `true` if the slot was destroyed.
    ///
    /// # Safety
    ///
    /// - Caller must guarantee no command buffer in flight references
    ///   `slot_index`'s TLAS / instance / scratch buffers. Typical
    ///   call site is the App's end-of-frame path **after** the
    ///   per-frame fence wait that gates the next recording into
    ///   `slot_index` — at that point the previous use has
    ///   completed by definition. See `draw.rs::draw_frame` end-of-
    ///   frame block (`#504` SIBLING).
    /// - The `device` and `allocator` must be the same ones that
    ///   allocated the slot's buffers.
    pub unsafe fn shrink_tlas_to_fit(
        &mut self,
        slot_index: usize,
        working_set: u32,
        device: &ash::Device,
        allocator: &SharedAllocator,
    ) -> bool {
        const INSTANCE_STRIDE: vk::DeviceSize =
            std::mem::size_of::<vk::AccelerationStructureInstanceKHR>() as vk::DeviceSize;
        // [`WORKING_SET_FLOOR`] matches the build-path floor
        // `MIN_TLAS_INSTANCE_RESERVE` imposes on every resize so a
        // shrink targeting a tiny working set can't churn below the
        // floor — the next build would just re-pad back to it and
        // we'd burn a free+create cycle for no behavioural change.

        let Some(slot) = self.tlas[slot_index].as_ref() else {
            return false;
        };
        let current_capacity_bytes = (slot.max_instances as vk::DeviceSize) * INSTANCE_STRIDE;
        let working_floor = working_set.max(WORKING_SET_FLOOR);
        let working_set_bytes = (working_floor as vk::DeviceSize) * INSTANCE_STRIDE;
        if !tlas_instance_should_shrink(current_capacity_bytes, working_set_bytes) {
            return false;
        }

        // Tear down the slot. The next build_tlas re-creates from
        // scratch via the `tlas[slot_index].is_none()` arm at line
        // 1804 — that path also sets `needs_full_rebuild = true` so
        // we don't try to UPDATE-mode an empty slot.
        if let Some(mut old) = self.tlas[slot_index].take() {
            log::debug!(
                "TLAS[{}] instance buffer shrunk: {} → 0 instances ({:.1} MB → 0 MB, working set {})",
                slot_index,
                old.max_instances,
                current_capacity_bytes as f64 / (1024.0 * 1024.0),
                working_set,
            );
            self.accel_loader
                .destroy_acceleration_structure(old.accel, None);
            old.buffer.destroy(device, allocator);
            old.instance_buffer.destroy(device, allocator);
            old.instance_buffer_device.destroy(device, allocator);
        }
        true
    }

    /// Drop or reallocate the per-frame TLAS build scratch on
    /// `slot_index` when its capacity has grown out of proportion to
    /// the current peak requirement. Mirror of
    /// [`Self::shrink_blas_scratch_to_fit`] for the per-frame
    /// `scratch_buffers[i]` (#682 / MEM-2-7) — those are grow-only via
    /// [`scratch_needs_growth`], so a single 8 K-instance exterior
    /// peak pinned MB-scale DEVICE_LOCAL VRAM for the rest of the
    /// session even after the player walked into a small interior.
    ///
    /// Hysteresis matches the BLAS-scratch policy ([`scratch_should_shrink`]) —
    /// the same `2× + 16 MB slack` shape, since both paths allocate
    /// from the same DEVICE_LOCAL heap at comparable scale.
    ///
    /// Two cases:
    ///
    /// 1. `tlas[slot_index]` is `None` (slot was destroyed by
    ///    [`Self::shrink_tlas_to_fit`]) — drop the scratch entirely.
    ///    The next [`Self::build_tlas`] call sees `tlas[i].is_none()`,
    ///    re-runs the size query, and allocates a correctly-sized
    ///    scratch via [`scratch_needs_growth`]'s `None` arm.
    /// 2. `tlas[slot_index]` is live — compare the scratch capacity
    ///    against `tlas_scratch_peak_bytes[slot_index]` (recorded at
    ///    last fresh build). If hysteresis fires, reallocate at
    ///    peak. The peak is a static property of the live slot's
    ///    geometry between fresh builds, so this is a reliable
    ///    target.
    ///
    /// Returns `true` when the scratch was destroyed or reallocated.
    ///
    /// # Safety
    ///
    /// - Caller must guarantee no command buffer in flight references
    ///   `scratch_buffers[slot_index]`. Typical call site is the App's
    ///   end-of-frame path **after** the per-frame fence wait that
    ///   gates the next recording into `slot_index`. See
    ///   [`Self::shrink_tlas_to_fit`] doc for the same precondition.
    /// - The `device` and `allocator` must be the same ones that
    ///   allocated the slot's scratch buffer.
    pub unsafe fn shrink_tlas_scratch_to_fit(
        &mut self,
        slot_index: usize,
        device: &ash::Device,
        allocator: &SharedAllocator,
    ) -> bool {
        let current = match self.scratch_buffers[slot_index].as_ref().map(|b| b.size) {
            Some(c) => c,
            None => return false,
        };

        // Slot was destroyed (e.g. by `shrink_tlas_to_fit` on the
        // previous tick) — its scratch is now backing nothing live.
        // Drop entirely; the next build allocates fresh.
        if self.tlas[slot_index].is_none() {
            if let Some(mut old) = self.scratch_buffers[slot_index].take() {
                old.destroy(device, allocator);
                log::debug!(
                    "TLAS[{}] scratch dropped: {:.1} MB → 0 (slot destroyed)",
                    slot_index,
                    current as f64 / (1024.0 * 1024.0),
                );
            }
            self.tlas_scratch_peak_bytes[slot_index] = 0;
            return true;
        }

        // Live slot — compare against last fresh-build peak.
        let peak = self.tlas_scratch_peak_bytes[slot_index];
        if peak == 0 || !scratch_should_shrink(current, peak) {
            return false;
        }

        if let Some(mut old) = self.scratch_buffers[slot_index].take() {
            old.destroy(device, allocator);
        }
        match GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            peak,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        ) {
            Ok(new_buf) => {
                log::debug!(
                    "TLAS[{}] scratch shrunk: {:.1} MB → {:.1} MB (slot peak)",
                    slot_index,
                    current as f64 / (1024.0 * 1024.0),
                    peak as f64 / (1024.0 * 1024.0),
                );
                self.scratch_buffers[slot_index] = Some(new_buf);
                true
            }
            Err(e) => {
                // Allocation failed — leave the slot's scratch as
                // `None`. The next build's `scratch_needs_growth(None,
                // ...)` arm will re-allocate. Degraded but correct.
                log::warn!(
                    "TLAS[{}] scratch shrink realloc failed: {e}; next build will re-allocate",
                    slot_index,
                );
                true
            }
        }
    }

    /// Current total BLAS memory in bytes (static + skinned). Use for
    /// telemetry / `tex.stats` console output. Use `static_blas_bytes()`
    /// for residency-budget decisions — see #920.
    pub fn total_blas_bytes(&self) -> vk::DeviceSize {
        self.total_blas_bytes
    }

    /// Current static (mesh-keyed) BLAS memory in bytes — the subset of
    /// `total_blas_bytes` that lives in `blas_entries` and is eligible
    /// for LRU eviction. Skinned per-entity BLAS (in `skinned_blas`) are
    /// not counted here and are not eviction candidates; their lifecycle
    /// is tied to entity visibility via `drop_skinned_blas`. See #920.
    pub fn static_blas_bytes(&self) -> vk::DeviceSize {
        self.static_blas_bytes
    }

    /// CPU-side TLAS instance staging Vec — `(len, capacity)`. Element
    /// size is `size_of::<vk::AccelerationStructureInstanceKHR>()` (64
    /// bytes). Surfaced for the `ctx.scratch` console command (R6).
    pub fn tlas_instances_scratch_telemetry(&self) -> (usize, usize) {
        (
            self.tlas_instances_scratch.len(),
            self.tlas_instances_scratch.capacity(),
        )
    }

}
