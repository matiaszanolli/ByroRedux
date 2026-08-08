# SF-D6-02: opaque 38-byte Starfield tail is BSSPLuminanceParams at documented defaults, not opaque

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2622
**Finding ID**: SF-D6-02

**Severity**: MEDIUM
**Dimension**: 6 (NIF Shader Blocks, BSVER 155+)
**Location**: `crates/nif/src/blocks/shader.rs:1183-1187` (the `bsver < STARFIELD` luminance/translucency/texture-array gate), `:1276-1303` (`read_wetness_block`), `:740-751` (the tail doc comment)
**Status**: NEW

## Description
#1510 concluded the FO76 luminance tail is absent on Starfield; #1606 then
declared the residual 38 bytes opaque. The tuple `(wetness.metalness,
wetness.unknown_1, tail_f32[0], tail_f32[1])` takes exactly two values
across 1,879 Meshes01 blocks: `1868× (100.0, 13.5, 2.0, 3.0)` and
`11× (-1.0, 100.0, 13.5, 2.0)` (the SF-D6-01-shifted outliers).
`(100.0, 13.5, 2.0, 3.0)` are, in order, nif.xml's documented
`BSSPLuminanceParams` defaults — the same quad that is the *decoded,
aligned* `luminance` on every Starfield `BSEffectShaderProperty`. Four
documented defaults appearing as an invariant contiguous quad across 1,879
materially different blocks is not coincidence.

## Evidence
Corpus measurement across Meshes01: the "opaque tail" decomposes cleanly
into `BSSPLuminanceParams` defaults once SF-D6-01's shift is accounted for.

## Impact
`LuminanceParams` is `None` for every Starfield `BSLightingShaderProperty`
(exposure-offset/emittance authoring unavailable for the era's HDR path),
and `WetnessParams.metalness` is populated with `100.0` (an emittance
value, not consumed downstream today, but a live trap for whoever wires
Starfield wetness up).

## Suggested Fix
After SF-D6-01 lands, re-enable the `BSSPLuminanceParams` read for
`bsver >= STARFIELD` and re-derive the wetness field count from the corpus.
Do not name the remaining ~30 undocumented bytes.

## Related
SF-D6-01 (must land first — the +4 shift in 11 blocks is its artifact);
#1510; #1606.

## Completeness Checks
- [ ] **TESTS**: A corrected-alignment fixture asserts `LuminanceParams` is populated with the documented defaults, not `None`
