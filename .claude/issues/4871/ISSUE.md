# #4871: REN-D4-2026-09-24-03: frame-order docs lag the code — groundcover_models step, #4602 flush edge, 'eight' vs ten record_*_pass helpers, debug-mode list, verdict prose

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D4-2026-09-24-03**._

**Severity**: LOW · **Status**: NEW · every claim below was checked against the code by the auditor and re-checked against the tip of `main` at publish time.

**Dimension**: Pipeline/RenderPass; MERGED with D12-06

`shader-pipeline.md` has no row for `groundcover_models.comp`, no submission-order step for it (it records between `build_and_upload_instances` and `record_geometry_pass`; its indirect draws run in the main pass before the first blended batch) and no step for the #4602 flush edge; `post_passes.rs` rustdoc says "eight" `record_*_pass` helpers in the pre-#3572 order, but there are ten (svgf, caustic, volumetrics, ssao, composite, bloom, exposure meter, taa, upscale, presentation), and the infallibility invariant is anchored on that count; the #1211 rustdoc block in `draw.rs` sits above the wrong test module; `docs/engine/renderer.md` lists 9 of 16 `RenderDebugMode`s (missing `volumetric_term`, `water_term`, `water_normal`, `terrain_lod`, `water_refl`, `facing_ratio`, `restir_light`); the `RtIntegrityStats` doc says "joins the three gates" while listing four clauses and omitting `tlas_build_succeeded`, and `LodCoverageStats::verdict` says "three-state" while `RtIntegrityStats::verdict` is now four-state. The 09-21 suggestion of a test that `shader-pipeline.md` names every `record_*_pass` is still unimplemented.

## Completeness Checks
- [ ] **SIBLING**: The same stale claim is checked in the neighbouring docs and code comments (doc moves with the pass)

