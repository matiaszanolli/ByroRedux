# PERF-D1-01: target_has_line_of_sight rebuilds an O(all-rigid-bodies) Vec and linear-scans it, every frame the crosshair is on an activator

Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-16.md` (Dimension 1 — CPU hot paths).

**Location**: `byroredux/src/interaction.rs`:750-782 (`target_has_line_of_sight`)

## Description

`target_has_line_of_sight` rebuilds an **O(all-rigid-bodies) `Vec`** and linear-scans it, **every frame the crosshair is on an activator**.

## Evidence

```rust
// byroredux/src/interaction.rs (re-verified 2026-08-17)
    .iter()
    …
    .collect::<Vec<_>>();
```

The collection is rebuilt per call with no reuse and no spatial index; the subsequent test is a linear scan over it.

## Impact

Per-frame allocation plus a linear scan proportional to the whole rigid-body set, on the interaction path that runs whenever the player looks at an activator — i.e. constantly during normal play.

The user's hardware profile (Ryzen 7950X) makes CPU-side per-frame waste the kind that shows as a frame-time floor rather than a GPU stall, and `_audit-common.md` treats a CPU bottleneck as a bug outright.

## Suggested Fix

Hoist the collection into per-frame scratch reused across calls (the pattern `context/mod.rs` already uses for its skin/blend scratch), or query the physics `QueryPipeline` directly instead of materialising a `Vec` — `PhysicsWorld` already exposes cast helpers.

Measure first: this is a hot-path claim, and the fix is only worth landing if the saving is real.

## Related

- #3061 (PERF-D1-02 — a second per-frame allocation in the same file)
- #3062 (PERF-D1-03 — two per-frame `HashSet` clones in the same file)

## Completeness Checks
- [ ] **MEASURED**: The saving is benchmarked on a real cell, not assumed
- [ ] **SIBLING**: Fixed alongside the other two `interaction.rs` per-frame allocations
- [ ] **CORRECTNESS**: LOS results are identical before and after
- [ ] **TESTS**: A regression test pins LOS behaviour; a bench pins the allocation count

