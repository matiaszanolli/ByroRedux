# REN-D15-NEW-03: Composite binding 8 falls back to the glass caustic view when WaterCausticAccum is absent, double-counting glass caustics

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2508
**Finding ID**: REN-D15-NEW-03 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 15 — Water
**Location**: `crates/renderer/src/vulkan/context/mod.rs:2596-2603` (init `water_caustic_views` fallback); `crates/renderer/src/vulkan/context/resize.rs:852-857` (resize fallback); consumed at `crates/renderer/shaders/composite.frag:439-447`
**Status**: NEW

## Description
Both the init and resize paths bind composite's binding 8 (`waterCausticTex`) to `caustic_views` — the **glass/MultiLayerParallax** accumulator's sampled views, i.e. the exact same images already bound at binding 5 (`causticTex`) — whenever `water_caustic_accum` is `None`. `composite.frag` then sums the two:
```glsl
uint causticRaw      = texelFetch(causticTex, causticPixel, 0).r;
uint waterCausticRaw = texelFetch(waterCausticTex, causticPixel, 0).r;
float causticLum = (float(causticRaw) + float(waterCausticRaw)) / CAUSTIC_FIXED_SCALE;
```
With both bindings aliasing one image that is 2× glass caustic luminance, not glass + 0. The init-site comment justifying the fallback is factually wrong — it claims `causticAccum` is all-zero, but `caustic_splat.comp` writes it every frame for glass/MLP refractors. The premise only holds when the glass caustic pipeline is *also* absent, in which case `caustic_views` is `mesh_id_views_seed` (a different, unrelated aliasing hazard already documented at `mod.rs:2511-2517`). Note the codebase already has the right resource for this — `placeholder_caustic_sink`, used by the resize path for the *write* side (WaterPipeline set 2) — but composite's read side never uses it.

## Evidence
`mod.rs:2598-2602` and `resize.rs:852-857` both `=> caustic_views.clone()`; `mod.rs:2518-2521` shows `caustic_views` = the live glass caustic sampled views when `caustic.is_some()`.

## Impact
Degraded-path only — fires when `WaterCausticAccum::new` fails at init or `recreate_on_resize`/`initialize_layouts` fails at resize (VRAM pressure / OOM). Result is 2× glass caustic brightness for the rest of the session, partially masked by the `CAUSTIC_FIREFLY_MAX = 16.0` clamp. No crash, no validation error, no memory hazard. Its main cost is that the false "this is safe" comment will defeat the next reviewer who checks this path.

## Related
#2142 / RL-D6-02 (the sibling bug on the *write* side of the same fallback, already fixed with `placeholder_caustic_sink`); #1257 / #1210 Phase E.

## Suggested Fix
Bind binding 8 to a genuinely zero-valued R32_UINT image on the fallback path (a full-render-resolution sibling of `placeholder_caustic_sink`, since `composite.frag` `texelFetch`es at `textureSize(causticTex, 0)` coordinates and a 1×1 sink would be out of range), and correct the comment. Minimum viable alternative: keep the aliasing but gate the sum in the shader on a "water caustics enabled" flag bit.

## Completeness Checks
- [ ] **TESTS**: A regression test forces the water-caustic-init-failure path and confirms binding 8 reads zero, not 2× glass caustic
- [ ] **SIBLING**: Compare against `placeholder_caustic_sink`'s write-side fix (#2142) for the same pattern
