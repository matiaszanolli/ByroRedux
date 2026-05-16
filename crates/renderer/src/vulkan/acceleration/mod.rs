//! Acceleration structure management for RT ray queries.
//!
//! Builds BLAS (bottom-level) per unique mesh and a single TLAS (top-level)
//! rebuilt each frame from all draw instances. The TLAS is bound as a
//! descriptor in the fragment shader for shadow ray queries.
//!
//! ## Module layout
//!
//! Split into submodules during the Session 35 refactor — the
//! 4 383-line monolith now lives in:
//!
//! - [`constants`] — sizing / threshold tunables
//! - [`types`] — `BlasEntry`, `TlasState` data structs
//! - [`predicates`] — pure decision functions (unit-testable, no `&self`)
//! - [`blas_static`] — mesh-keyed BLAS lifecycle + builds + eviction
//! - [`blas_skinned`] — per-entity BLAS lifecycle + builds + refit
//! - [`tlas`] — TLAS build / refit + `tlas_handle` accessor
//! - [`memory`] — `shrink_*_to_fit` + telemetry getters
//!
//! `AccelerationManager` owns the cross-cutting state (struct fields)
//! and the small constructor / destructor methods that don't belong to
//! a single axis. Every submodule extends `impl AccelerationManager`
//! with its own methods — descendants of this module see all private
//! fields by Rust's standard visibility rules.

mod blas_skinned;
mod blas_static;
mod constants;
mod memory;
mod predicates;
mod tlas;
mod types;

pub use constants::SKINNED_BLAS_REFIT_THRESHOLD;
pub use predicates::build_instance_map;
pub use types::{BlasEntry, TlasState};

// Surfaces a CPU-side scratch-shrink helper to sibling modules (notably
// `vulkan::context::draw`, which calls it on the per-frame `Vec` it
// owns alongside the AccelerationManager's own scratch). `pub(super)`
// restricts visibility to the `vulkan` module — no external crate
// callers, by design.
pub(super) use predicates::shrink_scratch_if_oversized;

use super::allocator::SharedAllocator;
use super::buffer::GpuBuffer;
use super::sync::MAX_FRAMES_IN_FLIGHT;
use crate::deferred_destroy::DeferredDestroyQueue;
use ash::vk;
use byroredux_core::ecs::storage::EntityId;
use predicates::{compute_blas_budget, is_scratch_aligned};

