# REN-D8-N03: depth_params.z volumetric-consumption gate no longer exists in the shader, but the host still documents it as the flip switch

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2493
**Finding ID**: REN-D8-N03 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 8 — Denoiser/Composite
**Location**: `crates/renderer/src/vulkan/context/draw.rs:582-592` (`build_composite_params`, `depth_params[2]`), `crates/renderer/src/vulkan/composite.rs:54-58` (`depth_params` field doc)
**Status**: NEW (broken contract / doc-rot)

## Description
The host comment reads "Composite reads this slot to decide whether to consume `vol.a` (transmittance) and `vol.rgb` (in-scattering). Pinned to the host const so a future flip of `VOLUMETRIC_OUTPUT_CONSUMED` is a single-line change." That is no longer true: `#1926` removed the shader-side branch, and `composite.frag:512` applies `combined = combined * vol.a + vol.rgb;` unconditionally. Meanwhile `post_passes.rs:425` wraps *both* volumetric dispatches in `if VOLUMETRIC_OUTPUT_CONSUMED`. Flipping the const to `false` would therefore stop all volume writes while composite keeps multiplying the scene by whatever the froxel volume last held — i.e. the advertised "single-line change" would now be a two-file change, and its safety rests entirely on the implicit `volumetrics.rs::initialize_layouts` neutral clear (`float32: [0.0, 0.0, 0.0, 1.0]`), which nothing documents as load-bearing for that path.

## Evidence
`composite.frag:563-573` documents the removal of the fallback branch ("`depth_params.z < 0.5` guard can never pass. Removed per the lockstep note this branch used to carry (#1926 / REN-D8-01)") while the host comment at `draw.rs:582` still advertises the gate. `volumetrics.rs:1297` is the clear that silently rescues the flip. `gpu_timers.rs:166` doc still claims `false` is "the current default", so the misinformation is spread across three files.

## Impact
None today (`VOLUMETRIC_OUTPUT_CONSUMED = true`). Latent trap for anyone bisecting a lighting regression by flipping the const.

## Related
#928, #1013, #1926, REN-D8-01 (`AUDIT_RENDERER_2026-07-14`).

## Suggested Fix
Rewrite the `draw.rs` / `composite.rs` comments to say the slot is vestigial and that the const's off-path relies on the neutral froxel clear; add that note to `volumetrics.rs::initialize_layouts` so the clear value is not "optimized" to a plain zero.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change); if the const is ever flipped, confirm the neutral clear rescues the off-path
