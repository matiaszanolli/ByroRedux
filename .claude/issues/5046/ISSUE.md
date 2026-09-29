# #5046 — GAME-D4-2026-09-29-01: A scripted `StartCombat` on an actor already ambient-engaged with the same target keeps the ambient marker, so the scripted fight auto-drops

**Labels**: low,gameplay,combat,ai,bug

**Source report**: `docs/audits/AUDIT_GAMEPLAY_2026-09-29.md`
**Severity**: LOW
**Dimension**: 4 — Combat

## Location
- `crates/scripting/src/fragment/effects.rs` — `Effect::StartCombat` arm (inserts `AiCombatState` only)
- `byroredux/src/systems/faction_hostility.rs` — `disengage_lost_contact`

## Description
`Effect::StartCombat` overwrites `AiCombatState` only; it cannot see `AmbientEngagement` (a binary-crate type). `disengage_lost_contact` treats any combat whose current target equals `engagement.target` as ambient and drops it after `DISENGAGE_GRACE_SECS` out of contact; the out-of-contact timer is not reset either. The #4816 contract says "scripted combat is never dropped"; `scripted_combat_never_disengages_on_lost_contact` only covers a *re-targeted* scripted combat.

## Evidence
`disengage_lost_contact`: `if current != Some(engagement.target) { updates.push((entity, None)); continue; }` — a same-target scripted combat falls through to the lost-contact timer.

## Impact
A quest fragment running `StartCombat(player)` on an NPC that `faction_hostility` already armed against the player (hostile-faction ambush actor) disengages after the player breaks sight/range for 10 s.

## Related
#4816 (closed; this is an edge case).

## Suggested Fix
Have `StartCombat` clear the ambient marker, e.g. through a scripting-side `ScriptedCombat` tag that `faction_hostility` honours. Add a same-target test.

Validated at HEAD 9fcfdc3fc: `Effect::StartCombat` still only inserts `AiCombatState`; `disengage_lost_contact` keeps same-target engagements on the grace timer.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
