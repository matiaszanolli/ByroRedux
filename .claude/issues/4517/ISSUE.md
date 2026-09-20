# REN-D1-2026-09-20-01: mask_divert_cause doc and pin claim 'same predicates / whole input space' — both stale after 84bbc44ed's blended-actor policy

- **ID**: REN-D1-2026-09-20-01
- **Labels**: low,renderer,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: AS Correctness
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D1-2026-09-20-01)

**Location**: `crates/renderer/src/vulkan/acceleration/predicates.rs:1047` (`mask_divert_cause`) + its pinning test

**Description**
84bbc44ed changed shadow-mask policy so blended RenderLayer::Actor draws keep the opaque shadow bucket; the doc's 'same predicates in the same order' premise and the test's 'whole input space' claim no longer describe the sweep (Actor layer + one Architecture point). Values still agree on every manually-enumerated input cell — doc/test premise rot, not a behavior bug.

**Evidence**
Enumeration performed during the 2026-09-20 audit (D1).

**Impact**
The next mask-policy edit starts from a doc/test that overstate their coverage.

**Suggested Fix**
Reword the doc to the post-84bbc44ed policy; extend the test sweep to the new input space.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
