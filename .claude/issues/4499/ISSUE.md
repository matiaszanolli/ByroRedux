# EXT-D3-2026-09-19-04: post-#4378 GLSL literal census — record the R2 constants as cited-math in §12.12

- **ID**: EXT-D3-2026-09-19-04
- **Labels**: low,terrain-exterior,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4499

**Severity**: LOW (bookkeeping) · **Dimension**: Ground cover (shader-constant hygiene)
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D3-2026-09-19-04)

**Location**: `crates/renderer/shaders/include/groundcover_candidate.glsl:31-32`; census register at `docs/engine/exal-groundcover.md` §12.12

**Description**
A full literal sweep of all 15 `groundcover_*` GLSL files against the #4378/§12.12 census (diff from `4fc8ab8b2`, 2026-09-15) found exactly one numeric arrival since: `const float A1 = 0.7548776662466927; const float A2 = 0.5698402909980532;` — the R2 sequence's generalised-golden constants, introduced with `groundcover_candidate.glsl` by `242cbb451`. They carry an in-file citation (Roberts 2018, plastic constant 1.32471795724474602596) — the same exempt-by-class category as the guard-pinned hash constants, not uncited tuning values. Filed so the next census sweep records the disposition instead of re-flagging them. No uncited arrivals; no game tokens downstream of `groundcover_translate.rs`. The nearest #4378-class straggler remains the pre-existing debug-only `gl_PointSize` ramp (cosmetic, §12.12-visible).

**Related**: #4378; exal-groundcover.md §12.12

**Suggested Fix**
One sentence in §12.12's list noting the R2 constants are cited-math (Roberts 2018) and exempt; optionally cite a source for the debug point-size ramp next time §12.12 is touched.

## Completeness Checks
- [ ] **TESTS**: N/A (docs)
