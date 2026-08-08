# #2387 — ECS-D1-04: The cross-rayon-worker ABBA guarantee has zero test coverage — the only test is single-threaded and bypasses `World`

- **Severity**: LOW
- **Domain**: ecs, sync
- **Audit**: `docs/audits/AUDIT_ECS_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2387


- **Severity**: LOW
- **Dimension**: 1 — Lock Ordering & Deadlock (test coverage)
- **Location**: `crates/core/src/ecs/lock_tracker.rs:505-571`; `crates/core/src/ecs/world_tests.rs:1160-1440`
- **Status**: NEW

**Description**

The headline claim of the graph — "two systems on separate rayon workers acquiring the same pair of single-type queries in opposite orders" — is exercised by nothing. `global_graph_detector_end_to_end` runs all three of its scenarios on a single thread, calling `track_read`/`untrack_read` directly: it never goes through `World::query*`, never spawns a second thread, and therefore never touches the shared-static cross-thread path or the write-path race re-check (the only path that can poison the graph, ECS-D1-02). `world_tests.rs` does spawn threads but exclusively to poison a lock for the #137 regression suite, never to construct opposing acquisition orders.

**Evidence**: `lock_tracker.rs:528-540` — scenario 1 is `track_read(a); track_read(b); untrack…;` then `catch_unwind(|| { track_read(b); track_read(a); })`, all on the calling thread. The test's own doc comment explains the scenarios are combined into one body "so the runtime opt-in flag doesn't race with the parallel test runner" — i.e. the design deliberately avoids concurrency.

**Impact**

The mechanism that generalises the `query_2_mut` pair guarantee to arbitrary N-lock patterns across the scheduler is unverified end-to-end. A refactor that broke the shared static, the `is_new` edge gate, or the write-path re-check would not fail any test. Combined with ECS-D1-01 (doc rot — regression of closed #1784, tracked separately), the cross-thread half of the deadlock guard is both mis-documented and untested.

**Related**: ECS-D1-02, ECS-D1-05, #313, #2155.

**Suggested Fix**: Add one `#[cfg(debug_assertions)]` test that builds a real `World` with two registered storages and drives `world.query::<A>() → query::<B>()` on one `std::thread` and `query::<B>() → query::<A>()` on another (barrier-synchronised, `catch_unwind` on both) under `set_enabled_for_tests(true)`, asserting exactly one side panics with "cross-thread deadlock risk".

## Completeness Checks
- [ ] **TESTS**: Add the described real-`World`, two-thread, barrier-synchronised test — this finding's entire content is a test-coverage gap
- [ ] **LOCK_ORDER**: Confirm the new test exercises the write-path race re-check, not just the fast path

---
Filed from `docs/audits/AUDIT_ECS_2026-08-07.md` via `/audit-publish`.
