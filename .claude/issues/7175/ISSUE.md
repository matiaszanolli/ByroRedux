# #7175 — PAR-D2-2026-11-21b-04: DBFileIndex read errors and `DuplicateDbFileIndex` are still not attributed to a stream instance

Labels: low,import-pipeline,bug,game:starfield

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/sfmaterial/src/index.rs` `MaterialIndex::build` (`stream_db_file_index(&mut state, &mut idx)?`, `Err(Error::DuplicateDbFileIndex)`)
- **Status**: NEW (residual of #7161, closed)
- **Description**: a reader EOF inside the index's own LIST (a cut `Objects` or `Components` row) surfaces bare, although `error.rs` says the index names itself.
- **Suggested Fix**: wrap with `AtInstance { index: pos, class: "BSComponentDB2::DBFileIndex" }`, leaving the already-typed variants alone.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other codec / archive format / cache key)
- [ ] **TESTS**: A regression test pins this specific fix

Source: `docs/audits/AUDIT_PARSERS_2026-11-21b.md`
