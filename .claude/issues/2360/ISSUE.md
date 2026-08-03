# SF-BA2-02: v3 header-boundary diagnostic log reads the stream position 4 bytes early

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2360
**Labels**: bug,import-pipeline,low

---

**Severity**: LOW
**Dimension**: 1 — BA2 v2/v3 LZ4 Block Decompression (Starfield audit, 2026-08-03)
**Location**: `crates/bsa/src/ba2.rs:233-236`, `:447-472` (`log_v2_v3_extra_bytes`)
**Status**: NEW, CONFIRMED against current code

## Description

For v3, the header-boundary sanity log captures `stream_position()` before the 4-byte `compression_method` field is read (32 bytes in, not the true 36-byte post-header offset). The v2 branch captures it correctly (nothing left to read at that point).

## Evidence

Confirmed by reading `ba2.rs:233-236` directly: the `BA2_V_STARFIELD_V3` arm calls `log_v2_v3_extra_bytes("v3", &extra, name_table_offset, reader.stream_position()?)` **before** `method_buf` (the 4-byte compression method) is read a few lines later.

## Impact

Log-only — a `log::trace!`/`log::debug!` diagnostic, never affects control flow or parsing correctness.

## Suggested Fix

Move the log call to after `method_buf` is read, or pass `stream_pos + 4` with a comment explaining the offset.

## Completeness Checks
- [ ] **TESTS**: A regression test pins the corrected stream-position value in the v3 diagnostic log (or asserts logic equivalence if untestable via log capture)
