# PHYS-D6-05

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2888

---

Found by `/audit-physics` Dimension 6 (WATAL Physics Sink — canonical consistency). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: LOW · **Status**: NEW
**Location**: `crates/physics/src/water.rs:376-388` (`surfaces.iter().find(..)`) vs `byroredux/src/systems/water.rs:136-143` (smallest-depth wins)

## Trigger Conditions
Two `WaterPlane` + `WaterVolume` entities whose XZ extents overlap at a body's position — a cell-transition frame with both the outgoing and incoming cell's planes live, an interior pool inside an exterior tile, or a future multi-body / shoreline-fit spawn (an explicit WATAL §2 "Spawn — functional, coarse" gap).

## Description
The physics sink resolves a body's containing surface with `Vec::find` — i.e. whichever `WaterPlane` entity happened to be iterated first by `collect_water_surfaces` (`water.rs:197-216`, ECS storage order, **non-deterministic across cell loads**).

`submersion_system` resolves the camera's plane by explicitly picking the *closest* one (*"for nested / overlapping water volumes, the one closest to the camera controls the underwater FX"*, `byroredux/src/systems/water.rs:136-139`).

Two consumers of the same canonical state use different tie-break rules.

## Evidence
```rust
// crates/physics/src/water.rs:376-388 (physics)   // byroredux/src/systems/water.rs:139-143 (camera)
surfaces.iter().find(|s| { ... }).map(...)         match best { Some((prev,_)) if depth < prev => ... }
```

## Impact
With overlapping planes, a body and the camera at the same spot can be attributed to different water bodies — different `surface_y` (so a different `submerged_fraction` and lift), different `WaterMaterial`, different `WaterFlow`. Today real content rarely overlaps (exterior tiles are disjoint 4096-unit squares, one plane per cell), but the cell-transition case produces exactly this.

## Suggested Fix
Use the same nearest-surface rule at both ends — pick the candidate with the smallest `surface_y - center_y` among matches — and factor the predicate into one shared helper so the two ends cannot drift again.

## Related
- WATAL §2 spawn gaps ("one plane per cell (can't represent multiple bodies at different heights)")
