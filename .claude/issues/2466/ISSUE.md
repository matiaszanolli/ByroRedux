# REN-D8-N01: Composite is_sky branch discards alpha-blended geometry drawn against the sky

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2466
**Finding ID**: REN-D8-N01 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 8 — Denoiser/Composite
**Location**: `crates/renderer/shaders/composite.frag:406` (`if (is_sky) { ... combined = compute_sky(dir); }` in `main`), against `crates/renderer/src/vulkan/pipeline.rs:663` (`create_blend_pipeline` → `.depth_write_enable(false)`)
**Status**: NEW

## Description
Composite classifies a pixel as sky purely from the depth attachment (`bool has_surface = depth < 1.0; bool is_sky = !has_surface && (params.depth_params.x > 0.5);`). The sky branch then *replaces* the pixel: `combined = compute_sky(dir);` — `direct4.rgb` (the main pass's HDR colour attachment) is never read into `combined` on that path. But every draw that goes through `create_blend_pipeline` runs with `depth_write_enable(false)`, so an alpha-blended fragment with nothing opaque behind it leaves depth at the cleared `1.0`. Its HDR contribution was blended into attachment 0 in the main pass and is then thrown away in composite and overpainted with the procedural sky. Prior audits covered a *different* gap in this same branch (missing bloom and volumetric fog), now fixed. The discarded-`direct` problem is a separate, still-live defect in the restructured code.

## Evidence
```glsl
vec3 combined;
if (is_sky) {
    vec3 dir = screen_to_world_dir(fragUV);
    combined = compute_sky(dir);      // direct4.rgb dropped entirely
} else {
    ...
    combined = direct + indirect * albedo + caustic;
}
```
`pipeline.rs::create_blend_pipeline`: `.depth_test_enable(true).depth_write_enable(false)` — "Transparent surfaces never write depth". Main-pass clear leaves attachment 0 at `clear_color` and depth at `1.0`, so the only content at a `depth == 1.0` exterior pixel is exactly the transparent geometry that was blended over the clear.

## Impact
Exterior only (`depth_params.x > 0.5`). Any translucent draw silhouetted purely against the sky vanishes: smoke / steam / magic particle billboards, alpha-blended banners and glass panes seen against open sky, and any `AlphaBlend`-flagged mesh on a skyline. Geometry-backed transparents are unaffected. Note the FSR masks are *not* lost — `outReactive` / `outTransparency` MAX-blend correctly — so FSR is told a transparent surface is there while its colour has already been erased.

## Related
REN-D8-02 / REN-D16-02 (`AUDIT_RENDERER_2026-08-02.md`, `2026-08-03.md`) — same branch, bloom/fog half, now fixed. `DEN-11` / `#676` (the `direct4.a` alpha-marker pass-through, already forwarded symmetrically from both branches).

## Suggested Fix
Composite the sky *behind* the main pass result rather than instead of it — e.g. `combined = compute_sky(dir) * (1.0 - coverage) + direct;` where `coverage` comes from a real accumulated-coverage lane. The cheapest correct-for-one-layer version uses the existing `direct4.a`; a fully correct version wants an accumulated `ONE_MINUS_SRC_ALPHA` coverage lane on the HDR attachment's alpha. Worth confirming the intended layering with a RenderDoc capture of an exterior particle-over-sky frame before shipping.

## Completeness Checks
- [ ] **TESTS**: Needs a RenderDoc capture of an exterior particle-over-sky frame to confirm the fix
