# #4822 — GAME-D7-2026-09-24-03: A script's `AddSpell` silently does nothing on an actor spawned with no spells

**Labels**: low,gameplay,scripting,bug
**Filed from**: docs/audits/AUDIT_GAMEPLAY_2026-09-24.md

From `docs/audits/AUDIT_GAMEPLAY_2026-09-24.md` (HEAD `aabd99a05`).

- **Severity**: LOW
- **Dimension**: 7 — Tested but unwired (#4415)
- **Location**:
  - `byroredux/src/npc_spawn.rs:128-146`: `stamp_spell_list` returns at `:135` without inserting a component.
  - `crates/scripting/src/magic.rs:152-166`.
  - `crates/scripting/src/fragment/effects.rs:709-716`: the `bool` result is dropped and nothing is logged.
- **Status**: NEW
- **Description**: `add_spell` returns false when the actor has no `SpellList`. NPCs and creatures whose own and racial SPLO are empty never get one. The fragment layer accepted the effect, which breaks the translator's decline-on-unmodeled contract, and then drops it with no log. The player always gets a `SpellList`, possibly empty (`inventory.rs:659`).
- **Impact**: quest abilities and diseases added to spell-less actors (mostly creatures) have no effect and are not saved.
- **Suggested Fix**: always insert `SpellList` at spawn (an empty `Vec` round-trips fine), or have `add_spell` insert it. Log when a `false` result is dropped.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both NPC spawn paths, sibling interaction kinds, other parked `ReferenceState` facts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (no guard held across `world.get`/physics queries)
- [ ] **SAVE**: If a saved shape changes, `FORMAT_MAJOR` is bumped and no `serde(default)` is added (#4465)
- [ ] **TESTS**: A regression test pins this specific fix
