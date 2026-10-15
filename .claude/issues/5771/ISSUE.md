# REN-D4-2026-10-14-02: About ten refactored pass layouts have no device-free `validate_set_layout` test; the one reflect test validates a hand-copied SSAO layout

**Labels**: low,renderer,vulkan,test-gap,bug
**Source**: docs/audits/AUDIT_RENDERER_2026-10-14.md

Found by `/audit-renderer` (docs/audits/AUDIT_RENDERER_2026-10-14.md). Finding ID: `REN-D4-2026-10-14-02`. Status: NEW, confirmed against HEAD `adbd468e2`.

**Location**: `crates/renderer/src/vulkan/reflect.rs` (`validate_ssao_layout_matches`); the pass layouts rebuilt by #5593 (`svgf`, `taa`, `ssao`, `bloom`, `composite`, `exposure_meter`, `compute`, `caustic`, `volumetrics`, `skin_compute`, …)

## Description
After #5593 the layouts are built with `descriptors::layout_binding`/`compute_binding` and validated against SPIR-V only at device init. `validate_ssao_layout_matches` checks a hand-copied layout rather than the array the constructor uses, so a future binding drift in those passes is caught only when a device is created, not by `cargo test`.

## Evidence
The Dim 4 agent compared the old and new binding tuples file by file for all of them (they match), so this is coverage, not a current defect.

## Impact
Future drift is invisible to CI. Related: #5593, #4110.

## Suggested Fix
Have each layout builder return a testable array shared by the constructor and a device-free reflect test.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
