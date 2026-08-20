# PERF-D1-01: apply_buoyancy's quiesced-scene fast path is unreachable in any cell containing water

**Issue**: #3135 — https://github.com/matiaszanolli/ByroRedux/issues/3135
**Labels**: `medium,performance,bug`
**Filed**: 2026-08-20 · comprehensive audit suite
**Report**: `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md`

---

**Severity**: MEDIUM
**Dimension**: CPU Per-Frame Allocations & Hot Paths
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-08-20.md` (PERF-D1-01)

## Location

- `crates/physics/src/water.rs` — the `waves_active` computation and the fast path it disarms (`:484-511`), the O(all-rigid-bodies) `targets` build it guards (`:551-575`), `collect_water_surfaces` (`:355-375`), `collect_water_current_volumes` (`:378-383`)
- Default at `crates/core/src/ecs/components/water.rs:347` (`wave_amplitude: 0.05`)

## Status

NEW — introduced by `6b960349` (2026-08-20), inside this delta.

## Description

`apply_buoyancy` carries a deliberately-engineered quiesced-scene fast path — the WATAL §0 "exterior freeze" contract — whose own comment says *"With nothing awake, nothing pending, and no newcomer this frame, no body moved since the last buoyancy eval, so the per-body scan is pure waste."*

`6b960349` added a fourth term:

```rust
let waves_active = time_secs.is_some()
    && surfaces.iter().any(|s| s.material.wave_amplitude.abs() > 1.0e-4);
...
if pw.awake_counts().0 == 0 && !pw.pending_wake() && !had_newcomers && !waves_active {
    return;
}
```

`WaterMaterial::default().wave_amplitude` is **0.05** — four orders of magnitude above the `1.0e-4` epsilon — and the WATAL sentinel comment at `water.rs:344-346` states it is deliberately the value *"a record that omits wave data resolves to … across all games"*.

**So `waves_active` is true for effectively every water surface ever spawned, and the fast path is dead code in exactly the scenes it was written for.**

## Evidence

Confirmed at HEAD:
```
crates/physics/src/water.rs:484:    let waves_active = time_secs.is_some()
crates/physics/src/water.rs:487:            .any(|surface| surface.material.wave_amplitude.abs() > 1.0e-4);
crates/physics/src/water.rs:509:        if pw.awake_counts().0 == 0 && !pw.pending_wake() && !had_newcomers && !waves_active {
crates/core/src/ecs/components/water.rs:347:            wave_amplitude: 0.05,
```

With the gate disarmed, **every frame in a water cell runs the full body of `apply_buoyancy`**:
- `collect_water_surfaces` allocates a fresh `Vec<WaterSurface>` whose element embeds a **436 B** `WaterMaterial` **by value**;
- `collect_water_current_volumes` allocates a second fresh `Vec`;
- then `targets: Vec<Target> = Vec::new()` is built by walking **every** `RapierHandles` entity with a `RigidBodyData` probe **and** a `WaterContact` probe each — the full rigid-body set, not the wet subset.

None of the three `Vec`s is a reused scratch.

## Impact

The static-scene step fast-path in `PhysicsWorld::step` still engages, so the **solver** stays cheap; what is lost is the buoyancy phase's own O(all bodies) prologue, **every frame, in every water-bearing cell** — including a fully settled one where the correctness motivation (a wave crest wetting a body at the waterline) cannot apply to any body not already adjacent to the surface.

There is **no quantitative guard for this site**: per `/audit-performance`'s Regression-Guard Posture, `dhat` is a process singleton and the live engine loop has no allocation-bound coverage.

## Suggested Fix

**Narrow the term rather than deleting it.** A wave crest can only change a body's wetness if the body is within one wave amplitude of a surface, and every such body was wet or borderline last frame. So gate on *"any resident `WaterContact` with `submerged_fraction > 0`, or a prior depth inside the crest band"* instead of *"any surface has waves"*.

Separately, hoist `surfaces` / `current_volumes` / `targets` into persistent scratch on `PhysicsWaterConstants` (or a small `BuoyancyScratch` resource) reused via clear+extend, matching the `AnimScratch` (#1372) pattern.

## Related

- The `WaterContact` allocation half of the same function (filed separately) — the suite's ECS audit surfaced that lead independently; this is the CPU-cost framing.
- #2871 (`PHYS-D6-02`, OPEN — the same function's wake gate)
- #2880 (`PHYS-D3-05`, OPEN — phase-2.5 docs)

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved — `apply_buoyancy` already defers its storage writes until the `PhysicsWorld` lock drops; a narrowed gate must not pull a `WaterContact` read inside that scope
- [ ] **SIBLING**: Same pattern checked in related files — the other WATAL phase-2.5 helpers with an "is anything moving?" early-out
- [ ] **TESTS**: A regression test pins this specific fix (a settled water cell with default `wave_amplitude` must take the fast path; a body inside the crest band must not)
