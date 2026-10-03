# #5199 — REN-D6-2026-10-03-04: `perturbNormalGrad` Path 2 (screen-derivative TBN) has no zero-length guard, and the comment justifying that misses the T_raw = 0 case

**Labels**: medium,renderer,shaders,bug
**Filed from**: `docs/audits/AUDIT_RENDERER_2026-10-03.md` (audit HEAD `f002763b4`)

- **Severity**: MEDIUM (defence-in-depth; NaN propagation)
- **Dimension**: Tangent-Space
- **Location**: `crates/renderer/shaders/include/material_sampling.glsl` (`perturbNormalGrad`, Path 2)
- **Status**: NEW. #2815 guarded Path 1 only; #3984 fixed the anisotropic builder.
- **Description**:
  - Path 2 computes `T = normalize(dPdx * dUVdy.y - dPdy * dUVdx.y)` and then `normalize(T - dot(T,N)*N)`, with no length check.
  - The Path-1 comment says Path 2 "needs no equivalent guard because its derivative-built T is already tangent-plane by construction". That only covers T ∥ N. When V is constant across the pixel quad (`dUVdx.y == dUVdy.y == 0`, for example a U-only strip mapping or float-equal V at large tiled UVs), the raw numerator is the zero vector and `normalize` returns NaN.
  - The NaN reaches the shaded normal, the G-buffer `octEncode`, RT origins, SVGF/TAA history and the bloom pyramid.
  - The sibling derivative builder `parallaxDisplaceUV` guards the same quantity (`dot(T, T) < 1e-8 … return uv`). `getRayHitTangentFrame` guards it as well.
- **Trigger Conditions**: A normal-mapped draw whose vertex tangent is zero-length reaches Path 2. That covers empty synthesis output, which `synthesize_tangents_yup` documents as falling back to Path 2, and Starfield BSGeometry without UDEC3 tangents. Hitting the bug also needs constant V over a quad. Degenerate-UV triangles get a permutation fallback tangent (#3176) and do not reach Path 2.
- **Suggested Fix**: Add `if (dot(Traw, Traw) < 1e-8) return N;` before the first normalize, and the same guard after projection. Correct the Path-1 comment. Add a `shader_contract` source pin next to the #2815 pin.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other call sites, other docs naming the same fact)
- [ ] **TESTS**: A regression test pins this specific fix
