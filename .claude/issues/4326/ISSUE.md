# #4326 SCR-D7-2026-09-14-01: invisible trigger volumes bypass the `ReferenceEnableState` spawn gate, and `disable_after_advance` is only a live component removal — a once-only quest trigger re-arms and can re-fire `SetStage` after every cell reload

**Labels**: medium,scripting,quests,bug
**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`

- **Severity**: MEDIUM
- **Dimension**: Engine Attach & Trigger Wiring
- **Untrusted-Input**: No
- **Location**: `byroredux/src/cell_loader/references/synth_child.rs` `spawn_synth_child` trigger branch (returns before the only `spawn_placed_instances` call); `crates/scripting/src/papyrus_demo/quest_advance.rs` `quest_advance_system` (`disable_sources` → `components.remove`); `crates/scripting/src/translate/recognizers/quest_stage_gate.rs` `recognize_specific_actor_trigger`; `crates/scripting/src/quest_stages.rs` `QuestStageState::set_stage`
- **Status**: NEW (#3278 and #3789, both closed, added and ordered the gate only on the mesh path)
- **Description**: #3278 put the only runtime consumer of `ReferenceEnableState` (`placement_is_disabled`) inside `spawn_placed_instances`. The trigger branch spawns its `TriggerVolume`, stamps identity, attaches the script and returns without consulting the ledger. This has two consequences:
  1. A fragment `Disable()` on a trigger writes the ledger, but no load path reads it for triggers.
  2. The vanilla actor-base stage trigger's `disableWhenDone` / `onlyOnce` lowers to `disable_after_advance: true`, which `quest_advance_system` implements only as `components.remove(entity)`. Nothing is persisted, and on reload the attach chain inserts a fresh armed component. Its only condition, `GetStageDone(prereq) == 1`, stays true. `set_stage` overwrites `current_stage` (it can move backward) and re-queues the stage fragment.
- **Evidence**: The orchestrator confirmed that `placement_is_disabled` has one production call site (`spawn.rs` inside `spawn_placed_instances`, called at `synth_child.rs:671`), and that the trigger branch (`synth_child.rs` ~216–243) `return`s before reaching it.
- **Impact**: After any cell revisit or save load, a once-only quest trigger fires again when its gated actor re-enters, for example MQ101's cart-horse `BaseForm` triggers. Non-idempotent stage fragments (`AddItem`, `MoveTo`) re-run, and `current_stage` can regress, with nothing logged. **UNVERIFIED**: the corpus count of such triggers, and whether shipped Papyrus re-runs a fragment on `SetStage` of a done stage. The engine-side re-fire does not depend on either.
- **Related**: #3278, #3789, SCR-D7-2026-09-14-02
- **Suggested Fix**: In the trigger branch, skip volume + attach when `placement_is_disabled` holds for the placement FormID. In `quest_advance_system`, record `disable_after_advance` into `ReferenceEnableState`, keyed by the source's `SceneAliasCandidate::reference_form_id`. Test: fire → unload → reload → no re-fire.

_Source: `docs/audits/AUDIT_SCRIPTING_2026-09-14.md`_

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives / spawn paths / walkers)
- [ ] **TESTS**: A regression test pins this specific fix
