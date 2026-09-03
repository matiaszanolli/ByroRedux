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
//! 2. **Global lock-order graph** (debug builds and opt-in via
//!    `BYRO_LOCK_ORDER_CHECK=1` — see #313). Records
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
//!    Cycles of **any length** are reported, not just the direct `A → B` /
//!    `B → A` pair: the closure test is a reachability search over the
//!    recorded edges, so a three-system triangle (`A → B` on one thread,
//!    `B → C` on a second, `C → A` on a third — each edge individually
//!    legal) panics on the observation that closes it. See #2675.
//!
//! For scheduler-managed parallel systems, the primary cross-thread guarantee
//! is static: `Scheduler::access_report` classifies undeclared/unknown pairs
//! and every write/read or write/write overlap within a stage. A zeroed report
//! therefore proves that declared parallel pairs have no blocking lock edge
//! and cannot form ABBA. `install_runtime_registries` enforces that report in
//! every build. This runtime graph supplements the proof for incomplete
//! declarations and other multi-lock paths that an enabled run actually
//! reaches; it is deliberately not presented as whole-program coverage.
//!
//! The per-acquisition cost is a thread-local HashMap lookup plus (debug
//! only) a fast-path `RwLock::read()` + one `HashMap::contains_key()` per
//! held lock — negligible compared to the real RwLock the check is
//! guarding. The graph's write-lock path (and with it the reachability
//! search) fires only on novel edge observations; once the graph has
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
    /// a `&'static str` for every distinct T. Only read by the
    /// debug-only `record_and_check` lock-order audit (#823); kept on
    /// the struct in release builds too so the storage layout stays
    /// identical between profiles.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    type_name: &'static str,
}

