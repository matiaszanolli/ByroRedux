# ECS-2026-09-29-D6-01: the P4 Talk arm of populate_candidates scans every placement root and every installed quest, every frame

Labels: medium, ecs, performance, dialogue, bug

**Source reports**: `docs/audits/AUDIT_ECS_2026-09-29.md` (ECS-2026-09-29-D6-01) + `docs/audits/AUDIT_PERFORMANCE_2026-09-29.md` (PERF-D1-2026-09-29-01). The two reports describe the same defect, so it is filed once here.
**Severity**: MEDIUM
**Dimension**: ECS Dim 6 (Hot-Path Performance) / Performance (CPU Hot Paths)
**Location**: `byroredux/src/interaction.rs` (`populate_candidates`, the P4 "talkable" block); `crates/scripting/src/scene/quest_alias.rs` (`running_quests_binding_entity`)

## Description
`interaction_system` runs `select_interaction_target` → `collect_candidates` → `populate_candidates` unconditionally every frame to drive the HUD prompt. The P4 NPC "Talk" arm added by `ab31cfefe` iterates every `SceneAliasCandidate`. `stamp_quest_reference` puts one on **every placed REFR/ACHR root** in all loaded cells, not only on actors. For each entity, the arm:
- calls `world.get::<Dead>` and then `world.get::<ActorValues>` while the query guard is held. That is two TypeId lookups, two `TrackedRead`s and two RwLock reads per loaded reference per frame;
- for each living actor, calls `running_quests_binding_entity`. That call takes three resource read locks and walks every alias-bearing quest in `SceneQuestAliasRegistry` (FO4 has 1,336), with std-`HashMap` probes into `QuestStageState` and `SceneActorBindings`. It then allocates and sorts a `Vec` that the caller only tests with `is_empty()`.

The result is then reach-filtered down to a single target within `INTERACTION_REACH_BU`.

## Evidence
```rust
let talkable: Vec<EntityId> = world
    .query::<byroredux_scripting::SceneAliasCandidate>()
    .map(|identities| identities.iter()
        .filter(|(entity, _)| Some(*entity) != player)
        .filter(|(entity, _)| world.get::<Dead>(*entity).is_none())
        .filter(|(entity, _)| world.get::<ActorValues>(*entity).is_some())
        .filter(|(entity, _)| !running_quests_binding_entity(world, *entity).is_empty())
        ...
```

## Impact
Every frame, the arm costs lock traffic that grows with the number of loaded references, plus SipHash probes that grow with actors × installed quests and actors × running aliases. This runs on the `Stage::Update` exclusive head. The cost rises with grid radius and load order; a CPU bottleneck is treated as a bug. The O(n·m) `PlacementContentWithheld` retain (`f87490826`) in the same function adds to it.

## Related
- **#3475** (open, LOW): the older per-candidate lock re-acquisition on this path, over a small, fixed candidate set. Its fix reportedly landed (PERFORMANCE_FIX_STATUS_2026-09-26); the Performance report calls this a regression of it. It is filed separately because the scan is new code, grows with every loaded reference, and needs a different fix. Don't close #3475 as fixed on the assumption that it covers this.
- #5025 (ECS-D1-01): lock nesting in the same arm. One rewrite should fix both.
- Dialogue cluster: #5032, #5035, #5038, #5041, #5045, #5048, #5037, #5066.

## Suggested Fix
Build an `FxHashSet<EntityId>` of actors bound by running-quest aliases once per frame, or cache it behind `SceneActorBindings`'s own refresh/dirty edge. Intersect it with the collected `SceneAliasCandidate` ids, checking `Dead` and `ActorValues` through queries taken once outside the loop. Keep `running_quests_binding_entity` for the one-NPC activation path.

Validated at HEAD 9fcfdc3fc: the talkable block in `populate_candidates` still chains `world.get::<Dead>` / `world.get::<ActorValues>` / `running_quests_binding_entity` per `SceneAliasCandidate` inside the query guard.

## Completeness Checks
- [ ] **SIBLING**: The other `populate_candidates` arms and `activation_is_blocked` / `interaction_bound` were checked for per-entity `World::get` inside a held query
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved (and #5025's cycles aren't reintroduced)
- [ ] **TESTS**: A regression test pins this specific fix (e.g. the talkable set is computed without a per-root `World::get`, or a scaling bench)
