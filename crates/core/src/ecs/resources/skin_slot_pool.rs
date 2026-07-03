//! M29.6 — per-entity persistent bone-palette slot pool (`bind_inverses` SSBO).
//!
//! Split out of `resources.rs` at #1869 (TD1-2026-07-03-01) once that file
//! crossed the 2000-LOC Dim-1 threshold — this was the single largest
//! cohesive unit in it (struct + impl + tests, ~870 lines).

use crate::ecs::resource::Resource;
use crate::ecs::storage::EntityId;

/// Per-entity persistent slot pool for the GPU bone-palette
/// (`bone_world` + `bind_inverses`) SSBOs.
///
/// Pre-M29.6 the `build_skinned_palettes` pass packed every skinned
/// entity into iteration-order slots — entity E could land at offset
/// 100 one frame and offset 144 the next. M29.5's GPU compute pass
/// works fine with that because both inputs (`bone_world` and
/// `bind_inverses`) get re-uploaded per frame in the same packing.
///
/// M29.6 promotes `bind_inverses` to a persistent SSBO that's only
/// written when an entity first appears. For that to work the slot ID
/// must be stable across frames for a given entity — this resource
/// owns the per-entity slot assignment.
///
/// Slot 0 is reserved for the global identity slot
/// (`build_render_data` pushes IDENTITY at `bone_world[0]` /
/// `bind_inverses[0]`); the pool's `next_slot` starts at 1 and
/// `allocate` never returns 0.
///
/// Lifecycle:
/// - First sight (entity in SkinnedMesh query but not yet in pool):
///   `allocate(entity, frame)` returns a fresh slot ID and pushes
///   `(slot, entity)` onto `pending_uploads`. The caller drains
///   `pending_uploads` after `build_skinned_palettes` and the
///   renderer schedules a one-time `bind_inverses` upload for each
///   pending slot.
/// - Steady-state: `allocate(entity, frame)` returns the existing
///   slot ID and refreshes `last_seen_frame`.
/// - Reclaim: `sweep(current_frame, min_idle)` returns slots whose
///   entities haven't been seen for `min_idle` frames; the caller can
///   then queue the slot for reuse. Slot data on the GPU is not
///   cleared — overwritten by the next `allocate`'s upload.
///
/// Capacity: `max_skinned` is set at construction (typical value is
/// `MAX_TOTAL_BONES / MAX_BONES_PER_MESH`, currently 196608 / 144 =
/// 1366 with slot 0 reserved → 1365 allocatable; see #1284); `allocate`
/// returns `None` past it. The caller is expected to warn-once and
/// fall back to bind-pose rendering for the overflowed entity. See
/// `Self::overflow_warned` (one-shot log) and `Self::overflow_attempt_count`
/// (cumulative spill telemetry surfaced via `DebugStats::skin_pool_*` and
/// the `engine::stats` `skin=L/M+S` line); see #1284 for the cap-sizing
/// feedback loop.
pub struct SkinSlotPool {
    /// Stable slot ID per entity. Values are in `1..=max_slot`; slot 0
    /// is reserved.
    entity_to_slot: std::collections::HashMap<EntityId, u32>,
    /// Recycled slot IDs (popped LIFO so the most-recently-freed slot
    /// is reused first — cache-friendlier than FIFO on the persistent
    /// `bind_inverses` SSBO).
    free_list: Vec<u32>,
    /// Monotonic ceiling for fresh allocations. Starts at 1 (slot 0
    /// reserved). Bumped only when `free_list` is empty.
    next_slot: u32,
    /// Frame at which each entity was last seen via `mark_seen`.
    /// Drives the `sweep` reclaim.
    last_seen_frame: std::collections::HashMap<EntityId, u64>,
    /// `(slot_id, entity)` for entities that allocated a slot this
    /// frame and still need their `bind_inverses` uploaded to the
    /// persistent SSBO. Drained by the renderer once per frame.
    pending_uploads: Vec<(u32, EntityId)>,
    /// Capacity — the pool refuses to allocate past `next_slot >
    /// max_slot`. Set at construction; never changes.
    max_slot: u32,
    /// One-shot warn gate for the overflow path. Latched on first
    /// `allocate` failure; never reset (the warn is observability,
    /// not flow control).
    overflow_warned: bool,
    /// Per-entity hash of the last-uploaded `bone_world` slice. Set by
    /// `try_mark_pose_dirty`; absent for entities that haven't been
    /// hashed yet. #1195 / PERF-DIM7-01.
    last_pose_hash: std::collections::HashMap<EntityId, u64>,
    /// Entities whose pose hash differs from the previous frame's
    /// (or who have never been hashed). Cleared at frame start by
    /// `clear_pose_dirty`; populated by `try_mark_pose_dirty`. Drained
    /// by the renderer to gate skin compute dispatch + skinned-BLAS
    /// refit. #1195 / PERF-DIM7-01, paired with #1196 / PERF-DIM7-02.
    pose_dirty: std::collections::HashSet<EntityId>,
    /// Pre-image of `last_pose_hash` for every entity `try_mark_pose_dirty`
    /// committed dirty *this frame*, keyed by entity. `None` means the
    /// entity had no prior hash (first-sight). Cleared by `clear_pose_dirty`
    /// at the start of the next frame's hashing pass; drained by
    /// `rollback_pending_pose_commits` when the renderer discovers the
    /// commit was premature. #1796 / D6-02.
    rollback_pose_hash: std::collections::HashMap<EntityId, Option<u64>>,
    /// Monotonic cumulative count of over-cap `allocate()` **calls**
    /// (i.e. calls that returned `None`) since construction — **not** a
    /// distinct-entity count. An over-cap entity is never inserted into
    /// `entity_to_slot`, so it is never resident and re-hits the spill
    /// branch on every subsequent `allocate()`: one persistently
    /// over-cap entity re-counts every frame. Treat the value as an
    /// upper bound on demand, not per-frame distinct-entity demand —
    /// reading it as the latter overshoots by the frame count (#1296 /
    /// D12-C1). Drives the #1284 cap-sizing notes: the one-shot warning
    /// says the cap was exceeded; this says (roughly) by how much. A
    /// future loop wanting per-frame distinct demand must track a
    /// separate frame-reset `HashSet<EntityId>` high-water.
    overflow_attempt_count: u32,
}

