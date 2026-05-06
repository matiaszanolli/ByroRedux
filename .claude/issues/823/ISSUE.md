ECS-PERF-01: lock_tracker allocates Vec<(TypeId, &str)> on every novel-type acquire — release builds too|## Description

Both `track_read` and `track_write` build a `Vec<(TypeId, &'static str)>` of all currently-held lock types every time a lock is taken on a type whose entry isn't already present (`is_new`). That `held_others` Vec is then handed off to `global_order::record_and_check`, which is a **no-op in release builds** (the `#[cfg(not(debug_assertions))]` arm just does `let _ = held_others;`). The allocation happens unconditionally before the cfg switch — so release builds pay for a Vec they immediately throw away.

## Location

- `crates/core/src/ecs/lock_tracker.rs:74-84` (read path)
- `crates/core/src/ecs/lock_tracker.rs:115-125` (write path)

## Evidence

```rust
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
    let _ = held_others;  // allocation already happened
}
```

## Impact

`build_render_data` alone takes ~15 distinct read locks per frame; `animation_system` adds another ~17; transform/bound propagation each add 4–5; particle/billboard add 2–3. Conservatively 40+ novel-type acquires per frame.

In `build_render_data` alone the vector grows from 0 to 14 elements over the 15 acquisitions, so the cumulative allocation work is O(N²/2) ≈ 100 small allocations per frame just from that one function. Per allocation is ~50–100 ns (allocator fast path). At 60 fps: ~6 µs/frame, ~12 KB/frame churn.

Sub-noise on its own, but every per-frame allocation increases allocator fragmentation and is a paper cut for the parallel-scheduler future where many threads hammer the same allocator.

## Suggested Fix

Either (a) gate the entire `held_others` collection inside `#[cfg(debug_assertions)]`, or (b) pass an iterator (`map.iter().filter(...).map(...)`) into a debug-only helper that accepts `impl Iterator`, eliding the Vec entirely. (a) is the one-line fix.

## Completeness Checks
- [ ] **UNSAFE**: N/A — no unsafe involved
- [ ] **SIBLING**: Both `track_read` (line 74) AND `track_write` (line 115) have the same pattern — fix must touch both
- [ ] **DROP**: N/A — no Vulkan objects
- [ ] **LOCK_ORDER**: Verify `record_and_check` in debug builds still observes the correct lock-order context (it already does — receives `held_others` as `&[...]`)
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add a release-build microbenchmark that asserts zero allocations during 1000 ECS query acquisitions on the same set of types

## Source Audit

`docs/audits/AUDIT_PERFORMANCE_2026-05-04_DIM4.md` — ECS-PERF-01