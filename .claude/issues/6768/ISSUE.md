# CHAR-2026-11-07-L-09: ResolvedNpc::level() (#4232) is adopted by gear and spells only; CharacterLevel, the Health derivation and the player templates still read the unbackfilled level

**Issue**: #6768
**Labels**: bug, low, game:fnv, game:fo3, character
**Source audit**: `docs/audits/AUDIT_CHARACTER_2026-11-07.md` (finding L-09, Dim 4). Game: FNV (FO3 by the same data rule).

## Description
For the ~30 of 3,816 FNV `NPC_` records that author level 0 and are reached through an LVLN pick, #4232 deliberately lets only the list expansions use the pick's entry level (`ResolvedNpc::level()`). `CharacterLevel` (which feeds `GetLevel`, the leveling model, kill-XP victim level and the save), the NPC Health curve and the player templates still use `effective_actor_level(stats)`, which is 0. So gear is expanded at level N while the actor reports level 0. No doc, including the `effective_actor_level` docblock, mentions `ResolvedNpc::level()`, and a comment now false for exactly these actors claims one shared gate.

## Location
Backfilled readers `byroredux/src/npc_spawn/equip.rs:229`, `:1032`, `:1048`; unbackfilled `byroredux/src/npc_spawn/stamps.rs:218`, `crates/plugin/src/esm/records/actor_value_derive.rs:663`, `byroredux/src/inventory/catalog.rs:362`, `:646`; stale comment `byroredux/src/npc_spawn/equip.rs:219` ("same gate as `stamp_character_components`"); docblock `crates/plugin/src/esm/records/actor/mod.rs:150-168`.

## Evidence
`grep -rn 'effective_actor_level(' byroredux crates` (production) lists the unbackfilled readers; `.level()` is read only inside `crates/plugin/src/equip.rs` and `npc_spawn/equip.rs`. The Health term is sourced (`charal-fnv-fo3-ruleset.md:94`, `:113-114`: `base + 5·(END-1) + 5·(Level-1)`); which level a level-0 LVLN-picked terminal should carry for stats is unsourced, so no value is asserted.

## Impact
At most a few HP, a level-0 `GetLevel` and kill XP for about 30 FNV actors; the likelier cost is a future reader choosing between two "actor level" helpers with no doc to say which.

## Related
#4232 (closed), #3081, #3171, #2955.

## Suggested Fix
Either route `stamp_character_components` and the derivation through `resolved.level()`, or document the two-level contract on `effective_actor_level` and `ResolvedNpc::level()` and correct the `equip.rs:219` comment. Pin whichever is chosen with a test reading both.

## Completeness Checks
- [ ] **SIBLING**: every `effective_actor_level(` production reader reviewed
- [ ] **TESTS**: a test reads both level helpers for a level-0 LVLN-picked actor
