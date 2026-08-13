# Issues 2757, 2759, 2761, 2760

All four filed from `docs/audits/AUDIT_RENDERER_2026-08-12b.md`. Domain: **renderer** (`byroredux-renderer`).

## #2757 — REN-D11-2026-08-12-03: helpers.rs comment pins stale triangle.frag:1532 anchor for bit-31 flag
- **Severity**: LOW (documentation)
- **Location**: `crates/renderer/src/vulkan/context/helpers.rs` (`create_render_pass`, attachment 3)
- **Bug**: Comment pins `triangle.frag:1532` for the bit-31 flag, but that line is now inside the glass Fresnel block, ~1000 lines from the real `outMeshID` write.
- **Fix**: Update the comment to reference the correct location (or a symbol-anchor instead of a raw line number, matching the "symbol-anchor rule" the audit references).

## #2759 — REN-D11-2026-08-12-04: triangle.frag outMeshID comment still says "per-instance ID + 1"
- **Severity**: LOW (documentation)
- **Location**: `crates/renderer/shaders/triangle.frag` (`layout(location = 3) out uint outMeshID;`)
- **Bug**: Trailing comment still reads "per-instance ID + 1", the pre-`883f57cd` meaning for all draws. `gbuffer.rs` and `shader-pipeline.md` were updated after `883f57cd`; the shader itself — the actual writer — was not. Third site of the #2499/#2500 drift class.
- **Fix**: Update comment to match the current two-meaning encoding (need to check `883f57cd` / gbuffer.rs / shader-pipeline.md for current wording).

## #2761 — REN-D11-2026-08-12-05: pipeline.rs UI builder comment sizes Vertex at stale 100 bytes
- **Severity**: LOW (documentation)
- **Location**: `crates/renderer/src/vulkan/pipeline.rs` (UI pipeline builder)
- **Bug**: Comment sizes `Vertex` at 100 bytes; it's been **104** since vertex colour widened `vec3 → vec4` (`cd2b5fe4`). `vertex_size_matches_attribute_stride` asserts 104. `vertex.rs`'s own `UiVertex` doc already says 104.
- **Fix**: Update comment to 104 bytes.

## #2760 — REN-D13-02: geometry silhouettes against the sky can never accumulate TAA history, causing per-frame edge crawl
- **Severity**: MEDIUM
- **Location**: `crates/renderer/shaders/taa.comp` (`background`/`disocclusion` early-outs), `crates/renderer/shaders/composite.frag` (`is_sky` branch), `crates/renderer/src/vulkan/context/draw.rs` (mesh-ID clear value)
- **Bug**: Sky is synthesized in `composite.frag`, never exists in the HDR attachment TAA operates on. At geometry/sky silhouettes, sub-pixel Halton jitter flips pixel coverage each frame; BOTH possible states reject TAA history (covered → disocclusion vs prevMid=0; uncovered → background). With a parked camera the motion vector is exactly zero, so the pixel re-disoccludes itself every frame — permanent disocclusion, not a one-frame event. Result: visible edge crawl along every exterior geometry/sky silhouette when TAA is on, worse than no jitter at all.
- **Suggested fix** (direction only, not prescriptive): instead of a hard history reject when `prevMid == 0` while `currSurface != 0` (and the mirror case), accept history but force the tightest neighborhood clamp (collapse `gamma` toward 0) so the sample gets clipped to the current neighborhood mean — preserves AA on jitter-driven coverage flips while still refusing off-surface color import.
- **Completeness**: Must not regress `taa_comp_keeps_history_bounded_and_rejects_unstable_surfaces` (pins the exact reject-list expression). Issue explicitly flags: "needs visual/RenderDoc verification of perceptual magnitude before sizing the actual fix (not a cargo test question)" — this is a source-level, test-verifiable fix; visual verification is out of scope for this pass, but must not regress the pinned reject-list test.

## Domain
renderer → `byroredux-renderer` (all four)
