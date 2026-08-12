# #2672: SCR-D6-NEW11-07: cleanup.rs's stated drain contract contradicts the 10 markers that legitimately self-drain in their consumer

**Severity**: LOW
**Dimension**: Scripting Runtime Systems (Dimension 6)
**Untrusted-Input**: No
**Location**: `crates/scripting/src/cleanup.rs` module doc (the drain list is `:32-52`); the competing rule exists only as prose in `byroredux/src/save_io.rs`
**Status**: NEW

## Description

`cleanup.rs`'s module doc states that every transient marker is drained by `event_cleanup_system`. A full sweep of all 44 `impl Component for` types in the crate shows that is not the contract in force:

- **14** are drained by `event_cleanup_system` (`ActivateEvent`, `HitEvent`, `TimerExpired`, `AnimationTextKeyEvents`, `OnUpdateEvent`, `QuestStageAdvancedBatch`, `CameraShakeCommand`, `ControllerRumbleCommand`, `UiMessageCommand`, `SceneEventBatch`, `SceneFragmentInvocationBatch`, `OnTriggerEnterEvent`, `OnCellLoadEvent`, `OnEquipEvent`);
- **10** self-drain unconditionally at the head of their own consumer, each verified to have no early return before the drain (`SceneStartRequest`/`SceneStopRequest`/`SceneActionCompletionBatch`, `DialoguePresentationEventBatch`/`DialogueLineCompletionBatch`, `ScenePackageEventBatch`/`ScenePackageCompletionBatch`/`EvaluatePackageRequest`, `TwoStateTransitionBatch`, `MotionTypeChangeRequest`);
- the remainder are persistent state, correctly excluded.

Both patterns are legitimate. Neither is written down anywhere authoritative -- the real house rule ("drain at the head of your consumer, or register with cleanup") exists only as prose in an unrelated file.

## Evidence

`grep -rn "^impl Component for" crates/scripting/src/` enumerates the 44 types; the drain list in `crates/scripting/src/cleanup.rs:32-52` covers 14 of them. `event_cleanup_system` is confirmed as the last scheduled system overall (`byroredux/src/boot.rs:1196`, `Stage::Late`).

No marker was found that re-fires every frame -- the sweep came back clean on the invariant itself. This finding is purely about the contract being mis-stated.

## Impact

Documentation and contract clarity. A future marker author reading `cleanup.rs` concludes registration is mandatory; one reading a self-draining consumer concludes it is optional; nothing adjudicates.

That ambiguity is not hypothetical in this crate -- SCR-D6-NEW11-01 (the `Effect::Activate` ordering defect) is precisely a marker-lifecycle bug that a written, checkable contract would have made visible at review time.

## Related

#2270 (open -- the undocumented "snapshot before iterate" house rule, same class of missing-house-rule finding); SCR-D6-NEW11-01

## Suggested Fix

State both sanctioned patterns in `cleanup.rs`'s module doc and list which markers use which. Fold into #2270's documentation sweep so the crate's implicit house rules land in one place.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives, other parsers, other spawn paths)

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
