# REN-D8-02: The god-ray contact test takes a bilinearly filtered depth sample through the LINEAR `hdr_sampler`

Labels: low, renderer, shaders, water, bug
Source: docs/audits/AUDIT_RENDERER_2026-10-27.md

**Source**: `docs/audits/AUDIT_RENDERER_2026-10-27.md` — finding `REN-D8-02` (LOW, Dim 8 Water / Pipeline; same class as closed #299, SSAO's bilinear depth read)
**Location (current)**: `crates/renderer/shaders/composite.frag` (`underwaterSunVisibility`, line ~552); `crates/renderer/src/vulkan/composite.rs` (binding 4 = `depth_read_only_image_info(partial.hdr_sampler, depth_view)`).

## Description
`underwaterSunVisibility` reads `linearViewDepth(uv, texture(depthTex, uv).r)`. Every other depth tap in `composite.frag` is a `texelFetch`, which ignores the sampler. This tap goes through the LINEAR `hdr_sampler`, with two consequences:
- (a) Across a silhouette it averages near and far depth into a phantom depth in between, so the `6 < behind < 160` slab test misclassifies a 1-px halo around every occluder.
- (b) LINEAR filtering of `D32_SFLOAT` requires `SAMPLED_IMAGE_FILTER_LINEAR`, which Vulkan does not mandate for depth formats (VUID-vkCmdDraw-magFilter-04553), and `find_depth_format` checks only `DEPTH_STENCIL_ATTACHMENT`. RT-class desktop GPUs expose the feature, so this is a portability gap, not an error on the dev card.

## Evidence
```glsl
float scene = linearViewDepth(uv, texture(depthTex, uv).r);   // composite.frag ~552
```
versus e.g. `texelFetch(depthTex, ivec2(gl_FragCoord.xy), 0).r` at ~656 and the neighbour taps at ~450-453.

## Impact
Visual halo artifacts around occluders in underwater god rays; a validation / portability risk on GPUs without linear depth filtering.

## Related
#299 (closed, SSAO bilinear depth), #6415, #6418 (exterior god-ray findings).

## Suggested Fix
`texelFetch(depthTex, ivec2(uv * vec2(textureSize(depthTex, 0))), 0).r`, matching every other depth reader in the file.

## Completeness Checks
- [ ] **SIBLING**: Other `texture(depthTex, ...)` / filtered depth taps across composite/presentation/water shaders are checked
- [ ] **SHADER-STRUCT**: Recompile `composite.frag.spv` (plain `-V`) and run `scripts/check-shader-artifacts.sh`
- [ ] **TESTS**: A source-scan or reflection test pins that depth is only read through `texelFetch` in `composite.frag`

