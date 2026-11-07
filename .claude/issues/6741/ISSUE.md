# REN-D2-02: `phase2_gi_energy_contracts` doc comment still describes the reverted RLT-GI-012 gate

Labels: low,renderer,documentation,doc-rot
URL: https://github.com/matiaszanolli/ByroRedux/issues/6741

**Severity**: LOW · **Dimension**: Ray Queries

## Location
`crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs`, doc comment on `phase2_gi_energy_contracts` (third bullet)

## Description
The doc says the GI gate reads the isotropic footprint with the full-range ceiling. The body (after `a89ca367e`) asserts the opposite: `RT_LOD_GI = 2.0`, `giTransportEligible && rtLOD < RT_LOD_GI`, and no `rtLodGi`.

## Evidence
The `"RLT-GI-012 deferred"` assertion messages in the same function; `triangle.frag` itself documents the revert correctly.

## Impact
None on rendering. A maintainer may "restore" the widening the test exists to block (+3..11 ms measured).

## Suggested Fix
"RLT-GI-012 is deferred to the Q5 radiance cache; the Phase 1b gate is pinned below."

Source: `docs/audits/AUDIT_RENDERER_2026-11-07.md` (delta audit against baseline 670f81917, HEAD 9409e7b20).

## Completeness Checks
- [ ] **SIBLING**: Same wording checked in related files (docs, other comments)
- [ ] **TESTS**: Covered by existing guards / `.claude/commands/_audit-validate.sh`; no new test needed for pure comment edits
