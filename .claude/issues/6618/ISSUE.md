# ECS-2026-11-02-01: `material_dump::material_rows` reads `Material` then `MeshHandle` out of TypeId order; a second cycle on the ABBA lane

Labels: high,ecs,concurrency,bug
Source: CI run 41549379246 (Session 107 close)

**Severity**: HIGH (ECS lock-order cycle; it reddens the ABBA lock-order detector lane, so CI fails on every `main` run, the same class as #6561)
**Dimension**: ECS / Lock Ordering
**Source**: found at the Session 107 close in CI run 41549379246 (HEAD `7f2d98944`), job "ABBA lock-order detector". Introduced by `e7d7af3c9` (M-LIGHT Phase 1b baseline apparatus, material dump).
**Status**: NEW

## Location
`byroredux/src/material_dump.rs:56` (`material_rows`):

```rust
if let (Some(materials), Some(meshes)) = (world.query::<Material>(), world.query::<MeshHandle>()) {
```

Two single-component read queries, taken in source order (`Material`, then `MeshHandle`) instead of `TypeId` order. Caller: `App::advance_mitsuba_export` (`byroredux/src/app_events.rs:586`, `--material-dump`), and the test `material_dump::tests::rows_count_instances_and_skip_meshless_materials` (`material_dump.rs:114-129`).

## Description
With `BYRO_LOCK_ORDER_CHECK=1` the test panics:

```
ECS cross-thread deadlock risk (lock-order cycle): attempted acquisition of `MeshHandle` while holding `Material` on this thread — that closes a cycle over the acquisition orders already observed across threads: `MeshHandle` → `TextureHandle` → `Material` → `MeshHandle`.
```

The rest of the engine reaches these storages in the opposite order, so the dump closes a cycle the detector already knows the other edges of. Read-read on its own cannot deadlock, but `std::sync::RwLock` is writer-preferring on Linux: a writer parked on `MeshHandle` between the two acquisitions blocks this thread while it holds `Material`.

## Impact
The lock-order lane is red on every run (together with #6561), so it cannot catch a new cycle. In production the dump runs once, on the main thread, during the first rendered frame of a bench run; the deadlock window is small but real while the parallel scheduler runs.

## Suggested Fix
Acquire both read guards in `TypeId` order through the existing sorted helper (or sort the two acquisitions the way `query_2_mut` does), and keep the test running under the detector.

## Completeness Checks
- [ ] **LOCK_ORDER**: Both guards acquired in `TypeId` order
- [ ] **SIBLING**: The other M-LIGHT / Mitsuba-export readers (`cornell/mitsuba_export.rs`, `cornell/reference.rs`) checked for the same two-query shape
- [ ] **TESTS**: `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux material_dump` passes
