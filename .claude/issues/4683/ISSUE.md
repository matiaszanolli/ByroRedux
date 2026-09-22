# PHYS-D3-2026-09-21-01: Solver-explosion recovery is invisible to every ragdoll stability gate — and the FO3 P2 pass already depends on it

**Issue**: #4683
**Filed**: 2026-09-22 (audit-publish, AUDIT_PHYSICS_2026-09-21.md)

**Severity**: MEDIUM
**Dimension**: Ragdoll & Constraint Seam
**Location**: `byroredux/src/ragdoll_installed_tests.rs:215-236`; `crates/physics/src/world.rs:717-732` (recovery, `log::error!` only), `:258-263` (multibody detach); `byroredux/src/commands/ragdoll_status.rs:41-81`; `docs/smoke-tests/p2-melee-core.sh:326-329`, `:414-436`

## Description
- Data tests. The six FO3 data tests assert after each `physics.step()` that every body is finite and within 512 BU of placement. `step()` now reverts any body that went non-finite or jumped, sleeps it and detaches its multibody before returning — a solver explosion satisfies both assertions by construction.
- Runtime command. `ragdoll.status` reports only `bodies/live/complete/finite/max_distance/max_speed`, all read after recovery. No joint, articulation or recovery field.
- Smoke gate. `p2-melee-core.sh` greps `complete=true finite=true` and bounds `max_distance`.
- Only signal. The recovery reports itself through one `log::error!`. No test installs a logger and no smoke script scans for it. No counter exposes it.

## Evidence
`docs/engine/playable-vertical-slice.md:1383-1399`: the FO3 copied-save restore's first solve jumps the root to about 1e12 BU; the recovery "logs one recovery"; "the full FO3 P2 gate passed". A recovery calls `remove_multibody_articulations` (`world.rs:262`), so that PASS certifies a corpse with no joints. No issue tracks the first-solve root cause.

## Impact
A regression in the real stability fixes (×12 ragdoll iterations, seed composition, joint seeding) now ships green through unit tests, data tests and the smoke gate. In game the regression shows as a corpse whose limbs silently come apart. Slow divergence is still caught by the distance bounds; the fast NaN/huge-jump class (Rapier multibody signature, #2337/#1534) is not.

## Related
#2337, #1534, #3968; PHYS-D2-2026-09-21-02 (#4687).

## Suggested Fix
1. Count recoveries on `PhysicsWorld` and surface in `phys.stats`/`ragdoll.status`, with live multibody/joint count.
2. Assert zero recoveries in the installed tests and the p2 gate.
3. File the FO3 restore first-solve jump as its own root-cause issue.
