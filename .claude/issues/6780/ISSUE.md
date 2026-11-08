# ESM-D1-03: `character_rules_profile` re-bands on raw `hedr_version < 1.0`, so a defaulted header selects the FO3 profile

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6780
Labels: info, esm-plugin, bug
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: INFO (malformed input only; no vanilla or installed plugin triggers it)
- **Dimension**: 1, missing/short-HEDR fallback audit
- **Location**: `crates/plugin/src/esm/records/parse.rs:85` (`GameKind::Fallout3NV if hedr_version < 1.0 => CharacterRulesProfile::FALLOUT3`); called at `:140`
- **Status**: NEW
- **Description**: `read_file_header` leaves `hedr_version = 0.0` when HEDR is absent or shorter than 12 bytes.
  `GameKind::from_header` then defaults to `Fallout3NV` (documented). The second key, `hedr_version < 1.0`,
  then reads that 0.0 as "FO3 (0.85/0.94)" and applies FALLOUT3 character rules, so the "defaulted" case is
  treated as "detected FO3". The checklist asks that no branch do this.
- **Evidence**: Census of all installed TES4 headers: every FO3 file is 0.94 and every FNV file is 1.32 to 1.34,
  so no real plugin hits the branch. Pre-GOTY FO3 (0.85) is the intended case.
- **Impact**: A headerless/garbled plugin would parse with FO3 leveling rules instead of the FNV default.
  The two lineages share record layouts, so the damage is limited to the CHARAL policy row.
- **Suggested fix**: Optional. Gate on `hedr_version > 0.0 && hedr_version < 1.0`, or carry a `hedr_present`
  bool on `FileHeader`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
