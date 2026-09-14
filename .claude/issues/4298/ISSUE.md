# #4298: REN-2026-09-14-D2-02: `shader-pipeline.md` descriptor table and submission order omit the new Set-1 binding 20 (`skyCube`) and the per-frame sky-cube bake

- **Labels**: low,renderer,doc-rot,documentation
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4298
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: Ray Queries (doc-rot)
- **Location**: `docs/engine/shader-pipeline.md` (Descriptor Sets table, Set 1 rows; Per-Frame Submission Order block); code source of truth `crates/renderer/src/vulkan/scene_buffer/buffers.rs` (`build_scene_descriptor_bindings`) and `crates/renderer/src/vulkan/context/build_and_upload_instances.rs` (`record_bake` call)
- **Status**: NEW
- **Description**: `6db9eac2` added scene Set 1 binding 20 (`COMBINED_IMAGE_SAMPLER`, `samplerCube skyCube`, FRAGMENT, `PARTIALLY_BOUND`, read by `raytrace.glsl` and `lighting.glsl`), and a per-frame `SkyCubePipeline::record_bake` compute dispatch recorded before the main render pass. `shader-pipeline.md` was not touched after `1efc5251`: its Set-1 table stops at binding 19 and its submission-order block has no sky-cube step. The skill tells auditors to prefer this doc over source for descriptor facts, and `skyal.md` is currently the only place binding 20 is documented.
- **Evidence**:
  - `grep -n "skyCube\|sky_cube" docs/engine/shader-pipeline.md` returns no hits. The only `| 20 |` row in the doc is `volumetrics_inject.comp`'s private `BoundaryVertexBuffer`.
  - `git log -1 -- docs/engine/shader-pipeline.md` is `1efc5251`, which predates `b54b86b7` / `6db9eac2`.
  - `grep -n "binding(20)" crates/renderer/src/vulkan/scene_buffer/buffers.rs` → the SKYAL binding.
- **Impact**: Documentation only. Anyone using the table to answer "which bindings does Set 1 carry / what must be written when the scene set is rebuilt" will miss a binding whose absence from a write is undefined data by design (`PARTIALLY_BOUND`), not a validation error. This is the same rot class as #3577 / #2918 / #4019.
- **Related**: #4019 (`the_descriptor_table_does_not_credit_private_layout_passes_with_global_sets` checks only "Used by" cells, not completeness); REN-2026-09-14-D3-01 (same commit, GpuCamera row); the submission-order half overlaps Dim 4 scope.
- **Suggested Fix**:
  - Add a `| 1 | 20 | COMBINED_IMAGE_SAMPLER | SKYAL baked sky cubemap (per frame in flight, gated by exteriorSkyTint.w) | triangle (raytrace.glsl / lighting.glsl) |` row.
  - Add a sky-cube bake step before the main render pass in the submission order.
  - Consider a doc-completeness pin that walks `build_scene_descriptor_bindings`' binding numbers against the Set-1 rows.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