/// Manages BLAS and TLAS for RT ray queries.
///
/// TLAS state is double-buffered per frame-in-flight to avoid
/// synchronization hazards: each frame slot has its own accel structure,
/// instance buffer, and scratch buffer. The per-frame fence wait
/// guarantees the previous use of each slot is complete before reuse,
/// so no additional barriers or `device_wait_idle` calls are needed.
pub struct AccelerationManager {
    pub(super) accel_loader: ash::khr::acceleration_structure::Device,
    /// One BLAS per mesh in MeshRegistry (indexed by mesh handle).
    pub(super) blas_entries: Vec<Option<BlasEntry>>,
    /// Per-frame-in-flight TLAS state. Each slot is independently
    /// created/resized when that frame slot first needs it.
    pub tlas: [Option<TlasState>; MAX_FRAMES_IN_FLIGHT],
    /// Per-frame-in-flight TLAS scratch buffer. Grows to the high-water
    /// mark across full rebuilds (`need_new_tlas`); refit/update passes
    /// reuse the existing buffer. See #60 / #424 SIBLING — never a
    /// per-build allocation.
    pub(super) scratch_buffers: [Option<GpuBuffer>; MAX_FRAMES_IN_FLIGHT],
    /// Per-slot record of the most recent fresh-build's
    /// `sizes.build_scratch_size`. Drives [`Self::shrink_tlas_scratch_to_fit`]'s
    /// hysteresis check (#682 / MEM-2-7) — the BLAS path can derive its
    /// peak from `blas_entries`, but TLAS scratch sizing is determined
    /// at slot-create time by the AS spec's
    /// `vkGetAccelerationStructureBuildSizesKHR` query and we don't
    /// re-run it on refit/update, so we cache the last value here.
    /// `0` until the slot has had its first fresh build.
    pub(super) tlas_scratch_peak_bytes: [vk::DeviceSize; MAX_FRAMES_IN_FLIGHT],
    /// Shared BLAS scratch buffer (reused across builds, grows to the
    /// high-water mark across single and batched builds). BLAS builds
    /// use one-time command buffers with a fence wait, so a single
    /// shared buffer is safe (no overlapping BLAS builds). Fix #60,
    /// extended to the batched path in M31.
    pub(super) blas_scratch_buffer: Option<GpuBuffer>,
    /// Reusable scratch buffer for TLAS instance data. Amortized across
    /// frames to avoid ~320KB/frame heap allocation for large scenes.
    ///
    /// Pulled out / restored via `std::mem::take` at the `build_tlas`
    /// call site. If a panic interleaves between `take` and the field
    /// re-assignment, the field reverts to `Vec::new()` — empty capacity,
    /// same as a fresh start. No deferred-destroy / device-handle drama
    /// on the host-only scratch type makes this a clean unwind path.
    /// See REN-D8-NEW-03 (audit 2026-05-09).
    ///
    /// One-frame capacity-amortisation lag (REN-D8-NEW-09, audit
    /// 2026-05-09): when a cell unload drops the working-set size
    /// dramatically (exterior → small interior), this Vec's capacity
    /// stays at the high watermark until the next `build_tlas` call
    /// runs `shrink_scratch_if_oversized`. The post-unload frame
    /// continues to hold the larger backing buffer until then. ~320 KB
    /// at the 8 k-instance ceiling — bounded and self-correcting,
    /// flagged for visibility rather than fixed (a synchronous
    /// shrink on unload would also free the buffer the next BUILD
    /// would re-allocate, which is the worse trade-off).
    pub(super) tlas_instances_scratch: Vec<vk::AccelerationStructureInstanceKHR>,
    /// Reusable scratch for the per-frame BLAS address sequence used by
    /// the BUILD-vs-UPDATE decision (#660). Same amortization story as
    /// `tlas_instances_scratch`: ping-ponged with `tlas[i].last_blas_addresses`
    /// via `std::mem::swap` so neither Vec churns the heap. 8 bytes per
    /// instance — ~64 KB at the 8k-instance ceiling. Same `mem::take`
    /// panic-safe restore + capacity-amortisation lag behaviours as
    /// `tlas_instances_scratch` (REN-D8-NEW-03 / NEW-09).
    pub(super) tlas_addresses_scratch: Vec<u64>,
    /// Monotonic frame counter for BLAS LRU tracking. **Shared across
    /// every TLAS slot** — there's no per-slot counter. Each TLAS slot's
    /// `last_used_frame` field on its `BlasEntry` references stamp this
    /// counter, so a BLAS used by frame N + 1 (slot 0) keeps a
    /// strictly-greater stamp than one last used by frame N (slot 1).
    /// Correct because LRU is a global property; per-slot counters
    /// would make a BLAS used only in even-slot frames look stale
    /// from the odd-slot perspective. See REN-D8-NEW-12 (audit
    /// 2026-05-09) — cosmetic note, no behaviour change implied.
    pub(super) frame_counter: u64,
    /// Total BLAS memory currently allocated (static + skinned), reported
    /// by `total_blas_bytes()` for telemetry / `tex.stats` console output.
    pub(super) total_blas_bytes: vk::DeviceSize,
    /// Subset of `total_blas_bytes` that lives in `blas_entries` (static,
    /// mesh-keyed BLAS). Skinned per-entity BLAS in `skinned_blas` are NOT
    /// counted here. Only this counter is compared against
    /// `blas_budget_bytes` for eviction decisions because only static BLAS
    /// are eviction candidates — skinned BLAS lifecycle is tied to entity
    /// visibility and managed via `drop_skinned_blas`. Without this split,
    /// post-M41 NPC-heavy scenes (50+ skinned actors) could push
    /// `total_blas_bytes` permanently over budget and LRU-thrash static
    /// BLAS every frame. See #920 / REN-D12-NEW-03.
    pub(super) static_blas_bytes: vk::DeviceSize,
    /// Maximum BLAS memory budget in bytes. Eviction triggers when exceeded.
    /// Derived at construction time from DEVICE_LOCAL heap size (VRAM / 3)
    /// with a 256 MB floor. On a 12 GB GPU this yields 4 GB (eviction
    /// virtually never fires); on a 6 GB GPU it yields 2 GB (eviction
    /// fires before OOM).
    pub(super) blas_budget_bytes: vk::DeviceSize,
    /// Entries removed by [`blas_static::drop_blas`] still referenced by
    /// an in-flight TLAS build. Each entry carries a countdown measured
    /// in `MAX_FRAMES_IN_FLIGHT` frames; when it hits zero the underlying
    /// `VkAccelerationStructureKHR` + buffer are finally destroyed. See #372.
    pub(super) pending_destroy_blas: DeferredDestroyQueue<BlasEntry>,
    /// Monotonic counter bumped whenever the `blas_entries` map mutates
    /// (add via `build_blas` / `build_blas_batched`, remove via
    /// `drop_blas` / `evict_unused_blas`). Each [`TlasState`] caches
    /// the value seen at its last BUILD; when the counters disagree the
    /// next `build_tlas` knows the per-instance BLAS device addresses
    /// could have shifted and short-circuits to BUILD without paying
    /// the O(N) zip-compare against the cached address list. Steady-
    /// state frames where no BLAS lifecycle events fired keep paying
    /// the zip (it still has to run to detect frustum / draw-list
    /// composition changes), but cell load / unload / eviction frames
    /// — where the comparison is guaranteed to mismatch — skip it. See #300.
    pub(super) blas_map_generation: u64,
    /// M29 Phase 2 — per-skinned-entity BLAS. One per animated
    /// instance, refit each frame against the SkinComputePipeline
    /// output buffer for that entity. 32 NPCs sharing a malebody mesh
    /// need 32 independent BLAS so each instance's pose feeds its own
    /// shadow / reflection / GI rays. Build flag `ALLOW_UPDATE` is
    /// always set so `cmd_build_acceleration_structures` with
    /// `mode = UPDATE` is legal each frame. Insert / remove bumps
    /// `blas_map_generation` so the TLAS-side cache invalidation
    /// tracks skinned BLAS alongside the static `blas_entries`.
    pub(super) skinned_blas: std::collections::HashMap<EntityId, BlasEntry>,
    /// `minAccelerationStructureScratchOffsetAlignment` queried at
    /// device init (#659 / #260 R-05). Every `scratch_data.device_address`
    /// passed to `cmd_build_acceleration_structures` must be a multiple
    /// of this value. Held to drive the
    /// `debug_assert_scratch_aligned` helper at every scratch-site;
    /// `gpu-allocator` returns sufficiently-aligned GpuOnly allocations
    /// on every desktop driver today, but nothing in the allocator API
    /// guarantees it, so the assert catches a regression before the
    /// driver does.
    pub(super) scratch_align: u32,
}

