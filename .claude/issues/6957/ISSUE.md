# #6957: TD2-2026-11-15-04: `consume_list` / `consume_map` re-inline the count check that `checked_container_count` already holds; `consume_object` / `skip_object` / `stream_db_file_index` triplicate the chunk-kind match

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6957
- **Labels**: low,import-pipeline,game:starfield,tech-debt,bug
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Dimension**: 2 · **Severity**: LOW · **Effort**: small · **Labels**: `tech-debt` · **Existing**: none
- **Age**: `checked_container_count` 9d94875e5 (2026-09-17); read/skip split drift fixed by #5323 (258ba2b90, 2026-10-05)
- **Location**: `crates/sfmaterial/src/reader.rs:586` (`consume_object`), `:628` (`consume_list`), `:671` (`consume_map`), `:697` (`skip_object`), `:728` (`skip_list`), `:743` (`skip_map`), `:760` (`checked_container_count`); `crates/sfmaterial/src/index.rs:741` (`stream_db_file_index`)
- **Description**: three things.
  - `skip_list` and `skip_map` call `checked_container_count(raw, payload.len(), what)?`. `consume_list` and `consume_map` spell the same `usize::try_from(raw).map_err(NegativeCount).min(payload.len())` inline, with a 25-line #2623 comment on the first and a "same rationale" pointer on the second.
  - The `(is_cast, is_diff)` match over `ChunkType::{Objt, User, Diff, Usrd}`, the `WrongChunkType` fallback, the `consume_chunk` / `Cursor` open, the cast `target_ref` prefix, the trailing cast `u32` and the trailing-bytes check are copied between `consume_object` and `skip_object`. `stream_db_file_index` carries the third copy, deliberately narrowed (it rejects DIFF/USRD since #5320).
  - History: #5323 was exactly a read/skip drift (the read path moved to `Class::read_order`, the skip path did not, so a reordered class desynced); the fix added a byte-exact skip-vs-read equivalence test rather than removing the second copy.
- **Suggested Fix**: `State::open_object(&mut self) -> Result<ObjectChunk { payload, cur, is_cast, is_diff }>` and `ObjectChunk::finish(self) -> Result<()>` (cast trailer plus the `ObjectTrailingBytes` check), used by all three; `consume_list` / `consume_map` call `checked_container_count`. `stream_db_file_index` keeps its DIFF/USRD refusal as a pre-check on `peek_kind()` before calling `open_object`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or an extended guard) pins this specific fix
