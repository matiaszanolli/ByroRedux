# #3477 — PERF-2026-08-27b-05: collect_newcomers rescans every collider row every tick to answer "nothing new"

**Labels**: low, performance, physics, bug
**Filed**: 2026-08-27 from `docs/audits/AUDIT_PERFORMANCE_2026-08-27b.md`
**HEAD at audit**: `969d81c8`

---

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-27b.md` — finding `PERF-2026-08-27b-05`
**Severity**: LOW · **Dimension**: CPU Hot Paths
**Location**: `crates/physics/src/sync.rs:807-866`

## Description

Phase 1 of the physics tick re-derives the newcomer set from scratch on every frame by walking the **entire** `CollisionShape` storage and probing `RapierHandles` for each row. In steady state — every collider already registered, which is the overwhelming majority of frames — the loop's entire output is an empty `Vec`, and the work is proportional to the resident collider count rather than to the (zero) number of newcomers.

## Evidence

```rust
for (entity, shape) in shape_q.iter() {
    if handles_q.contains(entity) {
        continue;
    }
    let Some(body_data) = body_q.get(entity) else {
```

There is no dirty set, insertion queue, or generation counter feeding this; `physics_sync_system` calls it unconditionally each tick (`crates/physics/src/sync.rs:129`), and the same scan is reached a second time through `register_newcomers_and_refresh_queries` (`:207-217`, added this cycle for cold-start spawn probing and cell-arrival grounding, calling `collect_newcomers` at `:212`).

## Impact

Derived: a linear walk plus one sparse-set probe per collider row per frame — order 10 us at the few-thousand-collider scale a Skyrim exterior reaches (one interior went 19 -> 416 colliders after the `#1832` mass=0 Dynamic-family reclassification alone). Below the noise floor of the current bench, hence LOW. Recorded because it is the structural reason the `#2867` bug was as expensive as it was — that defect re-collected and re-registered *the full newcomer set including `TriMesh` vertex/index clones* every frame, and the fix removed the re-registration without removing the rescan that made it possible.

## Related

#2867 (the re-registration leak this scan enabled), #1520.

## Suggested Fix

Give `PhysicsWorld` a `pending_registration: Vec<EntityId>` fed at the two sites that actually create colliders (cell spawn and ragdoll activation), and fall back to the full scan only when that queue is absent — the same "explicit dirty set beats a rescan" move `#1195`'s `pose_dirty` and `#3319`'s `NavPath` cache both made. A cheaper interim: skip the walk entirely when `shape_q.len() == handles_q.len()`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other per-tick full-storage rescans in `crates/physics/src/sync.rs`)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (the Handles -> Body -> Global order at `:838-843` is load-bearing per `#313`)
- [ ] **TESTS**: A regression test pins this specific fix
