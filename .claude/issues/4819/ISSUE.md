# #4819 — GAME-D5-2026-09-24-02: `SpellList` is not part of the parked `ReferenceState` — after an NPC's cell reloads, its spell membership and its ability actor-value deltas disagree

**Labels**: medium,gameplay,save-load,scripting,bug
**Filed from**: docs/audits/AUDIT_GAMEPLAY_2026-09-24.md

From `docs/audits/AUDIT_GAMEPLAY_2026-09-24.md` (HEAD `aabd99a05`).

- **Severity**: MEDIUM
- **Dimension**: 5 / 2 — Persistence (also Dim 3, actor-value layers)
- **Location**:
  - `byroredux/src/cell_loader/reference_state.rs:26-39`: `ReferenceState` has no spell field.
  - `reference_state.rs:96-166`: `capture` stores `ActorValues` verbatim.
  - `reference_state.rs:206-214`: restore.
  - `byroredux/src/npc_spawn.rs:128-150`: `stamp_spell_list` re-derives the list from SPLO.
  - `crates/scripting/src/magic.rs:152-185`: `add_spell`/`remove_spell` gate on list membership.
- **Status**: NEW. Introduced by cd4fc019a (Refs #4415); found independently by two audit legs.
- **Description**:
  - A fragment's `AddSpell` or `RemoveSpell` on an NPC changes both `SpellList` and the permanent AV modifier.
  - On eviction, `capture` keeps the modified `ActorValues` but not `SpellList`. On return, the spawn re-stamps the authored list and `restore` puts back the modified values. After that:
    - an added ability is missing from the list while its bonus remains, so `RemoveSpell` returns false and the bonus becomes permanent;
    - a second `AddSpell` applies the bonus twice;
    - a removed authored ability desyncs the other way.
  - A save taken while the cell is parked loses the list too.
  - Save/load of a *resident* actor is consistent, because both components overlay verbatim.
- **Impact**: silent, permanent AV drift on NPCs whose abilities a quest changes. This is the "load silently drops gameplay state" class, bounded by how often fragments add or remove NPC spells.
- **Trigger**: any fragment `AddSpell`/`RemoveSpell` of a constant spell on an NPC, then leave and re-enter its cell.
- **Related**: #4415, #4465 (the no-`serde(default)` / FORMAT_MAJOR rule); GAME-D7-2026-09-24-03.
- **Suggested Fix**: add `spells: Option<Vec<u32>>` to `ReferenceState`, with a FORMAT_MAJOR bump and no `serde(default)`. Restore it together with `actor_values`, after `stamp_spell_list`.

### LOW

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (both NPC spawn paths, sibling interaction kinds, other parked `ReferenceState` facts)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (no guard held across `world.get`/physics queries)
- [ ] **SAVE**: If a saved shape changes, `FORMAT_MAJOR` is bumped and no `serde(default)` is added (#4465)
- [ ] **TESTS**: A regression test pins this specific fix
