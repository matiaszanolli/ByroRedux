# SCR-D7-NEW10-01: No regression test pins the is_primary_synth gate on stamp_quest_reference/spawn_logical_quest_reference

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2541
**Finding ID**: SCR-D7-NEW10-01

**Severity**: LOW (test-coverage gap; every one of the 8 gated call sites reads correctly by direct inspection)
**Dimension**: Engine Attach & Trigger Wiring
**Untrusted-Input**: No
**Location**: `byroredux/src/cell_loader/references/mod.rs` — the `stamp_quest_reference`/`spawn_logical_quest_reference`/`attach_quest_reference_script` functions (added `a844c26b`) and their 8 call sites inside `spawn_synth_child`, plus the standalone `synth_idx == 0` gate in `load_references_budgeted`'s NPC-actor path
**Status**: NEW

## Description
A SCOL/PKIN-expanded REFR fanning into N synthetic children, only the first of which should register a `SceneAliasCandidate`, is correctly implemented at all 8+1 sites (verified by direct read), but no test in this file's `mod tests` spawns a multi-child SCOL/PKIN expansion and asserts exactly one `SceneAliasCandidate` is registered for the whole REFR.

## Evidence
Confirmed directly: `grep -n "SceneAliasCandidate\|stamp_quest_reference\|spawn_logical_quest_reference\|is_primary_synth" byroredux/src/cell_loader/references/mod.rs | grep -i test` returns nothing.

## Impact
None today — verified correct by direct reading of every call site. But this is exactly the kind of invariant (a boolean gate repeated across 8 near-identical branches in a 500-line dispatch function) a future 9th branch or a collapsing refactor could silently drop without any test catching it. A dropped gate would register N `SceneAliasCandidate`s for one authored alias-fillable reference, corrupting `SceneActorBindings`'s alias-fill resolution for that REFR.

## Suggested Fix
Add one regression test exercising `spawn_synth_child` against a REFR whose `base_form_id` is a SCOL/PKIN with ≥2 child placements, asserting `world.query::<SceneAliasCandidate>().iter().count() == 1`. If a full spawn fixture is too heavy, a source-scan test (mirroring `scol_expansion_is_cached_across_a_budget_yield`'s technique) asserting every `stamp_quest_reference(`/`spawn_logical_quest_reference(` call site is preceded by an `is_primary_synth` guard would close the gap at zero runtime cost.

## Completeness Checks
- [ ] **TESTS**: New regression test (spawn-fixture or source-scan) pins the `is_primary_synth` invariant across all 8 call sites
