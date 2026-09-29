# PAR-D2-2026-09-29-01: BA2 DX10 open panics in debug builds on two file-controlled fields

**Labels**: medium,bug,import-pipeline,safety

**Source report**: `docs/audits/AUDIT_PARSERS_2026-09-29.md`

- **Severity**: MEDIUM
- **Dimension**: Error Semantics
- **Location**: `crates/bsa/src/ba2.rs:653-658` (`debug_assert_eq!(chunk_hdr_len, 24, …)`), `crates/bsa/src/ba2.rs:771-777` (`debug_assert!(monotonic, …)` over chunk `start_mip`)
- **Status**: NEW
- **Trigger Input**: a DX10 BA2 record whose `chunk_hdr_len` (bytes 14..16 of the record) is not 24, or whose chunks' `start_mip` is not non-decreasing.
- **Description**:
  - Both asserts sit in `read_dx10_records`, which runs from `Ba2Archive::open`.
  - Release builds compile them out and warn instead (`ba2.rs:672-681`, `:778-787`).
  - Debug builds panic. `Archive::open` runs at boot from `open_with_numeric_siblings` on the main thread, with no `catch_unwind`.
  - The asserts are deliberate: #1079 and #1176 chose "`debug_assert` catches it in dev". That still violates the reader contract "malformed bytes give `Err`, never a panic".
  - #4656 fixed the sibling debug-only panic (a `u32` overflow) in this same file at MEDIUM.
- **Evidence** (probe `ba2-dx10-asserts`, a one-record DX10 v1 BA2):
  ```
  debug:   control: open Ok (1 files)
           chunk_hdr_len=32: PANIC: assertion `left == right` failed: BA2 DX10 record has chunk_hdr_len=32 (expected 24) …
           start_mip 1,0: PANIC: BA2 DX10 chunks non-monotonic on start_mip: [1, 0] — synthesized DDS header would misdescribe payload
  release: control / chunk_hdr_len=32 / start_mip 1,0: open Ok (1 files) each
  ```
- **Impact**: a debug-build engine (`cargo run`, the documented developer path) aborts at startup on a third-party repacked or crafted mod BA2. Release builds are unaffected.
- **Related**: #4656, #4155, #1079, #1176, #1825
- **Suggested Fix**:
  - Delete both `debug_assert`s; the warns already cover them.
  - Or make `chunk_hdr_len != 24` an `InvalidData` naming the record, because the reader cannot parse any other length correctly anyway.
  - Add the two probe cases as unit tests (`read_dx10_records` needs a `ReadAt`/`Read` seam, or build a temp file).

**Validated at HEAD 9fcfdc3fc**: `read_dx10_records` in `crates/bsa/src/ba2.rs` still carries `debug_assert_eq!(chunk_hdr_len, 24, …)` and `debug_assert!(monotonic, …)` on file-controlled fields ahead of the release-mode warns.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
