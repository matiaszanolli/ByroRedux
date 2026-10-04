# #5234 — FO4-D2-02: the #3639 "template parent supplies the gloss map" case is unpinned; the sibling test claims coverage it lacks

https://github.com/matiaszanolli/ByroRedux/issues/5234

Source: `docs/audits/AUDIT_FO4_2026-10-03.md` (HEAD `32f4450d9`)

**Source**: `docs/audits/AUDIT_FO4_2026-10-03.md` (FO4-2026-10-03-D2-02)

- **Severity**: LOW (test gap; production is correct)
- **Dimension**: BGSM/BGEM merge
- **Location**: `byroredux/src/asset_provider/tests/bgsm_merge.rs:1052-1090`
- **Status**: NEW
- **Description**:
  - The fallback at `merge.rs:1291` runs after the full chain walk, so a parent's `smooth_spec` correctly disarms it.
  - The guard's doc claims "even from a template parent", but its fixture puts `smooth_spec_texture` on the leaf with `parent: None`. No test has a smooth-spec supplied only by a parent.
  - Two regressions would stay green: moving the check inside the loop, or keying it on the leaf's own path.
- **Impact**: guard gap only. 0 vanilla BGSMs are in this state.
- **Related**: #3639, #5012.
- **Suggested Fix**: add a chain fixture (leaf: spec on, smoothness 1.0; parent: `smooth_spec_texture`) that asserts `roughness_override == Some(0.04)`, and fix the sibling doc.

## Completeness Checks
- [ ] **TESTS**: a chain fixture (leaf spec-on + smoothness 1.0, parent supplies `smooth_spec_texture`) asserts `roughness_override == Some(0.04)`
