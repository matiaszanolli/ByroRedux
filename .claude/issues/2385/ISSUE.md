# #2385 — ECS-D1-02: ABBA detector's slow path poisons the global graph, and neither `GRAPH` acquisition resolves the `PoisonError`

- **Severity**: LOW
- **Domain**: ecs, sync
- **Audit**: `docs/audits/AUDIT_ECS_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2385


- **Severity**: LOW
- **Dimension**: 1 — Lock Ordering & Deadlock (poison resolution)
- **Location**: `crates/core/src/ecs/lock_tracker.rs:253`, `:293-305`, `:324`
- **Status**: NEW

**Description**

`record_and_check`'s slow path takes `GRAPH.write()` and then may `panic!` while that write guard is still alive. `std::sync::RwLock` poisons on a panic under a write guard, so this permanently poisons the process-global graph. Both `GRAPH` acquisitions then fail: `GRAPH.read().expect("GRAPH poisoned")` and `GRAPH.write().expect("GRAPH poisoned")`. This is the one lock acquisition in the ECS core that does not route a `PoisonError` through a diagnostic resolver, unlike every `storage_lock_poisoned::<T>()`/`resource_lock_poisoned::<R>()` site in `world.rs`. The test-only `reset()` (`GRAPH.write().unwrap()`) also panics on a poisoned graph. (Deliberately not claimed for the fast path: that panic is under a read guard, and std explicitly does not poison an `RwLock` on read-guard panic. Only the write path poisons — reachable when another thread inserts the reverse edge in the window between the fast-path read release and the write acquire, i.e. exactly the multi-threaded case the detector exists for.)

**Evidence** (re-confirmed at publish time against the same commit `79bfc76e`):

```rust
// lock_tracker.rs:293-305
let mut graph = GRAPH.write().expect("GRAPH poisoned");
if let Some(new_edges) = graph.get(&new_id) {
    for (held_id, held_name) in held_others {
        if new_edges.contains(held_id) {
            panic!("ECS cross-thread deadlock risk (ABBA): ...");   // panics holding the write guard
```

**Impact**

After the first slow-path hit, every subsequent tracked ECS lock acquisition in the process panics with the opaque "GRAPH poisoned" instead of the real ABBA diagnostic naming the two types — the detector destroys the very report it exists to produce. Bounded: debug-only, opt-in, and the triggering panic is already fatal in the shipping engine (no per-frame `catch_unwind`). Worst realistic case is a confusing CI failure in a `lock-order-check` job.

**Related**: ECS-D1-03 (same panic path, thread-local side), #313.

**Suggested Fix**: Compute the cycle verdict under the guard, `drop(graph)`, then panic; and resolve both `GRAPH` acquisitions with `.unwrap_or_else(PoisonError::into_inner)` so a poisoned graph degrades to "keep detecting" rather than "panic on everything".

## Completeness Checks
- [ ] **SIBLING**: Confirm no other process-global `RwLock`/`Mutex` in the ECS core has the same unresolved-poison shape
- [ ] **LOCK_ORDER**: Verify the `drop(graph)`-then-panic restructuring doesn't change ABBA detection semantics
- [ ] **TESTS**: A regression test that triggers the write-path ABBA panic and then asserts a subsequent unrelated tracked acquisition still succeeds (graph degrades instead of poisoning everything)

---
Filed from `docs/audits/AUDIT_ECS_2026-08-07.md` via `/audit-publish`.
