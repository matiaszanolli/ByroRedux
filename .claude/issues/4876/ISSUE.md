# #4876: REN-D9-2026-09-24-06: skin/morph doc drift — step 1a barrier attribution and the skin_vertices.comp header after b9e961eeb

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D9-2026-09-24-06**._

**Severity**: LOW · **Status**: NEW · every claim below was checked against the code by the auditor and re-checked against the tip of `main` at publish time.

**Dimension**: Skinning

`shader-pipeline.md` step 1a attributes morph-weight visibility to the step-5b barrier (the real guarantee for the step-3 consumer is the submit-time host-write rule); the `skin_vertices.comp` header says RT hit shading has no reader for skinned normals/tangents, stale since `b9e961eeb`'s `getRayHitTangentFrame`.

## Completeness Checks
- [ ] **SIBLING**: The same stale claim is checked in the neighbouring docs and code comments (doc moves with the pass)

