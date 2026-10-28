# GAME-D5-2026-10-27-02: FleeNotCombat (and Ambush) install from a fail-open condition pass; the #5376 fully-modelled gate covers only Find, UseWeapon and Dialogue

Labels: low, gameplay, ai, game:fnv, game:fo3, bug
Source: docs/audits/AUDIT_GAMEPLAY_2026-10-27.md

**Source**: `docs/audits/AUDIT_GAMEPLAY_2026-10-27.md` finding `GAME-D5-2026-10-27-02` (LOW)
**Location (re-mapped to current tree)**: `byroredux/src/npc_spawn/ai_package.rs` (`is_flee_not_combat` / `is_ambush` arms ~:383-401; `selectable_by_installer` ~:180-191; gate `package_conditions_fully_modeled` ~:69)

**Severity**: LOW (latent until a vanilla Flee package with an uncatalogued CTDA is shown). Dimension: NPC Spawn -> AI Package (Dim 5). Games: FO3/FNV. From `b540b104a`, `9aaf5e1cf` (2026-10-24).

## Description
`use_weapon_behavior` and `find_behavior` return `None` unless `package_conditions_fully_modeled` holds, because a fail-open pass would start an intrusive takeover on content the quest has not reached. The Flee arm builds `Self::Flee { ... }` unconditionally, and so does Ambush. FleeNotCombat takes the actor over (runs from its `PTDT` target or holds at a refuge). A quest-gated flee whose gate uses an uncatalogued function (FNV's fn 79 `GetQuestVariable` was the #5376 example) would fire on first contact. `selectable_by_installer` handles only Find, Dialogue and UseWeapon. The skill's Dim 5 text says every 10-24 runtime applies the rule; `npc-spawn-ai-packages.md` states it only for UseWeapon and Find.

## Evidence
`package_conditions_fully_modeled` is called at ai_package.rs lines ~88, 104, 139, 474 (Find / UseWeapon / Dialogue paths); the `is_flee_not_combat()` and `is_ambush()` arms contain no such call (verified at HEAD).

## Impact
An NPC flees, or waits at an ambush post, out of quest sequence. How many of the 23 FNV/FO3 Flee packages carry an unmodelled CTDA was not measured; latent until mod content triggers it.

## Related
#5376, #5499, #5722, #6051.

## Suggested Fix
Gate Flee (and Ambush, if judged intrusive) on `package_conditions_fully_modeled` in `from_package` and `selectable_by_installer`, or record in `npc-spawn-ai-packages.md` why Flee and Ambush are exempt.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling systems
- [ ] **TESTS**: A regression test pins this specific fix