impl AccelerationManager {
    pub fn new(
        instance: &ash::Instance,
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        scratch_align: u32,
    ) -> Self {
        let accel_loader = ash::khr::acceleration_structure::Device::new(instance, device);
        let blas_budget_bytes = compute_blas_budget(instance, physical_device);
        log::info!(
            "BLAS memory budget: {} MB (derived from VRAM); scratch alignment: {} B",
            blas_budget_bytes / (1024 * 1024),
            scratch_align,
        );
        // Caller (`device::pick_physical_device`) clamps to 1 when RT is
        // unsupported or the driver reports zero, so this is always at
        // least 1 here.
        debug_assert!(
            scratch_align > 0,
            "scratch_align must be >=1; caller clamps zero / non-RT to 1"
        );
        Self {
            accel_loader,
            blas_entries: Vec::new(),
            tlas: [None, None],
            scratch_buffers: [None, None],
            tlas_scratch_peak_bytes: [0, 0],
            blas_scratch_buffer: None,
            tlas_instances_scratch: Vec::new(),
            tlas_addresses_scratch: Vec::new(),
            frame_counter: 0,
            total_blas_bytes: 0,
            static_blas_bytes: 0,
            blas_budget_bytes,
            pending_destroy_blas: DeferredDestroyQueue::new(),
            blas_map_generation: 0,
            skinned_blas: std::collections::HashMap::new(),
            scratch_align,
        }
    }

