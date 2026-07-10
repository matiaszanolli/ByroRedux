# REN-D1-03: SHADOW_MASK_* → u8 truncation site has no 8-bit ceiling pin (sibling of the #957 24-bit mirror-assert)

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/1913

**Severity**: low
**Dimension**: renderer audit 2026-07-09
**Location**: `crates/renderer/src/vulkan/acceleration/tlas.rs:264-268` (`shadow_mask` cast in `build_tlas`); `crates/renderer/src/shader_constants_data.rs:71-72` (`SHADOW_MASK_OPAQUE`/`SHADOW_MASK_GLASS` as `u32`)
**Status**: NEW

## Description
The new per-instance mask (commit `977eb95a`) selects a `u32` constant and casts `as u8` into `Packed24_8::new(ssbo_idx, shadow_mask)`. Today's values (0x01/0x02) are safe, but the constants are `u32` in a file positioned as an extension point for more buckets: a future `SHADOW_MASK_FOLIAGE = 0x100` would truncate to **0** — an instance with mask 0 is skipped by every ray query regardless of `cullMask`, i.e. silent, total RT dropout (no shadows/reflections/GI hits) for that bucket, invisible to `cargo test` and to validation layers. This is the exact failure class the file already pins for the 24-bit custom index (`debug_assert!(ssbo_idx < 1<<24)` mirroring the `MAX_INSTANCES` const-assert, #957). There is also no unit test pinning the bucket assignment.

## Evidence
`shader_constants_data.rs:71-72` declares the two constants with no `const`-assert; `tlas.rs:268` casts `as u8` with no guard; `grep SHADOW_MASK acceleration/tests.rs` → no hits.

## Impact
No live bug — hardening gap at a freshly-opened extension point whose failure mode is silent geometry disappearance from all ray queries.

## Related
#957 (the 24-bit sibling pattern); commit `977eb95a`

## Suggested Fix
Add a compile-time assert that both constants are ≤ 0xFF, nonzero, and distinct; add a tests.rs pin for the glass/opaque bucket selection.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
