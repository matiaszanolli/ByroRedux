# TD1-001: draw_frame() regressed to 3325 LOC (regression of closed #1052)

_Filed 2026-06-26 as #1748 from docs/audits/AUDIT_TECH_DEBT_2026-06-26.md (immutable snapshot; query `gh issue view 1748` for live state)._

**Severity**: MEDIUM (stale-fix regression) · **Dimension**: 1 — Function Complexity
**Location**: `crates/renderer/src/vulkan/context/draw.rs:410-3735`
**Status**: Regression of #1052 (CLOSED) — filed fresh per audit §8
**Audit**: docs/audits/AUDIT_TECH_DEBT_2026-06-26.md (TD1-001)

## Description
`draw_frame()` is a single 3325-LOC function. #1052 (CLOSED) extracted it to "2322 LOC"; it has since grown +1000 LOC as M55/M58 (volumetrics + bloom) passes were appended inline. It is the worst single complexity site in the repo and holds the worst nesting (466 lines indented ≥6 levels).

## Evidence
Separable phases (verified against begin/end command-buffer markers):
- acquire + fence + setup ~410-670
- GPU skinning + skinned-BLAS lifecycle ~671-1700 (~1000 LOC, largest sub-block)
- camera / jitter / instance + material upload ~782-2300
- geometry render pass ~2516-3114
- denoise + post (SVGF/volumetrics/TAA/SSAO/bloom/composite) ~3136-3463
- submit + present ~3509-3735

## Impact
Unreviewable in one pass; the skinned-BLAS state machine is hidden in deep nesting; every new post-pass appends inline so the function monotonically grows.

## Suggested Fix
Extract `&mut self` private helpers each taking the open `cmd: vk::CommandBuffer` and recording one phase, called in the same order: `record_skinning_and_skinned_blas` ←671-1700, `record_geometry_pass` ←2516-3114, `record_denoise_and_post` ←3136-3463. Target host fn <600 LOC. **Mechanical only — do not move/merge any `cmd_pipeline_barrier` across a boundary** (see feedback_speculative_vulkan_fixes). Consider reopening #1052.

## Completeness Checks
- [ ] **DROP**: No Vulkan object lifetimes change; Drop stays reverse-order correct
- [ ] **SIBLING**: No barrier/recording-order change vs current `draw_frame`
- [ ] **TESTS**: Existing draw.rs unit tests still pass; the extraction is behavior-preserving
