# VOL-D16-02: Stale gate/consumption comment block in draw.rs contradicts the 977eb95a flip

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1938

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/vulkan/context/draw.rs:573-598`
**Status**: NEW

## Description
The comment block still asserts "The composite shader currently multiplies the volumetric result by 0.0 (composite.frag:362) ... While the output is unused, dispatching the inject + integrate passes is pure GPU waste." Post-`977eb95a`, `VOLUMETRIC_OUTPUT_CONSUMED = true` and `composite.frag:445` does `combined = combined * vol.a + vol.rgb` — the output is consumed and the `* 0.0` no longer exists. The referenced `composite.frag:362` line number is also stale.

## Evidence
`volumetrics.rs:154` (`= true`); `composite.frag:445`; the `977eb95a` diff replaced the `*0.0` site.

## Impact
Misleads future maintainers into thinking the dispatch is dead weight; risks an erroneous "optimization" that removes a now-live pass.

## Related
REN-D8-01 (composite dead-fog-fallback); DIM12-01 (cross-pass doc dependency)

## Suggested Fix
Rewrite the block to reflect the consumed state; drop the `*0.0` / `composite.frag:362` references.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
