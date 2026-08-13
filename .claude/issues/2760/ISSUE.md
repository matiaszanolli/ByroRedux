# REN-D13-02: geometry silhouettes against the sky can never accumulate TAA history, causing per-frame edge crawl

- **Severity**: MEDIUM
- **Dimension**: 13 — TAA
- **Location**: `crates/renderer/shaders/taa.comp` — the `background` (`currSurface == 0u`) and `disocclusion` early-outs; `crates/renderer/shaders/composite.frag` — the `is_sky` branch; the mesh-ID clear value in `crates/renderer/src/vulkan/context/draw.rs`
- **Description**: Sky is synthesised inside `composite.frag` and never exists in the HDR attachment TAA operates on; sky pixels' mesh ID holds the clear value 0. At a geometry/sky silhouette, sub-pixel Halton jitter flips whether the pixel centre is covered, and both possible states reject history: covered → disocclusion against `prevMid == 0`; uncovered → `background`. With a parked camera the motion vector is exactly zero, so the pixel re-disoccludes itself every frame — permanent, not one-frame. The pixel alternates between shaded geometry and `compute_sky` at the jitter period with no temporal resolve.
- **Evidence**: `composite.frag` `bool is_sky = !has_surface && (params.depth_params.x > 0.5);` then `combined = compute_sky(dir);`; `draw.rs`'s clear_values comment "Mesh ID: 0 reserved for background"; `triangle.vert` applies jitter after `fragCurrClipPos = currClip;`.
- **Impact**: Visible pixel-level edge crawl along every exterior geometry-against-sky silhouette in TAA mode, including with the camera stationary — the highest-contrast aliasing case, and the one TAA is nominally there to fix. Enabling TAA makes these edges worse than jitter off. Interiors unaffected. Visual only. Confidence: mechanism traced end-to-end but perceptual magnitude unmeasured.
- **Related**: #2466 (same "sky is not in the G-buffer" root condition, different consumer); REN-D16-01 (third consumer of the same root condition).
- **Suggested Fix**: Instead of a hard history reject when `prevMid == 0` while `currSurface != 0` (and mirror), accept the history sample but force the tightest clamp. Must be re-checked against `taa_comp_keeps_history_bounded_and_rejects_unstable_surfaces`.

## Completeness Checks
- [ ] TESTS: A regression test pins this specific fix; must not regress `taa_comp_keeps_history_bounded_and_rejects_unstable_surfaces`
- [ ] Needs visual/RenderDoc verification of perceptual magnitude before sizing the fix

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2760
