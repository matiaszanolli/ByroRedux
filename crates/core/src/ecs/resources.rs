//! Built-in engine resources.

use super::components::ItemInstanceId;
use super::resource::Resource;
use std::num::NonZeroU32;

/// System names stored as a resource for debug and console queries.
pub struct SystemList(pub Vec<String>);
impl Resource for SystemList {}

/// Snapshot of [`crate::ecs::scheduler::AccessReport`] captured at
/// scheduler-build time and stored as a resource so console commands
/// (`sys.accesses`) can read the per-stage declared-access map without
/// a live `Scheduler` reference. R7.
pub struct SchedulerAccessReport(pub crate::ecs::scheduler::AccessReport);
impl Resource for SchedulerAccessReport {}

/// Bridge for requesting screenshots from the renderer.
/// Set `requested` to true; the renderer will capture the next frame and
/// place the PNG bytes in `result`.
pub struct ScreenshotBridge {
    pub requested: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub result: std::sync::Arc<std::sync::Mutex<Option<Vec<u8>>>>,
}

impl ScreenshotBridge {
    pub fn request(&self) {
        self.requested
            .store(true, std::sync::atomic::Ordering::Release);
    }

    pub fn take_result(&self) -> Option<Vec<u8>> {
        self.result.lock().unwrap().take()
    }
}

impl Resource for ScreenshotBridge {}

/// Per-frame delta time in seconds.
pub struct DeltaTime(pub f32);
impl Resource for DeltaTime {}

/// Accumulated wall-clock time since engine start, in seconds.
pub struct TotalTime(pub f32);
impl Resource for TotalTime {}

/// Global engine configuration.
pub struct EngineConfig {
    pub vsync: bool,
    pub target_fps: Option<u32>,
    pub debug_logging: bool,
}

impl Resource for EngineConfig {}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            vsync: true,
            target_fps: None,
            debug_logging: cfg!(debug_assertions),
        }
    }
}

const FRAME_HISTORY_SIZE: usize = 128;

/// Per-frame engine diagnostics, updated by the main loop.
///
/// Contains a rolling window of frame times for FPS calculation,
/// plus counts of entities, meshes, textures, and draw calls.
pub struct DebugStats {
    /// Current instantaneous FPS (1.0 / last dt).
    pub fps: f32,
    /// Current frame time in milliseconds.
    pub frame_time_ms: f32,
    /// Rolling buffer of frame times in seconds.
    frame_times: [f32; FRAME_HISTORY_SIZE],
    /// Write cursor into frame_times.
    frame_index: usize,
    /// Number of frames recorded (saturates at FRAME_HISTORY_SIZE).
    frame_count: usize,
    /// Total entities in the world.
    pub entity_count: u32,
    /// GPU meshes in the MeshRegistry. **Registry-wide** — never
    /// drops on cell unload, so a leak that holds the last reference
    /// to a mesh keeps this counter inflated. Pair with
    /// [`Self::meshes_in_use`] to spot the leak: `mesh_count >
    /// meshes_in_use` is the count of meshes the registry is retaining
    /// for no active consumer.
    pub mesh_count: u32,
    /// GPU textures in the TextureRegistry. **Registry-wide** — same
    /// caveats as [`Self::mesh_count`]. Pair with
    /// [`Self::textures_in_use`]. See #637 / FNV-D5-02.
    pub texture_count: u32,
    /// Distinct non-zero `MeshHandle` values reachable from live ECS
    /// entities at the last `stats_system` tick. Scene-scoped: drops
    /// the moment a cell unload removes the last entity holding the
    /// handle, so a regression that keeps a mesh in the registry
    /// after unload shows up as `mesh_count > meshes_in_use`. See
    /// #637 / FNV-D5-02.
    pub meshes_in_use: u32,
    /// Distinct non-zero `TextureHandle` values reachable from live
    /// ECS entities at the last `stats_system` tick. Same scene-scoped
    /// semantics as [`Self::meshes_in_use`]. See #637 / FNV-D5-02.
    pub textures_in_use: u32,
    /// Draw calls last frame.
    pub draw_call_count: u32,
}

impl Resource for DebugStats {}

impl Default for DebugStats {
    fn default() -> Self {
        Self {
            fps: 0.0,
            frame_time_ms: 0.0,
            frame_times: [0.0; FRAME_HISTORY_SIZE],
            frame_index: 0,
            frame_count: 0,
            entity_count: 0,
            mesh_count: 0,
            texture_count: 0,
            meshes_in_use: 0,
            textures_in_use: 0,
            draw_call_count: 0,
        }
    }
}

