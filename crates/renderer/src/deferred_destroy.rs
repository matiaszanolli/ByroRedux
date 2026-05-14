//! Generic deferred-destruction queue for GPU resources whose freeing
//! must be delayed until any in-flight command buffer that references
//! them has retired (typically MAX_FRAMES_IN_FLIGHT frames).
//!
//! Two production users today, both on the countdown variant:
//!   * [`crate::mesh::MeshRegistry::deferred_destroy`] — pairs of
//!     vertex / index `GpuBuffer`s queued by `drop_mesh` (#372 /
//!     #879).
//!   * [`crate::vulkan::acceleration::AccelerationManager::pending_destroy_blas`]
//!     — `BlasEntry` queued by `drop_blas` (#372 / #495).
//!
//! Both predate this primitive and reimplemented the same
//! `Vec<(T, u32)>` + per-frame `retain_mut` decrement loop +
//! shutdown drain shape. Consolidating them here keeps the countdown
//! semantics + safety contract in one place.
//!
//! Texture deferred destruction
//! ([`crate::texture_registry::TextureEntry::pending_destroy`]) uses
//! a different pattern (per-`TextureEntry` `VecDeque<(frame_id, T)>`
//! with frame-id timestamps rather than countdowns) and is NOT
//! migrated to this primitive — its per-entry layout is load-bearing
//! for the `update_rgba` "stack multiple in-flight replacements per
//! slot" flow.

/// Default countdown value used by every production caller. Expressed
/// directly as `MAX_FRAMES_IN_FLIGHT as u32` so a future bump to
/// `MAX_FRAMES_IN_FLIGHT` (currently 2 per the `vulkan::sync` #870
/// const-assert) automatically widens the deferred-destroy window
/// instead of silently lagging behind. Items pushed at frame N are
/// safe to destroy at frame N+`DEFAULT_COUNTDOWN` because every
/// command buffer that could reference them has finished by then.
/// Mirrors the correct pattern at `draw.rs:889` and
/// `acceleration.rs::tick_pending_destroy_blas`.
pub(crate) const DEFAULT_COUNTDOWN: u32 = crate::vulkan::sync::MAX_FRAMES_IN_FLIGHT as u32;

/// Queue of items waiting `countdown` more frames before destruction.
/// `tick` decrements every entry's countdown each frame, calls the
/// caller-provided destroyer on entries whose countdown reached zero,
/// and removes them. `drain` runs the destroyer on every queued entry
/// regardless of countdown — used at shutdown after `device_wait_idle`
/// has settled all in-flight command buffers.
pub(crate) struct DeferredDestroyQueue<T> {
    queue: Vec<(T, u32)>,
}

impl<T> Default for DeferredDestroyQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> DeferredDestroyQueue<T> {
    pub(crate) fn new() -> Self {
        Self { queue: Vec::new() }
    }

    /// Push an item with a fresh countdown. Production callers pass
    /// [`DEFAULT_COUNTDOWN`] so the item survives at least
    /// `MAX_FRAMES_IN_FLIGHT` frames before destruction.
    pub(crate) fn push(&mut self, item: T, countdown: u32) {
        self.queue.push((item, countdown));
    }

    /// Decrement every entry's countdown. Entries whose countdown
    /// reaches zero are passed to `destroyer` (consuming the owned
    /// `T`) and removed; entries with countdown > 0 have their
    /// countdown decremented and stay. Per-frame entry point — pair
    /// with [`Self::drain`] for the shutdown path that bypasses
    /// countdown.
    ///
    /// Implementation note: uses `swap_remove` to extract expired
    /// entries by owned value. The iteration order across surviving
    /// entries shifts after a destroy (the last element moves into
    /// the destroyed slot), but every queued item is still visited
    /// exactly once per `tick` call. Order of destruction does not
    /// matter for the GPU-resource lifetime contract — countdowns
    /// stand in for fence waits, and any expired entry is
    /// independently safe to free.
    pub(crate) fn tick<F: FnMut(T)>(&mut self, mut destroyer: F) {
        let mut i = 0;
        while i < self.queue.len() {
            if self.queue[i].1 == 0 {
                // O(1) extract by owned value; the last element
                // moves into slot `i` and we re-test that slot on
                // the next iteration without advancing `i`.
                let (item, _) = self.queue.swap_remove(i);
                destroyer(item);
            } else {
                self.queue[i].1 -= 1;
                i += 1;
            }
        }
    }

