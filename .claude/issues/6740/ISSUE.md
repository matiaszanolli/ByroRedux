# REN-D2-01: "only sample 0 runs the detailed trace" comment is stale after PAF-001

Labels: low,renderer,shaders,documentation,doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/6740

**Severity**: LOW · **Dimension**: Ray Queries

## Location
`crates/renderer/shaders/triangle.frag` (ReSTIR finalize "Mover taint" comment block) and the same sentence in `.claude/commands/audit-renderer/SKILL.md` Dim 2 ("Documented hole")

## Description
Since the PAF-001 hunk, samples `sr > 0` also call `traceLightTransmittanceDetailed` (for `sampleHitDistance`, the penumbra estimate) but discard `sampleMask` / `sampleInstance`, so the mover-taint hole is unchanged; the stated reason ("only sample 0 runs the detailed trace") is no longer true.

## Evidence
`uint sampleMask; uint sampleInstance; sampleTransmission = traceLightTransmittanceDetailed(i, rayOrigin, rayDir, rayTMax, sampleMask, sampleInstance, sampleHitDistance);` in the `else` arm of `if (sr == 0)`.

## Impact
None on rendering. It suggests widening the taint needs a new trace when it only needs the instance output kept.

## Suggested Fix
Reword to "only sample 0's blocker instance feeds the taint; samples > 0 run the detailed trace but drop their instance"; update the SKILL.md sentence the same way. A comment edit in `triangle.frag` leaves the compiled SPIR-V unchanged; confirm with `scripts/check-shader-artifacts.sh`.

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **SIBLING**: Same wording checked in related files (docs, other comments)
- [ ] **TESTS**: Covered by existing guards / `.claude/commands/_audit-validate.sh`; no new test needed for pure comment edits
