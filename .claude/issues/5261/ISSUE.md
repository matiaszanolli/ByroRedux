# #5261: CONC-D2-2026-10-05-01: #5188's geometry-dead arm turns RT off for the CI demo scene, so the validation lane no longer exercises any RT consumer

**Labels**: medium,concurrency,sync,renderer,test-gap,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5261

**Source**: `docs/audits/AUDIT_CONCURRENCY_2026-10-05.md` — `CONC-D2-2026-10-05-01` (HEAD `a2c24b16e`)

- **Severity**: MEDIUM. There is no runtime defect in shipped content. The cost is that the CRITICAL-surface validation gate is now
  blind, which is the #4596 / #4987 "inert lane" class.
- **Dimension**: Compute → AS → Fragment Chains
- **Location**:
  - The RT drop: `crates/renderer/src/vulkan/context/dispatch_skin_and_cluster.rs:428-441` (`geometry_dead` →
    `tlas_written[frame] = false`, `patch_camera_rt_flag(.., 0.0)`, and the skipped `if !tlas_build_failed && !geometry_dead` arm).
  - The caustic gate close: `crates/renderer/src/vulkan/context/sync_and_acquire_frame.rs:302-310`.
  - The demo scene: `byroredux/src/scene.rs:820-854` (`spawn_demo_primitives`, which uploads with `MeshRegistry::upload`).
  - The non-global path: `crates/renderer/src/mesh.rs:562` (`upload`, which never reaches `accumulate_global_geometry`).
- **Status**: NEW. Searched "rt_flag=0", "geometry_dead", "demo scene RT" and "validation lane RT"; there were no matches.
- **Verification Path**: CI log (already captured below). After a fix, the lane's `rt-integrity:` line must read
  `rt_flag=1 tlas_build=1` again.
- **Description**: #5188 correctly stops RT shading through stale scene-set bindings 8/9 after a failed geometry rebuild. Its trigger,
  though, is `self.mesh_registry.global_vertex_buffer.is_none()`, and that is also the permanent state of the content-free demo scene.
  The baseline already noted that the demo primitives never enter the global pool. The CI `vulkan-validation` job runs exactly that
  scene.

  The TLAS is still built, and the AS_WRITE → AS_READ barrier is still recorded. But with `rt_flag = 0` and `tlas_written = false`,
  none of the following run any more:
  - the fragment ray queries;
  - the caustic splat (its latch is closed by the new else arm);
  - the volumetrics TLAS path;
  - RT GI.
- **Evidence**: The `rt-integrity:` line in the `vulkan-validation` job:

  | Run | Commit | Result |
  |---|---|---|
  | 37151756863 | `3cf32ee5a` | `rt_flag=1 tlas_build=1` |
  | 37155077588 | `be3cd9468` | `rt_flag=1 tlas_build=1` |
  | 37161409881 | `2c36c29d8` (first run containing `d59f3c7a4`) | `rt_flag=0 tlas_build=0` |
  | every later run through 37341506292 | `23524b446` | `rt_flag=0 tlas_build=0` |
- **Trigger Conditions**: Every CI run, every frame (the demo scene has no global geometry).
- **Impact**: A new barrier, descriptor or layout hazard on any RT consumer cannot surface in CI. #5062 and #5064 were both found only
  because the lane did reach those consumers on 09-29.
- **Related**: #5188, #5064, #4987, #4596, CONC-D3-2026-10-05-01.
- **Suggested Fix**:
  - Upload the demo primitives through `upload_scene_mesh` (the global pool), so the demo scene has live geometry globals and RT stays on.
  - Add a lane assertion that the `rt-integrity:` line carries `rt_flag=1 tlas_build=1`, so a silent RT-off fails the job instead of passing it.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
