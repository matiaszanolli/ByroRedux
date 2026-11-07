# REN-D4-01: shader-pipeline.md's per-frame order omits the linear-capture copies and the local-grid sim, and still says the shadow filter runs x2

Labels: low,renderer,sync,documentation,doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/6744

**Severity**: LOW · **Dimension**: Sync/Barriers

## Location
`docs/engine/shader-pipeline.md` (step 11b `shadow_filter.comp x2`, step 13 volumetrics, the composite -> bloom gap); guard `every_frame_recorder_is_documented` (`crates/renderer/src/vulkan/context/post_passes.rs`)

## Description
No row for `linear_capture_record_copy` (composite -> bloom, two layout round-trip barriers) or `linear_capture_record_copy_post_taa` (after the TAA dispatch), nor for the volumetrics stage B2 local-grid sim (`cmd_fill_buffer` of the brick buffer, TRANSFER->COMPUTE barrier, three history barriers, timed dispatch, publish barrier, plus composite-side `record_publish_bricks` / `composite_barriers`). Step 11b says x2; `ShadowFilterPipeline::dispatch` runs `iterations` = 2/2/3/4 by ray-quality tier (`GpuRayBudget::shadow_filter_iterations`, `SHADOW_FILTER_MAX_ITERATIONS` = 4).

## Evidence
`grep -n -i "linear_capture\|linear capture\|local.grid\|local_sim" docs/engine/shader-pipeline.md` is empty. `every_frame_recorder_is_documented` collects only `record_*` method names from nine named files, so `linear_capture_record_copy*` (name does not start with `record_`) and `grid.record_frame` (in `volumetrics.rs`, not a scanned file) are invisible to it.

## Impact
None at runtime. A reader debugging a sync-validation report on the capture or local-grid barriers has no row to check against, and the guard gives false assurance that the recorder list is complete.

## Suggested Fix
Add rows for the two capture taps (with the restore barrier's stage masks) and the local-grid sim and publish barriers; change 11b to "x iterations (2..4 by tier)". Optionally widen the scan to match `*_record_*` names and add `volumetrics.rs` to the file list.

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **SIBLING**: Same wording checked in related files (docs, other comments)
- [ ] **TESTS**: Covered by existing guards / `.claude/commands/_audit-validate.sh`; no new test needed for pure comment edits