impl SkinSlotPool {
    /// Construct a pool with capacity `max_skinned` (slots 1..=
    /// `max_skinned` are allocatable; slot 0 is reserved).
    pub fn new(max_skinned: u32) -> Self {
        assert!(
            max_skinned >= 1,
            "SkinSlotPool requires capacity ≥ 1; got {max_skinned}"
        );
        Self {
            entity_to_slot: std::collections::HashMap::new(),
            free_list: Vec::new(),
            next_slot: 1,
            last_seen_frame: std::collections::HashMap::new(),
            pending_uploads: Vec::new(),
            max_slot: max_skinned,
            overflow_warned: false,
            last_pose_hash: std::collections::HashMap::new(),
            pose_dirty: std::collections::HashSet::new(),
            rollback_pose_hash: std::collections::HashMap::new(),
            overflow_attempt_count: 0,
        }
    }

    /// Return the slot ID assigned to `entity`, allocating a fresh
    /// slot on first sight. `Some(slot)` on success; `None` when the
    /// pool is full (logs once per session — see `overflow_warned`).
    ///
    /// Side effects:
    /// - First-sight calls push `(slot, entity)` onto `pending_uploads`.
    /// - Every call refreshes `last_seen_frame[entity] = frame`.
    pub fn allocate(&mut self, entity: EntityId, frame: u64) -> Option<u32> {
        if let Some(&slot) = self.entity_to_slot.get(&entity) {
            self.last_seen_frame.insert(entity, frame);
            return Some(slot);
        }
        let slot = if let Some(reused) = self.free_list.pop() {
            reused
        } else if self.next_slot <= self.max_slot {
            let s = self.next_slot;
            self.next_slot += 1;
            s
        } else {
            self.overflow_attempt_count = self.overflow_attempt_count.saturating_add(1);
            if !self.overflow_warned {
                self.overflow_warned = true;
                log::warn!(
                    "SkinSlotPool exhausted at capacity {} (slot 0 reserved). \
                     Excess skinned entities silently fall back to bind pose. \
                     Bump MAX_TOTAL_BONES or implement variable-stride packing. \
                     (Subsequent spills counted silently — query \
                     `overflow_attempt_count` for total demand.)",
                    self.max_slot,
                );
            }
            return None;
        };
        self.entity_to_slot.insert(entity, slot);
        self.last_seen_frame.insert(entity, frame);
        self.pending_uploads.push((slot, entity));
        Some(slot)
    }

    /// Refresh `last_seen_frame[entity]` without changing allocation.
    /// Equivalent to `allocate` for already-resident entities; provided
    /// as a separate entry-point for paths that look up the slot
    /// without re-allocating.
    pub fn mark_seen(&mut self, entity: EntityId, frame: u64) {
        if self.entity_to_slot.contains_key(&entity) {
            self.last_seen_frame.insert(entity, frame);
        }
    }

    /// Return the slot ID for `entity` without touching `last_seen_frame`
    /// or `pending_uploads`. Returns `None` if the entity has never
    /// been allocated. For draws that already routed through
    /// `allocate`; useful for diagnostics.
    pub fn get(&self, entity: EntityId) -> Option<u32> {
        self.entity_to_slot.get(&entity).copied()
    }

