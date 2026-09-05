//! Memory housekeeping: shrink-to-fit on BLAS / TLAS scratch and
//! instance buffers, plus the telemetry getters that surface state to
//! the debug console.

use super::super::allocator::SharedAllocator;
use super::super::buffer::GpuBuffer;
use super::constants::WORKING_SET_FLOOR;
use super::predicates::{
    blas_budget_for_heap, scratch_alignment_padding, scratch_should_shrink,
    screen_scaled_reservation_bytes, shared_blas_scratch_peak, tlas_instance_should_shrink,
    tlas_scratch_should_shrink,
};
use super::AccelerationManager;
use crate::deferred_destroy::DEFAULT_COUNTDOWN;
use ash::vk;

impl AccelerationManager {
    /// Shrink `blas_scratch_buffer` down to the size required by the
    /// current surviving BLAS set, if the high-water mark has grown
    /// disproportionately vs the current peak (see
    /// [`scratch_should_shrink`] for the threshold).
    ///
    /// "Surviving BLAS set" means **both** maps that share this one
    /// buffer — static `blas_entries` and per-entity `skinned_blas`
    /// (see [`shared_blas_scratch_peak`]). A static-only peak walk
    /// under-sizes the buffer beneath a live skinned entity's next
    /// `refit_skinned_blas`, which validates no size and grows nothing:
    /// an AS build-scratch overrun, not just wasted VRAM. #2460 /
    /// AS-D1-NEW-01.
    ///
    /// Call at cell-unload boundaries — **not** from inside a BLAS
    /// build path.
    ///
    /// Per-frame TLAS `scratch_buffers[i]` are NOT touched here: they
    /// can be in flight on the GPU at this point and dropping them
    /// without the pending-destroy pattern would be a use-after-free.
    /// TLAS scratch shrink is handled separately by
    /// [`Self::shrink_tlas_scratch_to_fit`], called at its own
    /// fence-gated end-of-frame call site (see that fn's `# Safety`
    /// section for the precondition).
    ///
    /// # Safety
    ///
    /// - The `device` and `allocator` must be the same ones that
    ///   allocated the current scratch buffer.
    ///
    /// Retiring the *old* `blas_scratch_buffer` itself does NOT require
    /// "no BLAS build in flight": this method runs from the streaming
    /// path (`unload_cell`), which can execute in `about_to_wait` while
    /// the previously-submitted frame's skinned-BLAS refit / first-
    /// sight build is still executing on the GPU and referencing the
    /// old scratch buffer's device address. The premise that "any call
    /// site not inside a BLAS build is safe by construction" was true
    /// pre-M29 (#911) but is false since skinned-BLAS refit/build
    /// capture the scratch address into the *per-frame* command
    /// buffer. Fixed by routing the retired buffer through
    /// `pending_destroy_scratch` (deferred, `MAX_FRAMES_IN_FLIGHT`
    /// countdown) below instead of destroying it immediately. See
    /// #1782 / CONC-D1-01.
    pub unsafe fn shrink_blas_scratch_to_fit(
        &mut self,
        device: &ash::Device,
        allocator: &SharedAllocator,
    ) {
        let current = match self.blas_scratch_buffer.as_ref().map(|b| b.size) {
            Some(c) => c,
            None => return, // nothing to shrink
        };

        let peak: vk::DeviceSize = shared_blas_scratch_peak(
            self.blas_entries
                .iter()
                .flatten()
                .map(|e| e.build_scratch_size),
            self.skinned_blas.values().map(|e| e.build_scratch_size),
        );

        if peak == 0 {
            // Neither map holds a BLAS — drop the scratch entirely. Next
            // build will allocate fresh (via `scratch_needs_growth`'s
            // None arm) at whatever the new build's peak is. The union
            // walk is what makes this arm safe: on a static-only walk it
            // fired with skinned entities still resident, and every one
            // of their refits then failed the `blas_scratch_buffer
            // absent` context until a first-sight rebuild.
            //
            // #1782: deferred, not immediate — see this fn's doc.
            if let Some(old) = self.blas_scratch_buffer.take() {
                log::debug!(
                    "BLAS scratch dropped: {:.1} MB → 0 (no BLAS survives)",
                    current as f64 / (1024.0 * 1024.0),
                );
                self.pending_destroy_scratch.push(old, DEFAULT_COUNTDOWN);
            }
            return;
        }

        if !scratch_should_shrink(current, peak) {
            return;
        }

        // Reallocate to the current peak size. A future build that
        // exceeds the new capacity will grow via `scratch_needs_growth`.
        // #1782: deferred, not immediate — see this fn's doc.
        //
        // Carries the same `scratch_alignment_padding` headroom every
        // build path allocates (#1386): consumers round the buffer's
        // device address up to `scratch_align` before submitting, and
        // the skinned refit has no growth check to correct a buffer
        // sized to the bare peak. Immaterial to the shrink decision
        // above — `align` is 128–256 bytes against a 16 MB slack.
        let target = peak.saturating_add(scratch_alignment_padding(self.scratch_align));
        if let Some(old) = self.blas_scratch_buffer.take() {
            self.pending_destroy_scratch.push(old, DEFAULT_COUNTDOWN);
        }
        match GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            target,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        ) {
            Ok(new_buf) => {
                log::debug!(
                    "BLAS scratch shrunk: {:.1} MB → {:.1} MB (peak survivor across {} static \
                     + {} skinned BLAS)",
                    current as f64 / (1024.0 * 1024.0),
                    target as f64 / (1024.0 * 1024.0),
                    self.live_static_blas_count(),
                    self.skinned_blas.len(),
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
        // #2929 — `device` / `allocator` are retained for call-site
        // stability now that this function only RECORDS the shrink intent
        // and `ensure_tlas_state` performs the destroy (same convention as
        // `evict_unused_blas`, #2692). They come back into use the moment
        // this path needs to free anything directly again.
        let _ = (device, allocator);

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

        // #2929 / CON-D1-01 — REQUEST the shrink; do not perform it here.
        //
        // This used to `take()` the slot and destroy the AS + its three
        // buffers outright, relying on the next `build_tlas` to recreate
        // from the `tlas[slot_index].is_none()` arm. That published a
        // dangling handle: scene descriptor set-1 binding 2 goes on naming
        // the destroyed `VkAccelerationStructureKHR` until a *successful*
        // build calls `write_tlas`, and `draw_frame`'s build-failure arm
        // can only re-point the binding at an AS the manager still owns
        // (`if let Some(stale_handle) = accel.tlas_handle(frame)`) — after
        // this teardown it owns nothing, so that guard cannot fire and the
        // geometry pass runs with an invalid statically-used descriptor
        // (binding 2 is not `PARTIALLY_BOUND`, and `triangle.frag`
        // statically uses `topLevelAS`, so the runtime `rt_flag` gate does
        // not downgrade static use to dynamic use).
        //
        // The two events are correlated, not independent: this shrink is
        // triggered by VRAM pressure, which is precisely when the
        // replacement allocation is likeliest to fail.
        //
        // Recording the intent instead lets `ensure_tlas_state` fold the
        // shrink into its existing ALLOCATE-THEN-SWAP path (#2673), which
        // retires the old slot only after every fallible step of the
        // replacement has succeeded. Memory is reclaimed one build later
        // rather than immediately; if that build fails we simply keep the
        // oversized TLAS, downgrading a dangling-descriptor hazard into a
        // missed optimisation.
        let old_max = slot.max_instances;
        self.tlas_shrink_pending[slot_index] = true;
        log::debug!(
            "TLAS[{}] shrink requested: {} instances ({:.1} MB) vs working set {} — \
             will be rebuilt smaller on the next build via allocate-then-swap (#2929)",
            slot_index,
            old_max,
            current_capacity_bytes as f64 / (1024.0 * 1024.0),
            working_set,
        );
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
    /// 1. `tlas[slot_index]` is `None` — a fresh slot at startup, or a
    ///    slot never (re)built after a failed [`Self::ensure_tlas_state`].
    ///    **Not** produced by [`Self::shrink_tlas_to_fit`] since #2929: that
    ///    function no longer destroys the slot, it sets
    ///    `tlas_shrink_pending[slot_index] = true` and lets
    ///    `ensure_tlas_state` fold the shrink into its allocate-then-swap
    ///    path — the slot stays `Some` throughout. Drop the scratch
    ///    entirely here. The next [`Self::build_tlas`] call sees
    ///    `tlas[i].is_none()`, re-runs the size query, and allocates a
    ///    correctly-sized scratch via [`scratch_needs_growth`]'s `None` arm.
    /// 2. `tlas[slot_index]` is live — compare the scratch capacity
    ///    against `tlas_scratch_peak_bytes[slot_index]` (recorded at
    ///    last fresh build). If hysteresis fires, reallocate at peak
    ///    **plus [`scratch_alignment_padding`]** (#2915 — the recorded
    ///    peak is the unpadded `build_scratch_size`, and `build_tlas`
    ///    rounds the device address up before submitting). The peak is a
    ///    static property of the live slot's geometry between fresh
    ///    builds, so this is a reliable target. The replacement is
    ///    allocated before the old buffer is retired, so a failed
    ///    allocation leaves the slot exactly as it was rather than
    ///    stranding a live TLAS with no scratch (#2915 / #2673).
    ///
    ///    #2774 questioned whether this arm is reachable at all — the
    ///    scratch buffer and the recorded peak are written together on
    ///    the FIRST fresh build of a size (differing by only the
    ///    alignment padding), which would keep `tlas_scratch_should_shrink`
    ///    permanently false. They decouple across a **later**
    ///    shrink-triggered rebuild (`tlas_shrink_pending`, set by
    ///    [`Self::shrink_tlas_to_fit`]): the per-frame scratch buffer is
    ///    grow-only ([`scratch_needs_growth`]), so a rebuild that now
    ///    needs less scratch than the slot's still-oversized buffer
    ///    lowers `tlas_scratch_peak_bytes` without touching the buffer
    ///    itself — see `tlas_scratch_shrink_tests::
    ///    fresh_build_records_peak_unconditionally_of_scratch_regrow` for
    ///    the source-level pin of the write ordering that makes this
    ///    possible. The arm is reachable; it is not dead code.
    ///
    /// Returns `true` when the scratch was destroyed or reallocated —
    /// `false` when nothing changed, including when the case-2 realloc
    /// failed and the existing buffer was kept.
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
        // #1226 — TLAS scratch lives at tens of KB to <1 MB; the
        // BLAS-scale `scratch_should_shrink` (16 MB slack) effectively
        // disabled shrink on this path. Switch to the TLAS-calibrated
        // predicate (256 KB slack).
        if peak == 0 || !tlas_scratch_should_shrink(current, peak) {
            return false;
        }

        // #2915 / REN-D1-03 (defect 1) — carry the same
        // `scratch_alignment_padding` headroom `ensure_tlas_state` and
        // `shrink_blas_scratch_to_fit` allocate (#1386). `build_tlas`
        // rounds the buffer's device address up via
        // `align_scratch_address` before submitting, so a buffer sized to
        // the bare `peak` lets the build's scratch range run past the
        // allocation by up to `align - 1` bytes on a driver whose
        // `GpuOnly` addresses aren't already
        // `minAccelerationStructureScratchOffsetAlignment`-aligned.
        //
        // Unlike the BLAS path this is NOT self-correcting: the TLAS
        // growth check (`scratch_needs_growth`) lives inside
        // `ensure_tlas_state`'s `need_new_tlas` block, which may not run
        // for many frames after a shrink. Immaterial to the shrink
        // decision above — `align` is 128–256 B against a 256 KB slack.
        let target = peak.saturating_add(scratch_alignment_padding(self.scratch_align));

        // #2915 / REN-D1-03 (defect 2) — allocate the replacement BEFORE
        // retiring the old buffer, the same allocate-then-swap discipline
        // #2673 applied to `ensure_tlas_state` (whose own comment names
        // this exact failure mode) and which the BLAS sibling gets for
        // free by deferring the old buffer instead of destroying it.
        //
        // Destroy-first left `scratch_buffers[slot] == None` on the error
        // path **with `tlas[slot]` still `Some`**. The next `build_tlas`
        // for that slot then finds `max_instances >= instance_count`, so
        // `ensure_tlas_state` returns early without allocating scratch,
        // and `build_tlas` reaches its scratch lookup with nothing there —
        // a hard abort mid-`draw_frame`, under exactly the VRAM pressure
        // this shrink exists to relieve. Case 1 above may still leave
        // `None`, but only because it also leaves `tlas[slot] == None`,
        // which routes the next build through the fresh-allocate arm.
        let new_buf = match GpuBuffer::create_device_local_uninit(
            device,
            allocator,
            target,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        ) {
            Ok(b) => b,
            Err(e) => {
                // Keep the oversized-but-live buffer. Nothing was
                // retired, so the slot stays fully buildable and the
                // shrink simply doesn't happen this tick — `false`
                // because no scratch was destroyed or reallocated.
                log::warn!(
                    "TLAS[{}] scratch shrink realloc failed: {e}; keeping the existing \
                     {:.1} MB scratch",
                    slot_index,
                    current as f64 / (1024.0 * 1024.0),
                );
                return false;
            }
        };

        // Past the last fallible step — retire the old buffer and commit.
        if let Some(mut old) = self.scratch_buffers[slot_index].take() {
            old.destroy(device, allocator);
        }
        log::debug!(
            "TLAS[{}] scratch shrunk: {:.1} MB → {:.1} MB (slot peak)",
            slot_index,
            current as f64 / (1024.0 * 1024.0),
            target as f64 / (1024.0 * 1024.0),
        );
        self.scratch_buffers[slot_index] = Some(new_buf);
        true
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

    /// The static-BLAS residency budget in bytes — the line
    /// `evict_unused_blas` reclaims against and `should_evict_mid_batch`
    /// measures its 90% early warning from.
    ///
    /// Exposed (#3540) so the per-frame recovery pass can tell a
    /// transient miss ("this mesh was evicted while off-screen, restore
    /// it") from a structural one ("the visible set is larger than the
    /// budget, so restoring anything only displaces something else this
    /// frame needs"). Without that distinction the pass rebuild/evict
    /// thrashes forever. See `plan_static_blas_restore`.
    pub fn blas_budget_bytes(&self) -> vk::DeviceSize {
        self.blas_budget_bytes
    }

    /// Re-derive `blas_budget_bytes` for a render extent, subtracting what the
    /// resolution-scaled post-process passes hold (#3839).
    ///
    /// Call after initial setup and at the end of every swapchain recreate.
    /// The froxel grid alone quadruples when the window doubles in each axis —
    /// roughly 183 MB at 1080p against 730 MB at native 4K — so a budget frozen
    /// at its construction-time value lets the eviction threshold drift a long
    /// way from the memory actually available to BLAS. Re-deriving is pure
    /// arithmetic over a cached heap size: no device probe, no allocation, and
    /// nothing here touches a Vulkan object, so it is safe to call from the
    /// resize path.
    ///
    /// A `--rt-test-blas-budget` override always wins and is never recomputed.
    pub fn recompute_blas_budget(
        &mut self,
        render_extent: vk::Extent2D,
        volumetrics: crate::vulkan::upscaling::VolumetricsConfig,
    ) {
        if self.blas_budget_override.is_some() {
            return;
        }
        let reserved = screen_scaled_reservation_bytes(render_extent, volumetrics);
        let updated = blas_budget_for_heap(self.blas_heap_bytes, reserved);
        if updated == self.blas_budget_bytes {
            return;
        }
        log::info!(
            "BLAS memory budget re-derived for {}x{}: {:.1} MB (heap {:.1} MB less {:.1} MB \
             reserved for resolution-scaled passes); was {:.1} MB",
            render_extent.width,
            render_extent.height,
            updated as f64 / (1024.0 * 1024.0),
            self.blas_heap_bytes as f64 / (1024.0 * 1024.0),
            reserved as f64 / (1024.0 * 1024.0),
            self.blas_budget_bytes as f64 / (1024.0 * 1024.0),
        );
        self.blas_budget_bytes = updated;
    }

    /// Number of *populated* static BLAS slots.
    ///
    /// `blas_entries` is indexed by mesh handle, so its `len()` only ever grows
    /// with the highest handle ever seen and says nothing about residency.
    /// This counts the live entries, which is what the EX-08 ownership soak
    /// (#2374) holds to an exact return across a load/unload cycle.
    pub fn live_static_blas_count(&self) -> usize {
        self.blas_entries.iter().filter(|e| e.is_some()).count()
    }

    /// Number of per-entity skinned BLAS currently resident. Paired with
    /// [`Self::live_static_blas_count`] for the ownership snapshot — the two
    /// have independent lifecycles (mesh-keyed + LRU vs entity-visibility).
    pub fn live_skinned_blas_count(&self) -> usize {
        self.skinned_blas.len()
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

    /// CPU-side TLAS instance-address staging Vec — `(len, capacity)`.
    /// Element size is `size_of::<u64>()`. Surfaced for the `ctx.scratch`
    /// console command (#3693 — the sibling `tlas_instances_scratch` row
    /// existed since R6; this one and
    /// [`Self::tlas_missing_samples_scratch_telemetry`] were declared
    /// later and never added).
    pub fn tlas_addresses_scratch_telemetry(&self) -> (usize, usize) {
        (
            self.tlas_addresses_scratch.len(),
            self.tlas_addresses_scratch.capacity(),
        )
    }

    /// CPU-side "which instance addresses are missing a BLAS" sample Vec
    /// — `(len, capacity)`, bounded by `MISSING_BLAS_SAMPLE_LIMIT`.
    /// Element size is `size_of::<String>()` (the struct's own stack
    /// footprint — the heap bytes each `String` owns aren't reflected,
    /// same under-count caveat as the hash-container rows in
    /// `fill_scratch_telemetry`). Surfaced for the `ctx.scratch` console
    /// command (#3693).
    pub fn tlas_missing_samples_scratch_telemetry(&self) -> (usize, usize) {
        (
            self.tlas_missing_samples_scratch.len(),
            self.tlas_missing_samples_scratch.capacity(),
        )
    }
}
