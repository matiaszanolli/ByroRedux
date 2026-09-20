# REN-D8-2026-09-20-03: traceWaterRay header still quantifies the miss term as '~14%' — the number 7996edf61's normalization deleted (0% at the ramp end)

- **ID**: REN-D8-2026-09-20-03
- **Labels**: low,renderer,water,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: Water
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D8-2026-09-20-03)

**Location**: `crates/renderer/shaders/water.frag` — `traceWaterRay` header

**Description**
7996edf61 normalized the water ray-budget ramp; the header's quantified miss share predates it.

**Evidence**
Audit D8, 2026-09-20.

**Impact**
Cosmetic doc rot inside a hot shader's most-read comment.

**Suggested Fix**
Delete or re-derive the number.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
