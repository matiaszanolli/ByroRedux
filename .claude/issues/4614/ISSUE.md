# PERF-D1-2026-09-21-06: `move_character` heap-allocates the capsule shape on every call; M42.10 made that per walking NPC per tick

**Labels**: bug, low, performance, physics

Filed via /audit-publish from docs/audits/AUDIT_PERFORMANCE_2026-09-21.md.

**Severity**: LOW · **Dimension**: 1 — CPU Hot Paths (the physics code is owned by `/audit-physics`; the multiplier is in the NPC locomotion caller)
**Location**:
- `crates/physics/src/world.rs:1325-1328`: `SharedShape::capsule_y(..)`, an `Arc<dyn Shape>`, built in `PhysicsWorld::move_character` and passed as `shape.as_ref()` to `KinematicCharacterController::move_shape` at `:1356-1366`
- Callers:
  - `byroredux/src/systems/locomotion.rs:175` (`step_toward_detailed`). The KCC-stepping walkers reach it through `step_toward` / `step_along_waypoints` / `step_toward_detailed` (wander, travel, follow, escort, …), and so does `combat_ai`.
  - `byroredux/src/systems/character.rs:458` (the player).

**Status**: NEW. The per-NPC multiplier arrived with `913fd39d8` (M42.10, 2026-09-18), which walks ambient routines through the KCC by default.
**Verified against**: HEAD `73aaed7b9`; the code is identical to the audited `f97775ca8`.

## Description

`move_character` builds `SharedShape::capsule_y(half_height, radius)`, a heap-allocated `Arc<dyn Shape>`, on every call. It only needs `&dyn Shape` for `move_shape`. Since M42.10, that is one allocation and one free per walking NPC per tick, in every loaded cell, plus the player's.

The ground probe `cast_capsule_down_surface_and_normal` (`world.rs:1105`) and `capsule_overlaps_solid` (`:1146`) also build the same `SharedShape` on each call.

## Impact

A small per-call cost (one `Arc` alloc and free), multiplied by the number of walking NPCs per tick.

## Related

- #4134 (open, LOW): the same `capsule_y(.. .max(1e-3), ..)` constructor at this site and its siblings, filed for the missing `clamp_shape_extent` ceiling. `docs/audits/AUDIT_PHYSICS_2026-09-21.md` re-confirms #4134 at three sites (adding `capsule_overlaps_solid`) and cites this finding.

## Suggested Fix

Pass a stack `Capsule::new_y(half_height, radius)` as `&dyn Shape` to `move_shape`, and do the same at the two probe sites. Doing it together with #4134's clamp work touches all three sites once.

Source: docs/audits/AUDIT_PERFORMANCE_2026-09-21.md (PERF-D1-2026-09-21-06)

## Completeness Checks
- [ ] **SIBLING**: `cast_capsule_down_surface_and_normal` and `capsule_overlaps_solid` are converted in the same change (and checked against #4134's clamp)
- [ ] **TESTS**: The existing `move_character` tests in `crates/physics/src/world.rs` still pass, and the sweep result is unchanged for the stack-shape form

