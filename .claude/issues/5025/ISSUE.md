# #5025 — ECS-2026-09-29-D1-01: ab31cfefe nests guards in populate_candidates and running_quests_binding_entity, closing two lock-order cycles — ABBA lane red again (6 panics)

**Labels**: high, bug, ecs, concurrency, dialogue

**Source**: `docs/audits/AUDIT_ECS_2026-09-29.md` — finding `ECS-2026-09-29-D1-01`

**Severity**: HIGH. This follows the grading of #3819 and #4982: the lock-order CI job is red at HEAD. ECS deadlock potential has a floor of HIGH.

**Dimension**: 1 — Lock Ordering

**Location**:
- `byroredux/src/interaction.rs:1256-1292` (the `populate_candidates` Npc arm)
- `crates/scripting/src/scene/quest_alias.rs:916-941` (`running_quests_binding_entity`)

**Status in report**: NEW. Related: #4546 (closed), an earlier and different cycle that the same five walk_anim tests observed.

## Description

`ab31cfefe` adds two production lock nests.
1. **`populate_candidates`**: it holds the `SceneAliasCandidate` query guard across the whole filter chain. For each entity, that chain calls `world.get::<Dead>`, `world.get::<ActorValues>`, and `running_quests_binding_entity`. This records the edges `SceneAliasCandidate → {Dead, ActorValues, SceneQuestAliasRegistry, SceneActorBindings, QuestStageState}`.
2. **`running_quests_binding_entity`**: it holds `SceneQuestAliasRegistry` while taking `SceneActorBindings`, and holds both while taking `QuestStageState`.

Together with pre-existing edges, these close two cycles:
- **Cycle B** (binary, 5 panics): `ActorCinematicState → HorseTetherState → SceneAliasCandidate → Dead → ActorCinematicState`. The minimal failing set is the walk_anim tests plus `systems::cinematic::tests::tethered_horse_advances_through_authored_linked_reference_route`, `save_io::round_trip_tests::cinematic_trio_survives_save_load_round_trip` and `interaction::tests::dead_player_neither_activates_nor_loots`. The edges come from:

  | Edge | Source |
  |---|---|
  | `Dead → ActorCinematicState` | production: `npc_walk_animation_system_inner` hoists seven read guards (`walk_anim.rs:116-126`; unchanged since 09-21) |
  | `ActorCinematicState → HorseTetherState` | test hygiene (D1-02) |
  | `HorseTetherState → SceneAliasCandidate` | production: `cinematic_horse_route_system` (`systems/cinematic.rs:295-301`) |
  | `SceneAliasCandidate → Dead` | **new**, `populate_candidates` |

- **Cycle A** (scripting, 1 panic): `SceneActorBindings → SceneQuestAliasRegistry → SceneActorBindings`. The minimal failing set is two tests. The `Registry → Bindings` edge is **new**, from `running_quests_binding_entity`. The `Bindings → Registry` edge is test hygiene (D1-02).

## Evidence

- The panic message: `attempted acquisition of ActorCinematicState while holding Dead … cycle … ActorCinematicState → HorseTetherState → SceneAliasCandidate → Dead → ActorCinematicState` (`walk_anim.rs` read pass).
- The panic message: `attempted acquisition of SceneActorBindings while holding SceneQuestAliasRegistry` @ `quest_alias.rs:920`.
- CI: the ABBA job was green on `8b334c102` and red on `ab31cfefe` / `766e1746e`, with the identical 5 + 1 failure set.
- The populate arm, as it sits in the code:
  ```rust
  let talkable: Vec<EntityId> = world
      .query::<byroredux_scripting::SceneAliasCandidate>()   // guard held ↓
      .map(|identities| identities.iter()
          .filter(|(e, _)| world.get::<Dead>(*e).is_none())
          .filter(|(e, _)| world.get::<ActorValues>(*e).is_some())
          .filter(|(e, _)| !running_quests_binding_entity(world, *e).is_empty())
          ...
  ```

## Impact

- The lane cannot catch any other new cycle while it is red. This is the fifth time, after #3580, #3819, #4603 and #4982.
- Production is one edge away from a real `SceneAliasCandidate`-rooted cycle. `interaction_system` is exclusive today, but `SceneAliasCandidate` is also read by the parallel-adjacent cinematic/horse route.

## Related

D1-02, D6-01, #4546, #4982.

Dialogue-landing cluster (`ab31cfefe` / `766e1746e`), filed as distinct issues: ECS-2026-09-29-D1-02 (#5032), ECS-2026-09-29-D5-01 (#5035), ECS-2026-09-29-D7-01 (#5038), SCR-D3-2026-09-29-01 (#5041), LC-D3-01 (#5045), ESM-2026-09-29-D2-03 (#5048). The per-frame Talk-candidate scan on the same code (ECS-2026-09-29-D6-01 = PERF-D1-2026-09-29-01) is tracked under open #3475.

## Suggested Fix

- In `populate_candidates`, collect the `SceneAliasCandidate` entity ids and drop the guard before filtering. Better still, build one per-frame `FxHashSet<EntityId>` of entities bound by running quests, in a single pass over `SceneActorBindings.actors` (this also fixes D6-01).
- In `running_quests_binding_entity`, copy the registry's quest keys out before taking `SceneActorBindings`, rather than holding all three guards.
- Scope the two test guards (D1-02).

Validated at HEAD 9fcfdc3fc: `populate_candidates` (`byroredux/src/interaction.rs`) still calls `world.get::<Dead>` / `world.get::<ActorValues>` / `running_quests_binding_entity` inside the live `query::<SceneAliasCandidate>()` closure; `running_quests_binding_entity` (`crates/scripting/src/scene/quest_alias.rs`) holds `SceneQuestAliasRegistry` + `SceneActorBindings` + `QuestStageState` together. Lane state taken from the report's CI runs (not re-run per publish rules).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other `populate_candidates` arms, other callers of `running_quests_binding_entity` such as `npc_dialogue`)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux` and `-p byroredux-scripting` are both green
