# SF-2026-09-11-D3-03: the real-data CDB test runs the unlimited parse (9.19 GB measured peak) with no ParseLimits and no memory warning in its own documented run command

**Issue**: #4274 — https://github.com/matiaszanolli/ByroRedux/issues/4274
**Labels**: medium,import-pipeline,test-gap,game:starfield,legacy-compat,bug

**Severity**: MEDIUM
**Dimension**: Dimension 3 — CDB Material Database Correctness
**Location**: `crates/sfmaterial/ real-data test (CDB Phase 2 spike test); ParseLimits::unlimited()`
**Status**: NEW

## Description
The real-data CDB test measures a 9.19 GB peak memory footprint parsing a single full-size Starfield CDB, but the test invokes the parser with `ParseLimits::unlimited()` and its own documented run command carries no memory warning for whoever runs it — a contributor following the documented command on a memory-constrained machine gets no advance notice of the ~9-18 GB requirement (the corpus-wide total across all 13 CDBs, two of them full-size, is ~18 GB per this audit's remaining-work chain).

## Evidence
Verified during this audit: the test's own doc/run instructions state the command with no memory caveat; `ParseLimits::unlimited()` (`crates/sfmaterial/src/reader.rs:158-164`) is what the test passes, imposing no instance-count ceiling.

## Impact
A contributor running the documented real-data CDB test on a machine with less than ~10-20 GB free RAM can OOM or thrash with no warning from the test's own instructions.

## Related
Adjacent to #3398 (CDB Phase 2) and the already-known corpus-wide ~18 GB parse-peak figure this audit's remaining-work chain restates precisely (13 CDBs, two full-size at ~105 MB/~1.46M chunks each).

## Suggested Fix
Add a memory-requirement note to the test's run instructions/doc comment, and consider gating the full unlimited parse behind an opt-in env var (similar to other large real-data tests in this repo) rather than running it by default under `--ignored`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
