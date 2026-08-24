# 3268: PHYS-D6-2026-08-24-01: dynamic body resting in a WaterCurrentVolume with no overlapping WaterPlane never wakes to receive the current force

**Severity**: MEDIUM · **Report**: `docs/audits/AUDIT_PHYSICS_2026-08-24.md` (PHYS-D6-2026-08-24-01)

## Description

`apply_buoyancy_with_scratch`'s per-body loop has exactly one wake site, and it lives entirely inside the *surface* branch — a dry→wet transition wakes the body once. The current-volume branch that runs afterward is gated on `!b.is_sleeping()` and, by design, calls `add_force` with `wake_up = false` (correct for a per-frame re-derived force). But if the body never passes through the surface branch at all — because no `WaterPlane` overlaps its position — nothing ever wakes it, so the `!b.is_sleeping()` gate on the current branch is permanently false and the authored flow is never applied.

## Location
- `crates/physics/src/water.rs:920-937` (current-flow branch)
- `crates/physics/src/water.rs:825-831` (the *only* wake site, gated to the surface branch)
- `crates/physics/src/water.rs:1847-1990` (`current_volume_without_a_water_plane_does_not_wind_up_user_force`, whose own comment names this exact gap and works around it)

## Trigger Conditions

An authored `XWCU`/current marker whose box does not overlap any `WaterPlane` in XZ, containing a dynamic body that is asleep and receives no other disturbance (the common case for streamed-in clutter — spawn-asleep is the EXTERIOR-FREEZE default).

## Impact

An XWCU/current marker authored where it does not spatially coincide with a `WaterPlane` silently fails to move debris resting in it. No crash, no visible artifact beyond "the current doesn't do anything" for that placement. Same root cause family as the fixed PHYS-D6-2026-08-20-01 sibling.

## Related

PHYS-D6-2026-08-20-01 (CLOSED, opposite-signed defect in same path), `watal.md` §7 Phase 2.

## Suggested Fix

Wake a body once on entering a current volume from rest, mirroring the surface branch's one-shot pattern — track a `prior_in_current` bit and call `b.wake_up(true)` the first frame `current_flow.is_some()` is true for a sleeping body.

## Completeness Checks
- [ ] **SIBLING**: One-shot wake pattern matches the surface branch's `prior_wet` latch shape
- [ ] **TESTS**: A sibling test removing the manual wake and asserting the body accelerates from rest
