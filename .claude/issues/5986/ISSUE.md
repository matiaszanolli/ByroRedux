# SCR-D2-2026-10-17-06: `apply_effect`'s nested-lock inventory is wrong and its scan has blind spots

Issue: #5986
Labels: low, scripting, bug, concurrency, test-gap

Found by `/audit-scripting` (AUDIT_SCRIPTING_2026-10-17.md, HEAD a071ed217).

### SCR-D2-2026-10-17-06: `apply_effect`'s nested-lock inventory is wrong and its scan has blind spots

- **Severity**: LOW
- **Dimension**: Fragment Dispatch & Locks
- **Location**: doc block `crates/scripting/src/fragment/effects.rs:45-134`; scan `fragment/tests.rs:4252-4415` (`apply_effect_family_bodies`, `the_nested_lock_residual_list_names_every_type_apply_effect_acquires`)
- **Status**: NEW
- **Untrusted-Input**: No
- **Description**: The guards are green but the inventory they pin is incomplete, and one sentence is false:
  1. The doc says "The `FragmentExecutionQueue` write is *not* in this list: it belongs to the latent-continuation path in the dispatch system, outside these guards" (`effects.rs:115-117`). `apply_effects` itself takes `try_resource_mut::<FragmentExecutionQueue>()` at `effects.rs:792` (VM `Utility.Wait`), `:937` (`WaitWhile`) and `:983` (`Wait`) - all inside the paired `QuestStageState`+`QuestObjectiveState` guard (`deferred.rs:1222-1233`). Also `budget::defer` (`effects.rs:764`).
  2. Types acquired under the guards but absent from the doc: `EquipmentSlots` (`inventory.rs:97`, `query_2_mut_mut` at :208), `EquippedWeapon` (:109), `ItemInstancePool` (:120) and the `EquipmentEventBatch`/`ItemEventBatch` batches (`equipment.rs:51,75`) via `remove_items`/`add_items` (called inline from `apply_inventory_effect` :172/:189 and `effects.rs:371-373`); `MiscStats`, `Dead`, `ActorVitals`, `PendingScriptKills` via `evaluate.rs:185,283,594-604`.
  3. The Papyrus VM interpreter runs inside the same guards (`effects.rs:769` -> `run_fragment`) and its natives take well over twenty more component/resource locks (`Inventory`, `SpellList`, `Seated`, `PapyrusReferenceScripts`, `InputLayers`, `EvaluatePackageRequest`, `ConditionForms`, `HavokImpulseRules`, `VmFrameBudget` write at `interp.rs:145`, ...). The doc block says "this list still inventories every lock any of them takes" and has no mention of the VM.
  Why the scan passes: it only reads functions named `apply_*_effect`, only matches the `query/query_mut/resource/resource_mut/try_resource(_mut)/get/get_mut` turbofish forms (not `has`, `query_2_mut_mut`, `resource_2_mut`), and does not follow helper functions (`remove_items`, `add_items`, `evaluate_guard`, `evaluate_value`, `resolve_actor`, `update_actor_cinematic_state`, `emit_*`, any VM native).
- **Evidence**: see line refs above; `grep -n 'FragmentExecutionQueue' effects.rs` shows the three acquisitions below the doc block.
- **Impact**: No deadlock today - the safety argument (every quest-resource system exclusive; no non-exclusive system takes quest resources) holds and was re-verified - but the artefact meant to let an auditor spot a reverse-order acquirer is false for the highest-traffic path (the VM), so the next reverse-order acquirer would not be seen by the guard.
- **Related**: #3949 / #5071 (earlier rounds of the same inventory rot), #3493.
- **Suggested Fix**: Rewrite the block to describe the real invariant ("everything under the paired guards, including the VM, relies on exclusive scheduling; no component/resource in this crate may take QuestStageState/QuestObjectiveState while holding another lock") and extend the scan to follow `fn`s reachable from `apply_effects` (or scan all of `SOURCES` for `QuestStageState`/`QuestObjectiveState` acquisitions inside non-system functions).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other natives / recognizers / consumers of the same shape)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved and `BYRO_LOCK_ORDER_CHECK=1` lane stays green
- [ ] **TESTS**: A regression test pins this specific fix
