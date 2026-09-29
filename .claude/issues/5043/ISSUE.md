# #5043 — GAME-D2-2026-09-29-02: Dialogue selection skips #4701's `player_can_act` gate and does not refuse a combatant NPC

**Labels**: low,gameplay,dialogue,quests,bug

**Source report**: `docs/audits/AUDIT_GAMEPLAY_2026-09-29.md`
**Severity**: LOW
**Dimension**: 2 — NPC dialogue

## Location
- `byroredux/src/systems/npc_dialogue.rs` — `npc_dialogue_selection_system_inner` and `select_topic_by_form_id`
- `byroredux/src/interaction.rs` — the Talk candidate arm

## Description
`player_can_act` (`systems/character.rs`) fronts combat, interaction and inventory entry points, but the two dialogue entry points never call it. A scripted player activation (`script.activate`) or a topic click on an already-open surface still selects and advances dialogue for a Dead player. Neither the Talk arm nor the selection excludes an NPC carrying `AiCombatState`, so an alias-bound NPC fighting the player still shows a "Talk" prompt, and activating it opens the surface mid-fight (the simulation is deliberately not paused per `p4-quest-fixture.md`).

## Evidence
`grep -n 'player_can_act\|AiCombatState' byroredux/src/systems/npc_dialogue.rs` → no hits; `grep -n AiCombatState byroredux/src/interaction.rs` → no hits.

## Impact
Dead player can still drive dialogue; combatant NPCs offer Talk mid-fight.

## Related
#4701; GAME-D2-2026-09-29-01 (#5037); GAME-D7-2026-09-29-01 (#5027).

## Suggested Fix
Gate both entry points on `player_can_act`; skip NPCs with `AiCombatState` in the Talk arm and in the selection.

Validated at HEAD 9fcfdc3fc: no `player_can_act` or `AiCombatState` reference in `npc_dialogue.rs`; none in `interaction.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
