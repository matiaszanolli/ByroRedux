# TD3-2026-10-15-01: The grass LOD rewrite (af8323881) left the stochastic / blue-noise / clump-card / FSR-reactive story live in 14 doc and comment sites

Labels: low,terrain-exterior,renderer,tech-debt,documentation,doc-rot

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD3-2026-10-15-01`).

- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `crates/renderer/src/vulkan/groundcover/construct.rs:775-779`; `crates/renderer/shaders/groundcover_blade.vert:11, 84-93, 106-109`; `crates/renderer/src/vulkan/groundcover/frame.rs:122-127`; `crates/renderer/src/vulkan/groundcover.rs:159-162, 754-761`; `crates/renderer/shaders/include/groundcover_scene.glsl:137-140`; `byroredux/src/render/groundcover.rs:824`; `docs/engine/exal-groundcover.md:16, 378-381, 414-440, 638, 1795-1799`; `docs/engine/fsr3-upscaler-integration-plan.md:520-526`
- **Status**: NEW
- **Age**: `af8323881` (2026-10-14) replaced "every blade through every stream, blue-noise loser discarded per fragment" with one geomorphing 3/2/1-segment tier per 512-unit chunk (`include/groundcover_lod.glsl`). `17619c0bf` (same day) wrote the correct paragraph at `exal-groundcover.md:1854-1869` but did not retire the old ones.
- **Effort**: small
- **Description**: the code now writes `outFsrReactive = 0.0` / `outFsrTransparency = 0.0` for blades and has no mid/card tier; the test string at `construct.rs:797` says "there is no stochastic handoff left to flag", but the doc comment two lines above it (`:777-778`) still says only "either stochastic projected-size handoff raises a bounded reactive contribution". `exal-groundcover.md` contradicts itself: §6's table (3 tiers incl. "clump cards"), the "Current geometric LOD implementation" subsection (blue-noise cross-fade, bounded FSR reactive band, "Tier 2 groups a 4x4 set into a six-vertex card") and §12.14 ("since `fd0cd577c` the ribbon writes `0.9 * max(midTransition, cardTransition)` ... must not regress to a blanket 1.0") all describe the removed design, while `:1864` says "no stochastic cross-fade, no FSR reactive exception and no clump-card tier". `fsr3-upscaler-integration-plan.md:520-526` repeats the removed reactive contract and names the test that now asserts the opposite. Code comments (`blade.vert:84-93`, `frame.rs:122-127`, `groundcover.rs:754-761`) still describe the 2-bit tier word as carrying "the clump-card tier" and a "blue-noise LOD cross-fade". `GpuGroundCoverSpecies.card_atlas` / `cardAtlas` is documented as the Tier-2 card atlas but no ground-cover shader reads it any more (the sampling was deleted in `af8323881`'s frag diff; only the host copy and a unit test remain).
- **Evidence**: `grep -rn "cardAtlas\|card_atlas" crates byroredux --include='*.rs' --include='*.glsl'` -> 5 hits, none a shader read; `grep -n "stochastic\|midTransition\|cardTransition" docs/engine/exal-groundcover.md docs/engine/fsr3-upscaler-integration-plan.md`.
- **Impact**: a reader following §12.14 / the FSR plan could "restore" a reactive mask the shader deliberately dropped; the FSR plan names a test as the pin for a contract that test now inverts.
- **Related**: af8323881, 17619c0bf, fd0cd577c; the dead `card_atlas` field is a Dim 8 candidate.
- **Suggested Fix**: mark `exal-groundcover.md` §6 table + "Current geometric LOD implementation" + §12.14 reactive paragraph as superseded (or rewrite to the segment-tier model), rewrite the `construct.rs:775` comment and the tier-word comments to "tier index (3 segment levels)", delete or re-document `card_atlas`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