impl DebugStats {
    /// Record a frame's delta time and update FPS.
    pub fn push_frame_time(&mut self, dt: f32) {
        self.frame_times[self.frame_index] = dt;
        self.frame_index = (self.frame_index + 1) % FRAME_HISTORY_SIZE;
        if self.frame_count < FRAME_HISTORY_SIZE {
            self.frame_count += 1;
        }
        self.frame_time_ms = dt * 1000.0;
        self.fps = if dt > 0.0 { 1.0 / dt } else { 0.0 };
    }

    /// Average FPS over the rolling window.
    pub fn avg_fps(&self) -> f32 {
        if self.frame_count == 0 {
            return 0.0;
        }
        let sum: f32 = self.frame_times[..self.frame_count].iter().sum();
        let avg_dt = sum / self.frame_count as f32;
        if avg_dt > 0.0 {
            1.0 / avg_dt
        } else {
            0.0
        }
    }

    /// Min and max frame times (seconds) over the rolling window.
    pub fn min_max_frame_time(&self) -> (f32, f32) {
        if self.frame_count == 0 {
            return (0.0, 0.0);
        }
        let slice = &self.frame_times[..self.frame_count];
        let min = slice.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = slice.iter().cloned().fold(0.0f32, f32::max);
        (min, max)
    }

    /// Current write index (useful for throttling title bar updates).
    pub fn frame_index(&self) -> usize {
        self.frame_index
    }
}

/// One row of renderer-side scratch-buffer telemetry: name, current
/// `len`, current `capacity`, and the size in bytes of one element so
/// consumers can compute the heap footprint without knowing the
/// renderer's element types.
///
/// Used by R6 (ROADMAP) to catch unbounded `Vec` growth in the
/// renderer's per-frame scratch buffers, particularly across M40 cell
/// streaming where the high-water mark would otherwise grow silently.
#[derive(Debug, Clone, Copy)]
pub struct ScratchRow {
    pub name: &'static str,
    pub len: usize,
    pub capacity: usize,
    pub elem_size_bytes: usize,
}

impl ScratchRow {
    /// Heap footprint of the buffer at its current `capacity` (not `len`).
    pub fn bytes_used(&self) -> usize {
        self.capacity.saturating_mul(self.elem_size_bytes)
    }

    /// Bytes of headroom — `capacity - len` × element size. Sustained
    /// non-zero values across many frames mean the high-water mark
    /// drifted up at some point and never came back down.
    pub fn wasted_bytes(&self) -> usize {
        self.capacity
            .saturating_sub(self.len)
            .saturating_mul(self.elem_size_bytes)
    }
}

/// Snapshot of every renderer-side persistent `Vec` scratch's capacity.
///
/// Refreshed each frame by the engine binary (after `Scheduler::run`,
/// alongside `mesh_count` / `texture_count` on `DebugStats`). Read by
/// the `ctx.scratch` console command.
///
/// `rows` is a reused `Vec` — stabilises at the count of registered
/// scratches (5 today) after the first frame, and is the *only*
/// per-frame heap allocation in the telemetry path. Bounded by the
/// number of declared scratches at the call site
/// (`VulkanContext::fill_scratch_telemetry`), so it cannot itself
/// exhibit the unbounded-growth pattern this resource is designed
/// to catch.
#[derive(Debug, Default)]
pub struct ScratchTelemetry {
    pub rows: Vec<ScratchRow>,
    /// R1 / #780 — unique materials at end of last `build_render_data`
    /// (== `MaterialTable::len()`). Pairs with `materials_interned` to
    /// compute the dedup ratio.
    pub materials_unique: usize,
    /// R1 / #780 — total `intern()` calls during last
    /// `build_render_data` (== `MaterialTable::interned_count()`,
    /// one tick per emitted `DrawCommand`). Dedup ratio =
    /// `materials_unique / materials_interned`. A drop here flags a
    /// regression (alignment hole, non-deterministic float in the
    /// producer) that breaks byte-equality dedup before VRAM
    /// pressure shows it.
    pub materials_interned: usize,
}

impl Resource for ScratchTelemetry {}

impl ScratchTelemetry {
    pub fn total_bytes(&self) -> usize {
        self.rows.iter().map(ScratchRow::bytes_used).sum()
    }

