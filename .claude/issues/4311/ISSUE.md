# #4311: REN-2026-09-14-D16-01: `bloom_downsample.comp`'s "4-tap bilinear = 4×4 box" is geometrically a 2×2 box: taps offset ±0.5 *source* texels from a destination centre land exactly on source texel centres

- **Labels**: low,renderer,shaders,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4311
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: Bloom
- **Location**: `crates/renderer/shaders/bloom_downsample.comp` (`main`, header comment), `crates/renderer/src/vulkan/bloom.rs` (`BloomPipeline::upload_params`)
- **Status**: NEW
- **Description**:
  - The shader header states: "four taps placed at (±0.5, ±0.5) destination-pixel offsets cover an effective 4×4 source region with even weighting. Provably equivalent to a 4×4 box filter". That is the stated justification for choosing a box over Jimenez's 13-tap.
  - The code offsets by `src_pixel`, not destination pixels: `texture(src, uv + vec2(±0.5, ±0.5) * src_pixel)`, where `inv_resolutions.xy = 1 / src_extent` (`upload_params`).
  - Mip 0 has `src_extent = self.extent` (the render extent passed to `BloomPipeline::new`) and `dst = extent / 2`. Deeper levels halve again.
  - For an even source dimension, the destination texel `i` centre sits at source coordinate `(i + 0.5)·2 = 2i + 1`: the shared corner of source texels `2i` and `2i+1`.
  - A ±0.5 source-texel offset lands exactly on those two texel centres (`2i+0.5`, `2i+1.5`), so each bilinear tap degenerates to a single-texel point sample.
  - The four taps therefore average exactly the 2×2 footprint, with no overlap between neighbouring destination texels: the plain box-mip downsample, not a 4×4 filter. An odd source dimension (e.g. 1080-high chains at 135→67) only drifts slightly off that.
  - The 4×4 footprint the comment describes needs ±1 source-texel offsets. Those put each tap on a texel corner, so bilinear averages 2×2 per tap across a 4×4 region.
- **Evidence**:
  - `bloom_downsample.comp`: `vec2 src_pixel = params.inv_resolutions.xy;` … `texture(src, uv + vec2(-0.5, -0.5) * src_pixel)`.
  - `upload_params`: `1.0 / src_extent.width as f32` in lanes xy; mip-0 extent `(screen_extent.width / 2).max(1)` in the frame-state constructor.
- **Impact**:
  - The documented filter property does not hold, and it is the stated basis for "lands 80% of the visual win".
  - A non-overlapping 2×2 box chain is the downsample known to make small bright features pulse as they cross texel boundaries under sub-pixel motion. That now matters more, because since `62a09fd9` bloom is dominated by exactly those small above-threshold features (sun disc, emissive points).
  - Visual severity needs an in-engine A/B; it is not asserted here. No correctness or VRAM effect.
- **Related**: #2805 (closed, `BLOOM_INTENSITY` derivation), #1275 (upsample DC-gain note), `62a09fd9`.
- **Suggested Fix**: Either offset the taps by ±1.0 `src_pixel` to get the 4×4 footprint the comment promises (keeping the per-tap bright-pass and 0.25 weights), or correct the comment to say "2×2 box". Re-tune the knee numbers only if the footprint changes.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
