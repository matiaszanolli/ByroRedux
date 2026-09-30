# #4845: REN-D2-2026-09-24-03: `rt_hit_shaders_have_no_unsafe_vertex_data_reads` cannot match the `u`-suffixed indices the shaders actually use

**Labels**: bug,renderer,low,shaders,test-gap

_Filed from `docs/audits/AUDIT_RENDERER_2026-09-24.md` (full renderer audit, audited `main` @ `6c5555c70`; delta baseline `f97775ca8`). Finding ID: **REN-D2-2026-09-24-03**._

- **Severity**: LOW (test gap; the code it guards is currently correct).
- **Dimension**: Ray Queries
- **Location**: `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs` — `rt_hit_shaders_have_no_unsafe_vertex_data_reads` (needles `format!("+ {}]", forbidden)` / `format!("+{}]", forbidden)`).
- **Status**: NEW (#4018 fixed the discovery/completeness half; the needle was not touched).
- **Description / Evidence**: The scan looks for `+ 12]`..`+ 15]` and `+ 20]`/`+ 21]`, but every shader writes these indices with a `u` suffix (`+ 12u]`), so the needle can never match. `grep '+ 1[2-5]u\?\]'` finds `skin_vertices.comp` and, new in `b9e961eeb`, `include/ray_hit.glsl` `getHitVertexTransform` (`vertexData[base + 12u]`, on a line without `floatBitsToUint`). A scratch check showed `float raw = vertexData[base + 12u];` "NOT flagged" while `+ 12` is flagged. The correct new code would have panicked the test had the needle matched.
- **Impact**: A future RT/skinning author who reads a u32 bone-index lane or the packed u8 splat lanes as a float in the codebase's own `NNu` style gets no failure: the #575 / SH-1 hazard the test exists for.
- **Suggested Fix**: Make the needle suffix-tolerant, allow a multi-line wrapper (the new `getHitVertexTransform` needs that), and add a positive-control fixture like `the_depth_literal_scanner_recognises_the_shapes_it_exists_to_catch`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)

