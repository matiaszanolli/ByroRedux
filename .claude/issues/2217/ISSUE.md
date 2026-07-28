# #2217 — REN-2026-07-28-01: composite.frag zeroes causticLum while the committed SPIR-V still has the working expression — rebuilding kills all caustics

_Filed from `docs/audits/AUDIT_RENDERER_2026-07-28.md` by `/audit-publish` on 2026-07-28. Immutable snapshot of the issue **as filed** — GitHub is authoritative for current state (`gh issue view 2217 --json state`)._

---

**Severity:** HIGH · **Dimension:** 14 (caustic splat), with impact on 8 (denoiser/composite) and 15 (water)
**Source:** `docs/audits/AUDIT_RENDERER_2026-07-28.md` — REN-2026-07-28-01
**Status when filed:** NEW regression; same failure class as closed #1447

## Description

`crates/renderer/shaders/composite.frag` was edited to force `causticLum = 0.0`, but the
committed SPIR-V artifact was never rebuilt. The executable currently embeds the *older*,
working artifact, so caustics still appear at runtime — but the next canonical shader
rebuild will silently remove every glass and water caustic. Source review and runtime
behaviour describe two different renderers.

The edit landed inside a docs-titled commit, so it reads as unintentional.

## Evidence

`crates/renderer/shaders/composite.frag:381`:

```glsl
float causticLum = 0.0;
```

The surrounding comment block (lines 375–380) still documents the expression that is no
longer there — "promote each accumulator to float BEFORE the add so the sum can't wrap
u32" — and lines 373–374 still fetch both accumulators, whose results are now dead:

```glsl
uint causticRaw = texelFetch(causticTex, causticPixel, 0).r;
uint waterCausticRaw = texelFetch(waterCausticTex, causticPixel, 0).r;
```

The introducing diff (`0a3e0da5`, *"docs: Add runtime telemetry audit for 2026-07-27"*):

```diff
-        float causticLum = (float(causticRaw) + float(waterCausticRaw)) / CAUSTIC_FIXED_SCALE;
+        float causticLum = 0.0;
```

`crates/renderer/src/vulkan/composite.rs:39` embeds the artifact with `include_bytes!`,
so the stale `.spv` is what actually ships today.

The artifact gate fails at HEAD:

```
$ scripts/check-shader-artifacts.sh
DRIFT crates/renderer/shaders/composite.frag.spv
a95f95c2...  crates/renderer/shaders/composite.frag.spv      (committed)
a01e7055...  /tmp/.../composite.frag.spv                     (rebuilt, glslang 11:16.2.0)
check-shader-artifacts: committed SPIR-V is not reproducible from GLSL
```

## Impact

- A canonical rebuild zeroes every glass **and** water caustic contribution
  (`vec3 caustic = albedo * causticLum;` → black), losing the whole `#1257` / `#1210`
  Phase E dual-accumulator feature.
- Because the stale artifact is what ships, the defect is invisible to anyone running the
  binary and invisible to anyone reading only the GLSL — the two disagree.
- Blast radius: every scene with glass, MultiLayerParallax, or water caustics, on all games.

## Why tests missed it

473 renderer tests and 734 application tests pass. The existing SPIR-V reflection test
pins structural/branch properties, not the semantic caustic expression.
`scripts/check-shader-artifacts.sh` is the only automated check that fails.

## Suggested Fix

1. Restore the combined fixed-point decode
   (`(float(causticRaw) + float(waterCausticRaw)) / CAUSTIC_FIXED_SCALE`).
2. Rebuild `composite.frag.spv` with the pinned glslang (plain `-V`, not `-g0` — the
   reflection test needs `OpName`).
3. Add a source-semantic regression guard requiring both accumulator reads *and* the
   fixed-scale divide to be present.
4. Make the artifact gate non-optional for any shader source edit.

## Completeness Checks
- [ ] **SIBLING**: Check the other caustic writer (`caustic_splat.comp`) and every other
      shader whose `.spv` may have drifted — run the full artifact gate, not just this file
- [ ] **TESTS**: A regression test pins this specific fix (source-semantic guard, not just reflection)
- [ ] **CANONICAL-BOUNDARY**: n/a — shader-local fix; no per-game logic involved
