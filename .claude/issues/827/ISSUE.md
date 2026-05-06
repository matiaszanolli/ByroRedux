ECS-PERF-05: animation_system queries Name storage twice during the prelude (SubtreeCache + NameIndex)|## Description

The prelude takes two separate `world.query::<Name>()` read locks back-to-back, both just to call `.len()`. They could share one query handle.

Cost is dominated by the lock acquisition (RwLock fast-path read = ~10–30 ns each), but on the per-frame critical path with the lock-tracker bookkeeping in #823 (ECS-PERF-01) each acquisition also pays one Vec allocation in release builds.

## Location

`byroredux/src/systems.rs:306,324`

## Evidence

```rust
// Block 1 — SubtreeCache generation check
{
    let current_name_count = world.query::<Name>().map(|q| q.len()).unwrap_or(0);
    let needs_clear = world.try_resource::<SubtreeCache>()
        .map(|c| c.generation != current_name_count).unwrap_or(false);
    // ...
}
// Block 2 — NameIndex generation check
{
    let current_name_count = world.query::<Name>().map(|q| q.len()).unwrap_or(0);
    // ... identical pattern
}
```

## Impact

~50 ns/frame plus one Vec allocation in release (see #823). Trivial in isolation; included for completeness because it compounds with #823.

Also a minor correctness smell: the two checks observe `Name::len()` independently, so a Name spawn between block 1 and block 2 would invalidate one cache and not the other for one frame. Today the only path that mutates Names mid-system is `event_cleanup_system` in the Late stage, so the inconsistency is unreachable — but the pattern is fragile.

## Suggested Fix

Merge the two blocks into one — query Name once, capture the count, run both generation checks against the same value:

```rust
let current_name_count = world.query::<Name>().map(|q| q.len()).unwrap_or(0);

// SubtreeCache check
if world.try_resource::<SubtreeCache>().map(|c| c.generation != current_name_count).unwrap_or(false) {
    let mut cache = world.resource_mut::<SubtreeCache>();
    cache.map.clear();
    cache.generation = current_name_count;
}

// NameIndex check
if world.try_resource::<NameIndex>().map(|idx| idx.generation != current_name_count).unwrap_or(true) {
    // ... rebuild ...
}
```

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check `animation_system` for other duplicate-query patterns (search for repeat `world.query::<` of same type within one fn)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: Same locks, just merged scope — no change
- [ ] **FFI**: N/A
- [ ] **TESTS**: N/A — purely code-shape change, behavior unchanged

## Source Audit

`docs/audits/AUDIT_PERFORMANCE_2026-05-04_DIM4.md` — ECS-PERF-05