# #7171 — PAR-D1-2026-11-21b-03: #7156's loose bound is loose, and an exhausted budget still enumerates a whole directory

Labels: low,import-pipeline,bug

- **Severity**: LOW
- **Dimension**: Size Discipline
- **Location**: `byroredux/src/asset_provider/archive.rs` `sorted_entries`, `walk`
- **Status**: NEW (residual of #7156, closed)
- **Description**: memory is bounded at `max(2 * keep, 64)` entries, which at the default 2^21 budget is up to ~4 M `(OsString, DirEntry)` pairs, a few hundred MB transient (not measured). With `keep == 0` the whole listing is still walked to set `dropped`, holding nothing; stopping at the first entry would do. The test does not assert the 2x holding bound.
- **Suggested Fix**: return early for `keep == 0` after one entry, and consider a lower budget for the per-directory hold.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / cache key)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21b.md`
