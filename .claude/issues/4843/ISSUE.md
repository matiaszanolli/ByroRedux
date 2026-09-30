# #4843: REN-D1-2026-09-24-04: the #4779 pin does not cover the flag lifecycle it depends on

**Labels**: bug,renderer,low,sync,test-gap

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D1-2026-09-24-04**._

- **Severity**: LOW (test gap; the code is correct today).
- **Dimension**: AS Correctness
- **Location**: `context/dispatch_skin_and_cluster.rs` — `stale_tlas_compute_gate_tests::compute_ray_query_passes_take_the_build_gated_tlas`; field `tlas_build_succeeded_last_frame` (`context/mod.rs`).
- **Status**: NEW
- **Description / Evidence**: `ray_query_tlas` is only as good as `tlas_build_succeeded_last_frame` being cleared *before* `build_tlas` and set *only* in the `!tlas_build_failed` arm. The test asserts the helper's `.filter(..)`, the scatter call and the volumetrics recorder, and nothing about the reset/set sites. Removing `self.tlas_build_succeeded_last_frame = false;` still passes and reopens #4779 silently. `record_groundcover_models` (uses `ray_query_tlas`, correct) is unpinned; caustic deliberately keeps the raw handle and relies on the shader's `sceneFlags.x < 0.5` early-out plus `patch_camera_rt_flag(.., 0.0)`. The field name says "last frame" while holding *this* frame's result.
- **Impact**: A silent regression path for a freed-BLAS read (device loss); no current defect.
- **Suggested Fix**: Extend the source-shape test to assert the reset precedes `build_tlas` and the set is inside the success arm (runtime-composed needles), add a third arm for `record_groundcover_models`, consider renaming the field to `tlas_built_this_frame`, and add a pointer in `record_caustic_splat_pass` to the shader gate it relies on. No fault-injection hook forces a `build_tlas` failure, so a device confirmation needs one (see *Needs-RenderDoc*).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)

