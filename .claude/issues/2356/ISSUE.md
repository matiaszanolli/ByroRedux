# SF-BA2-01: DX10 per-chunk size caps aren't summed across a record — up to 255x allocation amplification per texture

**GitHub Issue**: https://github.com/matiaszanolli/ByroRedux/issues/2356
**Labels**: bug,import-pipeline,medium

---

**Severity**: MEDIUM
**Dimension**: 1 — BA2 v2/v3 LZ4 Block Decompression (Starfield audit, 2026-08-03)
**Location**: `crates/bsa/src/ba2.rs:601-626` (chunk read/cap), `:760-793` (`extract_dx10` loop), `crates/bsa/src/safety.rs:33-39,66-76`
**Status**: NEW, CONFIRMED against current code

## Description

`checked_chunk_size` caps each DX10 chunk's `packed_size`/`unpacked_size` individually at `MAX_CHUNK_BYTES` (1 GiB), but `num_chunks` is a `u8` (up to 255 chunks per record) and nothing caps the *sum* across a record's chunk list. A hostile/corrupted `.ba2` can declare 255 chunks each near the 1 GiB cap while the real backing `packed_size` bytes stay tiny.

## Evidence

- `ba2.rs`'s chunk-read loop calls `checked_chunk_size(packed_size, ...)` / `checked_chunk_size(unpacked_size, ...)` per chunk independently — no running total tracked across the loop.
- `extract_dx10`'s loop allocates per chunk with no aggregate cap: `packed_size == 0` branch does `vec![0u8; chunk.unpacked_size as usize]` (eager zero-fill); the compressed branch does `vec![0u8; chunk.packed_size as usize]` then calls `decompress_chunk` (itself allocating `unpacked_size` bytes).
- `safety.rs:33-39` documents `MAX_CHUNK_BYTES = 1024 * 1024 * 1024` as a per-field cap with no mention of a per-record aggregate.

## Impact

Resource-exhaustion / DoS vector on any path opening untrusted or mod-repacked `.ba2` files — up to 255 sequential ~1 GiB allocation attempts from a small on-disk file. Not memory corruption, but a crash-adjacent failure mode. Not Starfield-specific (shared DX10 code path, all BTDX versions: FO4/FO76/Starfield).

**Related**: Same theme as #2097 (LZ4-01) — untrusted declared size drives allocation — different mechanism (aggregate-across-chunks vs. single-chunk).

## Suggested Fix

Track a running total of `unpacked_size`/`packed_size` while reading a DX10 record's chunk list; reject if the sum exceeds a generous per-texture ceiling (e.g. 256 MiB), mirroring the existing `checked_entry_count` pattern in `safety.rs`.

## Completeness Checks
- [ ] **SIBLING**: Same DX10 chunk-read loop is shared across all BTDX versions (FO4 v1 → Starfield v3) — one fix covers all
- [ ] **TESTS**: A regression test pins this (a synthetic DX10 record with 255 near-1GiB chunks and a tiny real payload must be rejected, not attempted)
