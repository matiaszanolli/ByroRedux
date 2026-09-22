# CHAR-2026-09-21-D4-03: Player is half-populated — CharacterLevel/Background withheld on a justification the #4458 stamp falsified, deferred to two closed issues

**Severity**: LOW
**Dimension**: Population Boundary
**Game**: FO3 / FNV / FO4 / Skyrim

## Description

The note updated by `eb3784309` keeps `CharacterLevel` and `Background` off the player on two grounds, and neither holds:
- It says "`Background` has no honest value until the player has a real race/class". Yet the same commit derives the player's `ActorValues` *from* the Player record's class, race and level 1. That is exactly the provenance `Background` records.
- It defers population to "#3004 / #2986". Both issues are CLOSED and concern NPC AVIF prefixes / NPC Health derivation. No open issue tracks player level/background.

Meanwhile the absent level reads differently depending on the consumer:
- 0 in `GetLevel`, in `GetActorValue`'s derived rows and in `GetXPForNextLevel`. For FNV that last one gives `150*0+50` = 50 XP, versus 200 at L1.
- 1 in `melee_damage_charal_bonus` and in container leveled loot.

All of this is for an actor whose Health was derived at level 1.

## Evidence

Verified at HEAD `ee6d3fb39`. `byroredux/src/scene.rs`: "`CharacterLevel` and `Background` remain deliberately absent ... `Background` has no honest value until the player has a real race/class. Populating those is CHARAL work (#3004 / #2986)". `gh issue view`: #3004 CLOSED ("auto-calc derivation has no Health term"), #2986 CLOSED ("AVIF EditorIDs are AV-prefixed").

`GetLevel` (`crates/scripting/src/condition.rs`): `world.get::<CharacterLevel>(entity).map_or(0.0, ...)`. `GetXPForNextLevel`: `world.get::<CharacterLevel>(entity).map_or(0, ...)` feeding `rs.leveling.xp_to_next(level)`. `byroredux/src/combat.rs`'s `melee_damage_charal_bonus`: `world.get::<CharacterLevel>(aggressor).map_or(1, ...)`. `byroredux/src/cell_loader/references/attach.rs`: player level `.map_or(1, |level| level.level.max(1))`.

## Impact

- Player-level CTDA gates read 0.
- XP-to-next is computed for level 0.
- The deferral has no live tracker, so it cannot be scheduled.

## Related

CHAR-2026-09-21-D4-01, #4458, #3158; `/audit-gameplay` noted the level-1 loot bootstrap as documented policy (unfiled).

## Suggested Fix

Stamp `CharacterLevel { level: effective_actor_level(player) }` and `Background` from the resolved Player record beside `ActorValues`. Otherwise, re-point the deferral at an open issue with a reason that still holds.

Source: docs/audits/AUDIT_CHARACTER_2026-09-21.md (CHAR-2026-09-21-D4-03)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other per-game tables, other doc sites making the same claim)
- [ ] **TESTS**: A regression test pins this specific fix
