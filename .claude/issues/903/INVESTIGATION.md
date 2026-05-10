# Investigation — #903 + #904 (TAA + SVGF temporal sibling)

## Scope expansion

User invited batching #903 (NaN guard) + #904 (mesh_id 15-bit mask) + the SVGF temporal
sibling in one patch. SVGF temporal has the same shape on both axes:

- **#904 sibling**: `svgf_temporal.comp:136` — `if (prevID != currID) continue;` same
  full-u16 compare. `(currID & 0x8000u) != 0u` early-outs at line 93 prevent the
  current-alpha-blended case from reaching the comparison, but the
  current-opaque / prev-alpha-blended case still fires same as TAA #904.
- **#903 sibling**: `svgf_temporal.comp:148-149` reads `histInd` and `histMom` from
  `prevIndirectHistTex` and `prevMomentsHistTex` with NO YCoCg clamp afterwards —
  plain weighted blend at line 152-153. So NaN propagation here is arguably
  worse than TAA: no implicit `min`/`max` filter at all.

## Plan

### #903 — NaN/Inf guards on history reads

Apply the same `if (any(isnan(...)) || any(isinf(...))) { ... }` pattern at both
sites. The fix is a no-op on healthy frames (no NaN to drop); under a future
NaN-producing RT branch it falls back to current-frame data instead of
self-perpetuating poison.

- `taa.comp:151` — guard `histRgb` between Catmull-Rom sample and YCoCg clamp.
- `svgf_temporal.comp:148-149` — guard `histInd` and `histMom` reads.

### #904 — 15-bit mesh_id mask on disocclusion compares

- `taa.comp:111` — `(currMid & 0x7FFFu) != (prevMid & 0x7FFFu)`.
- `svgf_temporal.comp:136` — same masking, applied to the bilinear-tap rejection.

## Files (4)

1. `crates/renderer/shaders/taa.comp` — both fixes
2. `crates/renderer/shaders/svgf_temporal.comp` — both fixes (sibling)
3. `crates/renderer/shaders/taa.comp.spv` — recompiled
4. `crates/renderer/shaders/svgf_temporal.comp.spv` — recompiled

Under the 5-file scope-check threshold.

## Test strategy

Shader pipelines aren't unit-tested today (precedent: #871, #900, #903's own
audit text). Verification gate is the captured-baseline `--bench-frames 300`
Prospector run with no visible-quality regression. Both fixes are no-ops on
healthy frames, so the bench should be byte-identical pre/post.

## SAFETY: no Vulkan-state changes

Per `feedback_speculative_vulkan_fixes.md`: shader-only edits, no barrier or
pipeline state changes, no descriptor set changes. The two fixes touch only
the body of the compute shaders — descriptor bindings, push constants,
layouts all stay identical. SPV regeneration via `glslangValidator -V` is
deterministic for the same source.
