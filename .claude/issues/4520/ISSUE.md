# REN-3-2026-09-20-01: nothing pins the GLSL detail-combine (detailSample / max(mat.detailNeutral,1e-4)) or the two-condition tint gate — a revert to ×2.0 passes every test

- **ID**: REN-3-2026-09-20-01
- **Labels**: low,renderer,shaders,test-gap,bug
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: GPU-Struct Layout
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-3-2026-09-20-01)

**Location**: `crates/renderer/shaders/triangle.frag` (detail combine + tint gate); CPU legs pinned by #4422/#4423

**Description**
#4422/#4423 fixed the detail ×2 darkening and the tint collapse with CPU-side pins at every leg — except the GLSL arithmetic itself. A shader-side revert to the old multiply passes the whole suite; the exact lines that were the HIGH bugs of 2026-09-16 are the least-guarded ones.

**Evidence**
Audit D3 attempted and confirmed: no source-shape or value test exercises those two expressions.

**Impact**
The highest-risk regression surface of the last month has zero guard.

**Suggested Fix**
One source-shape test per expression (detail divide-by-neutral; tint gated on alpha weight bit) in shader_contract_tests.rs.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
