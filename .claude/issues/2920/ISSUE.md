# REN-D8-01: Composite's is_sky arm drops indirect * albedo (residual half of #2466)

- **Issue**: [#2920](https://github.com/matiaszanolli/ByroRedux/issues/2920)
- **Finding ID**: `REN-D8-01`
- **Labels**: `medium,renderer,vulkan,bug`
- **Source report**: [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](../../../docs/audits/AUDIT_RENDERER_2026-08-14.md)
- **Run**: `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`

> Immutable snapshot of the issue *as filed* (TD10-001 / #1156). GitHub is
> authoritative for current state — query `gh issue view 2920 --json state`.

---

- **Severity**: MEDIUM
- **Dimension**: Denoiser/Composite
- **Location**: `crates/renderer/shaders/composite.frag` — the `if (is_sky)` arm
  of `main()` (`combined = compute_sky(dir) * (1.0 - coverage) + direct;`),
  against the sibling `else` arm (`combined = direct + indirect * albedo + caustic;`)
- **Status**: NEW (residual half of **#2466** / REN-D8-N01, which is CLOSED and
  whose fix is present and correct as far as it goes)
- **Description**: #2466 established that an alpha-blended fragment with nothing
  opaque behind it leaves depth at the cleared `1.0` — blend pipelines run
  `depth_write_enable(false)` — so composite classifies the pixel as sky. The fix
  restored the pixel's **direct** term by weighting the procedural sky against the
  `direct4.a` coverage lane. The **indirect** term was not restored: the sky arm
  never reads `indirectTex` or `albedoTex`, so the same fragment's
  albedo-demodulated GI is still discarded. The identical surface drawn one pixel
  to the side — over opaque geometry — gets `indirect * albedo` added. The result
  is an exterior-only brightness discontinuity along the silhouette where an
  alpha-blended draw crosses the horizon.
- **Evidence**:
  ```glsl
  vec3 combined;
  if (is_sky) {
      vec3 dir = screen_to_world_dir(fragUV);
      float coverage = clamp(direct4.a, 0.0, 1.0);
      combined = compute_sky(dir) * (1.0 - coverage) + direct;   // no indirect term
  } else {
      vec3 indirect = texture(indirectTex, fragUV).rgb;
      vec3 albedo   = texture(albedoTex, fragUV).rgb;
      ...
      combined = direct + indirect * albedo + caustic;
  }
  ```
  The dropped term is genuinely non-zero at such a pixel. `triangle.frag`'s tail
  writes `outRawIndirect = vec4(indirectLight, auxiliaryAlpha)` and
  `outAlbedo = vec4(albedo, auxiliaryAlpha)`, and `pipeline.rs::blend_gbuffer_attachments`
  gives attachments 4 and 5 `auxiliary_blend` (`SRC_ALPHA`/`ONE_MINUS_SRC_ALPHA`)
  over the zero clear, so both lanes hold coverage-weighted content.
  `svgf_temporal.comp`'s bit-31 early-out passes that value straight through
  (`imageStore(outIndirect, p, vec4(currInd, 1.0))`), and `svgf_atrous.comp`
  filters rather than discards it, so `indirectTex` carries it into composite.
- **Impact**: Exterior only. Narrower than #2466's blast radius, because the draws
  that most often silhouette against sky take early-outs that zero
  `outRawIndirect` first — the `MAT_FLAG_EFFECT_SOFT` / effect-shader arm, the
  `MATERIAL_KIND_NO_LIGHTING` arm, and both glass exits all write
  `outRawIndirect = vec4(0.0)`. What remains affected is **lit** alpha-blended
  geometry reaching the general tail: cloth banners, hanging signs, lit
  alpha-blended decals and card geometry on a skyline. Those render with ambient +
  RT GI missing against sky and present against a wall.
- **Related**: #2466 (REN-D8-N01, the direct half), #2233 (REN-D8-02, the
  bloom/fog half of the same branch), #676 / DEN-11 (the `direct4.a` lane),
  `pipeline.rs::coverage_alpha_factors`.
- **Suggested Fix**: Read `indirectTex`/`albedoTex` unconditionally (they are
  already bound and cheap) and add `indirect * albedo` to the sky arm's
  `combined`, exactly as the geometry arm does. Note while doing so that the
  demodulated reassembly `indirect * albedo` is not linear in the blend operator
  — over a zero clear the product is `coverage²·(I·A)` — so if the sky arm is
  ever made exact rather than consistent, the premultiply must be divided out
  once, not twice. Consistency with the geometry arm is the smaller and safer
  change and is what this finding asks for.

---

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, the sibling BLAS/TLAS path)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed by `/audit-publish` from [`docs/audits/AUDIT_RENDERER_2026-08-14.md`](docs/audits/AUDIT_RENDERER_2026-08-14.md) — `/audit-suite rt-deep`, 2026-08-14, HEAD `205744ae`. Verified CONFIRMED against current code at publish time.*