    pub fn total_wasted(&self) -> usize {
        self.rows.iter().map(ScratchRow::wasted_bytes).sum()
    }
}

/// Per-frame skinned-mesh BLAS coverage telemetry.
///
/// Refreshed each frame by the engine binary via
/// `VulkanContext::fill_skin_coverage_stats`, alongside
/// [`ScratchTelemetry`]. Surfaced by the `skin.coverage` console
/// command.
///
/// Closes the M29.3 / "skinned BLAS coverage" observability gap: the
/// per-skinned-entity pre-skin + BLAS refit path is wired, but several
/// silent skips can drop a visible skinned entity from this frame's
/// refit — slot-pool exhaustion (`failed_skin_slots`), first-sight
/// prime/BUILD failure, missing `MeshHandle`. This resource is the
/// falsifiable signal that "every visible skinned entity refit this
/// frame": `refits_succeeded == dispatches_total` is the green-bar.
#[derive(Debug, Default)]
pub struct SkinCoverageStats {
    /// Unique skinned entities in this frame's draw_commands (those with
    /// `bone_offset > 0`). Denominator for everything below.
    pub dispatches_total: u32,
    /// Entities currently holding a `SkinSlot` (gauge — not per-frame).
    pub slots_active: u32,
    /// Slot-pool capacity (`SKIN_MAX_SLOTS`). Constant after init.
    /// `slots_active` approaching this is the pressure signal.
    pub slot_pool_capacity: u32,
    /// Entities whose `create_slot` call returned an error and are
    /// suppressed until LRU eviction frees a slot. Gauge — cleared on
    /// any eviction.
    pub slots_failed: u32,
    /// First-sight entities entering the sync prime + BUILD path this
    /// frame (slot newly created OR BLAS missing/rebuild-requested).
    pub first_sight_attempted: u32,
    /// First-sight entities for which both prime and BUILD succeeded.
    pub first_sight_succeeded: u32,
    /// Per-frame compute dispatch + refit attempts (entities with a
    /// `SkinSlot` AND with an existing BLAS — i.e. past first-sight).
    pub refits_attempted: u32,
    /// Refits that returned `Ok` this frame.
    pub refits_succeeded: u32,
    /// Sample of entity IDs currently in `failed_skin_slots`. Bounded
    /// snapshot (first ~16 entries) so the resource stays cheap to copy
    /// out of the renderer each frame; the full count is in
    /// `slots_failed`.
    pub failed_entity_ids: Vec<super::storage::EntityId>,
}

impl Resource for SkinCoverageStats {}

impl SkinCoverageStats {
    /// True when every visible skinned entity got a refit this frame.
    /// Reads correct only after `fill_skin_coverage_stats` populates
    /// the snapshot; before that all counters are 0 and this returns
    /// `true` trivially.
    pub fn fully_covered(&self) -> bool {
        self.refits_succeeded == self.dispatches_total
            && self.slots_failed == 0
    }
}

/// Per-stack divergent state.
///
/// Allocated only when an [`ItemStack`](super::components::ItemStack)
/// can't be represented by `(base_form_id, count)` alone — modlists,
/// condition deltas, charge state, named items. Most inventory rows
/// don't need one; the stack-only common case keeps `instance` at
/// `None`.
///
/// Fields are open-ended: future equip mechanics (FO4 OMOD, weapon
/// charge, food spoilage) extend this struct rather than parallel
/// inventory types. Phase A of #896 ships it minimal; Phase B/C and
/// M45 fill in real fields.
#[derive(Debug, Default, Clone)]
pub struct ItemInstance {
    /// Reserved for now. Real fields land alongside the consuming
    /// gameplay system (M45 save round-trip; FO4 OMOD wiring).
    _reserved: (),
}

/// Sparse arena for [`ItemInstance`]s with a free-list for slot reuse.
///
/// The free-list is what prevents Bethesda's pickup-drop-pickup save-
/// bloat tail. When an `ItemInstance` is released, its slot returns
/// to `free` and the next allocation reuses it. Save format dumps
/// `instances` + `free` verbatim — bounded, not log-shaped.
///
/// Slot 0 is reserved as a sentinel so [`ItemInstanceId`] (which
/// wraps `NonZeroU32`) can encode "no instance" as the absence of
/// the option without burning a u32 niche.
#[derive(Debug)]
pub struct ItemInstancePool {
    instances: Vec<Option<ItemInstance>>,
    free: Vec<u32>,
}

