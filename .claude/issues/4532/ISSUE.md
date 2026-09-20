# REN-D8-2026-09-20-01: VOLUMETRIC_OUTPUT_CONSUMED doc credits the 3×3 XY blur deleted on 2026-08-16 — the stated reason the flag is on names a mechanism that no longer exists

- **ID**: REN-D8-2026-09-20-01
- **Labels**: low,renderer,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: Volumetrics
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D8-2026-09-20-01)

**Location**: `crates/renderer/src/vulkan/volumetrics.rs` — the VOLUMETRIC_OUTPUT_CONSUMED doc site

**Description**
5be840d2b deleted the 3×3 XY blur from volumetrics_integrate.comp; the flag's doc still justifies itself by it.

**Evidence**
Audit D8, 2026-09-20.

**Impact**
The next volumetrics gating edit inherits a false premise.

**Suggested Fix**
Rewrite the doc to the current consumer (composite reads the volumetric output directly).

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
