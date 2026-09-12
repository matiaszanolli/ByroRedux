# SF-2026-09-11-D9-03: the guard test for D9-02's regression leaves both dropped fields empty in its fixture, so the every-BGEM-texture-role test covers 6 of 8 and cannot fail

**Issue**: #4288 — https://github.com/matiaszanolli/ByroRedux/issues/4288
**Labels**: low,nifal,test-gap,game:starfield,legacy-compat,bug

**Severity**: LOW
**Dimension**: Dimension 9 — BGSM/BGEM External Material Flow
**Location**: `byroredux/src/cell_loader/refr_texture_overlay_tests.rs:714 (fill_from_bgsm_forwards_every_bgem_texture_role)`
**Status**: NEW

## Description
`fill_from_bgsm_forwards_every_bgem_texture_role` (`byroredux/src/cell_loader/refr_texture_overlay_tests.rs:714`) is meant to pin that every BGEM texture role gets forwarded by `fill_from_bgsm`, but its fixture leaves `base_texture` and the envmap-mask field empty — the exact two fields D9-02 found are silently dropped by the production code. Because the fixture never populates them, the test asserts nothing about those two roles and would not fail even with the D9-02 bug present; it covers 6 of the 8 real BGEM texture roles.

## Evidence
Verified during this audit: the test fixture at that location constructs a BGEM with `base_texture`/env-mask left as their default-empty values, so the assertions never exercise the two roles D9-02's production-code defect drops.

## Impact
Test-coverage gap directly enabling D9-02 to exist undetected — "every BGEM texture role" is an inaccurate description of what the test actually covers.

## Related
Same root cause as D9-02; fixing the test alongside the production fix is the natural pairing.

## Suggested Fix
Populate `base_texture` and the envmap-mask field in the test fixture so the test genuinely covers all 8 BGEM texture roles, and pair the fix with D9-02's production-code fix so this test starts (and stays) green for the right reason.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
