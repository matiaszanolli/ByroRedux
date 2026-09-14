# #4309: REN-2026-09-14-D13-01: `taa.comp` and `coverage_alpha_factors` both still say HDR alpha has "no other consumer" / is "harmless today", but under `--upscaler taa` composite's sky arm reads TAA's forwarded alpha as transparent coverage

- **Labels**: low,renderer,shaders,test-gap,bug
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4309
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-14.md

Source: `docs/audits/AUDIT_RENDERER_2026-09-14.md` (renderer audit, HEAD `147d97c3`)

- **Severity**: LOW
- **Dimension**: TAA
- **Location**:
  - `crates/renderer/shaders/taa.comp` (`main`, comment above `vec4 curr = texelFetch(uCurrHdr, ...)`)
  - `crates/renderer/src/vulkan/pipeline.rs` (`coverage_alpha_factors` doc)
- **Status**: NEW
- **Description**:
  - `taa.comp` justifies forwarding `currA` at all three `imageStore` sites like this: "harmless today (composite forwards .a to the swapchain which ignores it) but a future composite branch that gates on alpha … would silently see a zeroed bit".
  - `coverage_alpha_factors`'s rustdoc makes the same claim: "The lane has no other consumer: `taa.comp` forwards HDR alpha untouched and `composite.frag` forwards it to the swapchain, which ignores it".
  - Both are false on both halves:
    1. Composite writes the offscreen `HDR_FORMAT` scene image, not the swapchain; presentation owns the swapchain (#3426).
    2. The "future composite branch that gates on alpha" already exists. `composite.frag`'s `is_sky` arm computes `coverage = clamp(direct4.a, 0, 1)` and weights `sky_radiance(...) * (1.0 - coverage)` (#2466).
  - With `--upscaler taa`, composite binding 0 (`hdrTex`) is TAA's output (`rebind_hdr_views(&taa_views, GENERAL)`). So TAA's alpha pass-through is exactly what that sky arm reads.
- **Evidence**:
  - `composite.frag`: `float coverage = clamp(direct4.a, 0.0, 1.0); … combined = sky_radiance(...) * (1.0 - coverage) + direct + skyIndirect * skyAlbedo;`
  - `crates/renderer/src/vulkan/context/init.rs` and `set_upscaler_mode`: `c.rebind_hdr_views(&device, &taa_views, vk::ImageLayout::GENERAL)`.
  - `grep -n currA crates/renderer/src/vulkan/taa.rs` finds nothing: no test pins the pass-through.
- **Impact**:
  - Documentation drift with a live trap. A "cleanup" trusting either comment could write `vec4(rgb, 1.0)` in `taa.comp`, since it is documented as harmless. That would make every clear-depth pixel under `--upscaler taa` report full coverage and replace the sky with black, and nothing would fail.
  - This includes the automatic FSR→TAA fallback (#2480).
- **Related**: #2466, #676 / DEN-6 (closed), #2799 (closed, similar stale "swapchain" attribution), #3572 (open).
- **Suggested Fix**:
  - Reword both comments: composite's sky arm consumes this lane as transparent coverage, so TAA's pass-through is load-bearing.
  - Add a source-scan pin in `taa.rs` that every `imageStore(uOutput, …)` forwards `currA`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types / pipelines / spawn sites)
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix
