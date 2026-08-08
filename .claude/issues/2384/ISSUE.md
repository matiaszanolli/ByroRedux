# #2384 — ECS-D1-03: ABBA panic orphans a thread-local tracker row — the #137/#2149 RAII guard cannot cover it

- **Severity**: LOW
- **Domain**: ecs, sync
- **Audit**: `docs/audits/AUDIT_ECS_2026-08-07.md`
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2384


- **Severity**: LOW
- **Dimension**: 1 — Lock Ordering & Deadlock (tracker state integrity)
- **Location**: `crates/core/src/ecs/lock_tracker.rs:74-93`, `:122-135`, `:342-401`, `:545`
- **Status**: NEW

**Description**

`track_read` performs `entry.read_count += 1` and `track_write` performs `entry.has_write = true` before calling `global_order::record_and_check`, which can panic. Because the panic fires inside `track_*`, the `TrackedRead`/`TrackedWrite` value has not been constructed yet — so the RAII mechanism introduced by #137 and hardened by #2149, whose entire purpose is to release an orphaned tracker row when a panic occurs between intent and guard construction, cannot fire. The row survives the unwind. Contrast the same-type reentrancy panics, which all fire before any mutation and are clean — the asymmetry is not documented.

**Evidence**: the module's own test has to hand-clear the map after catching the ABBA panic:

```rust
// lock_tracker.rs:543-545
.is_err();
assert!(panicked, "ABBA pattern must panic");
LOCKS.with(|l| l.borrow_mut().clear());
```

and `world_tests.rs:1312` documents that `is_clean()` "covers every `TrackedRead::new`/`TrackedWrite::new` call site in `world.rs`" — i.e. the poison path, not this one.

**Impact**

Any `catch_unwind` boundary around an ABBA panic (rayon's per-task catch inside `par_iter_mut().for_each`, the debug server, or a future per-frame recovery guard) leaves a phantom row on that thread; the next legitimate acquisition of that type on the same thread reports a spurious "ECS deadlock detected", sending the reader to the wrong system. Bounded by debug-only + opt-in + fail-fast today.

**Related**: #137, #2149, ECS-D1-02.

**Suggested Fix**: Perform the `record_and_check` call before mutating the `LockState` (compute `held_others` from the pre-insert map), or wrap the mutation in a scope guard that rolls back the `read_count`/`has_write` change if the graph check unwinds.

## Completeness Checks
- [ ] **SIBLING**: Verify the same-type reentrancy panic path is genuinely clean (mutation-after-check ordering) as a control case
- [ ] **TESTS**: A regression test that catches an ABBA panic via `catch_unwind` (not the module's manual `LOCKS.with(...).clear()`) and asserts the thread-local row was correctly released without the workaround

---
Filed from `docs/audits/AUDIT_ECS_2026-08-07.md` via `/audit-publish`.
