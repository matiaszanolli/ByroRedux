**Severity**: MEDIUM · **Dimension**: BA2/DDS Reader → Renderer (cross-cutting) · **Status**: NEW (introduced by `2aac5351`, from AUDIT_FO4_2026-06-18)

## Description
Commit `2aac5351` decodes BC1/DXT1 as `BC1_RGBA_SRGB_BLOCK` instead of `BC1_RGB_SRGB_BLOCK` so the 1-bit punch-through alpha (3-colour block mode, `color0 <= color1`, index-3 = transparent) reaches the shader — required for FO4 alpha-test cutouts. The "opaque content unchanged" claim is **verified correct**. But BC1's 3-colour mode is a legal *RGB-fidelity* encoder choice, not necessarily intent-to-be-transparent. For a mesh flagged `INSTANCE_FLAG_ALPHA_BLEND` with `alphaThreshold == 0` (the FNV picture/table case noted at `triangle.frag:216-222`) whose BC1 diffuse uses 3-colour blocks in opaque regions, those texels now sample `a == 0.0` and trip the implicit blend-discard at `triangle.frag:223` (`texColor.a < 1/255`). Before this commit that branch was unreachable for any BC1 mesh because BC1_RGB pinned alpha to 1.0.

## Location
- `crates/renderer/shaders/triangle.frag:223` (implicit blend-discard) interacting with the new decode at
- `crates/renderer/src/vulkan/dds.rs:322` (FourCC DXT1) and
- `crates/renderer/src/vulkan/dds.rs:361` (DXGI BC1_UNORM/_SRGB)

## Evidence
- Line 223 condition `ALPHA_BLEND && aThresh==0 && texColor.a < 1/255` — pre-change BC1 alpha was constant 1.0 → never true; post-change a 3-colour index-3 texel yields 0.0 → discards.
- `INSTANCE_FLAG_ALPHA_BLEND` derives from the NIF `NiAlphaProperty` blend bit (`render/static_meshes.rs:182,549`), entirely independent of texture encoding.
- The same texels also feed `decalWeight` (`triangle.frag:1580`, `smoothstep` → 0) and `finalAlpha` (`:2562`) for glass-classified blend meshes.

## Impact
Bounded, visual, cross-game (single DDS decode path serves all games). Affects only meshes that are *both* alpha-blend-flagged *and* textured with a BC1 diffuse an encoder authored with 3-colour blocks in opaque regions. Bethesda blend surfaces are predominantly BC3/DXT5 or BC1 4-colour, so corpus exposure is small but non-zero; symptom is pinhole/speckle dropout on blended surfaces (the inverse of the FO4 bug this commit fixed). The opaque majority is unaffected.

## Related
`2aac5351`; implicit-discard rationale at `triangle.frag:216-222` (#263 era). Distinct from the 2026-06-14 FO4-XCUT-MEDIUM-01 (DXGI 88, now landed via #1596).

## Suggested Fix
Plumb a per-material `diffuseHasPunchAlpha` bit set CPU-side only when the source material is genuinely cutout (NIF/BGSM alpha-test flag set), and require it in the line-223 implicit-discard condition — confining the new behavior to intended cutout content. This mirrors the existing `format_has_alpha`/`NORMAL_ALPHA_SPEC_BIT` plumbing pattern. Lower-effort interim: validate on real FNV/FO3 framed-picture interiors (`--esm FalloutNV.esm --cell <gallery interior> --bench-hold`) and only invest if speckle appears.

## Completeness Checks
- [ ] **SIBLING**: Both BC1 decode arms (`dds.rs:322` FourCC DXT1 + `dds.rs:361` DXGI BC1_UNORM/_SRGB) are covered by whichever gating approach is chosen
- [ ] **CANONICAL-BOUNDARY**: If a `diffuseHasPunchAlpha` bit is added, it is set CPU-side at the NIF/BGSM alpha-test boundary and plumbed through `Material`/`GpuMaterial` — never re-derived at render time from texture encoding
- [ ] **TESTS**: A regression test pins the gated behavior (3-colour-block opaque BC1 on an alpha-blend mesh no longer discards; genuine cutout still discards)
