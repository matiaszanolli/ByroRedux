# #990 — SK-D6-NEW-02: Per-record zlib decompression in EsmReader has no unit-test coverage

**Source**: `docs/audits/AUDIT_SKYRIM_2026-05-12.md` § Dim 6
**Severity**: LOW
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/990

## Location

`crates/plugin/src/esm/reader.rs:451-477` — `FLAG_COMPRESSED` branch of `read_sub_records`.

## Summary

The `FLAG_COMPRESSED = 0x00040000` zlib-decompression branch is exercised on vanilla Skyrim / FO4 masters at runtime, but no synthetic regression test rounds-trips a compressed record through `read_sub_records`. `grep -rn 'FLAG_COMPRESSED' crates/plugin/src/esm/` returns 3 hits (all in `reader.rs`) and 0 in any test file.

## Fix

Add a synthetic test that:
1. Builds a sub-record payload.
2. zlib-encodes via `flate2::write::ZlibEncoder`.
3. Prepends a 4-byte decompressed-size header.
4. Wraps in a record with `FLAG_COMPRESSED` set and asserts `read_sub_records` round-trips byte-for-byte.

~40 LOC.
