# REN-D19-2026-05-26-01: bloom_upsample.comp DC-gain absorption by BLOOM_INTENSITY is undocumented

## Severity: Low (documentation gap, no visual bug)

**Location**: `crates/renderer/shaders/bloom_upsample.comp:53-65`

## Problem

The down-pass shader explicitly states "weights sum to 1.0 — provably equivalent to a 4×4 box filter." The up-pass does NOT carry an equivalent annotation, but its weights deliberately sum to 2.0 — and the `BLOOM_INTENSITY = 0.15` constant in composite is tuned to absorb the cumulative un-normalised gain across the 4-step up-chain.

### Evidence

```glsl
// bloom_upsample.comp:53-65
vec3 upsampled = vec3(0.0);
upsampled += texture(src_smaller, uv + vec2(-0.5, -0.5) * smaller_pixel).rgb;
upsampled += texture(src_smaller, uv + vec2( 0.5, -0.5) * smaller_pixel).rgb;
upsampled += texture(src_smaller, uv + vec2(-0.5,  0.5) * smaller_pixel).rgb;
upsampled += texture(src_smaller, uv + vec2( 0.5,  0.5) * smaller_pixel).rgb;
upsampled *= 0.25;
// ...
vec3 same = texture(src_same, uv).rgb;
imageStore(dst, dst_coord, vec4(upsampled + same, 1.0));
```

`upsampled` has unit DC gain (4 × 0.25). `same` has unit DC gain (1.0). Sum is 2.0× per level. Across the 4-step up-chain, a DC-constant scene accumulates up to ~8× peak at `up[0]`.

`BLOOM_INTENSITY = 0.15` is tuned to absorb the un-normalised gain (Frostbite SIGGRAPH 2015 quotes ~0.04 for a normalised pyramid — `0.15 ≈ 4×` compensates the un-normalised sum + Bethesda LDR authoring per the `bloom.rs:80-92` docstring).

## Impact

**No visual artifact.** This is the standard Bjørge / Frostbite progressive accumulation pattern, and the chosen intensity compensates correctly.

The bug is **asymmetric documentation**: the down-pass loudly asserts its 1.0 DC invariant, while the up-pass silently relies on the opposite invariant. A reader making upsample changes is likely to assume the down-pass invariant applies and try to "fix" the perceived weight imbalance — which would visually halve bloom strength and require re-tuning the intensity constant.

## Fix

Add a comment to `bloom_upsample.comp` header (near the existing Bjørge SIGGRAPH 2015 reference, around line 17-20) stating:

> The additive `upsampled + same` sum carries DC gain ≥ 1.0 **by design**, growing geometrically across the up-chain. `BLOOM_INTENSITY = 0.15` (see `shader_constants_data.rs`) is the global scale that absorbs this — see Bjørge SIGGRAPH 2015 "Moving Frostbite to PBR" §5.5.

Mirror the doc style of the existing "weights sum to 1.0 — provably equivalent to a 4×4 box filter" claim in the downsample shader.

## Completeness Checks

- [ ] **TESTS**: None — doc-only.
- [ ] **SIBLING**: Verify the downsample comment block still reads correctly after the upsample comment lands (no duplication or drift).
- [ ] **SPIRV**: Re-emit `bloom_upsample.comp.spv` if the comment requires re-compilation (GLSL compilers usually strip comments, so the .spv may not change — check `git diff --stat` post-recompile).

## Related

- #931 (closed) — barrier reduction; the related bloom work that prompted closer reading of this shader
- #1126 (closed) — `BLOOM_INTENSITY` duplicated const/define in composite.frag (now resolved by the generated shader_constants.glsl path)
- Bjørge SIGGRAPH 2015 "Moving Frostbite to PBR" §5.5 — canonical reference for the un-normalised additive pattern

Audit: `docs/audits/AUDIT_RENDERER_2026-05-26_DIM19.md` (Finding L1)