thread_local! {
    static LOCKS: RefCell<HashMap<TypeId, LockState>> = RefCell::new(HashMap::new());
    #[cfg(test)]
    static RECURSIVE_READ_WARNINGS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Record a read lock acquisition. Panics if a write lock is already held
/// on this type from the same thread (would deadlock).
pub(crate) fn track_read(type_id: TypeId, type_name: &'static str) {
    LOCKS.with(|locks| {
        let recursive_read = {
            let map = locks.borrow();
            if let Some(entry) = map.get(&type_id) {
                if entry.has_write {
                    panic!(
                        "ECS deadlock detected: attempted read lock on `{}` while a write lock \
                         is already held on the same thread. Drop the write query/resource first.",
                        type_name,
                    );
                }
                true
            } else {
                false
            }
        };
        // #3696 (ECS-D1-02) — run the global lock-order check ahead of the
        // `recursive_read` early return, not just on the fresh-acquisition
        // path. The module doc's "re-entrant reads don't add edges" is true
        // for the *outgoing* edge (same type, same thread — nothing new to
        // record from T's perspective), but a recursive read is still an
        // *incoming* observation for whatever else this thread already
        // holds: if this thread holds H and re-reads T while some other
        // thread previously recorded `T -> H`, this acquisition closes the
        // `H -> T -> H` cycle right here — exactly the reachability check
        // every other acquisition gets (#2675), and pre-fix the recursive
        // branch returned before ever reaching it.
        //
        // `type_id` must be filtered out of `held_others` explicitly here:
        // on the non-recursive path below it's absent by construction (the
        // row hasn't been inserted yet), but a recursive read's row is
        // already in the map — including it would present a trivial
        // self-loop (`T` "held while acquiring" `T`) that panics
        // unconditionally on every recursive read, not just a real cycle.
        //
        // #2384 — the global check can panic. Running it before either
        // mutation below (the recursive bump or the fresh insert) keeps
        // the same "no orphaned half-acquired state" property: a panic
        // here leaves `LOCKS` exactly as it was before this call.
        //
        // #3680 — `is_enabled()` gates the borrow/filter/collect below,
        // not just `record_and_check`'s own internal check, so the
        // documented "one relaxed load" fast path is real: a debug build
        // with the detector off (the default) never touches `LOCKS` a
        // second time or allocates a `Vec` for this at all.
        #[cfg(debug_assertions)]
        if global_order::is_enabled() {
            let held_others = locks
                .borrow()
                .iter()
                .filter(|(id, _)| **id != type_id)
                .map(|(id, state)| (*id, state.type_name))
                .collect::<Vec<_>>();
            global_order::record_and_check(type_id, type_name, &held_others);
        }

        if recursive_read {
            let mut map = locks.borrow_mut();
            let entry = map.get_mut(&type_id).expect("recursive read row vanished");
            // #2386 — recursive reads can deadlock behind a parked writer on
            // some RwLock implementations. TypeId-only tracking cannot tell a
            // true recursive lock from the same component type in two Worlds,
            // so warn (once on 1→2) rather than rejecting valid multi-World use.
            if entry.read_count == 1 {
                log::warn!(
                    "ECS recursive-read hazard: a second `{type_name}` read guard is live on this thread; reuse/drop the first guard when both reads target one World (#2386)"
                );
                #[cfg(test)]
                RECURSIVE_READ_WARNINGS.with(|count| count.set(count.get() + 1));
            }
            entry.read_count = entry.read_count.saturating_add(1);
            return;
        }

        locks.borrow_mut().insert(
            type_id,
            LockState {
                read_count: 1,
                has_write: false,
                type_name,
            },
        );
    });
}

/// Record a write lock acquisition. Panics if any lock (read or write) is
/// already held on this type from the same thread (would deadlock).
pub(crate) fn track_write(type_id: TypeId, type_name: &'static str) {
    LOCKS.with(|locks| {
        {
            let map = locks.borrow();
            if let Some(entry) = map.get(&type_id) {
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
            }
        }

        // #3680 — see the identical comment in `track_read`: `is_enabled()`
        // gates the borrow/collect itself, so the disabled fast path is
        // really just the one relaxed load `ENABLED`'s doc promises.
        #[cfg(debug_assertions)]
        if global_order::is_enabled() {
            let held_others = locks
                .borrow()
                .iter()
                .map(|(id, state)| (*id, state.type_name))
                .collect::<Vec<_>>();
            global_order::record_and_check(type_id, type_name, &held_others);
        }

        locks.borrow_mut().insert(
            type_id,
            LockState {
                read_count: 0,
                has_write: true,
                type_name,
            },
        );
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
/// cycle rather than deadlocking silently. "Cycle-closing" is decided
/// by reachability, so cycles of length 3+ spanning three or more
/// threads are caught too (#2675).
///
/// The graph lives process-wide behind a `RwLock`: fast-path acquires
/// (no new edges) take the read side, novel edges upgrade to the write
/// side once. After the call graph has stabilized the steady-state
/// cost is one read-lock + one HashMap lookup per held lock; the
/// reachability probe runs only on the novel-edge slow path.
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
///
/// **Coverage is test-reachability-bounded, not exhaustive.** An edge
/// only enters `GRAPH` when some enabled run actually acquires that
/// pair of locks while both are held on a thread the run drives. A
/// code path no enabled run exercises contributes zero edges — it is
/// neither cleared as safe nor flagged as risky, it's just silent. A
/// green `lock-order-check` / `vulkan-validation` job is evidence for
/// the orderings those jobs actually drove, not a proof of absence
/// for the rest of the call graph (see #2155/CONC-D4-NEW-03).
#[cfg(debug_assertions)]
mod global_order {
    use std::any::TypeId;
    use std::collections::{HashMap, HashSet};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{LazyLock, RwLock};

    /// For each type H, the set of types observed to have been
    /// acquired while H was held, keyed by `TypeId` with the type's
    /// name as the value (so a reported cycle can name every hop, not
    /// just its endpoints). An edge `H → T` (i.e. `T ∈ GRAPH[H]`)
    /// means some thread acquired T while H was held.
    ///
    /// Cycle detection is the dual read: when acquiring T while
    /// holding H, we panic if H is **reachable** from T — that means
    /// the edges already observed carry an order `T → … → H` while
    /// we're now establishing `H → T`, closing a cycle. At path
    /// length 1 this is the classic two-lock ABBA pattern; longer
    /// paths are the N-thread generalization (#2675).
    static GRAPH: LazyLock<RwLock<HashMap<TypeId, HashMap<TypeId, &'static str>>>> =
        LazyLock::new(|| RwLock::new(HashMap::new()));

    /// Whether the env var was set at process start. Cached in an
    /// atomic so the per-acquire fast-path is one relaxed load.
    /// Tests can flip this directly via [`set_enabled_for_tests`].
    static ENABLED: LazyLock<AtomicBool> =
        LazyLock::new(|| AtomicBool::new(std::env::var_os("BYRO_LOCK_ORDER_CHECK").is_some()));

    /// Depth-first search for a path `from ⇝ target` over the observed
    /// edges. Returns the chain of type names along it
    /// (`[from, …, target]`) when one exists, so a reported cycle can
    /// name every hop instead of only its endpoints.
    ///
    /// `from_name` is passed in because `from` is the incoming
    /// acquisition, which may not appear as any edge's target yet.
    /// Every other node on the path was reached through an edge, and
    /// edges carry their target's name, so the chain is always fully
    /// nameable.
    ///
    /// The graph has one node per locked type — a few dozen at most —
    /// and this runs only on the novel-edge slow path, so the
    /// steady-state cost of the detector is unchanged (#2675).
    fn find_path(
        graph: &HashMap<TypeId, HashMap<TypeId, &'static str>>,
        from: TypeId,
        from_name: &'static str,
        target: TypeId,
    ) -> Option<Vec<&'static str>> {
        let mut visited: HashSet<TypeId> = HashSet::new();
        visited.insert(from);
        // node → (predecessor, that node's own type name)
        let mut prev: HashMap<TypeId, (TypeId, &'static str)> = HashMap::new();
        let mut stack = vec![from];
        while let Some(node) = stack.pop() {
            let Some(edges) = graph.get(&node) else {
                continue;
            };
            for (next, next_name) in edges {
                if !visited.insert(*next) {
                    continue;
                }
                prev.insert(*next, (node, *next_name));
                if *next == target {
                    // Walk the predecessor chain back to `from`.
                    let mut chain = Vec::new();
                    let mut cur = target;
                    while cur != from {
                        let (p, name) = prev[&cur];
                        chain.push(name);
                        cur = p;
                    }
                    chain.push(from_name);
                    chain.reverse();
                    return Some(chain);
                }
                stack.push(*next);
            }
        }
        None
    }

    /// Record each `held → new` edge in the graph and panic if the
    /// insert would close a cycle (`new ⇝ held` already observed).
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
        // Fast-path read lock: if every edge we'd add is already
        // recorded, the graph is unchanged by this acquisition and no
        // new cycle can appear — skip the write-lock upgrade and the
        // reachability probe entirely.
        //
        // This early-out cannot hide a cycle: the graph only ever
        // grows through the slow path below, which refuses to insert a
        // cycle-closing edge (it panics instead). So a
        // cycle-closing acquisition always presents at least one novel
        // edge and always reaches the probe. (#2675 — pre-fix this
        // block also ran a depth-1 `GRAPH[new].contains(held)` test,
        // which is now subsumed by the path-length-1 case of the
        // reachability search.)
        {
            let graph = GRAPH.read().unwrap_or_else(|poison| poison.into_inner());
            let mut all_present = true;
            for (held_id, _) in held_others {
                match graph.get(held_id) {
                    Some(edges) if edges.contains_key(&new_id) => {}
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
        // lock, then check the cycle condition under write because
        // another thread may have raced us (best-effort but correct:
        // the check is transitively sound on any consistent snapshot
        // because cycles, once observed, stay forever).
        //
        // #2675 / CONC-D3-NEW-01 — the test is reachability, not a
        // direct reverse edge. Pre-fix only `new → held` at depth 1
        // panicked, so a triangle whose three edges were each recorded
        // by a different thread (each individually legal) was accepted
        // silently — the detector reported "clean" for a whole class of
        // real deadlocks, and a live 3-cycle already sat in the graph on
        // every character-mode frame.
        let mut graph = GRAPH.write().unwrap_or_else(|poison| poison.into_inner());
        let mut cycle: Option<(&'static str, Vec<&'static str>)> = None;
        for (held_id, held_name) in held_others {
            if let Some(chain) = find_path(&graph, new_id, new_name, *held_id) {
                cycle = Some((held_name, chain));
                break;
            }
        }
        if let Some((held_name, chain)) = cycle {
            // Release the write guard BEFORE unwinding. `RwLock` poisons
            // only when a panic escapes an exclusive lock, and a poisoned
            // GRAPH would turn every later acquisition's `expect("GRAPH
            // poisoned")` into a second, unrelated panic — including
            // inside a `catch_unwind` harness probing the detector. The
            // pre-#2675 code panicked out of the read guard (which never
            // poisons); dropping here preserves that property now that
            // the probe runs under the write guard. The cycle-closing
            // edge is still never inserted.
            drop(graph);
            panic!(
                "ECS cross-thread deadlock risk (lock-order cycle): attempted \
                     acquisition of `{}` while holding `{}` on this thread — that \
                     closes a cycle over the acquisition orders already observed \
                     across threads: {} → `{}`. Threads acquiring these locks in \
                     these orders will deadlock. Use `query_2_mut`/`query_2_mut_mut` \
                     for paired access (TypeId-sorted), or acquire locks in a \
                     consistent process-wide order. See #313 / #2675.",
                new_name,
                held_name,
                chain
                    .iter()
                    .map(|n| format!("`{n}`"))
                    .collect::<Vec<_>>()
                    .join(" → "),
                new_name,
            );
        }
        for (held_id, _) in held_others {
            graph.entry(*held_id).or_default().insert(new_id, new_name);
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

    /// #3680 (PERF-D1-2026-08-30-04) — the one relaxed load `ENABLED`'s
    /// own doc comment promises the per-acquire fast path. `track_read`/
    /// `track_write` check this BEFORE ever borrowing `LOCKS` or building
    /// `held_others`, so a debug build with the detector off (the
    /// default) pays exactly this and nothing else per acquisition —
    /// `record_and_check` used to be the only place checking `ENABLED`,
    /// reached only after the caller had already collected a `Vec` that
    /// the disabled check then discarded.
    pub(super) fn is_enabled() -> bool {
        ENABLED.load(Ordering::Relaxed)
    }

    /// Test-only — clear the graph between tests so a previous
    /// observation doesn't leak into an unrelated test case.
    #[cfg(test)]
    pub(super) fn reset() {
        GRAPH
            .write()
            .unwrap_or_else(|poison| poison.into_inner())
            .clear();
    }

    #[cfg(test)]
    pub(super) fn poison_for_tests() {
        let _ = std::thread::spawn(|| {
            let _guard = GRAPH.write().unwrap_or_else(|poison| poison.into_inner());
            panic!("intentional GRAPH poison for recovery regression");
        })
        .join();
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
fn take_recursive_read_warning_count() -> usize {
    RECURSIVE_READ_WARNINGS.with(|count| count.replace(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::sparse_set::SparseSetStorage;
    use crate::ecs::storage::Component;
    use crate::ecs::World;
    use std::any::TypeId;

    struct FakeA;
    struct FakeB;

    struct WorldA;
    impl Component for WorldA {
        type Storage = SparseSetStorage<Self>;
    }

    struct WorldB;
    impl Component for WorldB {
        type Storage = SparseSetStorage<Self>;
    }

    #[test]
    fn recursive_read_warns_once_and_continues() {
        let id = TypeId::of::<FakeA>();
        take_recursive_read_warning_count();
        track_read(id, "FakeA");
        track_read(id, "FakeA");
        track_read(id, "FakeA");
        assert_eq!(take_recursive_read_warning_count(), 1);
        untrack_read(id);
        untrack_read(id);
        untrack_read(id);
        assert!(is_clean());
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
    struct Abba5;
    struct Abba6;
    struct Abba7;
    struct Recur1;
    struct Recur2;

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
    /// - **Cycles longer than 2 are caught** (#2675): `A → B` then
    ///   `B → C` then `C → A` panics on the third pattern, even though
    ///   no direct reverse edge exists for any single pair. Before the
    ///   fix the detector only closed length-2 cycles, so a triangle
    ///   spread over three scheduler stages was recorded silently.
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
            untrack_read(b);
            assert!(
                is_clean(),
                "#2384: a caught ABBA panic must leave no orphaned tracker row"
            );

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

            // Scenario 3: recursive reads warn but do not add graph self-edges.
            global_order::reset();
            let e = TypeId::of::<FakeA>();
            track_read(e, "FakeA");
            track_read(e, "FakeA");
            untrack_read(e);
            untrack_read(e);
            track_read(e, "FakeA"); // fresh acquire after release
            untrack_read(e);

            // Scenario 4 (#2675): a three-lock cycle. Each edge is
            // legal on its own and no pair has a direct reverse edge,
            // so only a reachability probe can see the closure.
            global_order::reset();
            let f = TypeId::of::<Abba5>();
            let g = TypeId::of::<Abba6>();
            let h = TypeId::of::<Abba7>();
            // Edge Abba5 → Abba6.
            track_read(f, "Abba5");
            track_read(g, "Abba6");
            untrack_read(g);
            untrack_read(f);
            // Edge Abba6 → Abba7.
            track_read(g, "Abba6");
            track_read(h, "Abba7");
            untrack_read(h);
            untrack_read(g);
            // Edge Abba7 → Abba5 closes the triangle
            // (Abba5 ⇝ Abba7 is reachable) → must panic.
            let panicked = std::panic::catch_unwind(|| {
                track_read(h, "Abba7");
                track_read(f, "Abba5"); // closes the 3-cycle
                untrack_read(f);
                untrack_read(h);
            })
            .is_err();
            assert!(
                panicked,
                "a 3-lock cycle must panic — the detector must close cycles of \
                 any length, not just direct A→B / B→A pairs (#2675)"
            );
            untrack_read(h);
            assert!(
                is_clean(),
                "cycle-closing panic must not poison thread-local state"
            );

            // Scenario 4b (#3696 / ECS-D1-02): a recursive read must still
            // be checked against the graph, even though it adds no new
            // *outgoing* edge. Hold Recur1, acquire Recur2 (records
            // Recur1 -> Recur2), then re-read Recur1 while Recur2 is still
            // held — this thread now holds Recur2 and is "acquiring"
            // Recur1 again, the incoming half of a Recur2 -> Recur1 edge
            // that would close the 2-cycle. Pre-fix the recursive-read
            // early return skipped `record_and_check` entirely and this
            // never panicked.
            global_order::reset();
            let recur1 = TypeId::of::<Recur1>();
            let recur2 = TypeId::of::<Recur2>();
            track_read(recur1, "Recur1");
            track_read(recur2, "Recur2"); // records Recur1 -> Recur2
            let panicked = std::panic::catch_unwind(|| {
                track_read(recur1, "Recur1"); // recursive read, closes the cycle
            })
            .is_err();
            assert!(
                panicked,
                "a recursive read that closes a cycle must panic, not be silently \
                 skipped by the recursive-read early return (#3696)"
            );
            untrack_read(recur1);
            untrack_read(recur2);
            assert!(
                is_clean(),
                "a caught recursive-read cycle panic must leave no orphaned tracker row"
            );

            // Scenario 5 (#2387): the headline cross-thread guarantee through
            // real World queries. Both workers hold their first storage before
            // racing the reverse second acquisition; exactly one side must be
            // rejected by the slow-path write-lock re-check, and the other must
            // finish once that first guard unwinds.
            global_order::reset();
            let mut world = World::new();
            let entity = world.spawn();
            world.insert(entity, WorldA);
            world.insert(entity, WorldB);
            let world = std::sync::Arc::new(world);
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));

            let left_world = std::sync::Arc::clone(&world);
            let left_barrier = std::sync::Arc::clone(&barrier);
            let left = std::thread::spawn(move || {
                let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _a = left_world.query::<WorldA>().unwrap();
                    left_barrier.wait();
                    let _b = left_world.query::<WorldB>().unwrap();
                }))
                .is_err();
                (panicked, is_clean())
            });
            let right_world = std::sync::Arc::clone(&world);
            let right_barrier = std::sync::Arc::clone(&barrier);
            let right = std::thread::spawn(move || {
                let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _b = right_world.query::<WorldB>().unwrap();
                    right_barrier.wait();
                    let _a = right_world.query::<WorldA>().unwrap();
                }))
                .is_err();
                (panicked, is_clean())
            });
            let left = left.join().unwrap();
            let right = right.join().unwrap();
            let panics = usize::from(left.0) + usize::from(right.0);
            assert_eq!(
                panics, 1,
                "opposite real-World acquisition orders must reject exactly one worker"
            );
            assert!(
                left.1 && right.1,
                "#2384: RAII query guards must clean both worker-local tracker maps after ABBA unwind"
            );

            // Scenario 6 (#2385): even if some unrelated writer poisoned the
            // process-global graph, every detector acquisition and reset must
            // recover the inner graph instead of replacing the useful ABBA
            // diagnostic with an opaque `GRAPH poisoned` panic.
            global_order::poison_for_tests();
            global_order::reset();
            let a = TypeId::of::<Abba1>();
            let b = TypeId::of::<Abba2>();
            track_read(a, "Abba1");
            track_read(b, "Abba2");
            untrack_read(b);
            untrack_read(a);
        }
    }
}
