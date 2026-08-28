# #3399 — ESM-2026-08-27-D1-02: the compressed-record path trusts the 4-byte uncompressed-size prefix for allocation and puts no ceiling on the inflated output

**Labels**: high, esm-plugin, bug
**Source**: `docs/audits/AUDIT_ESM_2026-08-27.md`

---

**Audit**: `docs/audits/AUDIT_ESM_2026-08-27.md` (`/audit-esm`, deep, tree `main` @ `969d81c8`)
**Severity**: HIGH · **Dimension**: Header & GRUP Walk (record decompression)
**Record / Sub-record**: any record with `FLAG_COMPRESSED` (`0x00040000`)
**Location**: `crates/plugin/src/esm/reader.rs` — `EsmReader::read_sub_records`

## Description

`EsmReader::read_sub_records` correctly checks `data_size >= 4` before subtracting the prefix (that half of the checklist is satisfied, and `compressed_record_too_small_returns_error` pins it). It then takes the record's own 4-byte prefix — a value entirely under the file's control, range `0..=0xFFFF_FFFF` — and passes it straight to `Vec::with_capacity`, and inflates with `read_to_end`, which has no output limit at all. Neither the reservation nor the inflation is bounded by anything derived from the actual compressed length.

## Evidence

`crates/plugin/src/esm/reader.rs` (`read_sub_records`):

```rust
let raw_data = if header.flags & FLAG_COMPRESSED != 0 {
    // First 4 bytes = uncompressed size, rest is zlib.
    ensure!(header.data_size >= 4, "Compressed record too small");
    let decompressed_size = self.read_u32() as usize;
    let compressed_len = header.data_size as usize - 4;
    ensure!(self.remaining() >= compressed_len, "Truncated compressed data");
    let compressed = &self.data[self.pos..self.pos + compressed_len];
    self.pos += compressed_len;

    let mut decoder = ZlibDecoder::new(compressed);
    let mut decompressed = Vec::with_capacity(decompressed_size);
    decoder
        .read_to_end(&mut decompressed)
        .context("Failed to decompress ESM record")?;
    decompressed
}
```

The only test covering the prefix, `compressed_record_prefix_matches_payload_length`, builds a *well-formed* record and asserts the round-trip — its own doc comment frames the prefix as a "capacity hint … a mismatch here would panic or silently truncate on strict allocators", i.e. it tests that a correct prefix works, never that a lying one is rejected. There is no test for a hostile prefix and no upper bound anywhere in the function.

## Impact

Two distinct untrusted-input failures on the same three lines.

(a) A 24-byte record header plus a 4-byte prefix of `0xFFFFFFFF` requests a 4 GiB reservation before a single byte is inflated.

(b) More seriously, `read_to_end` is unbounded independently of the prefix: zlib's practical ratio exceeds 1000:1, so a 100 MB compressed record inside an otherwise ordinary-looking plugin inflates toward ~100 GB and terminates the process on allocation failure.

Same trigger model and same unrecoverable outcome as `#3237` (crafted plugin → process death in the ESM parser), which is why this carries the same severity.

## Related

`#3237` (same untrusted-plugin threat model, HIGH); `#990` (CLOSED — added the zlib unit tests, but only happy-path ones); `crates/bsa`'s decompressors are a separate surface owned by `/audit-nif`.

## Suggested Fix

Bound both halves against the compressed length. Clamp the capacity hint to `min(decompressed_size, some_multiple_of(compressed_len))` so a lying prefix costs nothing, and replace `read_to_end` with `Read::take(limit).read_to_end(..)` where `limit` is that same ceiling, returning `Err` when the limit is hit so the record is diagnosably rejected rather than silently truncated. Add the two missing negative tests (lying prefix; over-ratio payload).

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other decompressors — `crates/bsa` LZ4/zlib paths carry the same shape)
- [ ] **TESTS**: A regression test pins this specific fix
