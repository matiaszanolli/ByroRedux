# NIF-10: NiPathInterpolator comment promises dispatch but no arm exists (138 blocks)

**Severity**: MEDIUM | **Dimension**: Coverage Gaps | **Game**: FO3, FNV, Skyrim SE | **Audit**: docs/audits/AUDIT_NIF_2026-04-22.md § NIF-10

## Summary
The comment at `blocks/mod.rs:490` references `NiPathInterpolator` as the post-10.2 replacement for `NiPathController`, but no dispatch arm exists. 138 blocks across 3 games fall into NiUnknown. Spline-path interpolation on cutscene cameras + environmental animations silently no-ops.

## Evidence
Skyrim SE: 71, FNV: 52, FO3: 15 blocks.

## Location
`crates/nif/src/blocks/mod.rs` — dispatch arm, `crates/nif/src/blocks/interpolator.rs` — parser.

## Suggested fix
Per nif.xml `NiPathInterpolator` — has `pos_data_ref + float_data_ref` (for the curve's XYZ + speed channels). ~30 LOC parser + dispatch + animation-system wiring (separate PR).

## Completeness Checks
- [ ] **SIBLING**: Compare to existing `NiPathController` parser — the interpolator replaces the controller post-10.2
- [ ] **TESTS**: Synthetic block fixture
- [ ] **REAL-DATA**: Three sweeps drop this bucket

Fix with: /fix-issue <number>