    /// Drain every queued entry synchronously, regardless of
    /// countdown. Calls `destroyer` on each owned item. Used at
    /// shutdown after `device_wait_idle` has settled all in-flight
    /// command buffers — the countdown's only purpose is to stand
    /// in for that wait.
    pub(crate) fn drain<F: FnMut(T)>(&mut self, mut destroyer: F) {
        for (item, _countdown) in self.queue.drain(..) {
            destroyer(item);
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.queue.len()
    }

    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// `tick` decrements every entry once per call. Entries hitting
    /// zero are destroyed; others stay with their countdown reduced.
    #[test]
    fn tick_decrements_and_destroys_at_zero() {
        let mut q: DeferredDestroyQueue<u32> = DeferredDestroyQueue::new();
        q.push(10, 2);
        q.push(20, 1);
        q.push(30, 0); // already at zero — destroyed on first tick.
        let destroyed = RefCell::new(Vec::<u32>::new());
        q.tick(|item| destroyed.borrow_mut().push(item));
        // 30 was destroyed (countdown was already 0).
        assert_eq!(*destroyed.borrow(), vec![30]);
        assert_eq!(q.len(), 2);

        q.tick(|item| destroyed.borrow_mut().push(item));
        // 20 was destroyed (countdown was 1 → 0 this tick → destroyed).
        // Wait — re-reading: tick decrements first, THEN checks ==0?
        // No, the implementation checks `*countdown == 0` BEFORE
        // decrementing, so an entry pushed with countdown 1 survives
        // tick #1 (decremented to 0) and is destroyed on tick #2.
        // 20 was pushed with countdown 1; after tick #1 it's at 0;
        // tick #2 destroys it.
        assert!(destroyed.borrow().contains(&20));
        assert_eq!(q.len(), 1);

        q.tick(|item| destroyed.borrow_mut().push(item));
        // 10 was pushed with countdown 2; ticks: 2→1→0→destroy.
        assert!(destroyed.borrow().contains(&10));
        assert_eq!(q.len(), 0);
    }

    /// `drain` destroys everything synchronously regardless of
    /// remaining countdown — the shutdown contract.
    #[test]
    fn drain_destroys_all_regardless_of_countdown() {
        let mut q: DeferredDestroyQueue<u32> = DeferredDestroyQueue::new();
        q.push(1, 2);
        q.push(2, 5);
        q.push(3, 0);
        let destroyed = RefCell::new(Vec::<u32>::new());
        q.drain(|item| destroyed.borrow_mut().push(item));
        let mut got = destroyed.into_inner();
        got.sort();
        assert_eq!(got, vec![1, 2, 3]);
        assert!(q.is_empty());
    }

    /// Pin: `len` reflects every queued row regardless of countdown
    /// — telemetry / shutdown sweeps assert "zero pending after
    /// drain" against this. Mirrors the existing
    /// `MeshRegistry::deferred_destroy_count_pins_to_queue_length`
    /// invariant from #732.
    #[test]
    fn len_pins_to_queue_size_across_countdowns() {
        let mut q: DeferredDestroyQueue<u32> = DeferredDestroyQueue::new();
        assert_eq!(q.len(), 0);
        q.push(1, 2);
        q.push(2, 1);
        q.push(3, 0);
        assert_eq!(q.len(), 3);
        q.drain(|_| ());
        assert_eq!(q.len(), 0);
    }

    /// Default-pushed entries with `DEFAULT_COUNTDOWN` survive
    /// exactly `MAX_FRAMES_IN_FLIGHT` ticks before destruction.
    /// Pre-fix the mesh + BLAS countdown was a hardcoded literal at
    /// each call site; this constant centralises the contract.
    #[test]
    fn default_countdown_survives_max_frames_in_flight_ticks() {
        let mut q: DeferredDestroyQueue<u32> = DeferredDestroyQueue::new();
        q.push(42, DEFAULT_COUNTDOWN);
        for tick_n in 0..(DEFAULT_COUNTDOWN as usize) {
            let destroyed = RefCell::new(Vec::<u32>::new());
            q.tick(|item| destroyed.borrow_mut().push(item));
            assert!(
                destroyed.borrow().is_empty(),
                "tick {tick_n} must not destroy yet"
            );
            assert_eq!(q.len(), 1);
        }
        // The (DEFAULT_COUNTDOWN+1)th tick fires the destroyer.
        let destroyed = RefCell::new(Vec::<u32>::new());
        q.tick(|item| destroyed.borrow_mut().push(item));
        assert_eq!(*destroyed.borrow(), vec![42]);
        assert!(q.is_empty());
    }
}
