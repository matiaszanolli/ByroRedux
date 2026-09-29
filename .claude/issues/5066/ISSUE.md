# #5066 — CONC-D3-2026-09-29-01: `npc_dialogue` shadows its `LoadedCellIndex` guard instead of dropping it, so the read guard spans the whole system including the write pass

**Labels**: low,concurrency,ecs,bug

**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-09-29.md`
**Severity**: LOW
**Dimension**: ECS Lock Ordering

## Location
- `byroredux/src/systems/npc_dialogue.rs` — `npc_dialogue_selection_system_inner` and `select_topic_by_form_id`

## Description
`let Some(index) = world.try_resource::<LoadedCellIndex>() else { … }; let index = index.0.clone();` shadows the `ResourceRead` but does not drop it, so it lives to the end of the function. In the selection system it is held across `world.get::<Dead>` / `world.get::<SceneAliasCandidate>`, `running_quests_binding_entity` (three resource locks), `select_first_info` (the CTDA evaluator's whole read set), and Pass 2's `apply_selection` writes to `DialogueRegistry`, `NpcDialogueTopic` and `DialogueSurfaceState`. The function's own comment says the selection is "applied after all reads drop". The two other `LoadedCellIndex` sites copy the `Arc` and `drop` the guard (`player_body.rs`, `npc_spawn/loot_appearance.rs`).

## Evidence
```rust
let Some(index) = world.try_resource::<LoadedCellIndex>() else {
    return;
};
let index = index.0.clone();   // the guard is shadowed, not dropped
```
present at both sites.

## Impact
No cycle closes today (npc_dialogue tests pass under the detector), but it records `LoadedCellIndex → {every evaluator-read type, three dialogue writes}` in the detector graph; any future site that holds one of those and then reads `LoadedCellIndex` closes a cycle rooted here, while the ABBA lane is already red. Triggered on every player activation of an alias-bound NPC and every dialogue-UI topic click.

## Related
ECS-2026-09-29-D1-01 (covers `populate_candidates` / `running_quests_binding_entity`), ECS-2026-09-29-D5-01 (Access row), #4982.

## Suggested Fix
`let index = { let Some(r) = world.try_resource::<LoadedCellIndex>() else { return; }; r.0.clone() };` (or an explicit `drop`) at both sites.

Validated at HEAD 9fcfdc3fc: both sites in `npc_dialogue.rs` still shadow the `try_resource::<LoadedCellIndex>()` guard with `let index = index.0.clone();`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
