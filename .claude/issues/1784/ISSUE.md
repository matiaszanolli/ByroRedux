# CONC-D3-01: World accessor docs claim the same-thread lock tracker is release no-op — it is active in release builds

_Filed as #1784 from `docs/audits/AUDIT_CONCURRENCY_2026-07-02.md`._

**Severity**: LOW · **Dimension**: ECS Lock Ordering (doc rot) · Source: `AUDIT_CONCURRENCY_2026-07-02` (CONC-D3-01)

## Location
`crates/core/src/ecs/world.rs:374-380` (and the "debug only" panic headers on `query_mut`, `get`, `has`, `count`, `try_resource`, `try_resource_mut`) vs. `crates/core/src/ecs/lock_tracker.rs:7-12`.

## Description
`world.rs:378-380` states: *"Release builds do not enforce the check (production hot paths get a zero-cost no-op)."* This is false. `track_read` / `track_write` (`lock_tracker.rs:58-137`) carry no `cfg(debug_assertions)` gate; only the `held_others` Vec + `global_order::record_and_check` block and the graph module are debug-only. `TrackedRead::new` / `TrackedWrite::new` are called unconditionally from every `&self` acquisition site in `world.rs`. The module doc has it right: *"Thread-local check (always on — debug and release builds)"* (`lock_tracker.rs:9`).

## Evidence
```rust
// world.rs:377-380 (query::<T> doc)
/// Drop the offending guard before calling. Release builds do
/// not enforce the check (production hot paths get a zero-cost
/// no-op).
```
vs. `lock_tracker.rs:99-137` — `track_write` panics on conflict with no cfg gate; the #823 fix comment explicitly discusses the *release-build* per-frame cost of this function.

## Impact
Documentation-only, but misleads on two operational facts: (a) a same-thread write-conflict acquisition **panics in release too** (good — converts a silent `RwLock` deadlock into a diagnosable crash), and (b) the release hot path pays a thread-local HashMap probe per acquisition, not zero. Verification: `cargo test --release -p byroredux-core` — `lock_tracker::tests::write_then_write_same_type_panics` passes in release, proving the check is live.

## Related
CONC-D3-02/03/04 (same declaration-trust surface).

## Suggested Fix
Rewrite the `# Panics (debug only)` headers: the thread-local re-entrancy check is always-on; only the cross-thread ABBA graph is debug-only + `BYRO_LOCK_ORDER_CHECK`-gated. Delete the "zero-cost no-op" sentence.

## Completeness Checks
- [ ] **SIBLING**: All the "debug only" panic headers listed above corrected consistently, not just `query::<T>`
- [ ] **TESTS**: The always-on behavior is already pinned by `write_then_write_same_type_panics` (run `--release`)
