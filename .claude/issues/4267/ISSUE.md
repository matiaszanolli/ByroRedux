# SF-DIM1-02: no byte-literal fixture pins the documented mixed-raw+LZ4-chunk DX10 BA2 record

**Issue**: #4267 — https://github.com/matiaszanolli/ByroRedux/issues/4267
**Labels**: low,import-pipeline,test-gap,game:starfield,legacy-compat,bug

**Severity**: LOW
**Dimension**: Dimension 1 — BA2 v2/v3 LZ4 Block Decompression
**Location**: `crates/bsa/src/ba2.rs; crates/bsa/tests/ (no unit fixture)`
**Status**: NEW

## Description
The BA2 reader documents and handles a mixed-raw/LZ4-chunk DX10 texture-record population (measured at 3.66% of the real corpus during this audit), but no byte-literal unit fixture exercises this specific shape — only the opt-in real-data sweep against the actual 129-archive Starfield corpus would catch a regression here.

## Evidence
Live-verified clean during this audit: the real 129-archive sweep (`--ignored`) opened 129/129 archives with 0 failures, and a standalone deep-extract probe on 9,244 real DX10 texture files found 0 errors, including archives large enough to exercise the mixed-raw/LZ4-chunk population. No committed synthetic fixture reproduces this shape in bytes, though.

## Impact
A regression in the mixed-raw/LZ4-chunk selector would only be caught by the opt-in real-data sweep (requires local game data + `--ignored`), not by the default `cargo test` run a contributor gets on a change to `ba2.rs`.

## Related
None filed.

## Suggested Fix
Add a synthetic byte-literal fixture (constructed record header + one raw chunk + one LZ4-compressed chunk) to the default (non-`--ignored`) BA2 test suite, mirroring the existing pattern of synthetic v103/v104/v105 BSA fixtures.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
