# SF-D1-01: LZ4 arm silently truncates on under-run; comment claims it hard-errors

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2618
**Finding ID**: SF-D1-01

**Severity**: MEDIUM
**Dimension**: 1 (BA2 v2/v3 LZ4 Block Decompression)
**Location**: `crates/bsa/src/ba2.rs:738-746` (LZ4 arm), `:712-735` (the misleading comment)
**Status**: NEW — partial overlap with #2097 (LZ4-01, OPEN, LOW), opposite failure direction, different fix

## Description
`lz4_flex::block::decompress(packed, unpacked_size)` allocates the declared
size, decodes, then `truncate`s to the actual decoded length and returns
`Ok` — so a record that declares *more* than the stream contains gets a
silent short buffer, no error, no log. The zlib arm handles the identical
condition with `log::warn!` (#812); the comment claiming the LZ4 branch
"hard-errors on the same condition" is factually wrong — `lz4_flex` only
hard-errors in the *other* direction (declared < actual).

## Evidence
Measured against the pinned `lz4_flex 0.11.6`: under-run → `Ok(len=13)` for
a declared 4096, no error; over-run → hard `Err`. Vanilla corpus is clean
(0/2,822 sampled chunks), so this is a robustness gap on malformed/
mod-repacked archives, not an active bug.

## Impact
LZ4 is the only codec for all 15 Starfield v3 texture archives; a DX10
texture is a concatenation of per-mip chunks, so a short decode on a
non-final chunk shifts every subsequent mip, and the synthesized DDS header
then misdescribes its own payload — garbled/offset mip data in the renderer
with no error signal.

## Suggested Fix
Compare `out.len()` against `unpacked_size` post-decode in the LZ4 arm and
`log::warn!` (or hard-error for chunk chains, where a short mid-chain chunk
is unrecoverable). Fix the comment. Add an under-run unit test.

## Related
#2097 (LZ4-01), #812, #2360.

## Completeness Checks
- [ ] **TESTS**: An under-run fixture (`unpacked_size` declared larger than the actual decoded stream) asserts a warn/error, not silent truncation
