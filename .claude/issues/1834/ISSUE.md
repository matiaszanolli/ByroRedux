# ECS-2026-07-02-01: ActorValues still absent from the M45 save registry — save→load silently reverts actor-value edits

**Labels**: medium, ecs, bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/1834
**Source**: docs/audits/AUDIT_ECS_2026-07-02.md

**Severity**: MEDIUM
**Dimension**: 7 — Component Lifecycles (save/load)
**Location**: `byroredux/src/save_io.rs:81-88` (`MUTABLE_DELTA_COLUMNS`), `byroredux/src/save_io.rs:155-186` (`build_save_registry`), `crates/core/src/ecs/components/actor_values.rs:66-69`

## Description
`ActorValues` derives only `Debug, Clone, Default` (`actor_values.rs:66`) — no `serde` — and is absent from both `build_save_registry()`'s registered components and `MUTABLE_DELTA_COLUMNS`. It is stamped on every NPC placement root at spawn (`byroredux/src/npc_spawn.rs:571`, `stamp_actor_values`) and mutated at runtime by the `setav`/`modav` console commands (`byroredux/src/commands/actor_value.rs:20-46`), and is now also read by the `GetActorValue` condition function (`crates/scripting/src/condition.rs`).

## Evidence
```rust
// actor_values.rs:66 — no serde derive
#[derive(Debug, Clone, Default)]
pub struct ActorValues { .. }
```
```rust
// npc_spawn.rs:570-572 — stamped at spawn, alongside the new CHARAL stamps
stamp_faction_ranks(world, placement_root, npc);
stamp_actor_values(world, placement_root, npc, index, game);
stamp_character_components(world, placement_root, npc);
```
`save_io.rs:166-184` lists Transform/Name/Parent/Children/Inventory/EquipmentSlots/LightSource/LightFlicker/AnimationPlayer/AnimationStack/ScriptTimer + 3 resources — no `ActorValues`.

## Impact
A save→load cycle drops every permanent/temporary/damage layer and any console-edited base; the reloaded cell re-derives only the class auto-calc base at NPC respawn. `GetActorValue` conditions evaluate against the freshly-rederived (not the edited) value post-load. Blast radius is still bounded to the console-command mutation source today, but widens with every perk/magic-effect system that composes into `ActorValues` per its module docs.

## Related
#1663 (component introduction); `ECS-2026-07-02-02` (same pattern, new components); `delta_columns_carry_only_session_stable_fields` tripwire test (`save_io.rs:716`). Same finding as `ECS-2026-07-01-01` (2026-07-01 report) — unfixed, carried over. Neither day's report had previously been filed as a GitHub issue.

## Suggested Fix
Add `#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]` (matching `ScriptTimer`) to `ActorValue`/`ActorValues`, register it in `build_save_registry()`, and add `"ActorValues"` to `MUTABLE_DELTA_COLUMNS` — then extend the tripwire test's pinned set.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other spawn-stamped components — see `ECS-2026-07-02-02`)
- [ ] **TESTS**: A regression test pins this specific fix (extend `delta_columns_carry_only_session_stable_fields`)
