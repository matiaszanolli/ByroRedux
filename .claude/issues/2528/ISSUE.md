# NIF-D2-2026-08-07-01: docs/engine/nif-parser.md overstates NifVariant's live role

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2528
**Finding ID**: NIF-D2-2026-08-07-01

**Severity**: LOW
**Dimension**: Version Gating
**Game Affected**: None functionally — documentation only
**Location**: `docs/engine/nif-parser.md:180-183` ("Version handling" section)
**Status**: NEW

## Description
The doc claims `NifVariant::detect()` "still drives a handful of genuinely variant-level decisions (which `ShaderFlags` vocabulary applies, which shader-property type a mesh uses)." This is stale post-`#1897` (which deleted the `ShaderFlags<'a>` typed view): the only production consumer of `NifVariant` outside `version.rs`/tests is `havok_scale_for`, mapping variant → Havok-to-engine unit scale. `shader.rs` never references `NifVariant` at all — shader dispatch is purely raw-`bsver`-band based.

## Evidence
Confirmed directly: `grep -rn "NifVariant" crates/nif/src/blocks/shader.rs` returns zero hits.

## Impact
Purely documentation. A contributor trusting this line could conclude there's a `NifVariant`-keyed shader-flags mechanism worth extending, reintroducing exactly the transitional-export foot-gun `#160`/`#1331`/`#1838`/`#1839` fixed.

## Related
Similar in kind to already-tracked `#2274` (SKILL doc-rot).

## Suggested Fix
Replace the parenthetical with the actual sole consumer — the Havok-to-engine unit scale in `lib.rs::havok_scale_for` (7.0 pre-Skyrim, 69.99125 Skyrim+). Delete the ShaderFlags/shader-property-type clause.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only change)
