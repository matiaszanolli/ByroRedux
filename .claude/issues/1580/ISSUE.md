**Severity**: LOW · **Dimension**: BGSM/BGEM External Flow
**Location**: `crates/bgsm/src/bgem.rs:49` (parsed) / `byroredux/src/asset_provider.rs:1399-1501` (not forwarded)
**Source**: `docs/audits/AUDIT_STARFIELD_2026-06-14.md` (SF-D9-02)

## Description
BGEM parses `grayscale_to_palette_alpha: bool` but the merge arm forwards only the LUT *texture* (→ `EFFECT_PALETTE_COLOR`), never the alpha-variant bool, so `EFFECT_PALETTE_ALPHA` is set only from the inline `BSEffectShaderProperty` SLSF1 source, never from the `.bgem` file.

## Evidence
`grayscale_to_palette_alpha` has zero consumers outside the parser (confirmed: only `bgem.rs:49,140,206`); `EFFECT_PALETTE_ALPHA` is only ORed from `es.effect_palette_alpha`.

## Impact
Narrow — only a FO4/Starfield-mod `.bgem` that sets palette-*alpha* (not color) and lacks the inline SLSF1 bit would remap by luminance into color instead of alpha. Vanilla FO4 palette-alpha effects use the inline path; near-zero visible impact.

## Related
SF-D9-01 (INFO — `BGSM_AUTHORED` telemetry flag on the effect path).

## Suggested Fix
In the BGEM arm, when `bgem.grayscale_to_palette_alpha`, set a corresponding `ImportedMesh` flag and OR in `EFFECT_PALETTE_ALPHA`. Confirm against a real `.bgem` corpus first — may be empty in practice.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: The alpha-vs-color palette decision is resolved at the BGEM→`Material`/`ImportedMesh` merge boundary (`asset_provider.rs`), not pushed into the effect shader
- [ ] **SIBLING**: The `EFFECT_PALETTE_COLOR` forward and the new `EFFECT_PALETTE_ALPHA` forward are handled symmetrically in the same merge arm
- [ ] **TESTS**: A test feeds a `.bgem` with `grayscale_to_palette_alpha=true` and asserts `EFFECT_PALETTE_ALPHA` is set
