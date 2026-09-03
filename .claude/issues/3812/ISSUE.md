# #3812 — BSA-2026-09-02: inflate_bounded has the same Adler-32-trailer-only failure mode #3720 fixed in ESM

**Severity**: LOW · **Location**: `crates/bsa/src/safety.rs::inflate_bounded`
**Source**: SIBLING check from #3720 (ESM-2026-08-30-D8-01)

## Fix

Added `inflate_bounded_zlib(compressed: &[u8], declared: usize, label: &str)` as
`inflate_bounded`'s zlib-specific sibling. Unlike the generic `inflate_bounded`
(which stays untouched — it's generic over `R: io::Read` and still serves the
LZ4 call site directly), this one takes the *raw* compressed bytes rather than
a pre-built decoder, because the retry needs to construct a second decoder
over the same bytes if the first fails.

On a zlib read error, retries as raw DEFLATE (skip the 2-byte zlib header, no
trailer to validate) and accepts the recovery ONLY when its length exactly
matches `declared` — mirroring `EsmReader::read_sub_records`'s #3720 fix
byte-for-byte in structure (retry scoped strictly to the initial
`read_to_end` error; the decompression-bomb bounds check applies uniformly
after, to both the primary and retry paths).

Updated the three genuinely zlib-wrapped call sites (per the issue's own
SIGNATURE checklist item — audited every `inflate_bounded` caller):

- `crates/bsa/src/archive/extract.rs` — BSA v103/v104 zlib arm. The sibling
  LZ4-frame arm (v105+) is untouched — no zlib trailer to mis-validate.
- `crates/bsa/src/ba2.rs::decompress_chunk` — BA2 `Ba2Compression::Zlib` arm.
  The sibling LZ4 arm is untouched.
- `crates/bsa/src/csg.rs::chunk_bytes` — CSG chunks are exclusively zlib (no
  LZ4 variant), so its only `inflate_bounded` call converts directly.

Removed the now-unused `flate2::read::ZlibDecoder` import from all three call
sites (the decoder construction moved inside `inflate_bounded_zlib` itself).

## REAL DATA (issue's own checklist item)

No confirmed corrupt-trailer BSA/BA2/CSG sample has been found (unlike #3720,
which pinned a real `FalloutNV.esm` LAND record) — this is defensive
hardening pending a real repro, exactly as the issue itself anticipated
("treat this as defensive hardening pending a real repro" was listed as an
acceptable outcome). The fix is a direct, low-risk structural port of an
already-proven pattern, not a speculative new mechanism.

## SIGNATURE (issue's own checklist item)

Audited every `inflate_bounded` caller (`grep -rn 'inflate_bounded'`):
BSA v103/v104 zlib (converted), BSA v105+ LZ4 frame (left on the generic
function — LZ4 has no zlib trailer), BA2 zlib (converted), BA2 LZ4 (left on
the generic function), CSG zlib-only (converted). No caller was missed or
misclassified.

## TESTS (issue's own checklist item)

Added `inflate_bounded_zlib_tests` to `crates/bsa/src/safety.rs`, mirroring
#3720's own test pair exactly:

- `corrupt_adler32_trailer_recovers_via_raw_deflate` — flips the trailing
  4 bytes of a well-formed zlib stream (the Adler-32 trailer) and confirms
  the exact original bytes still come back via the raw-DEFLATE retry.
- `corrupt_deflate_body_still_errors` — corrupts the byte immediately after
  the 2-byte zlib header (the DEFLATE block-type/length bits themselves, the
  same `compressed_start + 2` offset #3720's own body-corruption test uses)
  and confirms the retry does NOT silently recover a genuinely corrupt
  stream. (An earlier draft of this test corrupted a byte deep in a long
  repeating-pattern buffer instead — caught in review: a flip mid-run can
  land on a literal token and corrupt the *decoded content* while leaving
  the *decoded length* unchanged, which the length-only accept check would
  have silently let through, proving nothing. Matching #3720's own
  early-offset choice avoids that false pass.)
- `well_formed_stream_needs_no_retry` — pins that the ordinary fast path
  still round-trips with no spurious retry.

## Verification

- `cargo check -p byroredux-bsa --tests`: clean.
- `cargo test -q -p byroredux-bsa`: all passing (+3 new tests).
- `cargo test -q --no-fail-fast` (full workspace): **7077 passing, 0
  failing** (+3 new tests).
