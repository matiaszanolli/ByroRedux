# OBL-2026-08-16-BR-01: an Oblivion-named regression test asserts wrong semantics and blocks the #3036 fix

**Issue**: #3102
**Severity**: MEDIUM
**Labels**: `medium,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_OBLIVION_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_OBLIVION_2026-08-16.md` (blocking-regression sweep).

**Location**: `byroredux/src/cell_loader/finish_partial_tests.rs`:179-196

## Description

A regression test named for Oblivion **asserts the wrong semantics and will block the fix** for #3036.

```rust
/// Pre-FO4 content (Oblivion/FO3/FNV, BSVER < FALLOUT4) with bit 5 set
/// IS a genuine editor marker and must still be skipped — the fix must
/// not regress the case it was never wrong about.
#[test]
fn finish_partial_import_oblivion_bsx_bit5_is_still_editor_marker() {
    let mut world = world_with_registries();
    let partial = dummy_partial_with(0x20, byroredux_nif::version::bsver::OBLIVION);
    finish_partial_import(&mut world, None, None, "xmarkerheading.nif", partial);
```

Re-verified 2026-08-17.

## Impact

The test encodes the exact premise #3036 disproves. `BSXFlags` bit 5 means *"this file **contains** editor-marker children"*, not *"this file **is** an editor marker"* — nif.xml is explicit, and the corpus sweep found **223 FNV meshes carrying real geometry and collision** that are dropped by this rule.

The test's fixture (`0x20`, `xmarkerheading.nif`) happens to be a genuine pure marker, so the *assertion* is right for that input — but its **name and doc comment generalise it to all pre-FO4 bit-5 content**, which is the false claim. Anyone landing #3036's fix will see this test fail and reasonably conclude the fix is wrong.

## Suggested Fix

Rewrite the test to assert what is actually true: a NIF whose **only** content is an editor marker imports to zero meshes. Add a sibling asserting that a bit-5 NIF **carrying real geometry** keeps it — which is #3036's fix criterion.

Correct the doc comment, which is the part that generalises.

## Related

- **#3036 (FNV-D1-01 — the fix this test would block; 223 real meshes deleted)**
- #3070 (SKY-D1-01 — the misleading comment on the same predicate)

## Completeness Checks
- [ ] **CO-RESOLVE**: Updated as part of #3036's fix, not separately
- [ ] **RIGHT-ASSERTION**: The test pins "marker-only NIFs import empty", not "all bit-5 NIFs are skipped"
- [ ] **SIBLING-TEST**: A companion asserts a bit-5 NIF with real geometry survives
- [ ] **DOC-COMMENT**: The generalising comment is corrected
- [ ] **TESTS**: Both tests pass after #3036's fix and fail before it

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3102 --json state` when live state is needed.*
