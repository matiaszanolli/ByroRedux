//! Lock tracker for detecting deadlocks.
//!
//! `std::sync::RwLock` is not reentrant: acquiring a write lock while holding
//! a read lock on the same thread deadlocks silently. This module catches
//! that at the point of acquisition with a clear panic message.
//!
//! Two checks run in tandem:
//!
//! 1. **Thread-local check** (always on — debug and release builds). Catches
//!    same-thread reentrant deadlocks: a thread holding a read lock on `T`
//!    and then trying to `T.write()` on the same thread panics at tracking
//!    time instead of deadlocking silently.
//!
//! 2. **Global lock-order graph** (debug builds only — see #313). Records
//!    observed "acquired-while-held" edges per type across all threads. If
//!    thread `T1` observed `A → B` (acquired B while holding A) and thread
//!    `T2` observed `B → A` (acquired A while holding B), the graph has a
//!    cycle and the second observation panics. This catches ABBA risks the
//!    thread-local tracker cannot see — e.g. two systems on separate rayon
//!    workers acquiring the same pair of single-type queries in opposite
//!    orders. The `query_2_mut` API already prevents this for 2-component
//!    paired access via TypeId-sorted acquisition; the graph generalizes
//!    the guarantee to any N-lock hold pattern across the scheduler.
//!
//! The per-acquisition cost is a thread-local HashMap lookup plus (debug
//! only) a fast-path `RwLock::read()` + `HashSet::contains()` — negligible
//! compared to the real RwLock the check is guarding. The graph's write-
//! lock path fires only on novel edge observations; once the graph has
//! stabilized every acquisition takes the read-only fast path.

use std::any::TypeId;
use std::cell::RefCell;
use std::collections::HashMap;

/// Per-lock-type tracker record. `type_name` is stored so panic
/// messages can identify the conflict without the caller having to
/// redo the name lookup.
#[derive(Debug, Clone, Copy)]
struct LockState {
    read_count: u32,
    has_write: bool,
    /// Type name captured at the first track() call. Stable across
    /// reentrant acquires because `std::any::type_name::<T>()` returns
    /// a `&'static str` for every distinct T.
    type_name: &'static str,
}

thread_local! {
    static LOCKS: RefCell<HashMap<TypeId, LockState>> = RefCell::new(HashMap::new());
}

/// Record a read lock acquisition. Panics if a write lock is already held
/// on this type from the same thread (would deadlock).
pub(crate) fn track_read(type_id: TypeId, type_name: &'static str) {
    LOCKS.with(|locks| {
        let mut map = locks.borrow_mut();
        let is_new = !map.contains_key(&type_id);
        let entry = map.entry(type_id).or_insert(LockState {
            read_count: 0,
            has_write: false,
            type_name,
        });
        if entry.has_write {
            panic!(
                "ECS deadlock detected: attempted read lock on `{}` while a write lock \
                 is already held on the same thread. Drop the write query/resource first.",
                type_name,
            );
        }
        entry.read_count += 1;
        if is_new {
            // Only check global order on transition 0→held — re-entrant
            // read acquires on the same type don't add any new edges.
            let held_others: Vec<(TypeId, &'static str)> = map
                .iter()
                .filter(|(id, _)| **id != type_id)
                .map(|(id, state)| (*id, state.type_name))
                .collect();
            drop(map);
            #[cfg(debug_assertions)]
            global_order::record_and_check(type_id, type_name, &held_others);
            #[cfg(not(debug_assertions))]
            let _ = held_others;
        }
    });
}

/// Record a write lock acquisition. Panics if any lock (read or write) is
/// already held on this type from the same thread (would deadlock).
pub(crate) fn track_write(type_id: TypeId, type_name: &'static str) {
    LOCKS.with(|locks| {
        let mut map = locks.borrow_mut();
        let is_new = !map.contains_key(&type_id);
        let entry = map.entry(type_id).or_insert(LockState {
            read_count: 0,
            has_write: false,
            type_name,
        });
        if entry.has_write {
            panic!(
                "ECS deadlock detected: attempted write lock on `{}` while a write lock \
                 is already held on the same thread. Drop the existing query/resource first.",
                type_name,
            );
        }
        if entry.read_count > 0 {
            panic!(
                "ECS deadlock detected: attempted write lock on `{}` while {} read lock(s) \
                 are held on the same thread. Drop all read queries/resources first.",
                type_name, entry.read_count,
            );
        }
        entry.has_write = true;
        if is_new {
            let held_others: Vec<(TypeId, &'static str)> = map
                .iter()
                .filter(|(id, _)| **id != type_id)
                .map(|(id, state)| (*id, state.type_name))
                .collect();
            drop(map);
            #[cfg(debug_assertions)]
            global_order::record_and_check(type_id, type_name, &held_others);
            #[cfg(not(debug_assertions))]
            let _ = held_others;
        }
    });
}

/// Remove a read lock from tracking.
pub(crate) fn untrack_read(type_id: TypeId) {
    LOCKS.with(|locks| {
        let mut map = locks.borrow_mut();
        if let Some(entry) = map.get_mut(&type_id) {
            entry.read_count = entry.read_count.saturating_sub(1);
            if entry.read_count == 0 && !entry.has_write {
                map.remove(&type_id);
            }
        }
    });
}

/// Remove a write lock from tracking.
pub(crate) fn untrack_write(type_id: TypeId) {
    LOCKS.with(|locks| {
        let mut map = locks.borrow_mut();
        if let Some(entry) = map.get_mut(&type_id) {
            entry.has_write = false;
            if entry.read_count == 0 {
                map.remove(&type_id);
            }
        }
    });
}

/// Global lock-order graph — opt-in via `BYRO_LOCK_ORDER_CHECK=1` (#313).
///
/// Records every observed "acquired B while holding A" edge across all
/// threads in the process. If a later acquisition would add a
/// cycle-closing edge (e.g. "A → B" was observed and we now try to
/// acquire A while holding B), we panic at the observation of the
/// cycle rather than deadlocking silently.
///
/// The graph lives process-wide behind a `RwLock`: fast-path acquires
/// (no new edges) take the read side, novel edges upgrade to the write
/// side once. After the call graph has stabilized the steady-state
/// cost is one read-lock + two HashSet lookups per novel-pair acquire.
///
/// **Opt-in design:** the detector is conservative — it flags any pair
/// of acquisition orderings that *could* deadlock if the two threads'
/// hold periods overlap, even when in practice the holds don't overlap
/// (e.g. sequential temporary borrows in two different unit tests).
/// This makes it useful as a stress-testing tool but unnecessarily
/// strict for the everyday test run, where parallel test execution
/// would trip the detector on every legitimate per-test pattern.
///
/// To enable: set `BYRO_LOCK_ORDER_CHECK=1` in the environment before
/// running. Recommended for CI's deadlock-stress job and for local
/// debugging of suspected ABBA risks. Off by default so the existing
/// test suite stays green.
///
/// In release builds the module is compiled out — the hot-path check
/// becomes a no-op and the thread-local same-thread tracker remains
/// the only guard.
#[cfg(debug_assertions)]
mod global_order {
    use std::any::TypeId;
    use std::collections::{HashMap, HashSet};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{LazyLock, RwLock};

    /// For each type T, the set of types observed to have been held
    /// while T was acquired. An edge `H → T` (i.e. `T ∈ GRAPH[H]`)
    /// means some thread acquired T while H was held.
    ///
    /// Cycle detection is the dual read: when acquiring T while
    /// holding H, we panic if `H ∈ GRAPH[T]` — that would mean some
    /// other thread (or an earlier acquisition on this thread)
    /// already did "T while holding H" AND we're now doing "H while
    /// holding T", which is the ABBA pattern that deadlocks.
    static GRAPH: LazyLock<RwLock<HashMap<TypeId, HashSet<TypeId>>>> =
        LazyLock::new(|| RwLock::new(HashMap::new()));

    /// Whether the env var was set at process start. Cached in an
    /// atomic so the per-acquire fast-path is one relaxed load.
    /// Tests can flip this directly via [`set_enabled_for_tests`].
    static ENABLED: LazyLock<AtomicBool> = LazyLock::new(|| {
        AtomicBool::new(std::env::var_os("BYRO_LOCK_ORDER_CHECK").is_some())
    });

    /// Record each `held → new` edge in the graph and panic if the
    /// insert would close a cycle (`new → held` already observed).
    /// `held_others` carries the set of distinct types currently
    /// locked on this thread, excluding the incoming `new_id`
    /// itself (re-entrant read acquires on the same type are handled
    /// by the thread-local tracker's count and don't add edges).
    ///
    /// Returns immediately when the detector is disabled (the default
    /// — see module doc) or when no other locks are currently held on
    /// this thread.
    pub(super) fn record_and_check(
        new_id: TypeId,
        new_name: &'static str,
        held_others: &[(TypeId, &'static str)],
    ) {
        if held_others.is_empty() {
            return;
        }
        if !ENABLED.load(Ordering::Relaxed) {
            return;
        }
        // Fast-path read lock: check every edge is already known AND
        // the cycle condition doesn't fire. If both hold, skip the
        // write-lock upgrade entirely.
        {
            let graph = GRAPH.read().expect("GRAPH poisoned");
            // Cycle check: did some other thread observe the reverse
            // direction? (`new → held_i` for any i). If yes → ABBA.
            if let Some(new_edges) = graph.get(&new_id) {
                for (held_id, held_name) in held_others {
                    if new_edges.contains(held_id) {
                        panic!(
                            "ECS cross-thread deadlock risk (ABBA): attempted acquisition \
                             of `{}` while holding `{}` on this thread — the reverse edge \
                             `{}` → `{}` was previously observed on another thread. \
                             Two threads acquiring the same pair of locks in opposite \
                             orders will deadlock. Use `query_2_mut`/`query_2_mut_mut` \
                             for paired access (TypeId-sorted), or acquire locks in a \
                             consistent process-wide order. See #313.",
                            new_name, held_name, new_name, held_name,
                        );
                    }
                }
            }
            // Are all the edges we'd add already present? If yes, no
            // write needed.
            let mut all_present = true;
            for (held_id, _) in held_others {
                match graph.get(held_id) {
                    Some(edges) if edges.contains(&new_id) => {}
                    _ => {
                        all_present = false;
                        break;
                    }
                }
            }
            if all_present {
                return;
            }
        }
        // Slow path: we have at least one novel edge → take the write
        // lock and insert. Re-check the cycle condition under write
        // because another thread may have raced us (best-effort but
        // correct: the check is transitively sound on any consistent
        // snapshot because cycles, once observed, stay forever).
        let mut graph = GRAPH.write().expect("GRAPH poisoned");
        if let Some(new_edges) = graph.get(&new_id) {
            for (held_id, held_name) in held_others {
                if new_edges.contains(held_id) {
                    panic!(
                        "ECS cross-thread deadlock risk (ABBA): attempted acquisition \
                         of `{}` while holding `{}` on this thread — the reverse edge \
                         was observed on another thread. See #313.",
                        new_name, held_name,
                    );
                }
            }
        }
        for (held_id, _) in held_others {
            graph.entry(*held_id).or_default().insert(new_id);
        }
    }

    /// Test-only — flip the runtime opt-in flag so the unit tests
    /// in this module can exercise the detector without forcing the
    /// rest of the workspace to opt in via env var. Preserves the
    /// "default off" production posture.
    #[cfg(test)]
    pub(super) fn set_enabled_for_tests(on: bool) {
        ENABLED.store(on, Ordering::SeqCst);
    }

    /// Test-only — clear the graph between tests so a previous
    /// observation doesn't leak into an unrelated test case.
    #[cfg(test)]
    pub(super) fn reset() {
        GRAPH.write().unwrap().clear();
    }
}

/// RAII scope guard that tracks a read-lock intent on construction and
/// auto-untracks on drop unless [`TrackedRead::defuse`] is called first.
///
/// Use this instead of raw [`track_read`] when there's code between the
/// intent-to-lock and the actual guard construction that could panic
/// (e.g. a poisoned-lock `unwrap_or_else` panic helper). If the panic
/// fires, this guard's `Drop` releases the tracker row, preventing a
/// false "deadlock detected" report on a subsequent catch_unwind
/// recovery.
///
/// Once the real lock guard is successfully constructed, call
/// `defuse()` to transfer ownership of the tracker row — the `Drop`
/// impl of `QueryRead` / `ResourceRead` on the real guard will take
/// over. See issue #137.
pub(crate) struct TrackedRead {
    type_id: TypeId,
    armed: bool,
}

impl TrackedRead {
    #[inline]
    pub(crate) fn new(type_id: TypeId, type_name: &'static str) -> Self {
        track_read(type_id, type_name);
        Self {
            type_id,
            armed: true,
        }
    }

    /// Hand ownership of the tracker row off to the real lock guard.
    /// Call this once the lock has been successfully acquired.
    #[inline]
    pub(crate) fn defuse(mut self) {
        self.armed = false;
    }
}

impl Drop for TrackedRead {
    fn drop(&mut self) {
        if self.armed {
            untrack_read(self.type_id);
        }
    }
}

/// RAII scope guard for write-lock intents. Mirror of [`TrackedRead`].
pub(crate) struct TrackedWrite {
    type_id: TypeId,
    armed: bool,
}

impl TrackedWrite {
    #[inline]
    pub(crate) fn new(type_id: TypeId, type_name: &'static str) -> Self {
        track_write(type_id, type_name);
        Self {
            type_id,
            armed: true,
        }
    }

    #[inline]
    pub(crate) fn defuse(mut self) {
        self.armed = false;
    }
}

impl Drop for TrackedWrite {
    fn drop(&mut self) {
        if self.armed {
            untrack_write(self.type_id);
        }
    }
}

/// Test-only helper: returns `true` if the thread-local tracker map
/// has no live entries. Used by the #137 regression test to verify
/// that a panicked lock acquisition leaves no stale rows behind.
#[cfg(test)]
pub(crate) fn is_clean() -> bool {
    LOCKS.with(|locks| locks.borrow().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;

    struct FakeA;
    struct FakeB;

    #[test]
    fn multiple_reads_same_type_ok() {
        let id = TypeId::of::<FakeA>();
        track_read(id, "FakeA");
        track_read(id, "FakeA");
        track_read(id, "FakeA");
        untrack_read(id);
        untrack_read(id);
        untrack_read(id);
    }

    #[test]
    fn read_then_write_different_types_ok() {
        let id_a = TypeId::of::<FakeA>();
        let id_b = TypeId::of::<FakeB>();
        track_read(id_a, "FakeA");
        track_write(id_b, "FakeB");
        untrack_write(id_b);
        untrack_read(id_a);
    }

    #[test]
    #[should_panic(expected = "ECS deadlock detected")]
    fn read_then_write_same_type_panics() {
        let id = TypeId::of::<FakeA>();
        track_read(id, "FakeA");
        track_write(id, "FakeA"); // should panic
    }

    #[test]
    #[should_panic(expected = "ECS deadlock detected")]
    fn write_then_read_same_type_panics() {
        let id = TypeId::of::<FakeA>();
        track_write(id, "FakeA");
        track_read(id, "FakeA"); // should panic
    }

    #[test]
    #[should_panic(expected = "ECS deadlock detected")]
    fn write_then_write_same_type_panics() {
        let id = TypeId::of::<FakeA>();
        track_write(id, "FakeA");
        track_write(id, "FakeA"); // should panic
    }

    #[test]
    fn sequential_locks_after_drop_ok() {
        let id = TypeId::of::<FakeA>();
        track_write(id, "FakeA");
        untrack_write(id);
        // Should be fine now.
        track_read(id, "FakeA");
        untrack_read(id);
        track_write(id, "FakeA");
        untrack_write(id);
    }

    // ── Global lock-order graph tests (#313) ───────────────────────
    //
    // These tests exercise the cross-thread ABBA detector. Each one
    // resets the global graph on entry so observations don't leak
    // between tests, and uses dedicated type markers to avoid
    // colliding with other tests in the workspace that also exercise
    // the same graph.

    struct Abba1;
    struct Abba2;
    struct Abba3;
    struct Abba4;

    /// Single combined test for the global-graph detector — three
    /// scenarios run sequentially within one test body so the runtime
    /// opt-in flag (`global_order::set_enabled_for_tests`) doesn't
    /// race with the parallel test runner. Asserts:
    ///
    /// - **ABBA detected**: `A → B` then `B → A` panics on the second
    ///   pattern via [`std::panic::catch_unwind`].
    /// - **Consistent order is fine**: same `A → B` repeated does not
    ///   panic (steady-state fast path).
    /// - **Re-entrant reads don't self-edge**: holding two read locks
    ///   on the same type doesn't record `T → T`.
    ///
    /// Each scenario runs after `global_order::reset()` clears any
    /// edges left over from earlier scenarios. The flag stays enabled
    /// for the whole test and is restored to `false` on exit so other
    /// tests aren't contaminated.
    #[test]
    fn global_graph_detector_end_to_end() {
        // Skip the entire test body in release builds — the
        // global_order module is `cfg(debug_assertions)`-gated.
        #[cfg(debug_assertions)]
        {
            // Save current enable state and force-enable for this
            // test. Restored at end via the `Restore` guard so a
            // subsequent test can't inherit `true`.
            struct Restore;
            impl Drop for Restore {
                fn drop(&mut self) {
                    global_order::set_enabled_for_tests(false);
                    global_order::reset();
                }
            }
            let _restore = Restore;
            global_order::set_enabled_for_tests(true);

            // Scenario 1: ABBA detected.
            global_order::reset();
            let a = TypeId::of::<Abba1>();
            let b = TypeId::of::<Abba2>();
            track_read(a, "Abba1");
            track_read(b, "Abba2");
            untrack_read(b);
            untrack_read(a);
            // Now reverse pattern → should panic. catch_unwind isolates.
            let panicked = std::panic::catch_unwind(|| {
                track_read(b, "Abba2");
                track_read(a, "Abba1"); // closes the cycle
                                        // (unreachable on debug)
                untrack_read(a);
                untrack_read(b);
            })
            .is_err();
            assert!(panicked, "ABBA pattern must panic");
            // catch_unwind leaves the thread-local tracker in
            // whatever state the panic interrupted. Wipe it for the
            // next scenario; this is safe because we're isolated.
            LOCKS.with(|l| l.borrow_mut().clear());

            // Scenario 2: consistent order is fine.
            global_order::reset();
            let c = TypeId::of::<Abba3>();
            let d = TypeId::of::<Abba4>();
            track_read(c, "Abba3");
            track_read(d, "Abba4");
            untrack_read(d);
            untrack_read(c);
            // Repeat the same order — must not panic.
            track_read(c, "Abba3");
            track_read(d, "Abba4");
            untrack_read(d);
            untrack_read(c);

            // Scenario 3: re-entrant reads don't self-edge.
            global_order::reset();
            let e = TypeId::of::<FakeA>();
            track_read(e, "FakeA");
            track_read(e, "FakeA"); // second-entry on same type
            untrack_read(e);
            untrack_read(e);
            track_read(e, "FakeA"); // fresh acquire after release
            untrack_read(e);
        }
    }
}
