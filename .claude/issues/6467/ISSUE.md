# GAME-D4-2026-10-27-01: A script-started `UseWeapon` volley keeps firing from a knocked-down, fleeing, bleeding-out, unconscious or AI-disabled actor

Labels: medium, gameplay, combat, game:fnv, game:fo3, bug
Source: docs/audits/AUDIT_GAMEPLAY_2026-10-27.md

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-27.md` finding `GAME-D4-2026-10-27-01` (MEDIUM)
**Location (re-mapped to current tree)**: `byroredux/src/systems/script_requests.rs` (`use_weapon_system`, the `alive` closure ~:62, run :40-145); `byroredux/src/npc_spawn/ai_package.rs` (~:845-850, `clear_ambient_behavior`); `byroredux/src/systems/knockdown.rs` (~:128-141)

**Severity**: MEDIUM. Dimension: Combat & Death (Dim 4); M47.3 script-driven actor states. Games: FO3/FNV. `use_weapon_system` from `07bd22a6b` (2026-10-18).

**Trigger**: an ObScript `UseWeapon` volley is in progress, then the shooter receives `PushActorAway`, `ForceFlee`, an essential-actor bleedout, `SetUnconscious` or `SetActorsAI 0`.

## Description
The skill's rule is that every AI consumer honours `AiSuspended`, unconscious, `SetActorsAI 0` and bleedout via `byroredux_scripting::collect_unconscious` (scalar form `is_incapacitated`). `use_weapon_system` checks only `Dead` (`alive = ActorVitals absent || !Dead`).
- `release_actor_for_ragdoll` and `flee_system` tear the actor down through `clear_ambient_behavior`, which removes `UseWeaponRun` only when `UseWeaponPackageStarted` is present - deliberate, so a scripted volley survives a package handover, but it also means it survives incapacitation.
- Each frame the run rewrites the actor root's `Transform.rotation` toward the target. It is registered at `update.rs:723`, after `flee_system` at `:720`, so it overrides the flee facing.
- It still pushes `PendingProjectileLaunches` / `PendingTrapHits` shots.

## Evidence
`if !alive(actor) || !alive(target) || run.request.shots == 0 { end_run(..) }` is the only state gate (verified at HEAD); there is no `is_incapacitated` call anywhere in `script_requests.rs`.

## Impact
A ragdolled or bleeding-out actor keeps shooting; a `ForceFlee`d actor runs away facing its target and firing. For Repeat Fire volleys this lasts until `PATIENCE_SECS` or the shot count runs out. The number of vanilla scripts pairing `UseWeapon` with these states was not measured.

## Related
GAME-D4-2026-10-21-01 / #6230 (knockdown teardown), #6287 (`is_incapacitated`), PHYS-D3-2026-10-27-01 (PushActorAway never pushes; independent).

## Suggested Fix
Skip or end the run while `byroredux_scripting::is_incapacitated(world, actor)` holds. Ending matches vanilla's "the package is interrupted"; skipping would let it resume after a get-up. Add a knockdown/flee case to the `use_weapon_system` tests.

## Completeness Checks
- [ ] **SIBLING**: Other AI consumers of `UseWeaponRun`/script volleys honour `is_incapacitated`
- [ ] **TESTS**: A regression test pins this specific fix

