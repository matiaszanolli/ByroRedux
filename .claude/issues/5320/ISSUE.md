# #5320: PAR-D2-2026-10-05-02: `MaterialIndex::build` degrades silently: a missing index, a row/instance count mismatch, and an unchecked `DBFileIndex` payload all return `Ok`

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5320
- **Labels**: low,import-pipeline,nifal,bug,game:starfield
- **Source**: `docs/audits/AUDIT_PARSERS_2026-10-05.md` (PAR-D2-2026-10-05-02)

_From `docs/audits/AUDIT_PARSERS_2026-10-05.md` (2026-10-05 comprehensive audit suite, HEAD a2c24b16e)._

- **Severity**: LOW
- **Dimension**: Error Semantics
- **Location**: `crates/sfmaterial/src/index.rs:191-226` (`build`), `crates/sfmaterial/src/index.rs:355-358` (`capture_instance`), `crates/sfmaterial/src/index.rs:503-595` (`stream_db_file_index`)
- **Status**: NEW (introduced by `224a19372`)
- **Trigger Input**: a CDB (mod or Creation) with any of: no `BSComponentDB2::DBFileIndex` instance; a `Components` table whose length differs from the number of instances after the index; or a `DBFileIndex` chunk of kind `DIFF`/`USRD`, or one carrying trailing bytes.
- **Description**: contract (c) requires a degraded decode to be a documented `Ok` with a warning, or an error. `build` breaks it in four ways:
  1. **No `DBFileIndex`.** `base` stays `None` and every instance is skipped, so the result is `Ok` with an empty index. The consumer only logs "material index built (0 keyed objects)" at info level.
  2. **Row/instance mismatch.** The `Components[j]` ↔ instance `j + 2` alignment is the load-bearing join (measured 1,438,778 / 1,438,778 on the base CDB), but `build` never compares `rows.len()` with the number of instances after the index. `capture_instance` drops an unmatched instance silently, and a one-row shift would attribute every texture to the wrong object.
  3. **Unchecked payload.** `stream_db_file_index` has no `ObjectTrailingBytes` check, unlike `consume_object`. It also reads a `DIFF`/`USRD` payload's inline fields in non-diff offset order.
  4. **Contextless error.** A second `DBFileIndex` fails as `WrongChunkType { wanted: Objt, got: Objt }`, which names nothing useful.
- **Evidence**: see the locations. `cdbprobe` shows the base and SFBGS007 indexes agreeing on the one sampled path that resolves, so vanilla shows no symptom.
- **Impact**: a non-vanilla CDB yields wrong or empty Starfield materials with no warning. Vanilla is clean.
- **Related**: #3398, PAR-D4-2026-10-05-01
- **Suggested Fix**:
  - At the end of `build`, return an error, or a typed warning the consumer logs, when no index was seen or when `rows.len()` differs from the count of instances after the index.
  - Check trailing bytes in `stream_db_file_index`.
  - Return a dedicated `DuplicateDbFileIndex` error variant.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
