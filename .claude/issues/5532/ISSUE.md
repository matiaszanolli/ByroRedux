# #5532: REN-D3-2026-10-09-01: today's two renderer changes left their lane and order descriptions stale — five `CameraUBO` mirrors and `shader-pipeline.md` omit history mode 3, and `renderer.md` / `shader-pipeline.md` still show the pre-#5482…

**Labels**: doc-rot, documentation, low, renderer, shaders

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-09.md` — finding `REN-D3-2026-10-09-01` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW
- **Dimension**: GPU-Struct Layout (lane semantics) / FSR/Presentation (docs)
- **Location**:
  - The `vec4 dofParams;` comment in `crates/renderer/shaders/include/bindings.glsl`, `triangle.vert`, `water.vert`, `cluster_cull.comp` and `caustic_splat.comp`.
  - The `dof_params` row of the `GpuCamera` table in `docs/engine/shader-pipeline.md`.
  - The `presentation.frag` pass-table row and the presentation step in `docs/engine/shader-pipeline.md`.
  - The `presentation.rs` tree row and the presentation step in `docs/engine/renderer.md`.
- **Status**: NEW (related to #5022, which established the lane-description pin).
- **Description**:
  - Every mirror reads "w = history mode (0 moving, 1 parked, 2 parked + scene-static; …)". Mode 3 (#5369) is missing, although the Rust doc on `GpuCamera` and `restir_history_mode` describe it.
  - `every_camera_ubo_mirror_describes_dof_w_as_the_history_mode` matches that text as a prefix, so it stays green.
  - The presentation docs still give `tonemap(compressed * exposureTex)` and never mention the grade. Since #5482 the code is `exposed = scene × exposure` → grade (with the toe) → chroma compress → `tonemap(compressed)`.
  - `docs/engine/ui.md`'s `aces(graded * exposure)` is explicitly historical (pre-#3426), so it is not stale.
- **Suggested Fix**:
  - Append "3 parked + rig-geometry-static under flicker" to the five mirror comments and the table row, and extend the #5022 pin to require it.
  - Rewrite the two presentation passages to name the exposure → grade → tonemap order.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
