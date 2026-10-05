# #5293: PERF-D1-2026-10-05-01: #5109's bulk Talk filter rebuilds the whole alias-definition table and four entity sets every frame

**Labels**: low,performance,gameplay,quests,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5293

**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-10-05.md` — `PERF-D1-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/interaction.rs:1302-1331` (inside `populate_candidates`, called every frame via `interaction_system` → `select_interaction_target` → `collect_candidates`); `crates/scripting/src/scene/quest_alias.rs:960-1010` (`installed_alias_ids_by_quest`, `running_quest_bound_entities`)
- **Status**: NEW. This is the residual of a landed fix: #5109 replaced the per-root `World::get` chains with bulk set builds, as the baseline asked.
- **Description**: on every frame, ungated, `running_quest_bound_entities` calls `installed_alias_ids_by_quest`. That builds an `FxHashMap<QuestFormId, Vec<i32>>` with one Vec per installed quest. The registry is filled with every QUST in the load order (`asset_provider/script.rs:611-612`, `index.quests.values()`). It also builds a `running` set over all of them. The arm then collects, per frame:
  - an `FxHashSet` of every `ActorValues` entity;
  - a Vec of every `SceneAliasCandidate` root (every placement root);
  - the refusal sets;
  - the `withheld` set.
  
  The answer changes only on a binding refresh, a quest start or stop, a death, or an AV-membership change.
- **Evidence**: see Location.
- **Impact**: O(alias-bearing quests) small allocations plus O(placement roots + actors) set and Vec building, every frame, on the `Stage::Update` exclusive head. FO3/FNV QUST records carry no aliases, so they pay only the root-scaled part. Skyrim, FO4 and Starfield pay the quest-scaled part too. Small but permanent. No quantitative guard exists for this site.
- **Related**: #5109, #3475, #5025 (the lock-order reason the guards are taken one at a time).
- **Suggested Fix**: cache the bound set, keyed on the `SceneActorBindings` refresh generation plus a `QuestStageState` running-set generation. Then iterate the small `bound` set and probe `SceneAliasCandidate`, `ActorValues` and the refusal storages, instead of materializing every root.

## Completeness Checks
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
