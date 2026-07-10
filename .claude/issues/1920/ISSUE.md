# REN-D3-04: generated_header_contains_all_defines value-pins omit 10 non-DBG emitted defines, including the 2-day-old SHADOW_MASK_* pair

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1920

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/shader_constants.rs:69-131` (test `generated_header_contains_all_defines`)
**Status**: NEW

## Description
The value-pin walks 40+ expected `#define` lines but omits 10 constants that `build.rs` emits: `CLUSTER_NEAR`, `CLUSTER_FAR_FLOOR`, `CLUSTER_FAR_FALLBACK`, `VERTEX_NORMAL_OFFSET_FLOATS`, `VERTEX_UV_OFFSET_FLOATS`, `SHADOW_MASK_OPAQUE`, `SHADOW_MASK_GLASS`, `GI_HIT_LIGHT_CAP`, `CAUSTIC_FIXED_SCALE`, and the `ENABLE_LEGACY_WRS` header line. A `build.rs` mis-emission of any of these (the #1482 failure class) would ship silently.

## Evidence
Expected-lines list at `shader_constants.rs:71-115` vs emissions in `build.rs:58-367`; current header values verified correct by manual diff (this is purely a regression-net gap, not a live mismatch).

## Impact
None today; a future `build.rs` edit to an unpinned constant could bake a wrong value into recompiled shaders with green tests.

## Related
#1860 (open, tracks the DBG_BITS catalog subset of this same net — not duplicated here); #1482 (original 4-of-13 value-pin gap)

## Suggested Fix
Extend the expected-lines array with the 10 missing entries; alternatively drive the whole test from a shared catalog the way DBG_BITS does.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
