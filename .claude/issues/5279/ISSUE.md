# #5279: NIFAL-D1-2026-10-05-01: `nifal.md`'s lowering step 3 still calls `resolve_pbr`'s NaN classifier arm "a backstop for future non-pre-classified sources", which is the third copy of the claim #4441 corrected in rustdoc

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5279
- **Labels**: low,nifal,documentation,doc-rot
- **Source**: `docs/audits/AUDIT_NIFAL_2026-10-05.md` (NIFAL-D1-2026-10-05-01)

_From `docs/audits/AUDIT_NIFAL_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW · **Dimension**: Material · **Tier Violated**: doc / record-keeping · **Game Affected**: FO4, FO76, Starfield
- **Location**: `docs/engine/nifal.md:854-859`
- **Status**: NEW. This is an incomplete sibling of #4441 (closed by `c2b67d81e`, which touched only `material_translate.rs` and
  `material.rs`). The rustdoc half is REN-D6-2026-10-05-01.
- **Description**: The spec says "For NIF-imported content … `Some(…)` is always present and `Material::resolve_pbr()` only clamps —
  its classifier arm (the `NaN` sentinel path) is a backstop for future non-pre-classified sources." Both halves are false today:
  - #2707's Starfield material-reference stubs leave both overrides `None` at NIF import;
  - the BGEM merge deliberately leaves them NaN.

  The paragraph also does not record #5197: a CDB **hit** stamps `PbrMaterial::NO_SIGNAL_NEUTRAL` (`merge.rs:219-236`), so only CDB
  misses and BGEM reach the classifier. This is the deletion-inviting text #4284 and #4441 were filed against, left standing in
  the spec that auditors read first.
- **Evidence**: `grep -n backstop docs/engine/nifal.md` → `:858`. The commit body of `c2b67d81e` lists only the two rustdoc
  statements.
- **Impact**: A contributor working from the spec concludes the classifier arm is dead for current content.
- **Related**: #4441, #4284, #5197, REN-D6-2026-10-05-01, #5210 (the same doc pass).
- **Suggested Fix**: Rewrite step 3 to name the live NaN producers (BGEM, and Starfield stubs whose `.mat` misses the CDB or that
  run with no CDB) and the `NO_SIGNAL_NEUTRAL` CDB-hit outcome. Fold this into #5210 and REN-D6-2026-10-05-01.

**Publish note**: this is the spec half of the same stale claim. The rustdoc half (`material_translate.rs` boundary contract and `resolve_pbr` inline comment) is filed separately as REN-D6-2026-10-05-01 from AUDIT_RENDERER_2026-10-05. Fix both together with #5210's NIFAL doc pass.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
