# OB-D2-01: no permanent brute-force full-archive extraction test for BSA v103, unlike the existing v105 test

**Issue**: #4259 — https://github.com/matiaszanolli/ByroRedux/issues/4259
**Labels**: low,import-pipeline,test-gap,game:oblivion,legacy-compat,bug

**Severity**: LOW
**Dimension**: Dimension 2 — BSA v103 Archive
**Location**: `crates/bsa/tests/bsa_real.rs`
**Status**: NEW

## Description
`crates/bsa/tests/bsa_real.rs` has a brute-force full-archive extraction regression test for Skyrim SE's v105 (`skyrimse_meshes_bsa_v105_brute_force_extract_zero_errors`) but only a single-file smoke test for Oblivion's v103 (`oblivion_meshes_bsa_v103_extracts_nif_with_gamebryo_magic`) — no equivalent permanent brute-force sweep across all v103 files.

## Evidence
A live ad hoc sweep run during this audit found 147,629 files / 0 failures across all 17 vanilla + DLC Oblivion archives, so v103 extraction is provably 100% clean today. That sweep is not a committed test, so a future regression in v103's zlib codec path, 16-byte folder-record handling, or the Xbox-archive-flag gate would not be caught by `cargo test`.

## Impact
Coverage gap only, not a present defect — a future regression in the v103 extraction path (zlib decompression, folder-record parsing) has no permanent test gate, unlike the equivalent v105 path.

## Related
None filed. Adjacent to the already-closed #699 ("v103 is broken" premise).

## Suggested Fix
Add a `oblivion_meshes_bsa_v103_brute_force_extract_zero_errors` test mirroring the existing v105 test, gated the same way behind the real-data env var, iterating every file in the vanilla + DLC v103 archives.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_OBLIVION_2026-09-11.md — findings verified against live code during this publish run.*
