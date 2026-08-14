# PHYS-D6-04

Filed: 2026-08-13 · Source: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2887

---

Found by `/audit-physics` Dimension 6 (WATAL Physics Sink). Report: `docs/audits/AUDIT_PHYSICS_2026-08-13.md`.

**Severity**: LOW · **Status**: NEW
**Location**: `crates/physics/src/water.rs:360` + `:437`, vs `crates/core/src/ecs/components/water.rs:287-289`

## Trigger Conditions
Any body whose collider AABB centre != body origin — compound bhk shapes, ragdoll bones, offset trimeshes. Not visible on the tests' centred balls.

## Description
`let center_y = pos.y;` where `pos = *body.translation()` — the rigid body's **origin**, not the centre of the collider AABB. `WaterContact::depth` is documented as *"Surface Y minus the body's **centre** Y"*.

The AABB is already in hand two lines later (`aabb.mins.y` / `aabb.maxs.y`, `water.rs:374-375`), so the correct value costs nothing. Compare `submerged_fraction`, which correctly uses the AABB span.

## Evidence
`water.rs:360` `let center_y = pos.y;` -> `water.rs:437` `depth: s.surface_y - center_y`.

## Impact
Wrong `depth` for every body whose collider is offset from its body origin — which is the norm for the bhk import path (`collision_shape_to_parts` attaches each part at its own local isometry) and for ragdoll bones.

Cosmetic today (only `water.contacts` reads it, `byroredux/src/commands/water.rs:191`), but `depth` is the field the **not-yet-built drowning / underwater-FX gate is documented to consume**, so the error would be inherited rather than discovered.

## Suggested Fix
`let center_y = 0.5 * (min_y + max_y);` inside the `Some` arm — or amend the component doc to say "body origin" and keep the cheap value deliberately.

## Related
- PHYS-D6-01 (same `Some(..)` arm)