impl Resource for ItemInstancePool {}

impl Default for ItemInstancePool {
    fn default() -> Self {
        // Pre-fill slot 0 as the reserved sentinel.
        Self {
            instances: vec![None],
            free: Vec::new(),
        }
    }
}

impl ItemInstancePool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate an instance, reusing a freed slot if available.
    pub fn allocate(&mut self, inst: ItemInstance) -> ItemInstanceId {
        if let Some(slot) = self.free.pop() {
            self.instances[slot as usize] = Some(inst);
            ItemInstanceId(NonZeroU32::new(slot).expect("free-list never holds slot 0"))
        } else {
            let slot = self.instances.len();
            // `usize::MAX` would wrap; in practice the cap is u32::MAX
            // entries, far above any realistic save's instance count.
            assert!(slot <= u32::MAX as usize, "ItemInstancePool overflow");
            self.instances.push(Some(inst));
            ItemInstanceId(
                NonZeroU32::new(slot as u32).expect("slot >= 1 since slot 0 is reserved"),
            )
        }
    }

    /// Release a slot back to the free-list. Returns the freed
    /// instance if it was live, `None` if the slot was already free
    /// or out of bounds (defensive — duplicate-free is a logic bug
    /// elsewhere but we don't want to corrupt the arena over it).
    pub fn release(&mut self, id: ItemInstanceId) -> Option<ItemInstance> {
        let slot = id.0.get();
        let cell = self.instances.get_mut(slot as usize)?;
        let taken = cell.take()?;
        // Avoid double-pushing onto `free` if release is called twice
        // for the same id (the `cell.take` above guards the live state
        // but the free-list contract still needs deduping).
        if !self.free.contains(&slot) {
            self.free.push(slot);
        }
        Some(taken)
    }

    pub fn get(&self, id: ItemInstanceId) -> Option<&ItemInstance> {
        self.instances
            .get(id.0.get() as usize)
            .and_then(|cell| cell.as_ref())
    }

    /// Number of live instances (excludes free slots and the sentinel).
    pub fn live_count(&self) -> usize {
        self.instances
            .iter()
            .skip(1) // skip the sentinel
            .filter(|c| c.is_some())
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_stats_default_is_zero() {
        let stats = DebugStats::default();
        assert_eq!(stats.fps, 0.0);
        assert_eq!(stats.avg_fps(), 0.0);
        assert_eq!(stats.min_max_frame_time(), (0.0, 0.0));
    }

    #[test]
    fn push_frame_time_updates_fps() {
        let mut stats = DebugStats::default();
        stats.push_frame_time(1.0 / 60.0); // 60 FPS
        assert!((stats.fps - 60.0).abs() < 0.5);
        assert!((stats.frame_time_ms - 16.67).abs() < 0.1);
    }

    #[test]
    fn avg_fps_over_multiple_frames() {
        let mut stats = DebugStats::default();
        for _ in 0..10 {
            stats.push_frame_time(1.0 / 30.0); // 30 FPS
        }
        assert!((stats.avg_fps() - 30.0).abs() < 0.5);
    }

    #[test]
    fn min_max_frame_time_correct() {
        let mut stats = DebugStats::default();
        stats.push_frame_time(0.010); // 10ms
        stats.push_frame_time(0.020); // 20ms
        stats.push_frame_time(0.005); // 5ms
        let (min, max) = stats.min_max_frame_time();
        assert!((min - 0.005).abs() < 1e-6);
        assert!((max - 0.020).abs() < 1e-6);
    }

    #[test]
    fn circular_buffer_wraps() {
        let mut stats = DebugStats::default();
        // Fill more than 128 frames
        for i in 0..200 {
            stats.push_frame_time(0.016);
            assert!(stats.frame_count <= FRAME_HISTORY_SIZE);
            if i >= FRAME_HISTORY_SIZE {
                assert_eq!(stats.frame_count, FRAME_HISTORY_SIZE);
            }
        }
        assert_eq!(stats.frame_index, 200 % FRAME_HISTORY_SIZE);
        assert!((stats.avg_fps() - 62.5).abs() < 1.0);
    }

    #[test]
    fn scratch_row_bytes_used_is_capacity_times_elem_size() {
        let row = ScratchRow {
            name: "x",
            len: 10,
            capacity: 100,
            elem_size_bytes: 32,
        };
        assert_eq!(row.bytes_used(), 100 * 32);
        assert_eq!(row.wasted_bytes(), (100 - 10) * 32);
    }

    #[test]
    fn scratch_row_wasted_is_zero_when_full() {
        let row = ScratchRow {
            name: "full",
            len: 50,
            capacity: 50,
            elem_size_bytes: 8,
        };
        assert_eq!(row.wasted_bytes(), 0);
    }

    #[test]
    fn skin_coverage_default_is_trivially_covered() {
        // Default reports zero dispatches → no work missed; this is
        // the no-skinned-content-this-frame baseline that `skin.
        // coverage` should print as "n/a".
        let cov = SkinCoverageStats::default();
        assert!(cov.fully_covered());
        assert_eq!(cov.dispatches_total, 0);
        assert_eq!(cov.refits_succeeded, 0);
    }

    #[test]
    fn skin_coverage_full_when_refits_match_dispatches() {
        let cov = SkinCoverageStats {
            dispatches_total: 23,
            refits_succeeded: 23,
            ..Default::default()
        };
        assert!(cov.fully_covered());
    }

    #[test]
    fn skin_coverage_partial_when_refits_lag() {
        // 23 visible skinned entities but only 22 refit — one was
        // dropped somewhere between dispatches collection and the refit
        // loop. This is exactly the regression mode the instrumentation
        // exists to surface.
        let cov = SkinCoverageStats {
            dispatches_total: 23,
            refits_succeeded: 22,
            ..Default::default()
        };
        assert!(!cov.fully_covered());
    }

    #[test]
    fn skin_coverage_partial_when_slots_failed_nonzero() {
        // Even if refits == dispatches arithmetically, a non-zero
        // failed-slot count means visible entities were silently
        // skipped from the dispatches stream (they never reached
        // first-sight). The green-bar must fail.
        let cov = SkinCoverageStats {
            dispatches_total: 10,
            refits_succeeded: 10,
            slots_failed: 2,
            ..Default::default()
        };
        assert!(!cov.fully_covered());
    }

    #[test]
    fn item_instance_pool_allocate_starts_at_one() {
        let mut pool = ItemInstancePool::new();
        let id = pool.allocate(ItemInstance::default());
        // Slot 0 is reserved; first allocation must land at 1.
        assert_eq!(id.0.get(), 1);
        assert_eq!(pool.live_count(), 1);
    }

    #[test]
    fn item_instance_pool_release_reclaims_slot() {
        let mut pool = ItemInstancePool::new();
        let a = pool.allocate(ItemInstance::default());
        let b = pool.allocate(ItemInstance::default());
        assert_eq!(pool.live_count(), 2);
        let released = pool.release(a);
        assert!(released.is_some());
        assert_eq!(pool.live_count(), 1);
        // Next allocation reuses `a`'s slot (LIFO from free-list).
        let c = pool.allocate(ItemInstance::default());
        assert_eq!(c, a);
        assert_eq!(pool.live_count(), 2);
        // `b` stays valid throughout.
        assert!(pool.get(b).is_some());
    }

    #[test]
    fn item_instance_pool_double_release_does_not_corrupt_free_list() {
        let mut pool = ItemInstancePool::new();
        let a = pool.allocate(ItemInstance::default());
        let _ = pool.allocate(ItemInstance::default());
        assert!(pool.release(a).is_some());
        // Second release of the same id is a no-op rather than a
        // duplicate free-list entry.
        assert!(pool.release(a).is_none());
        let c = pool.allocate(ItemInstance::default());
        // Should reuse `a`'s slot exactly once.
        assert_eq!(c, a);
        let d = pool.allocate(ItemInstance::default());
        // `d` lands at a fresh slot — would be slot 1's reuse if the
        // free-list were corrupted with a duplicate entry.
        assert_ne!(d, a);
    }

    #[test]
    fn scratch_telemetry_aggregates_rows() {
        let mut tlm = ScratchTelemetry::default();
        tlm.rows.push(ScratchRow {
            name: "a",
            len: 1,
            capacity: 10,
            elem_size_bytes: 4,
        });
        tlm.rows.push(ScratchRow {
            name: "b",
            len: 5,
            capacity: 5,
            elem_size_bytes: 16,
        });
        assert_eq!(tlm.total_bytes(), 10 * 4 + 5 * 16);
        assert_eq!(tlm.total_wasted(), (10 - 1) * 4 + 0);
    }
}
