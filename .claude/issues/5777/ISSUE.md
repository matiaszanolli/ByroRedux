# REN-D1-2026-10-14-01: Doc blocks displaced onto the wrong item by recent edits: `step_cell_transition` (`app_step.rs`) and `merge_bgsm_arm` (`merge.rs`)

**Labels**: low,tech-debt,documentation,doc-rot
**Source**: docs/audits/AUDIT_RENDERER_2026-10-14.md

Found by `/audit-renderer` (docs/audits/AUDIT_RENDERER_2026-10-14.md). Finding ID: `REN-D1-2026-10-14-01`. Status: NEW, confirmed against HEAD `adbd468e2`.

**Location**: `byroredux/src/app_step.rs` (`step_cell_transition`, `step_player_move_request`, `step_created_references`, `step_actor_graphs`); `byroredux/src/asset_provider/material/merge.rs` (`merge_bgsm_arm`, `BgsmTextureSlotState`)

## Description
- `16c9bd0b2` inserted `step_player_move_request`, `step_created_references` and `step_actor_graphs` between `step_cell_transition`'s long orchestrator doc block (incl. the #5288 caching note) and its `fn`. The whole block now renders on `step_player_move_request`; `step_cell_transition` has no doc.
- After #5735 (`a6cef30a5`) the `merge_bgsm_arm` doc sits on `struct BgsmTextureSlotState` and the old `set_*` explanatory comments are orphaned at the top of `merge_bgsm_arm`. The refactor is behaviour-preserving (loop body moved verbatim into three helpers, same call order).

## Evidence
`app_step.rs`: the doc ending "…survive every rebuild." is followed directly by the `M47.2 — drain a fragment's …MoveTo` doc and `fn step_player_move_request` (l.947); `fn step_cell_transition` (l.1094) has no doc. `merge.rs:768` `struct BgsmTextureSlotState` follows the unrelated `set_*` comment run; `fn merge_bgsm_arm` at l.1181 has none.

## Impact
Documentation only. Same defect class as closed #5692 and open #5704.

## Suggested Fix
Move each doc block back onto its function.
