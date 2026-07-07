# #1905 — SCR-D5-NEW-03: quest_stage_gate drops per-predicate quest — multi-quest GetStageDone gate emitted with extra predicate silently retargeted

_Filed from `docs/audits/AUDIT_SCRIPTING_2026-07-06.md`. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 1905 --json state`)._

---

**Severity**: HIGH · **Dimension**: Recognizer-Chain Soundness · **Untrusted-Input**: Yes (reachable via a decompiled `.pex` or a `.psc` REFR script)
**Location**: `crates/scripting/src/translate/recognizers/quest_stage_gate.rs:272-297` (`classify_if_condition`), consumed at `:246-260` (`match_guarded_if`) and `:61-97` (`recognize`)
**Source**: audit `docs/audits/AUDIT_SCRIPTING_2026-07-06.md` (SCR-D5-NEW-03)

## Description
`classify_if_condition` runs each `&&`-split atom through `classify_guard_atom`, which returns a `GuardMatch::StageDone { via, stage, expected }` carrying the *per-predicate* quest reference (`via` = `Property(name)` or `OwningQuest`, from `compose::quest_via` on that predicate's receiver). But the loop keeps only the **first** atom's `via` via `quest_via.get_or_insert(via)` (line 292) and discards the rest, with **no check that all predicates name the same quest**. The downstream cross-check (`match_guarded_if:251`, `quest_via != set_via`) compares only that first `via` to the `SetStage` receiver. `recognize` then resolves the single `owning_quest` and stamps it onto `param_1` of **every** emitted `Condition`, while `GetStageDone` evaluation reads `param_1` as the quest FormID.

A mixed gate — e.g. `If Self.GetOwningQuest().GetStageDone(37)==1 && MyOtherQuest.GetStageDone(5)==1` with `Self.GetOwningQuest().SetStage(40)` — is therefore **emitted, not declined**, with the `MyOtherQuest` predicate silently retargeted to the owning quest (and `MyOtherQuest`'s VMAD FormID never even resolved). This is exactly the silent game-logic corruption the decline-on-unmodeled invariant exists to prevent.

## Evidence
`via` provably differs per atom — `compose.rs` `as_get_stage_done` → `quest_via(object)` returns `QuestRef::Property("MyQuest")` for a property receiver and `QuestRef::OwningQuest` for `Self.GetOwningQuest()`. `classify_if_condition:292` uses `get_or_insert`, a no-op after the first insert. No test in the file covers cross-quest predicates (all use a single quest), so the gap is untested.

## Impact
A quest-gate that predicates on another quest's progress advances on the *wrong* quest's stage — either firing when it shouldn't or never firing. Silent, no fallback to mask it. Narrow trigger (needs a multi-predicate AND referencing ≥2 distinct quests where the `SetStage` receiver equals the first predicate's quest), but cross-quest `GetStageDone` gates do occur in vanilla Bethesda scripts. Blast radius is all games the recognizer runs on.

## Related
The `.psc`-vs-`.pex` fidelity gate (DA10) uses same-quest predicates, so it does not catch this. Sits alongside the (fixed) single-statement-body invariant #1719.

## Suggested Fix
In `classify_if_condition`, replace `get_or_insert(via)` with a compare-or-decline: if `quest_via` is already set and the new atom's `via != quest_via`, `return None`. Preserves existing same-quest behavior (DA10) and declines the mixed-quest case rather than mis-attributing it.

## Completeness Checks
- [ ] **SIBLING**: Same `get_or_insert`-vs-compare pattern checked in any other recognizer that folds per-atom quest/target refs (`compose.rs` guard folding)
- [ ] **DECLINE-INVARIANT**: The fix declines (returns `None`) rather than emitting a partial/approximated match — no component built from a mixed-quest condition
- [ ] **TESTS**: A regression test pins a mixed-quest AND-conjunction asserting *decline*, and the DA10 same-quest gate still recognizes byte-for-byte