    /// Cumulative count of `allocate` calls that returned `None` (pool
    /// was full). Drives the #1284 cap-sizing feedback loop — capture
    /// this from `audit-runtime` baselines to know how far the
    /// next `MAX_TOTAL_BONES` bump needs to go.
    pub fn overflow_attempt_count(&self) -> u32 {
        self.overflow_attempt_count
    }

    /// Number of allocatable slots currently in use (excludes slot 0).
    pub fn live_slot_count(&self) -> u32 {
        self.entity_to_slot.len() as u32
    }

    /// Configured capacity (slots 1..=`max_slot` are allocatable).
    pub fn max_slot(&self) -> u32 {
        self.max_slot
    }

    /// Drain up to `max` pending uploads, returning the list of slots
    /// whose `bind_inverses` haven't been uploaded yet. The caller
    /// (renderer) must complete the GPU upload BEFORE the next
    /// compute dispatch reads `bind_inverses_persistent[slot × MBPM
    /// ..]`. Any entries beyond `max` STAY in `pending_uploads` and
    /// will surface on the next `drain_pending` call (M29.6 hotfix
    /// #1192 / SAFE-D7-NEW-02 — the renderer's staging buffer has a
    /// fixed `MAX_PENDING_BIND_INVERSE_UPLOADS_PER_FRAME` cap; pre-
    /// hotfix the renderer silently dropped the excess, leaving the
    /// pool's `entity_to_slot` populated but the persistent SSBO
    /// untouched at those slots).
    ///
    /// Pass `usize::MAX` to drain everything (the pre-hotfix shape).
    pub fn drain_pending(&mut self, max: usize) -> Vec<(u32, EntityId)> {
        let n = self.pending_uploads.len().min(max);
        self.pending_uploads.drain(..n).collect()
    }

    /// Undo a `drain_pending` whose upload didn't happen. #1791 / D6-01
    /// — `drain_pending` is called before `draw_frame`, and `draw_frame`
    /// has early-return paths (empty framebuffers, swapchain out-of-
    /// date) preceding the actual `bind_inverses` SSBO write. Without
    /// this, a drained-but-unwritten entry is simply lost: the slot
    /// stays resident in `entity_to_slot` (so `allocate` never re-queues
    /// it), but the persistent SSBO region for that slot is never
    /// written, permanently corrupting the entity's skinning palette.
    ///
    /// Prepends `entries` so they're the first candidates drained next
    /// frame — they were already overdue once. Caller supplies exactly
    /// the entries that were actually about to be uploaded (i.e. that
    /// survived any caller-side filtering of `drain_pending`'s output),
    /// not the raw drain — an entry the caller already decided to drop
    /// permanently (e.g. its `SkinnedMesh` component is gone) must not
    /// come back through this path.
    pub fn requeue_pending(&mut self, entries: Vec<(u32, EntityId)>) {
        if entries.is_empty() {
            return;
        }
        let mut merged = entries;
        merged.append(&mut self.pending_uploads);
        self.pending_uploads = merged;
    }

    /// Sweep entities idle for ≥ `min_idle` frames; return their slot
    /// IDs to the free-list. Returns the freed slot IDs (caller can
    /// log / telemetry if desired).
    ///
    /// `current_frame` should be the renderer's `frame_counter`;
    /// `min_idle` is typically `MAX_FRAMES_IN_FLIGHT + 1` so a slot is
    /// only reclaimed after no in-flight command buffer could
    /// reference it.
    pub fn sweep(&mut self, current_frame: u64, min_idle: u64) -> Vec<u32> {
        let mut freed = Vec::new();
        // Collect doomed entities first to avoid mutating the map
        // while iterating.
        let doomed: Vec<EntityId> = self
            .last_seen_frame
            .iter()
            .filter_map(|(entity, &last)| {
                let idle = current_frame.saturating_sub(last);
                if idle >= min_idle {
                    Some(*entity)
                } else {
                    None
                }
            })
            .collect();
        for entity in doomed {
            if let Some(slot) = self.entity_to_slot.remove(&entity) {
                self.free_list.push(slot);
                freed.push(slot);
            }
            self.last_seen_frame.remove(&entity);
            // #1195 / PERF-DIM7-01 — evicted entities must drop their
            // stale pose hash too; otherwise a recycled slot ID under
            // a new entity could collide with the prior tenant's hash
            // map entry (the keying is by EntityId so collisions are
            // impossible in practice, but dropping is the right
            // hygiene — keeps the map bounded to live entities).
            self.last_pose_hash.remove(&entity);
            self.pose_dirty.remove(&entity);
            // #1796 / D6-02 — same eviction hygiene for the rollback
            // pre-image map; an evicted entity's pending rollback (if
            // any) is moot once its slot is reclaimed.
            self.rollback_pose_hash.remove(&entity);
        }

        // #1379 — Contract the issued range: if the highest slots are now
        // free, lower `next_slot` so the bone-world copy + skin-palette
        // dispatch don't cover dead tail slots. Sort the free_list
        // ascending so we can pop the contiguous top in O(k·log f) total.
        // Only runs when something was freed this sweep; no-op when stable.
        // Never moves live slot mappings — the persistent bind_inverses
        // SSBO + descriptor_bindings cache stay valid.
        if !freed.is_empty() {
            self.free_list.sort_unstable();
            while self
                .free_list
                .last()
                .is_some_and(|&top| top == self.next_slot.saturating_sub(1))
            {
                self.free_list.pop();
                if self.next_slot > 1 {
                    self.next_slot -= 1;
                }
            }
        }

        freed
    }

