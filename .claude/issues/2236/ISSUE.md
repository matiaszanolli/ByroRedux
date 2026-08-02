# REN-D11-02: Fire-refraction proxies overwrite the opaque receiver's G-buffer normal at any coverage, including near-zero

Severity: medium
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2236

**Dimension**: 11 (G-buffer)
**Location**: `crates/renderer/shaders/triangle.frag` (fire-refraction branch, `outNormal = octEncode(macroN);` unconditional write)
**Status**: NEW

**Description**: The fire-refraction proxy branch writes `outNormal = octEncode(macroN)` unconditionally, regardless of `proxyCoverage` (`distortionStrength^2`). `outAlbedo`/`outRawIndirect` are correctly zeroed so the proxy only affects HDR color, but the G-buffer normal (consumed by SVGF/TAA for disocclusion and by downstream lighting reconstruction) is replaced by the haze proxy's own approximate normal even when the proxy is nearly invisible (very low `distortionStrength`).

**Evidence**: `triangle.frag`, fire-refraction branch — `outColor = vec4(distortedScene.rgb, proxyCoverage); outNormal = octEncode(macroN); ... outAlbedo = vec4(0.0);` — `outNormal` has no coverage gate, unlike `outAlbedo`/`outRawIndirect`.

**Impact**: Any opaque receiver behind a fire-refraction proxy has its true G-buffer normal replaced by the proxy's normal even at near-zero visible coverage, corrupting SVGF/TAA disocclusion and normal-dependent lighting reconstruction for that pixel.

**Related**: REN-D2-01 (same material kind, shares root cause per the report's Prioritized Fix Order — "the proxy was designed as excluded from everything but HDR color but only partially wired that way")

**Suggested Fix**: gate the `outNormal` write by `proxyCoverage` (blend toward the previously-written receiver normal, or skip the write below a coverage threshold) instead of writing unconditionally.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
