# REN-D3-01: explicit-gradient guard does not cover byroTerrainBlendCore

- **Issue**: #6347 (snapshot as filed; GitHub is authoritative for live state)
- **Labels**: low, renderer, shaders, test-gap, bug
- **Source**: docs/audits/AUDIT_RENDERER_2026-10-24.md

- **Severity**: LOW (test gap; the code is correct today)
- **Dimension**: GPU-Struct Layout (shader-contract guard coverage)
- **Location**: `crates/renderer/src/vulkan/scene_buffer/terrain_parallax_tests.rs` `glsl_parallax_uses_only_explicit_gradients`; `crates/renderer/shaders/include/bindings.glsl` `byroTerrainBlendCore` / `byroTerrainBlendedHeight`; `terrain_height_blend_tests.rs` `glsl_blend_fades_each_lane_with_its_share` pins the core's arithmetic but not its sampling form.
- **Status**: NEW (`b81a43443`)
- **Description**: the guard bans `texture(`, `dFdx(`, `fwidth(` and `textureQueryLod(` only inside `terrainParallaxDisplaceUV`. Every height read the march makes happens in `byroTerrainBlendCore`, reached through `byroTerrainBlendedHeight`, where `textureGrad(textures[nonuniformEXT(idx)], uv, dx, dy)` is correct today. The core also backs `byroTerrainHeightBlend`, which runs in uniform flow, so an editor there has no local reason to keep `textureGrad`; an implicit `texture()` added to the core would be undefined behaviour inside the march loop and pass every test.
- **Suggested Fix**: apply the same banned-token loop to `function_body(bindings, "float byroTerrainBlendCore(")` and `"float byroTerrainBlendedHeight("`.


## Completeness Checks
- [ ] **SIBLING**: the banned-token loop also covers `byroTerrainBlendedHeight` and any other helper reachable from inside the march loop
- [ ] **TESTS**: the test fails if `texture(`, `dFdx(`, `fwidth(` or `textureQueryLod(` is introduced in `byroTerrainBlendCore`

_Filed from `docs/audits/AUDIT_RENDERER_2026-10-24.md` (static review; no engine or Vulkan device was launched). Failures invisible to `cargo test` are flagged NEEDS-RENDERDOC in the report._
