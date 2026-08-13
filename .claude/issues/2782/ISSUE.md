# REN-D15-01: occluded water fragments still shade and deposit caustics — water.frag has no early_fragment_tests

- **Severity**: MEDIUM
- **Dimension**: 15 — Water
- **Location**: `crates/renderer/shaders/water.frag` (`main`, the caustic block and the `traceWaterRay` calls above it); pipeline state in `crates/renderer/src/vulkan/water.rs` (`build_pipeline`, `depth_test_enable(true)` / `depth_write_enable(false)`)
- **Description**: `water.frag` writes a storage image (`imageAtomicAdd`). Per Vulkan early-per-fragment-test rules an implementation may only hoist the depth test ahead of shading when the shader has no side effects, or declares `EarlyFragmentTests`. `water.frag` declares neither, so every rasterized water fragment runs the full shader (two `traceWaterRay` walks, foam ray, sun shadow trace, floor ray) and deposits into the caustic accumulator before the depth test decides visibility. The colour blend is discarded correctly by the late depth test; the caustic deposit is not, since it targets a storage image not an attachment.
- **Evidence**: No `early_fragment_tests` declaration anywhere in `water.frag`. Unguarded `imageAtomicAdd(waterCausticAccum, pixel, fixed_val);` at the tail of `main`. `water.rs` confirms `.depth_test_enable(true)` / `.depth_write_enable(false)`. Contrast `caustic_splat.comp`, a compute pass sourced from the G-buffer, so visible pixels only by construction.
- **Impact**: Exterior-only (gated on `sunDirection.w > 0.0`). (1) Correctness/light leak: an occluded-from-camera but sunlit water plane still projects its refracted floor hit to screen space; the `uv01` guard checks on-screen projection, not visibility, so caustics land on the wall in front of the water. (2) Cost: the dominant per-fragment cost is paid for the fully-occluded portion of every water quad in the frustum.
- **Related**: #779 (OPEN, same execution-mode class, `triangle.frag` only, perf-only, does not cover this side-effect half); `caustic_splat.comp`'s G-buffer-sourced design as contrasting sibling.
- **Suggested Fix**: Declare `layout(early_fragment_tests) in;` in `water.frag`. Needs RenderDoc/frame-timing verification before landing.

## Completeness Checks
- [ ] TESTS: SPIR-V reflection check for `OpExecutionMode EarlyFragmentTests`, where feasible
- [ ] Needs RenderDoc/frame-timing verification before landing, not just cargo test

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2782
