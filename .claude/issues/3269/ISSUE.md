# 3269: PERF-D1-2026-08-24-01: six NAVM-pathed AI procedures each clone a full VecDeque<Vec3> waypoint list twice per entity per tick

**Severity**: MEDIUM · **Report**: `docs/audits/AUDIT_PERFORMANCE_2026-08-24.md` (PERF-D1-2026-08-24-01)

## Description

`resolve_cached_waypoints`'s cache-hit arm clones `path.waypoints` every tick for frozen-goal callers (Travel, Guard's lead phase, Escort's lead phase — `repath_threshold = 0.0`, steady-state path). Four of six consumers (Travel, Guard, Escort, Follow) then pay a **second** clone to satisfy the borrow checker when calling `step_along_waypoints` from an immutable-borrow loop over per-tick scratch.

## Location

`byroredux/src/systems/navmesh_path.rs:343-362` (clone #1 at `:352`); `travel.rs:231/262`, `guard.rs:189/223`, `escort.rs:239/278/300/340`, `follow.rs:216-221/253` (clone #2 sites)

## Evidence

```rust
// travel.rs Pass 1b
for p in &scratch.pending {
    let (new_pos, rotation, waypoints) = step_along_waypoints(
        p.current, p.rotation, p.waypoints.clone(), p.destination, dt,
        physics.as_deref(),
    );
    ...
}
```

## Impact

Bounded by the population running Travel/Guard/Escort/Follow (all opt-in via env var, none in default scheduler). Real, measurable, easily-fixed inefficiency that scales with actor count once these packages are enabled by default.

## Related

`ECS-2026-08-24-08` (#3256, cache-invalidation correctness gap at the same location — different defect, cross-referenced not duplicated).

## Suggested Fix

Drop clone #2 by consuming `scratch.pending` by value (`drain(..)` or `std::mem::take(...).into_iter()`) instead of `for p in &scratch.pending` — nothing reads `scratch.pending` after Pass 1b. Clone #1 is structurally harder to remove without changing `NavPath`'s storage shape — leave it.

## Completeness Checks
- [ ] **TESTS**: A regression test/allocation-count assertion for the steady-state tick
