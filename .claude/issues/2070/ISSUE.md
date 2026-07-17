# TD2-111: CTDA to ConditionList push-and-remap triplet copy-pasted at 4 sites across two files

**GitHub Issue**: #2070
**Labels**: low,import-pipeline,legacy-compat,tech-debt,bug

**Severity**: LOW
**Dimension**: 2 (Logic Duplication)
**Location**: `misc/ai.rs:603-607,842-848,1018-1023`, `misc/magic.rs:438-444`

## Description
`parse_ctda` itself is correctly centralized; the 3-statement wrapper (parse → remap → push) is what's duplicated, 4 times across two files.

## Evidence
Confirmed live: `misc/ai.rs` contains the pattern `if let Some(mut cond) = parse_ctda(sub) { remap_condition_form_ids(&mut cond, remap); ... }` at 3 sites (lines ~604-605, 844-845, 1019-1020); `misc/magic.rs` has the identical pattern once (lines 439-440). Both files import `parse_ctda`/`remap_condition_form_ids`/`ConditionList` from `super::super::condition`.

## Suggested Fix
Add a `ConditionList::push_ctda_sub()` helper next to `parse_ctda`/`remap_condition_form_ids`.

**Effort**: small

## Completeness Checks
- [ ] **TESTS**: Existing condition-list parse tests across `ai.rs`/`magic.rs` cover all 4 call sites — purely mechanical extraction
