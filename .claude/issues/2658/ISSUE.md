# #2658: SCR-D5-NEW11-03: fragment_coverage and mq101_conformance measure the context-free lowering path, not the production one

**Severity**: MEDIUM
**Dimension**: Recognizer-Chain Soundness (Dimension 5)
**Untrusted-Input**: No
**Location**: `crates/scripting/examples/fragment_coverage.rs:147`, `crates/scripting/examples/mq101_conformance.rs:1407,1450`
**Status**: NEW

## Description

Both harnesses call `lower_fragment` (which passes an empty quest-property set) while the single production caller, `fragment.rs::populate_quest_fragments_from_script`, calls `lower_fragment_with_quest_properties` with a real set.

`fragment_coverage` is the crate's coverage-regression gate and the instrument `docs/engine/m47-3-quest-alias-design.md`'s Phase-2 checklist points at; `mq101_conformance` is the MQ101 behavioural gate. Neither measures what the engine actually does.

Complete call-site enumeration (`grep -rn "lower_fragment"`):

| Call site | Entry point | Populated set? |
|---|---|---|
| `crates/scripting/src/fragment.rs:1292` (`populate_quest_fragments_from_script`) | `lower_fragment_with_quest_properties` | **Yes** |
| `crates/scripting/src/fragment/tests.rs:489` | `lower_fragment` | no (test) |
| `crates/scripting/examples/fragment_coverage.rs:147` | `lower_fragment` | **no (gate)** |
| `crates/scripting/examples/mq101_conformance.rs:1407,1450` | `lower_fragment` | **no (gate)** |
| ~30 unit tests in `translate/effects.rs` | `lower_fragment` | no (tests) |

## Evidence

The enumeration above. Concretely: because these harnesses use the context-free path, the whole of SCR-D5-NEW11-01 -- a shipped fix that changes nothing on real data -- was invisible to every existing instrument.

Today the two paths happen to agree exactly (9361/9361 fragments, 11284/11284 effects), but **only because of that bug**; the moment it is fixed, the harnesses will silently diverge from production.

Both harnesses already decompile the full `Script`, so the property table is right there -- mirroring `quest_property_names` is a ~12-line change (done as a temporary experiment to produce SCR-D5-NEW11-01's evidence).

## Impact

The coverage and conformance gates can report a claim rate and effect histogram that production does not reproduce, in either direction -- over-reporting yield if the production guard declines more, under-reporting if it resolves more.

It also means the M47.3 Phase-2 "live-corpus re-measurement of `AddItem`/`MoveTo` yield" checkbox cannot be honestly ticked from the harness as written.

## Related

SCR-D5-NEW11-01, `docs/engine/m47-3-quest-alias-design.md` Phase 2, #2432 (a sibling gate that asserts nothing)

## Suggested Fix

Lift `quest_property_names` out of `fragment.rs` into `translate::effects` (or make it `pub(crate)` and re-export) and have both examples call `lower_fragment_with_quest_properties` with the per-script set.

Consider marking `lower_fragment` `#[doc(hidden)]` or test-only so a future call site cannot accidentally pick the context-free path.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives, other parsers, other spawn paths)
- [ ] **CORPUS**: Re-run `fragment_coverage` against real Skyrim SE + FO4 archives and record the yield delta
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
