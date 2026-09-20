# REN-D4-2026-09-20-04: shader-pipeline.md's authoritative submission order predates #3572 (TAA still at step 15, pre-composite) and #4182 (step 5b asserts the retired HOST_COHERENT premise)

- **ID**: REN-D4-2026-09-20-04
- **Labels**: low,renderer,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: Pipeline/RenderPass
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D4-2026-09-20-04)

**Location**: `docs/engine/shader-pipeline.md` (submission-order step list; `:141` for the HOST_COHERENT claim)

**Description**
The doc every audit is told to verify frame order against is itself two changes behind the code. Second consecutive audit where this block rotted after a pass move.

**Evidence**
Audit D4, 2026-09-20; verified against draw_frame at HEAD.

**Impact**
Future audits verify against a stale order; the #4182 correction never reached the doc copy.

**Suggested Fix**
Re-sync the step list to post-#3572 order and the corrected HOST-barrier premise; adopt the audit's new 'doc moves with the pass' skill rule.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
