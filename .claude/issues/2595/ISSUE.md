# FO4-D2-2026-08-07-02: fill_from_bgsm has zero test coverage (mat_provider always None)

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2595
**Finding ID**: FO4-D2-2026-08-07-02

**Severity**: LOW
**Dimension**: 2 (Materials)
**Location**: `byroredux/src/cell_loader/refr_texture_overlay_tests.rs`
**Status**: NEW

## Description
`fill_from_bgsm` (`byroredux/src/cell_loader/refr.rs:185-225`) has zero test
coverage — every test in `refr_texture_overlay_tests.rs` constructs its
`MaterialProvider` as `None`, which short-circuits before `fill_from_bgsm` is
ever reached. This is precisely why the partial-forwarding gap in
FO4-D2-2026-08-07-01 went unnoticed.

## Evidence
`refr_texture_overlay_tests.rs` uses `mat_provider: None` throughout; no test
exercises the `.bgsm`/`.bgem` resolve-and-fill branches of `fill_from_bgsm`.

## Impact
The BGSM/BGEM texture-role forwarding logic in `fill_from_bgsm` can silently
regress (as it already has, per FO4-D2-2026-08-07-01) with no test to catch
it.

## Suggested Fix
Add a test that constructs a real `MaterialProvider` (or a fake with a known
BGSM/BGEM fixture) and asserts every texture role `merge_external_material`
forwards is also forwarded by `fill_from_bgsm`.

## Related
FO4-D2-2026-08-07-01

## Completeness Checks
- [ ] **TESTS**: This finding IS the test-coverage fix
