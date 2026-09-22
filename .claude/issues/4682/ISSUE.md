# PHYS-D2-2026-09-21-01: The explosion-recovery snapshot walks the whole body arena (fixed bodies included) on every substep

**Issue**: #4682
**Filed**: 2026-09-22 (audit-publish, AUDIT_PHYSICS_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: Step & Sync
**Location**: `crates/physics/src/world.rs:680-690` (snapshot), `:717-721` (restore scan), `:603-635` (cost-attribution comment)

## Description
`613ad3124` added a per-substep snapshot of "all dynamics" so a freshly activated ragdoll's first solve is recoverable. It does so by iterating `self.bodies.iter()`: every arena slot, fixed and kinematic bodies included. That happens once per substep, before `pipeline.step`. Fixed bodies can never need recovery, so on a big world almost all of the walk is wasted.

## Evidence
Snapshot code at `world.rs:680-690` filters on `RigidBodyType::Dynamic && body_state_is_finite`. Timed in a standalone release proxy (40 iterations):

| World | Snapshot per substep | `pipeline.step` |
|---|---|---|
| 30 k fixed + 1 awake | 0.43–0.45 ms | 0.04 ms |
| 30 k fixed + 300 asleep + 1 awake | 0.46–0.58 ms | 0.26–0.28 ms |
| 95 k fixed (Cydonia `rapier_bodies=95,223`) | 1.77 ms | 0.62 ms |
| 1.5 k (interior) | 0.005 ms | — |

Walking only the active set instead costs 0.2–0.7 µs at every size.

## Impact
On a radius-12-class exterior with one awake body, the recovery costs about 10× the solver it protects. With `SUBSTEP_TIME_BUDGET = PHYSICS_DT`, five catch-up substeps fit on Cydonia, which is about 9 ms of snapshot alone. No correctness impact.

## Related
PHYS-D6-2026-09-21-02 (#4685); `docs/engine/playable-vertical-slice.md:1383-1391`.

## Suggested Fix
Keep an index of dynamic-body handles, maintained on insert/remove/`set_motion_type`, and snapshot from that. Re-measure and keep both recovery tests.
