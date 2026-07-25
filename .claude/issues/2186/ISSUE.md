# SCR-D5-NEW4-01: QuestRef::Property resolution ignores VMAD alias binding, unlike ObjectRef::Property

**Issue**: #2186
**Labels**: high, bug
**Dimension**: Recognizer-Chain Soundness / Decline-Invariant Audit (Dimension 5)
**Untrusted-Input**: No — a real/malformed-VMAD-data correctness gap, not an adversarial-input path
**Location**: `crates/plugin/src/esm/records/script_instance.rs:105-110` (`ScriptInstance::object_form_id`); consumed by `crates/scripting/src/fragment.rs:108-121` (`resolve_quest`) and `crates/scripting/src/translate/recognizers/quest_stage_gate.rs:76-81` (`recognize`)
**Status**: NEW — independently found by an orphaned sub-agent from an earlier attempt at this audit and re-confirmed empirically in this pass

## Description

`ObjectRef::Property` resolution (`fragment.rs:143-151`, `resolve_property_form_id`) explicitly requires `PropertyValue::Object { form_id, alias: -1 } => Some(form_id), _ => None` — declining for any other `alias` value, per its own doc comment ("declines here rather than trusting the raw `form_id` sitting next to a live alias index").

But the sibling `QuestRef::Property` resolution path uses a different helper, `ScriptInstance::object_form_id`, which matches `PropertyValue::Object { form_id, .. } => Some(form_id)` — the `alias` field is discarded (`..`) and never checked. This helper is the **only** resolver for `QuestRef::Property` in both live call sites:
- `resolve_quest` (`fragment.rs:108-121`) — used by every `SetStage`/`SetObjectiveDisplayed`/`SetObjectiveCompleted`/`SetObjectiveFailed`/`CompleteAllObjectives` effect dispatch
- `quest_stage_gate::recognize` (`crates/scripting/src/translate/recognizers/quest_stage_gate.rs:76-81`) — used to bind the whole `QuestAdvanceOnActivate` component's `owning_quest`

Confirmed directly:
- `script_instance.rs:105-110`: `pub fn object_form_id(&self, name: &str) -> Option<u32> { match self.property(name)?.value { PropertyValue::Object { form_id, .. } => Some(form_id), _ => None } }`
- `fragment.rs:143-151` (`resolve_property_form_id`): `PropertyValue::Object { form_id, alias: -1 } => Some(form_id), _ => None`
- `fragment.rs:118`: `resolve_quest`'s `QuestRef::Property` arm calls `s.object_form_id(name)` — the lax helper
- `quest_stage_gate.rs:80`: `recognize`'s `QuestRef::Property` arm also calls `s.object_form_id(name)` — same lax helper

## Evidence

`docs/engine/m47-3-quest-alias-design.md:66` lists "`QuestRef::Property` / `ObjectRef::Property` alias decline" as one row, claimed done for both — but only the `ObjectRef` side is actually implemented. No test anywhere in the crate exercises `object_form_id`/`resolve_quest`/`quest_stage_gate::recognize` with a non-`-1` alias. `AUDIT_SCRIPTING_2026-07-21.md`'s "Decline-Invariant Audit" section explicitly re-verified `resolve_property_form_id`'s alias branch and found it correct-by-design, but did not examine `object_form_id`/`resolve_quest`, so this gap wasn't previously caught. Independently re-confirmed via a throwaway test (`script_instance.rs`, built, run, reverted — tree confirmed clean) constructing `PropertyValue::Object { form_id, alias: 3 }` and asserting `object_form_id` still returns `Some` — it does.

## Impact

If a `Quest`-typed VMAD property is ever alias-bound (the wire format doesn't distinguish by the property's declared Papyrus type, only by the generic type-1 "Object" tag every form-reference property shares), both the quest-stage-gate recognizer and the fragment effect dispatcher will silently resolve the raw `form_id` field sitting next to the alias index — not the intended target once a property is alias-bound — instead of declining. For the recognizer this means emitting a `QuestAdvanceOnActivate` component stamped with the wrong `owning_quest`; for the fragment dispatcher it means a `SetStage`/objective effect silently mutating the wrong quest's state. Real-world reachability is uncertain — Quest-typed properties aren't the typical target of CK's alias-fill UI (aliases usually fill Actor/ObjectReference-typed properties) — so actual corpus yield may be low, but the code has no structural guard against it, unlike its `ObjectRef` sibling.

## Suggested Fix

Give `ScriptInstance` an `object_form_id`-equivalent that mirrors `resolve_property_form_id`'s explicit `alias: -1` match (or have both callers use that stricter matching directly), so `QuestRef::Property` on an alias-bound VMAD entry declines the same way `ObjectRef::Property` already does. Add a regression test with `alias: 3` (or any non-`-1` value) asserting both `fragment::apply_effects` and `quest_stage_gate::recognize` decline rather than resolve.

## Completeness Checks
- [ ] **TESTS**: A regression test with `PropertyValue::Object { alias: 3, .. }` asserting both `resolve_quest`/`apply_effects` and `quest_stage_gate::recognize` decline
- [ ] **SIBLING**: Verify no other consumer of `ScriptInstance::object_form_id` (or a future one) reintroduces this gap — consider removing the lax helper entirely once the strict path lands
