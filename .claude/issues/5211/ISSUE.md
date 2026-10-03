# #5211 — REN-D7-2026-10-03-01: SVGF's nearest-tap fallback re-accepts the non-finite history texel that #903's guard just dropped, so with a parked camera one NaN/Inf indirect sample persists indefinitely

**Labels**: low,renderer,shaders,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: LOW
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/shaders/svgf_temporal.comp` — `main()`, the bilinear-tap loop's `isnan(sInd)` guard and the `else if (length(motion * screen.xy) < 1.5)` nearest-tap branch (`nearID`)
- **Status**: NEW (gap in the closed #903 fix; #737 added the fallback before #903 landed, and #903 guarded only the bilinear loop)
- **Description**: #903 drops a non-finite history tap with `continue` inside the 4-tap bilinear loop. If every tap is dropped (or carries ~0 weight), `wTotal <= 0.01`, and the sub-pixel-motion fallback runs. That fallback picks `q = ivec2(round(prevPx))`. This is always one of the same four taps, so it is the one just rejected. It applies only the mesh-ID and normal tests, then reads `prevIndirectHistTex`/`prevMomentsHistTex` with **no** finite check. With a parked camera, `motion == 0`, so `prevPx == p` (within float error). The bilinear weight is ≈1 on the pixel itself and ≈0 on the other three taps, so a NaN in the pixel's own history always reaches the fallback and is taken back. `mix(NaN, currInd, alphaC)` then writes NaN to this frame's temporal output. That output *is* the next frame's history (à-trous does not feed back, per `svgf.rs`'s history wiring). The NaN therefore self-perpetuates for as long as the camera stays parked or moves under 1.5 px/frame.
- **Evidence**: bilinear loop: `if (any(isnan(sInd)) || any(isinf(sInd)) || any(isnan(sMom)) || any(isinf(sMom))) { continue; }`. Fallback branch: `histInd = texelFetch(prevIndirectHistTex, q, 0).rgb; histMom = sMom.xy; histAge = sMom.z; hasHistory = true;`, with no `isnan`. The current-frame sample has no guard either: the firefly clamp `if (currLum > maxL)` is false for NaN, so a single-frame NaN in raw indirect enters history directly. No test pins #903 on the SVGF side (`grep -n "isnan\|#903" crates/renderer/src/vulkan/svgf.rs` finds nothing).
- **Impact**: This is dormant defence-in-depth, and #903 itself records "no live NaN source today". But in the parked case, the one where history lives longest, the guard does nothing. A transient NaN from any future RT branch would become a permanent dark or garbage blob, spread each frame by the 3 à-trous iterations into composite until the camera moves. The `presentation.frag` `ImageHealth` non-finite counter (#2736) would report it but not clear it.
- **Related**: #903, #737, #1159 (all closed). #4782 (same class, V-buffer history, closed).
- **Suggested Fix**: Apply the same `isnan`/`isinf` rejection to the fallback's `sMom` and `histInd` before setting `hasHistory`. Better, sanitise `currInd` once at the top of `main()` so no non-finite value is ever written to history. Pin both with a GLSL source-scan test in `svgf.rs`.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix
