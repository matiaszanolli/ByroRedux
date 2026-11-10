# SAFE-2026-11-09-03: composite's two-pipeline `create_graphics_pipelines` discards a successfully built sibling when the other fails (pipeline leak on the error arm)

Labels: low,safety,vulkan,renderer,bug
Filed from: volumetrics-deep audit suite, 2026-11-09

**Severity**: LOW. **Status**: NEW (the pair form is #6748 / `15a4ecf10`; the discard shape is renderer-wide).

**Location**: `crates/renderer/src/vulkan/composite.rs` (~L1130-1146, the `Err((_, e))` arm). Same shape in `presentation.rs` (~L521), `pipeline.rs` (~L568, ~L942) and `water.rs` (~L1085, via `.map_err(|(_, err)| err)`).

## Description
`create_graphics_pipelines(cache, &[sunlit, sunless], None)` builds two pipelines in one call. The Vulkan spec lets a multi-pipeline create report an error while still returning valid handles for entries that succeeded (only failed entries are `VK_NULL_HANDLE`, unless `EARLY_RETURN_ON_FAILURE`). ash returns the `Vec<Pipeline>` in the `Err` tuple. The arm throws it away (`Err((_, e))`) and calls `partial.destroy`, which only knows `pipeline` and `pipeline_sunless` — both still null (assigned only in the `Ok` arm). A pipeline that did build is leaked. Composite was a single-info call before #6748, so this is a new exposure at this site.

## Impact
One leaked `VkPipeline` on an init path that already ends in `Err` (fatal for the context, on boot or resize). Does not compound per frame.

## Suggested Fix
In the `Err((pipelines, e))` arm assign any non-null element to the `partial` fields before `partial.destroy`, or build the two variants with two single-info calls. Apply the same pattern to the other multi-info call sites if the class should be closed.

Report: `docs/audits/AUDIT_SAFETY_2026-11-09.md`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`presentation.rs`, `pipeline.rs`, `water.rs`)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix

