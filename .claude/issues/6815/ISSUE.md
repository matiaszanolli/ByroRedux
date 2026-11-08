# CONC-2026-11-08-6.1: staging-free guard test and helper doc cover only two of the three capture twins; linear capture is unpinned

_Immutable snapshot of the issue as filed (GitHub is authoritative for state). Source: docs/audits/AUDIT_CONCURRENCY_2026-11-08.md. Labels: low,sync,renderer,bug,test-gap_

Found by `/audit-concurrency` 2026-11-08 (`docs/audits/AUDIT_CONCURRENCY_2026-11-08.md`, HEAD 0419437ab).

- **Severity**: LOW (guard/doc coverage gap, no runtime defect)
- **Dimension**: Resource Lifecycle
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs` — test `both_staging_frees_delegate_to_the_shared_helper` and the `# Safety` doc of `destroy_staging_buffer`; `crates/renderer/src/vulkan/context/linear_capture.rs` (`destroy_linear_capture_staging`)

## Description
#4039 pinned that the screenshot and depth-capture staging frees delegate to `helpers::destroy_staging_buffer`, so the "both-slots fence wait" contract lives in one place. `linear_capture.rs` (75b4024df) added a third twin and does delegate today, but the test's `[(label, src, needle)]` array has only the two original entries and the helper doc names only `screenshot_record_copy` / `depth_capture_record_copy` as callers. `sync.rs` already lists `linear_capture_staging` in the #870 rider list, so the free is sound today.

## Impact
Re-inlining a raw `destroy_buffer` + `free_allocation` in `destroy_linear_capture_staging` would fail no guard, so the #4039 divergence could repeat on the newest twin — a hazard if the all-slots wait is later narrowed (#5365).

## Suggested Fix
Add a `linear_capture` entry (`include_str!("linear_capture.rs")`, needle `pub(super) fn destroy_linear_capture_staging(&mut self) {`) to the test array, and add `linear_capture_record_copy` and `linear_capture_record_copy_post_taa` to the helper doc's caller list. (Rename the test if "both" no longer fits.)

## Completeness Checks
- [ ] **SIBLING**: any other staging-buffer owner in `context/` frees through the shared helper
- [ ] **DROP**: teardown order unchanged
- [ ] **TESTS**: the guard fails if the linear-capture free is inlined
