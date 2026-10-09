# #5413: CHAR-2026-10-08-D4-02: The #4415 racial spell pair always targets the player, but the player never receives `RaceSpells`, so `AddRaceSpells`/`RemoveRaceSpells` are silent no-ops in production

**Labels**: low,character,scripting,bug,game:skyrim
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5413

**Source**: `docs/audits/AUDIT_CHARACTER_2026-10-08.md` — `CHAR-2026-10-08-D4-02` (HEAD `00f580e09`)

- **Severity**: LOW. On vanilla MQ101 the visible state is the same today: the player's `SpellList` already holds the racial spells from the template, so the `AddRaceSpells` call would have been idempotent anyway. It becomes wrong for any `RemoveRaceSpells`, or once a race change lands. This is the "writers vs stamps" class of #4458 (MEDIUM), graded lower because no value changes yet.
- **Dimension**: Population Boundary (writers vs stamps)
- **Game**: skyrim (MQ101); all families by construction
- **Location**:
  - `crates/scripting/src/translate/effects.rs:1444-1468`: `lower_race_spells` always emits `ActorRef::Player`.
  - `crates/scripting/src/magic.rs:213-240`: `add_race_spells`/`remove_race_spells` read `world.get::<RaceSpells>(actor)…unwrap_or_default()`.
  - `byroredux/src/inventory.rs:742`: `attach_to_player` inserts `SpellList(character.spells)` but no `RaceSpells`.
  - `byroredux/src/npc_spawn/resumable/mod.rs:369-371`: `player_body` placement roots return before `stamp_spell_list`, the only `RaceSpells` insert (`npc_spawn.rs:156`).
- **Status**: NEW. It was introduced by `9813af435` (#4415 scope 4, closed).
- **Description**:
  - `RaceSpells` is stamped only by `stamp_spell_list`, which runs for NPC placement roots and is skipped for the player body.
  - The player's own seed (`build_player_character_template` → `attach_to_player`) resolves `resolve_actor_spells` (own + racial, merged) and never computes the racial subset separately.
  - The lowering sends every `AddRaceSpells()`/`RemoveRaceSpells()` to the player. Both functions find no `RaceSpells` on the player, return `false`, and the fragment arm logs only at `debug!` ("changed nothing").
  - `race_spell_pair_reapplies_and_clears_only_the_racial_set` inserts `RaceSpells` by hand on a bare entity, which hides the gap. That is the #4458 pattern.
- **Evidence**:
  - `grep -rn RaceSpells byroredux/src` finds a single insert, at `npc_spawn.rs:156`, inside `stamp_spell_list`. `spawn_placement_root` returns at `if player_body { return (placement_root, resolved); }` before that call.
  - ROADMAP ("MQ101 residue"): "`Fragment_11`'s `AddRaceSpells()` now lowers onto the player's RACE `SPLO` set (#4415, `9813af435`); no live MQ101 replay has re-checked it."
- **Impact**:
  - `RemoveRaceSpells` on the player never removes racial abilities.
  - `AddRaceSpells` never re-applies them after a removal.
  - The #4415 closure claim and the ROADMAP line describe behaviour that production never performs.
- **Related**: #4415 (closed), #4458 (the same writers-vs-stamps class). Cross-audit: `/audit-scripting` owns the lowering.
- **Suggested Fix**:
  1. In `build_player_character_template`, also compute `resolve_racial_spells(&resolved, index)`.
  2. Have `attach_to_player` insert `RaceSpells` beside `SpellList`.
  3. Add a test that runs `AddRaceSpells`/`RemoveRaceSpells` against a player built by `attach_to_player`, not one with hand-inserted components.

## Completeness Checks
- [ ] **SIBLING**: other player-only components stamped only by stamp_spell_list checked
- [ ] **TESTS**: A regression test pins this specific fix
