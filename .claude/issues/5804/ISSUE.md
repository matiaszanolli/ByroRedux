# TD1-2026-10-15-05: `crates/scripting/src/condition.rs` crossed 2000 production LOC (1235 → 2005 in three days); a 49-variant catalog is spelled in six parallel exhaustive matches

Labels: low,scripting,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD1-2026-10-15-05`).

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/scripting/src/condition.rs` (3691 lines total, tests from L1995)
- **Status**: NEW (the 10-12 report did not list it; only `PerkList` #5610 and #5616 name the file)
- **Age**: +770 lines since 5d31fd637 — `6f6a9eb3d` (location runtime), `5bf9ac87e` (HasKeyword), `87e43246c` (GetRelationshipRank), `a6d1f2e44` (cleared locations), `e490f6fd9`, `5ea89f3be`, `44e4d948d`, `f67d82b3c`, …
- **Effort**: medium
- **Description**: four layers: (a) the catalog `ConditionFunction` (L105–788, ~685 lines): the enum, `from_index` (L336, 49 arms), `first_param_is_form_id`, `name`, `reads_run_on_entity`, `is_deterministic`, `reads_context`, `is_identity_test`, `run_on_identity`; (b) `ConditionContext` + `EventDataSlots` + `resolve` (L790–1160, ~370); (c) evaluators (L1166–1880, ~715): `evaluate_contextual` (174 lines, 42 arms), `actor_value_view`, **`evaluate_function` (405 lines, 37 arms)**; (d) `subject_requirement` (L1887–1993). Layer (a) states every function six times — `from_index`, `name`, and four boolean-classification matches (the doc on `reads_run_on_entity` says they are deliberately exhaustive so a new variant must be classified) — so adding one function edits six `match` blocks plus `CATALOG`.
- **Evidence**: `grep -c` of variants in the enum = 49; `fnscan.py`: `evaluate_function 405`, `evaluate_contextual 174`.
- **Impact**: the catalog grows with every Condition function a quest or INFO uses; six matches that must stay in step are a cheap place to drift, and the file is the evaluator every dialogue / quest / package path calls.
- **Related**: #5616 (closed, CTDA length constants), #5610 (`PerkList`), finding 03.
- **Suggested Fix**: `condition/{catalog.rs, context.rs, eval.rs, requirement.rs}` plus, in `catalog.rs`, one `const ROWS: &[Row { func, index, name, first_param_form_id, reads_run_on_entity, deterministic, reads_context, identity_test }]` with the existing exhaustive-match guarantees recovered by a `const` assertion or a test that walks `CATALOG` against `ROWS`; `evaluate_function` is then split by evaluator family (quest/global, actor, location, alias, event data). Hazard: three in-file `include_str!("condition.rs")` pins (L2488, L2841, L3605) and `byroredux/src/boot/schedule/mod.rs:653` (`CONDITION_SRC`) must read the new files.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