    /// Number of slots currently allocated to entities. Diagnostic.
    pub fn allocated_count(&self) -> usize {
        self.entity_to_slot.len()
    }

    /// Free-list depth — slots reclaimed but not yet reused.
    /// Diagnostic.
    pub fn free_list_depth(&self) -> usize {
        self.free_list.len()
    }

    /// Highest slot ID issued (or 0 if none). The skin_palette
    /// dispatch covers slots `0..=max_used_slot` × MBPM. Diagnostic
    /// and dispatch-sizing.
    pub fn max_used_slot(&self) -> u32 {
        self.next_slot.saturating_sub(1)
    }

    /// Update the per-entity pose hash; mark dirty (and return `true`)
    /// when the new hash differs from the stored one (or the entity
    /// has never been hashed). #1195 / PERF-DIM7-01.
    ///
    /// Callers compute `new_hash` over the entity's bone-matrix slice
    /// of `bone_world_out` after [`crate::ecs::components::SkinnedMesh`]
    /// pose construction in `build_skinned_palettes`. The renderer
    /// drains [`pose_dirty`](Self::pose_dirty) once per frame and
    /// uses it to gate skin compute dispatch + skinned-BLAS refit.
    ///
    /// First-sight (no prior hash) always returns `true`. Idle bone
    /// poses converge to "not dirty" on the second consecutive frame.
    ///
    /// #1796 / D6-02 — this commits `last_pose_hash` (the dirty-gate
    /// baseline) at CPU pose-build time, which runs *before* `draw_frame`
    /// — and therefore before it's known whether the frame will actually
    /// reach the skin dispatch section (`draw_frame` has two early-return
    /// guards ahead of it: empty framebuffers, `ERROR_OUT_OF_DATE_KHR`).
    /// The pre-image of every commit made this frame is stashed in
    /// `rollback_pose_hash` so the caller can undo it via
    /// [`rollback_pending_pose_commits`](Self::rollback_pending_pose_commits)
    /// if `draw_frame` bails before dispatching.
    pub fn try_mark_pose_dirty(&mut self, entity: EntityId, new_hash: u64) -> bool {
        let old = self.last_pose_hash.get(&entity).copied();
        let dirty = match old {
            Some(old) => old != new_hash,
            None => true,
        };
        if dirty {
            self.rollback_pose_hash.entry(entity).or_insert(old);
            self.last_pose_hash.insert(entity, new_hash);
            self.pose_dirty.insert(entity);
        }
        dirty
    }

    /// Clear the dirty set (and the pending rollback pre-images); called
    /// at the start of each frame before `build_skinned_palettes`
    /// repopulates it. Leaves [`last_pose_hash`](Self::last_pose_hash)
    /// intact so the next frame's hash comparison still has a baseline.
    /// #1195.
    pub fn clear_pose_dirty(&mut self) {
        self.pose_dirty.clear();
        self.rollback_pose_hash.clear();
    }

    /// Undo every `last_pose_hash` commit made by `try_mark_pose_dirty`
    /// since the last `clear_pose_dirty` — i.e. this frame's commits.
    ///
    /// Call this when the renderer reports it did not reach the skin
    /// dispatch section this frame (`VulkanContext::skin_dispatch_ran ==
    /// false`): the CPU-side hash pass already ran and advanced the
    /// baseline before `draw_frame` was even called, so without this
    /// rollback an unchanged pose on the *next* frame would read "not
    /// dirty" against a baseline the GPU never actually dispatched
    /// against. #1796 / D6-02.
    ///
    /// First-sight commits (pre-image `None`) roll back to "never
    /// hashed" by removing the entry outright, preserving the
    /// always-dirty first-sight invariant.
    pub fn rollback_pending_pose_commits(&mut self) {
        for (entity, old) in self.rollback_pose_hash.drain() {
            match old {
                Some(old) => {
                    self.last_pose_hash.insert(entity, old);
                }
                None => {
                    self.last_pose_hash.remove(&entity);
                }
            }
        }
    }

    /// Read-only view of the per-frame dirty entity set. The renderer
    /// uses this to gate skin compute dispatch (#1195) and skinned-
    /// BLAS refit (#1196) — entities NOT in this set whose slots
    /// already have populated output + live BLAS can skip both passes.
    pub fn pose_dirty(&self) -> &std::collections::HashSet<EntityId> {
        &self.pose_dirty
    }
}

