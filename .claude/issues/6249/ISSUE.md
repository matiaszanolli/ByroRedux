# #6249: REN-M-01: The shadow filter adds a correction wherever the reservoir slot was claimed, including pixels whose visible surface never claims (unlit screens, water, ground-cover blades, glass, decals)

**Labels**: medium,renderer,shaders,bug

**Source**: docs/audits/AUDIT_RENDERER_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Denoiser/Composite (shadow filter) · **Location**: `crates/renderer/shaders/shadow_filter.comp (guideAt, storeResult)`

- **Location**:
  - `crates/renderer/shaders/shadow_filter.comp`: `guideAt` takes validity from `histLenAndDepth.x != 0`; `storeResult` writes the correction.
  - `crates/renderer/shaders/composite.frag`: `direct = max(direct + texture(shadowCorrectionTex, fragUV).rgb, 0)` under `caustic_flags.y`.
  - `crates/renderer/shaders/triangle.frag`: the claim block under `if (stableTemporalSurface)`; the `MATERIAL_KIND_NO_LIGHTING` early return.
- **Status**: NEW. The 2026-10-18 report recorded the blade/water instance as an observation only (its Needs-RenderDoc item 3); this finding generalises it and adds the opaque cases and the torn-slot case.
- **Description**:
  - A slot is valid if any `stableTemporalSurface` fragment (`!isAlphaBlend && !isGlass`) that reached the ReSTIR finalize won `atomicMin` this frame.
  - The filter then writes `U · strength · (filtered − raw)` for that pixel, and composite adds it to the visible pixel's direct light.
  - The visible surface is often not the claimant:
    - **Opaque `MATERIAL_KIND_NO_LIGHTING` (102)** surfaces (terminal screens, signs) return at the `outColor = vec4(emit, texColor.a)` early exit and never claim. They run in the late-test pipeline (kind > `MATERIAL_KIND_MAX_LIGHTING_SHADER`), and draw order is front-to-back only coarsely within a group (`draw_sort_key`). A lit surface behind them that was drawn earlier, or an alpha-tested cutout behind them in the late-test pipeline, owns the slot.
    - **Water and ground-cover blades** never claim, so the terrain or lake floor beneath owns the slot (the 2026-10-18 observation).
    - **Glass and blended decals** never claim. The opaque surface behind owns the slot, and its full correction is added to the composited colour although that surface reaches the pixel only through the blend's (1 − α).
  - **Torn slot.** The claim and the two stores are not atomic as a unit. The shader comment accepts that a farther fragment's late store can land after a nearer winner's, which costs only a history reset next frame. But `reservoirsCurr[pixelIdx] = rc` and `directUnshadowed[pixelIdx] = …` are separate stores, so the filter can pair one fragment's ratio and guides with another fragment's U in the same frame, with no gate.
- **Evidence**:
  - `composite.frag`'s comment says the term "replace[s] the shaded U * rawRatio by U * filteredRatio", which holds only if the slot owner is the visible surface.
  - Nothing in `shadow_filter.comp` compares the slot's surface against the visible pixel. The G-buffer mesh ID (opaque = stable surface ID, bit 31 = blended) is available but not bound to the pass.
  - Guards are thin (`bytes_per_pixel_matches_frame_images`, `a_failed_layout_submit_leaks_instead_of_destroying`), and no test covers ownership.
- **Impact**:
  - Light or dark smudges inside about 6 px of the owner's penumbra edges appear on surfaces the owner does not shade.
  - The worst cases are emissive screens in front of lit walls, and water over a shadowed shore floor.
  - Magnitude scales with the hidden owner's U and with `strength`, which is 1 while its history is short, i.e. whenever the camera moves.
  - Not measured (no engine launch).
- **Related**: #5765 (claim), #6035 (billing), REN-EXT 2026-10-18 Needs-RenderDoc item 3.
- **Suggested Fix**: bind the mesh-ID attachment to the filter. Write a zero correction when the visible mesh ID has bit 31 set or does not equal the slot's surface tag (`lightAndSurface` & `RESERVOIR_SURFACE_MASK`, the same identity the claim writes), and use the same test as the centre-pixel validity check. That also rejects torn slots whose reservoir came from a different surface. No barrier or layout change beyond one more sampled input (needs a `BYRO_VALIDATION=1` run when added).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
