# #3720 — ESM-2026-08-30-D8-01: the "cause not yet identified" FNV LAND 0x00150FC0 failure is an Adler-32 mismatch on an otherwise-perfect zlib stream

**Severity**: MEDIUM · **Location**: `crates/plugin/src/esm/reader.rs::read_sub_records`; soft-fail site `crates/plugin/src/esm/cell/walkers.rs`
**Source**: `docs/audits/AUDIT_ESM_2026-08-30.md` (ESM-2026-08-30-D8-01)

Root cause for the long-standing `walkers.rs` comment — *"At least one vanilla
FNV LAND record (form 0x00150FC0) reliably fails the body read on every ESM
open — cause not yet identified"* (#385 / D5-F5). `read_sub_records`'s
`ZlibDecoder::new(compressed).read_to_end(...)` validates the trailing
Adler-32 checksum and errors on mismatch, even when the DEFLATE data itself
inflates cleanly to the exact declared size.

## Verification

Independently confirmed against the real mounted `FalloutNV.esm` (throwaway
`crates/plugin/examples/_tmp_land_adler32_verify.rs`, walking the GRUP tree
via `EsmReader`'s public API to locate form `0x00150FC0`, deleted after use):
before the fix, `read_sub_records` on this record failed; after the fix it
succeeds with **exactly** `DATA(4) + VNML(3267) + VHGT(1096)` bytes — an
identical match to the issue's own evidence, at file offset within
`TheStripWorldNew` grid (-6, 26), parent CELL `0x0014F622`.

## Fix implemented

`read_sub_records`'s compressed-record path now retries on `ZlibDecoder`
failure: skip the 2-byte zlib header and re-decode as raw DEFLATE
(`flate2::read::DeflateDecoder`, no trailer to validate), accepting the
recovery **only** when its output length exactly matches the already-validated
declared size (the `#3399` ceiling). A length mismatch means the stream itself
is corrupt, not just its checksum — the original zlib error surfaces instead
of silently swallowing a real corruption. On successful recovery, logs once at
`warn` naming the FormID, per the issue's own suggested fix.

Every `#3399` bound stays intact — the `record_inflation_ceiling` check and
the `decompressed.len() <= decompressed_size` guard both still run on the raw
path exactly as before; the raw retry only replaces *how* the bytes are
produced, not any of the size validation around them.

**TESTS** (issue's own checklist item):
`compressed_record_with_corrupt_adler32_trailer_recovers_via_raw_deflate`
builds a well-formed compressed LAND record and flips the last 4 bytes (the
Adler-32 trailer) — recovery must produce the exact original sub-records.
`compressed_record_with_corrupt_deflate_body_still_errors` corrupts a byte
*inside* the DEFLATE stream instead — must still hard-fail, proving the retry
doesn't mask genuine corruption.

**SIBLING** (issue's own checklist item): checked `byroredux_bsa::safety::
inflate_bounded`, whose contract this mirrors (#3410) — confirmed it has the
identical exposure (propagates a checksum-only `Err` with no retry), but
fixing it isn't a drop-in port: it's generic over an already-constructed
`io::Read` (not raw bytes), and some of its call sites are LZ4-wrapped, not
zlib, so a uniform retry can't apply. Filed as a separate, correctly-scoped
follow-up: #3812.

Deleted the stale "cause not yet identified" explanation in `walkers.rs`
(kept the surrounding soft-fail `match` as a defensive floor for any *other*
future LAND failure, per its own #385/D5-F5 guidance — only the now-false
claim about this specific record was removed).

Full workspace: `cargo test --no-fail-fast` 7051 passing, 0 failing (+2 for
the new regression tests).
