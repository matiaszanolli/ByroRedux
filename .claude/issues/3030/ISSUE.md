# ECS-2026-08-16-02: nothing checks Dead before re-installing an AI behavior

**Issue**: #3030
**Severity**: MEDIUM
**Dimension**: 7 — Component Lifecycles
**Labels**: `medium,ecs,bug`
**Source report**: `docs/audits/AUDIT_ECS_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_ECS_2026-08-16.md` (Dimension 7 — Component Lifecycles, P2 gameplay slice).

**Location**: `byroredux/src/npc_spawn/ai_package.rs`:430-500 · `byroredux/src/combat.rs`:215-238

## Description

Nothing checks the `Dead` marker before re-installing an AI behaviour, so a corpse can be re-animated at the next package boundary.

## Evidence

Re-verified 2026-08-17: the behaviour-install range (`ai_package.rs`:430-500) contains **zero** references to `Dead`.

`combat.rs`:215-238 inserts `Dead` and tears the behaviour down (`disable_actor_ai`), but the install path has no corresponding guard — the teardown and the re-install are unaware of each other.

## Impact

An actor killed by the P2 combat slice can have a Sandbox/Wander/Travel/Follow/Escort/Guard/Patrol behaviour re-installed, at which point the corpse walks, sits or patrols again.

Package selection is spawn-time-only in the current v0 scope, which limits how often this fires today — but the guard's absence is what makes it reachable at all, and any move toward per-frame package re-evaluation makes it routine.

## Suggested Fix

Gate behaviour installation on the absence of `Dead` (and any future incapacitation marker), in the install path itself rather than at each call site — a single check where the behaviour is chosen.

## Related

- ECS-2026-08-16-01 (#3029) — the same kill path's seat-reservation leak
- #3022 (SAVE-D1-2026-08-16-01) — the save side of the same "death is a removal" problem

## Completeness Checks
- [ ] **SINGLE-GUARD**: The `Dead` check lives in the install path, not duplicated per call site
- [ ] **SIBLING**: All seven M42 procedure runtimes covered by the one guard
- [ ] **FUTURE-PROOF**: The guard still holds if package selection becomes per-frame
- [ ] **TESTS**: A regression test kills an actor and asserts no behaviour re-installs

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3030 --json state` when live state is needed.*
