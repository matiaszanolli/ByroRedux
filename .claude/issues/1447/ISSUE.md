# #1447 — PIPE-01/SHDR-01: Committed SPIR-V stale vs GLSL after DoF commit

_Snapshot as filed (2026-06-02) from AUDIT_RENDERER_2026-06-02.md. GitHub is authoritative for live state._

- **Severity**: HIGH
- **Dimension**: Pipeline State / Shader Correctness
- **Location**: `crates/renderer/shaders/{triangle.vert,triangle.frag,cluster_cull.comp,water.vert,caustic_splat.comp}.spv`; root cause commit `400fa68f`; ship path `crates/renderer/src/vulkan/pipeline.rs:9-10` + `compute.rs:18` (`include_bytes!`)
- **Status**: NEW

## Description
Commit `400fa68f` ("stochastic depth of field", 2026-06-01) appended `vec4 dofParams` to the `CameraUBO` block in 6 shader sources and grew Rust `GpuCamera` 304→320 B (`crates/renderer/src/vulkan/scene_buffer/gpu_types.rs:257`), and even updated the size test — but committed **no recompiled `.spv`**. `crates/renderer/build.rs` does not compile GLSL→SPIR-V (warning-only); shaders ship as committed binaries via `include_bytes!`, so the **stale binary is the runtime shader**.

## Evidence
Recompile + `cmp` confirms 5 committed `.spv` differ from their current GLSL: `triangle.vert`, `triangle.frag`, `cluster_cull.comp`, `water.vert`, `caustic_splat.comp` are STALE; `water.frag`, `composite.*`, `ssao.comp`, `svgf_temporal.comp`, `taa.comp`, `skin_vertices.comp` are CURRENT. `git show --stat 400fa68f` lists `triangle.frag | 3 +-` (GLSL) with **no `.spv`** in the file list.

```
$ glslangValidator -V triangle.frag -o /tmp/x.spv && cmp triangle.frag.spv /tmp/x.spv
triangle.frag.spv /tmp/x.spv differ: byte ...
```

## Impact
**Benign today** — `dofParams` is the trailing `CameraUBO` member, read 0 times in every shader (DoF is applied CPU-side via view-matrix displacement), and an over-sized UBO bind is spec-legal (no validation error, no artifact). **Latent CRITICAL**: the next mid-`CameraUBO` field insertion, or any shader read at/after offset 304, silently corrupts every camera-UBO consumer (position, view/proj, jitter, sky_tint, sun_direction) with **zero** test or validation-layer coverage. `reflect::validate_set_layout` checks binding index/type only, not member bytes, so it cannot catch this.

## Suggested Fix
1. Recompile + commit the 5 stale `.spv`.
2. Add a `cargo test` that recompiles each `shaders/*.{vert,frag,comp}` with `glslangValidator` and `cmp`s against the committed `.spv` — fail on drift. This closes the structural gap (`build.rs` warning-only) that let source and binary diverge silently.

## Related
Same class as the shader-struct-sync hazard in `feedback_shader_struct_sync.md` (GpuInstance lockstep across 5 shaders), but on the *compiled-output* axis rather than the source axis.

## Completeness Checks
- [ ] **SIBLING**: all 5 stale `.spv` recompiled (triangle.vert/frag, cluster_cull, water.vert, caustic_splat), not just one
- [ ] **TESTS**: GLSL↔`.spv` drift test added and fails on an intentionally-edited `.glsl`
- [ ] Verify `build.rs` warning is upgraded or the test covers the gap it leaves

_Filed from [docs/audits/AUDIT_RENDERER_2026-06-02.md](../blob/main/docs/audits/AUDIT_RENDERER_2026-06-02.md)._
