# #4435: SF-2026-09-16-D3-02: The Phase-2 spike (the source of truth for #3398) points at three examples deleted four days after it was written; the CDB key derivation has no in-tree reproduction

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4435
- **Labels**: low,documentation,doc-rot,game:starfield,legacy-compat
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_STARFIELD_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: LOW
- **Dimension**: 3
- **Location**: `docs/audits/SF_CDB_PHASE2_SPIKE_2026-08-29.md:18-25, 222-231, 234-242`
- **Status**: NEW
- **Description**: The spike's "Reproduce with" block and its Artifacts list
  name the following. All three were removed by `a823c13a1` (#3150,
  2026-09-02, "delete stale _tmp_ scratch examples"). The spike is dated
  2026-08-29.
  - `crates/sfmaterial/examples/_tmp_cdb_phase2_spike.rs`
  - `crates/sfmaterial/examples/_tmp_cdb_hash_probe.rs`
  - `crates/nif/examples/_tmp_sf_matpath_dump.rs`

  The hash probe is the only tool that established the 3,032 / 3,084 (98.3%)
  key match and the rotated `BSResource::ID` columns. Phase 2's lookup key
  rests on that result.

  The same section also cites stale locations:
  - §4 step 4: `byroredux/src/asset_provider/material.rs`, which is now a
    directory (#3857).
  - §4 step 6: `starfield_mat.rs:177-188`. The pinned test is now at lines
    168-209.

  (The XMCOLOR check the spike also housed now has a committed guard, #4275.)
- **Evidence**: `git show a823c13a1 --stat` lists all three deletions, and
  `crates/sfmaterial/examples/` no longer exists.
- **Impact**: A Phase-2 implementer who follows the tracker's source doc hits
  dead commands. The key derivation can only be re-verified by digging up
  `a823c13a1^`.
- **Suggested Fix**: Restore the hash probe as a named, documented diagnostic
  (the convention `sf_smoke.rs` follows). Or at minimum, rewrite the
  Reproduce and Artifacts sections to `git show a823c13a1^:<path>` and fix the
  two stale locations.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
