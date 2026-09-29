# #5041 — SCR-D3-2026-09-29-01: GetIsID compares the Run-On's placed-reference FormID, but its parameter is a base-object FormID — false on the actor it names

**Labels**: high, bug, scripting, dialogue

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-29.md` — finding `SCR-D3-2026-09-29-01`

**Severity**: HIGH

**Dimension**: Event Runtime (CTDA evaluator)

**Untrusted-Input**: No

**Location**:
`crates/scripting/src/condition.rs:321-336` (`ConditionFunction::run_on_identity`, `GetIsID` arm), used by
`evaluate_condition` `:807-814` and by the alias pre-filter `IdentityIndex::narrowest`
(`crates/scripting/src/scene/quest_alias.rs:404-440`); identity source
`byroredux/src/cell_loader/references/synth_child.rs:53-64` (`FormIdComponent` local = `placed_ref.form_id`, the
REFR/ACHR id).

**Status in report**: NEW. #1666 (closed) asked for "the Run-On's base/placement FormID == param_1"; only the placement half
shipped. Distinct from the unpublished SCR-D3-2026-09-22-01 remark about `GetIsID(0x14)` on the player (that was the
sentinel; this is base vs reference for every actor).

## Description

`GetIsID ObjectID` tests the calling reference's **base object**. The engine's own census
(`crates/plugin/src/esm/records/misc/dialogue.rs:362-395`, `speaker_from_conditions`) shows `param_1` resolves to an
`NPC_` on 19,344 of 19,345 `Oblivion.esm` `GetIsID` conditions, and the plugin's CTDA arg table labels function 72
"base FormID" (`records/condition.rs:452`). The evaluator instead compares the entity's `FormIdComponent`, which the cell
loader stamps with the placed reference's own FormID. The base id is already on the same entity
(`SceneAliasCandidate::base_form_id`; the player is stamped `{reference 0x14, base 0x7}`, `byroredux/src/scene.rs:1205-1213`),
and `dialogue::actor_matches` (`crates/scripting/src/dialogue.rs:178-191`) already accepts reference **or** base — so an
INFO's speaker check passes and the CTDA re-check of the same speaker then fails.

## Evidence

```rust
Self::GetIsID => {
    let fid_comp = world.get::<FormIdComponent>(entity)?;   // placed REFR/ACHR identity
    let pool = world.try_resource::<FormIdPool>()?;
    pool.resolve(fid_comp.0).map(|pair| pair.local.0)       // compared with a base NPC_ param_1
}
```

## Impact

`GetIsID <base> == 1` is false on the actor it names, and `GetIsID <base> == 0` exclusions always pass. Live
consumers: INFO selection through both the SCEN `select_info` path and the new P4 `select_first_info` activation path
(`byroredux/src/systems/npc_dialogue.rs:120-140`); AI-package condition lists (all games); perk entry-point conditions;
condition-only quest-alias fills (`IdentityIndex` buckets by the same value, so such an alias never binds). The failure is
silent, with no log line.

## Related

GAME-D2-2026-09-29-01 (topic ownership ignores the speaker; this is the evaluator half of speaker
filtering), #1666, #4694.

Dialogue-landing cluster (`ab31cfefe` / `766e1746e`), filed as distinct issues: ECS-2026-09-29-D1-01 (#5025), ECS-2026-09-29-D1-02 (#5032), ECS-2026-09-29-D5-01 (#5035), ECS-2026-09-29-D7-01 (#5038), LC-D3-01 (#5045), ESM-2026-09-29-D2-03 (#5048). The per-frame Talk-candidate scan on the same code (ECS-2026-09-29-D6-01 = PERF-D1-2026-09-29-01) is tracked under open #3475.

## Suggested Fix

have the `GetIsID` identity read `SceneAliasCandidate` and pass on `base_form_id == param_1`. Keep the
reference id as a second accepted value only if a census shows REFR-valued params. `run_on_identity` must then return the
base id (or a small set), so `IdentityIndex` stays an exact superset. Replace `get_is_id_matches_entity_global_form_id`
with a test where an ACHR's base `NPC_` is `param_1`.

Validated at HEAD 9fcfdc3fc: `ConditionFunction::run_on_identity`'s `GetIsID` arm (`crates/scripting/src/condition.rs`) resolves the entity's `FormIdComponent`, which `stamp_quest_reference` (`byroredux/src/cell_loader/references/synth_child.rs`) sets to the placed REFR/ACHR `form_id`; `records/condition.rs` labels function 72 "base FormID"; `dialogue::actor_matches` accepts reference or base.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other identity-valued CTDA functions in `run_on_identity`; `IdentityIndex` bucketing must stay an exact superset)
- [ ] **TESTS**: A regression test pins this specific fix (an ACHR whose base `NPC_` is `param_1`)
