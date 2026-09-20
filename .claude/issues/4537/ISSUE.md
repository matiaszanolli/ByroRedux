# REN-D10-2026-09-20-02: GI-priority sort rationale still names giHitIrradiance — deleted in #4017; the sort remains load-bearing for the overflow-drop policy and determinism

- **ID**: REN-D10-2026-09-20-02
- **Labels**: low,renderer,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: Light Animation
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D10-2026-09-20-02)

**Location**: `byroredux/src/render/lights.rs` (3 comment clusters) + `gi_light_priority_tests` doc

**Description**
#4017 replaced giHitIrradiance with pathHitRadiance scanning all lights with per-hit top-K; the comments still explain the sort by the dead consumer. The sort is still required (MAX_LIGHTS overflow drops the lowest-scoring tail — the policy 8ad6b0de8's warn documents) but for a different reason than stated.

**Evidence**
Audit D10, 2026-09-20.

**Impact**
A future 'the GI consumer is gone, drop the sort' edit would break overflow determinism.

**Suggested Fix**
Rewrite the rationale around the overflow-drop policy + determinism.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