impl Resource for SkinSlotPool {}

#[cfg(test)]
mod skin_slot_pool_tests {
    use super::SkinSlotPool;

    #[test]
    fn allocates_monotonically_then_recycles() {
        let mut pool = SkinSlotPool::new(10);
        let s1 = pool.allocate(1, 100).unwrap();
        let s2 = pool.allocate(2, 100).unwrap();
        let s3 = pool.allocate(3, 100).unwrap();
        assert_eq!((s1, s2, s3), (1, 2, 3), "monotonic from slot 1");

        // Free entity 2 by sweeping it past idle threshold.
        // Need to also refresh 1 and 3 so they survive.
        pool.mark_seen(1, 110);
        pool.mark_seen(3, 110);
        let freed = pool.sweep(110, /*min_idle=*/ 5);
        assert_eq!(freed, vec![s2], "only entity 2 was idle past threshold");

        // Next allocation should reuse slot 2.
        let s4 = pool.allocate(4, 111).unwrap();
        assert_eq!(s4, s2, "free-list LIFO reuse");
    }

    #[test]
    fn returns_none_at_max_skinned() {
        let mut pool = SkinSlotPool::new(3);
        assert_eq!(pool.allocate(1, 0), Some(1));
        assert_eq!(pool.allocate(2, 0), Some(2));
        assert_eq!(pool.allocate(3, 0), Some(3));
        assert_eq!(
            pool.allocate(4, 0),
            None,
            "fourth allocation past capacity 3 must return None"
        );
        // Subsequent overflow calls still return None and don't panic.
        assert_eq!(pool.allocate(5, 0), None);
    }

    /// #1296 / D12-C1 — `overflow_attempt_count` is a per-*call* count,
    /// NOT a distinct-entity count. An over-cap entity is never made
    /// resident, so it re-hits the spill branch every frame and
    /// re-increments. This pins that contract so a future refactor can't
    /// silently turn it into per-entity semantics (which the old field
    /// doc + #1284 cap-sizing comment wrongly assumed).
    #[test]
    fn overflow_attempt_count_is_per_call_not_per_entity() {
        let mut pool = SkinSlotPool::new(1); // only slot 1 allocatable
        assert_eq!(pool.allocate(1, 0), Some(1));
        assert_eq!(pool.overflow_attempt_count(), 0);

        // Same over-cap entity across three frames: it is never resident,
        // so each call re-counts → 3, not 1 (a distinct-entity count
        // would stay at 1).
        assert_eq!(pool.allocate(2, 0), None);
        assert_eq!(pool.allocate(2, 1), None);
        assert_eq!(pool.allocate(2, 2), None);
        assert_eq!(
            pool.overflow_attempt_count(),
            3,
            "one persistently over-cap entity must re-count each call — \
             the counter is per-call, not per distinct entity"
        );

        // A different over-cap entity adds one more call.
        assert_eq!(pool.allocate(3, 2), None);
        assert_eq!(pool.overflow_attempt_count(), 4);
    }

    #[test]
    fn sweep_reclaims_unseen() {
        let mut pool = SkinSlotPool::new(10);
        let slot = pool.allocate(42, 100).unwrap();
        // Frame 100 → 104, idle = 4 < min_idle 5 → keep.
        assert_eq!(pool.sweep(104, 5), Vec::<u32>::new());
        assert_eq!(pool.get(42), Some(slot));
        // Frame 105, idle = 5 ≥ min_idle 5 → evict.
        let freed = pool.sweep(105, 5);
        assert_eq!(freed, vec![slot]);
        assert_eq!(pool.get(42), None);
    }

    /// Regression for #1379 — sweep must contract next_slot when the freed
    /// slots are at the tail of the issued range, so the bone-world copy and
    /// skin-palette dispatch don't cover dead slots after a high-NPC scene
    /// unloads to a low-NPC one.
    #[test]
    fn sweep_contracts_next_slot_when_tail_is_freed() {
        // Allocate 5 entities — slots 1..=5, next_slot becomes 6.
        let mut pool = SkinSlotPool::new(20);
        for e in 1u32..=5 {
            let _ = pool.allocate(e, 100).unwrap();
        }
        assert_eq!(pool.max_used_slot(), 5);

        // Evict the top-3 entities (slots 3, 4, 5 in issue order; sort is
        // unstable so actual slot numbers may differ — what matters is that
        // max_used_slot shrinks).
        // Evict entity 5 only → slot 5 freed → next_slot contracts 6→5.
        pool.allocate(1, 101).unwrap();
        pool.allocate(2, 101).unwrap();
        pool.allocate(3, 101).unwrap();
        pool.allocate(4, 101).unwrap();
        // entity 5 NOT refreshed → idle ≥ 1 at frame 101.
        let freed = pool.sweep(101, 1);
        assert!(!freed.is_empty(), "entity 5 must be freed");
        // next_slot must have contracted: max_used_slot < 5.
        assert!(
            pool.max_used_slot() < 5,
            "max_used_slot must contract when the tail slot is freed; got {}",
            pool.max_used_slot()
        );

        // Verify the contracted pool still allocates correctly.
        let new_slot = pool.allocate(99, 102).unwrap();
        assert!(new_slot >= 1, "new allocation must get a valid slot");
    }