    /// Debug-only assertion that a scratch device address satisfies the
    /// AS-spec alignment requirement. See #659 / #260 R-05.
    #[inline]
    pub(super) fn debug_assert_scratch_aligned(
        &self,
        scratch_address: vk::DeviceAddress,
        site: &str,
    ) {
        debug_assert!(
            is_scratch_aligned(scratch_address, self.scratch_align),
            "{site}: scratch device address {scratch_address:#x} is not aligned to \
             minAccelerationStructureScratchOffsetAlignment ({align}); the build will \
             violate Vulkan spec. gpu-allocator returned a misaligned GpuOnly \
             allocation — wire round-up-at-use mitigation per #659.",
            site = site,
            align = self.scratch_align,
        );
    }

    /// Destroy all acceleration structures and buffers.
    pub unsafe fn destroy(&mut self, device: &ash::Device, allocator: &SharedAllocator) {
        // #639 / LIFE-H1: drain `pending_destroy_blas` first.
        // `drop_blas` queues entries with a 2-frame countdown that only
        // ticks down inside `tick_deferred_destroy` (called from
        // `draw_frame`); on shutdown the renderer skips the next draw,
        // so any entry whose countdown was still > 0 would leak its
        // VkAccelerationStructureKHR + GpuBuffer. The parent Drop's
        // `device_wait_idle` (`context/mod.rs:1300`) already covers
        // any in-flight command-buffer reference, so it is safe to
        // destroy these immediately regardless of the residual
        // countdown. Same drain shape as `tick_deferred_destroy`
        // above, minus the countdown branch. Sibling fixes already
        // landed in `mesh.rs::MeshRegistry::destroy` (#deferred_destroy
        // drain) and `texture_registry.rs::TextureRegistry::destroy`
        // (per-entry pending_destroy drain). #732 factored the body
        // into `drain_pending_destroys` so the App-level shutdown
        // sweep can call the same drain explicitly before `Drop`.
        self.drain_pending_destroys(device, allocator);
        for entry in self.blas_entries.drain(..) {
            if let Some(mut e) = entry {
                self.accel_loader
                    .destroy_acceleration_structure(e.accel, None);
                e.buffer.destroy(device, allocator);
            }
        }
        for slot in &mut self.tlas {
            if let Some(mut tlas) = slot.take() {
                self.accel_loader
                    .destroy_acceleration_structure(tlas.accel, None);
                tlas.buffer.destroy(device, allocator);
                tlas.instance_buffer.destroy(device, allocator);
                tlas.instance_buffer_device.destroy(device, allocator);
            }
        }
        // #1138 / CONC-D3-NEW-01 — drain `skinned_blas` here so
        // `destroy()` is self-contained regardless of whether the
        // caller pre-drained via `skinned_blas_entities` →
        // `drop_skinned_blas`. Pre-fix the only correct shutdown path
        // routed each entry through `pending_destroy_blas` first
        // (`context/mod.rs::Drop`), and any caller that skipped that
        // dance — a future test constructing `AccelerationManager`
        // directly, or an error-path refactor in `App::shutdown` that
        // bypasses the pre-drain — silently leaked every still-
        // resident per-entity `VkAccelerationStructureKHR` + buffer
        // because the `HashMap` drops as plain memory without the
        // explicit `destroy_acceleration_structure` call. The App-
        // level pre-drain remains in place as an optimization (it
        // routes through `pending_destroy_blas` so the
        // `MAX_FRAMES_IN_FLIGHT` countdown lets a still-flight refit
        // finish), but is no longer a correctness requirement: the
        // parent `device_wait_idle` (`context/mod.rs:1859` /
        // `context/mod.rs:2093`) already settles every in-flight
        // command-buffer reference, so destruction here is safe.
        // SAFETY: caller of `destroy()` (unsafe fn) guarantees no
        // command buffer is still referencing these resources — every
        // production caller pairs the call with `device_wait_idle`.
        for (_eid, mut entry) in self.skinned_blas.drain() {
            self.accel_loader
                .destroy_acceleration_structure(entry.accel, None);
            entry.buffer.destroy(device, allocator);
        }
        for scratch in &mut self.scratch_buffers {
            if let Some(mut s) = scratch.take() {
                s.destroy(device, allocator);
            }
        }
        if let Some(mut scratch) = self.blas_scratch_buffer.take() {
            scratch.destroy(device, allocator);
        }
    }
}

#[cfg(test)]
mod tests;
