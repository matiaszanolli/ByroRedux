# TD1-NEW-02: draw_frame regrew past its just-closed #1857 complexity fix (1927→2048 LOC in 4 days)

**Labels**: low, renderer, tech-debt, bug
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-07-25.md` (TD1-NEW-02)
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2197

## Severity
LOW

## Dimension
1 (File/Function/Module Complexity)

## Location
`crates/renderer/src/vulkan/context/draw.rs:425-2473` (`draw_frame`)

## Description
Commit `9a9a4c5d` (2026-07-21) closed #1857, leaving `draw_frame` at 1927 LOC. By this audit (2026-07-25), `draw_frame` is 2048 LOC — +121 lines / +6% in 4 days, from a new ~90-line inline FSR-frame-parameter-assembly block (jitter, DOF gating, camera-cut detection) added directly into the function body rather than extracted, unlike the file's established pattern (`dof_effective_view_proj`, `rebase_model_matrix`, `origin_corrected_prev_view_proj`).

## Evidence
`pub fn draw_frame` starts at line 425. Inline FSR-frame-parameter block confirmed live at ~lines 885-955. No `build_fsr_frame_parameters` free function exists yet.

## Impact
Not a correctness issue — purely maintainability. The function is trending back toward its pre-fix size.

## Related
Existing: #1857 (CLOSED).

## Suggested Fix
Extract the FSR-frame-parameter-assembly block into a `fn build_fsr_frame_parameters(...)` free function alongside `dof_effective_view_proj`.

## Completeness Checks
- [ ] **SIBLING**: Same extraction pattern applied consistently with `dof_effective_view_proj`/`rebase_model_matrix`/`origin_corrected_prev_view_proj`
- [ ] **TESTS**: A regression test pins the extracted function's behavior independent of `draw_frame`
