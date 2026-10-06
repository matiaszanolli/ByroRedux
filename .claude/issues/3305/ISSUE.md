# REN-2026-08-26-01: dynamic actors (creatures) show no ground-contact shadow despite correct light/instance visibility masks

**Labels**: bug,renderer,medium,vulkan

## Description

Observed in a live FO4 Commonwealth screenshot (`grid-cross` boundary benchmark, radius-1 population near GNN Plaza): two `feral hound`-class creature actors show no visible ground-contact shadow, while nearby static architecture (a balustrade) casts a normal, reasonably soft-edged shadow from the same sun. Screenshot retained at
`/tmp/claude-1000/-mnt-data-src-gamebyro-redux/d0355164-7ccb-4e08-a568-3c81daed22d0/scratchpad/fo4-boundary-2/fo4/frame.png` (session-local path, not guaranteed to survive — re-run `docs/smoke-tests/m-exteriors.sh fo4 boundary` to reproduce; the dogs are ambient wildlife on this route so a repro may need `--bench-camera grid-cross` again or a `coc`/manual approach near the same plaza).

## What's ruled out

Both layers of the RT shadow-ray visibility-mask system check out correctly on paper — this is **not** an obvious mask-exclusion bug:

1. **Light-level mask** — the exterior sun's `GpuLight.params.z` is hardcoded to `VisibilityMask::FULL.bits()` (`byroredux/src/render/lights.rs:192`), the widest possible policy. `visibilityOpaqueMask()` (`shadow_common.glsl:19-21`) ANDs this against `VISIBILITY_MASK_ALL_OPAQUE`, which is unaffected by `FULL` being a superset — the sun's shadow-ray cull mask includes every opaque layer, `DYNAMIC_ACTOR` among them.
2. **Instance-level mask** — `shadow_mask_for_instance()` (`crates/renderer/src/vulkan/acceleration/predicates.rs:719-751`) maps `RenderLayer::Actor` → `VISIBILITY_LAYER_DYNAMIC_ACTOR`, which is one of the four bits composing `VISIBILITY_MASK_ALL_OPAQUE` (`crates/core/src/lighting.rs:134-135`). So `(instance.mask & ray.cullMask) != 0` should hold for a dog's TLAS instance against the sun's shadow ray, by the standard Vulkan RT masking rule.

Neither of these — the two places a "dynamic actors excluded from shadow rays" policy bug would plausibly live — explains the observation.

## Not yet checked / candidate directions

- **Skinned-BLAS refit timing.** `blas_skinned.rs` has an explicit "first-sight frame" concept (`refit_skinned_blas`, doc at `:390-412`) for a same-frame build+refit ordering. A freshly-streamed-in actor (radius-1 population load, as in the reproducing screenshot) is exactly the shape of case that path exists for — worth checking whether a dog's BLAS is fully built/refit *before* the frame its shadow is expected to render, or whether there's a one-frame gap where the TLAS instance references stale/degenerate geometry.
- Whether the specific creature race/record in question resolves to `RenderLayer::Actor` at all (vs. some other classification that's technically still inside `ALL_OPAQUE` but wired differently upstream).
- Whether this is animal/creature-specific (as opposed to humanoid NPCs) — creatures may go through a different skin/BLAS registration path than the humanoid ragdoll/skeleton pipeline.

This needs actual capture/inspection tooling (RenderDoc — the TLAS instance list + the shadow ray's actual hit/miss result for the exact pixel) to pin down further, not more source-reading — per this project's own convention (`feedback_speculative_vulkan_fixes`): don't ship render-pass/pipeline changes whose failure modes are invisible to `cargo test`; RenderDoc or revert, not speculation.

## Suggested Fix

Capture the reproducing scene in RenderDoc, inspect the TLAS instance list for a shadow-less dog's entity, and check whether (a) its instance exists at all at the frame in question, (b) its instance mask is what `shadow_mask_for_instance` computed, and (c) the shadow ray from a fragment on the ground beneath it actually intersects that instance's geometry (vs. missing due to a BLAS bounds/refit issue).

## Related
Closed sibling investigations in the same visibility-mask area: #2227 (SHADOW_MASK_OPAQUE silently excluded glass), #2224 (fire-refraction proxies wrongly stayed opaque occluders), #2238 (MultiLayerParallax missed the glass mask).
