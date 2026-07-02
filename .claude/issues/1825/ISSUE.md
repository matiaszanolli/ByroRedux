# 1825: FO4-D3-01: BA2 DX10 chunk_hdr_len read but never used to advance the reader

URL: https://github.com/matiaszanolli/ByroRedux/issues/1825
Labels: bug, import-pipeline, low, legacy-compat

**Severity**: LOW
**Dimension**: 3 — BA2 Reader
**Location**: `crates/bsa/src/ba2.rs:538-551, 580-582`
**Status**: NEW (defense-in-depth gap; documented trade-off, not a live bug)

## Description

`read_dx10_records` reads `chunk_hdr_len` from `base[14..16]` and
`debug_assert_eq!`s it to 24, but the chunk loop unconditionally reads a fixed
`[0u8; 24]` per chunk. In release builds the assert compiles out, so a
(hypothetical, third-party) archive declaring `chunk_hdr_len != 24` would
misparse every following chunk with no telemetry. The inline comment already
acknowledges the tolerant-in-release trade-off. Zero impact on vanilla FO4 (all
DX10 records set 24, confirmed via successful live extraction of `Fallout4 -
Textures1.ba2` v7 and `DLCCoast - Textures.ba2` v1).

## Evidence

- `chunk_hdr_len` (`ba2.rs:539`) feeds only the `debug_assert` (`:546`).
- The chunk read uses the literal `[0u8; 24]` (`:581`), independent of the parsed `chunk_hdr_len` value.

## Impact

None on vanilla content; silent misparse only on a malformed / future DX10
archive with a wider chunk header.

## Suggested Fix

For parity with the `num_mips==0` (`ba2.rs:568`) and non-monotonic (`:628`)
siblings, add a release-path `log::warn!` when `chunk_hdr_len != 24` (keep the
tolerant clamp-to-24 behavior).

## Completeness Checks
- [ ] **TESTS**: A regression test constructs a synthetic DX10 record with `chunk_hdr_len != 24` and asserts the release-path warning fires (not just the debug_assert).

