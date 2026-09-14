# #4308: REN-2026-09-14-D12-01: the "authoritative" per-frame submission order in `shader-pipeline.md` omits every pass added since the last sweep — the sky-cube bake, the ground-cover interaction/scatter compute with its fill/copy transfers, and the groun…

- **Labels**: low,renderer,doc-rot,documentation
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4308
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: Pipeline/RenderPass (Command Buffer Recording)
- **Location**: `docs/engine/shader-pipeline.md` (Per-Frame Submission Order, Compute shader table); recording sites: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (`record_bake` call), `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs` (`record_scatter` call), `crates/renderer/src/vulkan/context/geometry_pass.rs` (`record_draw` call)
- **Status**: NEW
- **Description**:
  - `shader-pipeline.md` describes its step list as the strict submission order and numbers each barrier. Three things now recorded every frame are missing:
    - **Ground cover (before 5b)**: `record_interaction`'s dispatch and its two memory barriers, three `vkCmdFillBuffer` calls, a `TRANSFER → COMPUTE` barrier, the scatter dispatch, the #4181 publish barrier, and a `vkCmdCopyBuffer` readback. All sit between step 5 (cluster cull) and 5b.
    - **Sky-cube bake (before 5b)**: `record_bake`'s two image barriers and its compute dispatch, recorded in `build_and_upload_instances` ahead of the 5b HOST barrier.
    - **Ground-cover draw (inside step 6)**: blade/debug draw after water.
  - The compute-shader table likewise omits `sky_cube.comp`, `groundcover_scatter.comp` and `groundcover_interaction.comp`. No section documents the raster shaders `groundcover_blade.vert` / `groundcover_blade.frag` or `groundcover_debug.frag`.
  - `grep -in 'sky.cube|groundcover|ground.cover' docs/engine/shader-pipeline.md` returns only the `exterior_sky_tint` GpuCamera row, which D11-04 covers.
- **Evidence**:
  - The call sites listed under Location, confirmed by the phase-order trace in Checked above.
  - Commit `6db9eac2` ("baked every frame before the geometry pass") and `#4054`/`#4055` (ground cover) touched neither the doc's step list nor its shader tables.
- **Impact**: An auditor or contributor reasoning about barrier coverage from the doc's step list, which the skill tells auditors to cross-check against, cannot see two compute passes, a readback and a draw, including the sites of D4-01's hazard and #4181's fix. No runtime effect.
- **Related**: REN-2026-09-14-D11-04 (same doc: binding 20 and the `exterior_sky_tint.w` lane); REN-2026-09-14-D4-01; #4033 (closed — prior omission of barrier steps from this same list).
- **Suggested Fix**: Add numbered steps for the ground-cover interaction/scatter (with its fills, barriers and readback) and for the sky-cube bake between steps 5 and 5b, and a ground-cover draw line under step 6. Add the three compute shaders and the ground-cover raster shaders to the shader tables.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
