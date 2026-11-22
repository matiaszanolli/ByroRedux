# #7261: Skip shadow/GI rays in helper invocations (gl_HelperInvocation) in triangle.frag

Labels: medium, renderer, shaders, performance, enhancement

## Summary

`triangle.frag` traces shadow and GI rays from helper invocations: the extra lanes that fill out a 2×2 quad along a triangle edge. Their output is never stored, so these rays cost time and change no pixel. While measuring the TSD-4 patch premium (#7221), skipping the rays in helper lanes cut the patch delta by 35–43 %. Eye-level screenshots stayed within baseline-to-baseline noise. The frame with patches off also dropped by about one sd, so the saving probably applies to every dense mesh, not just the patches.

This issue covers making that skip permanent, engine-wide, after wider image-parity checks.

## Evidence

The full write-up is in [docs/engine/terrain-displacement.md](docs/engine/terrain-displacement.md) §8.3, under "The premium is per pixel rather than per ray: small triangles". It is committed in 146ba3e2a.

The runs used FNV, the release build and the `renderer-static` bench harness, with interleaved rounds and medians of `main_render_ms`. The table shows the patch delta in ms, with sd 0.22–0.59:

| pose | baseline | helper-lane skip | no main rays |
|---|---|---|---|
| down | +1.48 | +0.94 | +0.08 |
| eye | +1.87 | +1.06 | +0.15 |
| graze | +0.99 | +0.65 | +0.14 |

- With patches off, the skip saved 0.37–0.41 ms at the eye and graze poses. That is about one sd, so treat it as suggestive, not settled.
- **Image parity:** at the FNV eye pose, screenshots with the skip differ from baseline by no more than two baseline shots differ from each other. **No other pose, game or material class was checked.**

## The experimental change

The change is one line after the ray-gate flags at `crates/renderer/shaders/triangle.frag:1049-1057`. It sat behind a temporary debug bit and was reverted.

```glsl
if (gl_HelperInvocation) { directShadowRayEnabled = false; giRayEnabled = false; }
```

`reflectionGlassRayEnabled` was deliberately not changed, because the glass branch modifies `N`.

## Why it should be safe

Two things have to stay true:

1. **Helper lanes store nothing.** Fragment outputs from helper invocations are discarded, so their ray results are only ever read by quad neighbours through derivatives.
2. **Nothing after the ray branches takes derivatives of values that depend on rays.** At HEAD:
   - `aaRoughness`, which uses `dFdx(N)`, is computed at `triangle.frag:3043`, before the direct-light section that traces the rays.
   - The AO fetch is screen-space.

   A future edit that adds `dFdx`/`dFdy`, `fwidth`, or an implicit-LOD `texture()` on a value that depends on rays, placed after the ray branches, would silently break the skip. The fix should add a guard or a clear comment for this.

## Scope / what's needed before it ships

- [ ] Keep the skip permanent, without a debug bit. Rebuild **both** `triangle.frag.spv` and `triangle_early.frag.spv` (`-DBYRO_OPAQUE_EARLY_TESTS=1`); all opaque geometry draws with the early variant. Check with `spirv-dis triangle_early.frag.spv | grep HelperInvocation`.
- [ ] Image parity beyond the FNV eye pose:
  - an interior with alpha-tested foliage or grates (derivative-heavy);
  - a Skyrim exterior;
  - an FO4 PBR (BGSM) material;
  - a water-adjacent shore;
  - TSD patches on and off.
  
  Compare against a baseline-vs-baseline noise floor, since ReSTIR and TAA are stochastic.
- [ ] Timing at those positions, with patches off, to show whether the engine-wide saving is real or within noise.
- [ ] Check interaction with ReSTIR reservoirs and the GI EMA. Helper lanes should never write reservoirs, but this needs confirming. The GI ray count feeds frame-uniform budgets, so check whether skipped helper rays shift any adaptive budget.

## Completeness Checks
- [ ] **SIBLING**: `water.frag` and `include/groundcover_cover.glsl` also issue ray queries from fragment stages. Apply the same skip where it is sound.
- [ ] **TESTS**: add a pin, such as a source-scan test, that the helper-lane gate exists and sits after the ray-gate flags. Note in the shader that no ray-dependent derivative may follow the ray branches.
- [ ] **SPV**: commit both `.spv` variants with the GLSL change.

Related: #7221 (TSD-4.2, where this was measured), #7152.
