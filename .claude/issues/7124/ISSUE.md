# #7124: PAR-D2-2026-11-20-01: `stream_list` (the DBFileIndex LIST streamer) has no trailing-bytes check and its own copy of the count clamp; a short count or trailing bytes build an Ok index, a zero-count `Objects` list builds an Ok index with no keyed objects

**Labels**: medium,import-pipeline,nifal,bug,game:starfield
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/7124

**Source**: `docs/audits/AUDIT_PARSERS_2026-11-20.md`, finding PAR-D2-01 (`/audit-parsers`, HEAD f3713bf5e)

- **Severity**: MEDIUM
- **Dimension**: Error Semantics
- **Location**: crates/sfmaterial/src/index.rs:885 (`stream_list`), vs crates/sfmaterial/src/reader.rs:694 (`skip_list`, `expect_consumed`) and :713 (`checked_container_count`)
- **Status**: NEW (the coordinator confirmed this at `crates/sfmaterial/src/index.rs:885-900`: there is no `expect_consumed` after the loop, and the clamp is inline rather than through the shared `checked_container_count`) (#6957 consolidated consume_list/skip_list onto `expect_consumed` + `checked_container_count` but left `stream_list` as a third, divergent copy — the read/skip/stream drift #6957 set out to remove)
- **Description**: `consume_list` and `skip_list` end with `expect_consumed(&cur)` and take their count from `checked_container_count`. `stream_list` (used for the DBFileIndex `Components` / `Objects` / `Edges` side chunks) re-implements the count clamp inline and never calls `expect_consumed`. So a LIST chunk whose declared count is lower than the elements it holds, or that carries trailing bytes, is accepted silently, and the unread elements are dropped. `Components` is protected only indirectly by the row/instance join (`RowInstanceMismatch`); `Objects` (the PersistentID -> DBID key table) and `Edges` have no such backstop, and the `NoKeyedObjects` gate (#5899) only fires when `keyable_rows > 0`, so an `Objects` list that decodes to zero rows is not an error either. The result is the #5320 class: a degraded build that looks successful (every `.mat` lookup misses -> PBR fallback, no error, no warning).
- **Evidence**: probe (`sflist`, synthetic CDB from `index::test_support`, HEAD source): unmodified build `Ok(1 material)`. Appending 8 trailing bytes to each of the three DBFileIndex LIST chunks: `Ok(1)` for all three (a whole-value `consume_list`/`skip_list` would return `ObjectTrailingBytes`). Lowering the `Objects` LIST count 1 -> 0: `Ok(0)` — the index builds with zero keyed objects, no error. Lowering `Components` 9 -> 8: `Err(RowInstanceMismatch)` (the only backstop). Negative count: `Err(NegativeCount)`.
- **Impact**: A corrupt or mod-authored Starfield CDB whose DBFileIndex lists disagree with their payload loads with every material unresolved and nothing in the log; vanilla is unaffected (the five real-CDB tests pass, counts exact). Silent-degrade, not memory-unsafe.
- **Trigger Input**: Starfield `materialsbeta.cdb` whose DBFileIndex `Objects` LIST chunk has `count` < elements present (e.g. 0) or bytes after the last element.
- **Related**: #5320, #5899, #6957, #2623
- **Suggested Fix**: make `stream_list` call `checked_container_count` (export it) and finish with `expect_consumed(&cur)`; make a build whose `Objects` list yielded zero keyed objects an error regardless of `keyable_rows`; add a test beside `build_rejects_trailing_bytes_in_the_dbfile_index` per list chunk.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other Bink codec arm, the other reader that shares this helper)
- [ ] **TESTS**: A regression test pins this specific fix