    /// Regression: sweep must NOT contract next_slot when the freed slots are
    /// internal fragments (not the tail). Only the contiguous tail can be
    /// reclaimed without relocating live slots.
    #[test]
    fn sweep_does_not_contract_when_tail_is_live() {
        let mut pool = SkinSlotPool::new(20);
        for e in 1u32..=5 {
            let _ = pool.allocate(e, 100).unwrap();
        }
        let high_water = pool.max_used_slot();
        assert_eq!(high_water, 5);

        // Evict the lowest entity (slot 1) — NOT the tail.
        for e in 2u32..=5 {
            pool.allocate(e, 101).unwrap(); // refresh slots 2–5
        }
        // entity 1 NOT refreshed → evicted at frame 101.
        let freed = pool.sweep(101, 1);
        assert!(!freed.is_empty(), "entity 1 must be freed");
        // max_used_slot must NOT contract (slot 5 is still live).
        assert_eq!(
            pool.max_used_slot(),
            high_water,
            "max_used_slot must not change when the tail slot is still live"
        );
    }

    #[test]
    fn first_allocation_queues_pending_upload() {
        let mut pool = SkinSlotPool::new(10);
        let _ = pool.allocate(7, 100).unwrap();
        let pending = pool.drain_pending(usize::MAX);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].1, 7, "entity ID round-tripped");

        // Steady-state allocate for the same entity must NOT push to
        // pending_uploads (would re-upload the bind_inverses every
        // frame, defeating M29.6's purpose).
        let _ = pool.allocate(7, 101).unwrap();
        assert!(
            pool.drain_pending(usize::MAX).is_empty(),
            "steady-state allocate must not re-queue pending upload"
        );
    }

    /// M29.6 hotfix #1192 / SAFE-D7-NEW-02 — `drain_pending(max)` must
    /// return at most `max` entries and KEEP the remainder in
    /// `pending_uploads` for the next drain. Pre-hotfix `drain_pending`
    /// took everything; the renderer's staging buffer cap silently
    /// dropped the tail, leaving `entity_to_slot` populated but the
    /// persistent SSBO untouched at those slots.
    #[test]
    fn drain_pending_respects_max_cap() {
        let mut pool = SkinSlotPool::new(50);
        for i in 0..20u32 {
            let _ = pool.allocate(i, 100).unwrap();
        }
        let first = pool.drain_pending(16);
        assert_eq!(first.len(), 16, "first drain takes at most max=16");
        let second = pool.drain_pending(16);
        assert_eq!(
            second.len(),
            4,
            "second drain takes the remaining tail (20 - 16 = 4)"
        );
        assert!(
            pool.drain_pending(16).is_empty(),
            "third drain finds the queue empty"
        );
    }

    /// M29.6 hotfix #1192 — entities whose pending upload was capped
    /// must NOT be lost: their slot still maps via `entity_to_slot`,
    /// and the queue tail surfaces on the next drain. The pool's
    /// invariant is "every allocated entity surfaces in pending
    /// exactly once over its lifetime"; the renderer's cap mustn't
    /// turn that into "≤ MAX_PENDING entities over the lifetime".
    #[test]
    fn drain_pending_does_not_lose_capped_entities() {
        let mut pool = SkinSlotPool::new(50);
        let mut all_seen_slots = std::collections::HashSet::new();
        for i in 0..20u32 {
            let slot = pool.allocate(i, 100).unwrap();
            all_seen_slots.insert((slot, i));
        }
        // Drain with cap = 16; the tail (4 entries) stays in pool.
        let first_round: std::collections::HashSet<_> =
            pool.drain_pending(16).into_iter().collect();
        let second_round: std::collections::HashSet<_> =
            pool.drain_pending(usize::MAX).into_iter().collect();
        let combined: std::collections::HashSet<_> =
            first_round.union(&second_round).copied().collect();
        assert_eq!(
            combined, all_seen_slots,
            "every allocated entity must surface in some drain (#1192)"
        );
    }

    // ── #1791 / D6-01 — requeue on an unwritten drain ─────────────

    #[test]
    fn requeue_pending_restores_entries_for_the_next_drain() {
        // Simulates a `draw_frame` early return: the caller drained
        // entries but never actually wrote them to the SSBO, so it
        // requeues. The next drain must see them again — otherwise the
        // slot stays resident in `entity_to_slot` (per `allocate`'s
        // early-return-with-existing-slot path) while the persistent
        // SSBO region is never written, permanently corrupting that
        // entity's skinning palette.
        let mut pool = SkinSlotPool::new(5);
        let _ = pool.allocate(1, 0);
        let _ = pool.allocate(2, 0);
        let drained = pool.drain_pending(usize::MAX);
        assert_eq!(drained.len(), 2, "both first-sight entities are pending");

        assert!(
            pool.drain_pending(usize::MAX).is_empty(),
            "already drained — nothing left before requeue"
        );

        pool.requeue_pending(drained.clone());
        let redrained: std::collections::HashSet<_> =
            pool.drain_pending(usize::MAX).into_iter().collect();
        assert_eq!(
            redrained,
            drained.into_iter().collect(),
            "requeued entries must reappear on the next drain, unchanged"
        );
    }

    #[test]
    fn requeue_pending_is_a_no_op_for_an_empty_list() {
        // A caller that reaches the requeue call with nothing to
        // requeue (e.g. every drained entry was filtered out because
        // its SkinnedMesh was already gone) must not disturb whatever
        // is already queued.
        let mut pool = SkinSlotPool::new(5);
        let _ = pool.allocate(1, 0);
        pool.requeue_pending(Vec::new());
        assert_eq!(
            pool.drain_pending(usize::MAX).len(),
            1,
            "requeueing an empty list must not touch the existing queue"
        );
    }

    #[test]
    fn requeue_pending_entries_drain_before_newly_queued_ones() {
        // Requeued entries were already overdue once; they should be
        // the first candidates drained next frame, ahead of anything
        // that queued up normally in the meantime.
        let mut pool = SkinSlotPool::new(5);
        let _ = pool.allocate(1, 0); // queued first, then "lost" to a bad frame
        let lost = pool.drain_pending(usize::MAX);
        pool.requeue_pending(lost.clone());
        let _ = pool.allocate(2, 0); // queues normally afterward

        let first = pool.drain_pending(1);
        assert_eq!(
            first, lost,
            "the requeued (overdue) entry must drain before the freshly queued one"
        );
    }

    #[test]
    fn never_returns_slot_zero() {
        let mut pool = SkinSlotPool::new(5);
        for entity_id in 1..=5u32 {
            let slot = pool.allocate(entity_id, 0).unwrap();
            assert_ne!(
                slot, 0,
                "slot 0 is reserved for global identity; pool must not allocate it"
            );
        }
    }

    #[test]
    fn underflow_safe_when_last_used_in_future() {
        // Defensive: if a caller bumps last_seen_frame to a value
        // larger than current_frame (frame counter wrap / reset),
        // sweep must not flip eviction true via wrap-around.
        let mut pool = SkinSlotPool::new(5);
        let _slot = pool.allocate(99, 200).unwrap();
        // current=100, last=200, saturating_sub → idle=0 → keep.
        assert_eq!(pool.sweep(100, 5), Vec::<u32>::new());
        assert!(pool.get(99).is_some());
    }

    // ── #1195 / PERF-DIM7-01 — pose-dirty gate ────────────────────

    #[test]
    fn first_sight_pose_is_always_dirty() {
        let mut pool = SkinSlotPool::new(5);
        // Entity 1 has never been hashed → first call must return
        // dirty so the renderer dispatches at least once. The
        // "skip dispatch when unchanged" optimisation must never
        // bypass first-sight; otherwise the output buffer is never
        // populated and the BLAS holds garbage.
        let dirty = pool.try_mark_pose_dirty(1, 0x1234);
        assert!(dirty, "first hash for an entity must report dirty");
        assert!(
            pool.pose_dirty().contains(&1),
            "first-sight entity must land in the dirty set"
        );
    }

    #[test]
    fn unchanged_pose_is_not_dirty_on_second_call() {
        let mut pool = SkinSlotPool::new(5);
        let _ = pool.try_mark_pose_dirty(1, 0xCAFE);
        pool.clear_pose_dirty();

        // Same hash → not dirty; idle skinned entity skips dispatch.
        let dirty = pool.try_mark_pose_dirty(1, 0xCAFE);
        assert!(!dirty, "same hash must not report dirty");
        assert!(
            !pool.pose_dirty().contains(&1),
            "stable-pose entity must not re-enter the dirty set"
        );
    }

    #[test]
    fn changed_pose_re_dirties() {
        let mut pool = SkinSlotPool::new(5);
        let _ = pool.try_mark_pose_dirty(1, 0xCAFE);
        pool.clear_pose_dirty();
        let _ = pool.try_mark_pose_dirty(1, 0xCAFE);
        pool.clear_pose_dirty();

        // Hash changes (bone moved) → dirty again.
        let dirty = pool.try_mark_pose_dirty(1, 0xDEAD);
        assert!(dirty, "hash mismatch must report dirty");
        assert!(pool.pose_dirty().contains(&1));
    }

    #[test]
    fn clear_pose_dirty_preserves_baseline_hash() {
        // `clear_pose_dirty` runs at the top of every frame to empty
        // the per-frame dirty set; it must NOT wipe `last_pose_hash`,
        // because the next-frame comparison needs that baseline. If
        // the baseline were wiped, every frame would re-mark every
        // entity dirty — defeating the optimisation entirely.
        let mut pool = SkinSlotPool::new(5);
        let _ = pool.try_mark_pose_dirty(1, 0xCAFE);
        pool.clear_pose_dirty();
        // Same hash now → must still report not-dirty (baseline survived).
        assert!(!pool.try_mark_pose_dirty(1, 0xCAFE));
    }

    #[test]
    fn sweep_drops_stale_pose_hash_with_slot() {
        // When an entity's slot is reclaimed by `sweep`, its pose hash
        // should be evicted too — otherwise the map grows without
        // bound across long-running sessions (NPC churn from cell
        // streaming, particle skinned-mesh churn).
        let mut pool = SkinSlotPool::new(5);
        let _ = pool.allocate(7, 100);
        let _ = pool.try_mark_pose_dirty(7, 0xCAFE);
        // Sweep with idle threshold low enough to evict.
        let freed = pool.sweep(110, /*min_idle=*/ 5);
        assert_eq!(freed.len(), 1, "entity 7 idle past threshold → evicted");
        // After eviction, re-allocating entity 7 must hit the
        // first-sight dirty branch again — proving the stale hash
        // was dropped.
        pool.clear_pose_dirty();
        assert!(
            pool.try_mark_pose_dirty(7, 0xCAFE),
            "re-allocated entity must hit first-sight dirty after sweep"
        );
    }

    // ── #1796 / D6-02 — rollback on an aborted skin dispatch ──────

    #[test]
    fn rollback_restores_prior_hash_so_next_frame_stays_dirty() {
        // Steady state: entity 1 has a committed baseline (H1). Frame N
        // computes H2 (pose moved) → committed dirty. `draw_frame` then
        // bails (simulated: caller calls rollback instead of letting the
        // commit stand). Frame N+1's pose is unchanged from N (still H2)
        // — the gate must still read dirty, because H2 was never actually
        // dispatched against.
        let mut pool = SkinSlotPool::new(5);
        let _ = pool.try_mark_pose_dirty(1, 0xAAAA); // H1, first-sight
        pool.clear_pose_dirty();

        let dirty = pool.try_mark_pose_dirty(1, 0xBBBB); // H2
        assert!(dirty, "pose change must report dirty");
        pool.rollback_pending_pose_commits(); // simulate an early-return draw_frame

        pool.clear_pose_dirty();
        let dirty_again = pool.try_mark_pose_dirty(1, 0xBBBB); // still H2, unchanged since N
        assert!(
            dirty_again,
            "rolled-back commit must leave H1 as the baseline, so an \
             unchanged-since-N pose (H2) still compares dirty against it"
        );
    }

    #[test]
    fn rollback_of_first_sight_restores_always_dirty_invariant() {
        // First-sight entity whose only commit gets rolled back must go
        // back to "never hashed" — not accidentally treated as if H1 IS
        // the baseline (which would wrongly report "not dirty" next
        // frame for the same first pose, before it was ever dispatched).
        let mut pool = SkinSlotPool::new(5);
        let dirty = pool.try_mark_pose_dirty(1, 0xCAFE);
        assert!(dirty);
        pool.rollback_pending_pose_commits();

        pool.clear_pose_dirty();
        let dirty_again = pool.try_mark_pose_dirty(1, 0xCAFE);
        assert!(
            dirty_again,
            "rolled-back first-sight commit must still read dirty on retry"
        );
    }

    #[test]
    fn rollback_is_a_no_op_when_nothing_pending() {
        // A frame that reaches the dispatch section normally must not
        // have its committed baseline disturbed if `rollback_pending_
        // pose_commits` is (incorrectly) called after `clear_pose_dirty`
        // already ran — i.e. rollback only ever undoes the *current*
        // frame's commits, never a prior committed frame's.
        let mut pool = SkinSlotPool::new(5);
        let _ = pool.try_mark_pose_dirty(1, 0xCAFE);
        pool.clear_pose_dirty(); // this frame's dispatch succeeded; commit stands

        pool.rollback_pending_pose_commits(); // nothing pending — must be inert
        pool.clear_pose_dirty();
        assert!(
            !pool.try_mark_pose_dirty(1, 0xCAFE),
            "committed baseline from a successful frame must survive an \
             unrelated later rollback call"
        );
    }
}

