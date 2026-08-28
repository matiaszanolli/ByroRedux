# SKY-2026-08-27b-D5-01: BSA extraction validates the declared uncompressed size and then never enforces it — `read_to_end` inflates without a bound

- **Severity**: HIGH
- **Dimension**: 5 (BSA v105 / LZ4)
- **Location**: `crates/bsa/src/archive/extract.rs:131-141` (both codec arms; `:134` LZ4 frame — Skyrim's v105 path — and `:139` zlib)
- **Confidence**: CONFIRMED (code read; the bound is demonstrably absent)

## Description

The compressed branch reads the 4-byte declared uncompressed size, bounds it correctly through `checked_chunk_size` (`MAX_CHUNK_BYTES` = 1 GB) — and then spends that validated value only as a capacity hint:

```rust
// crates/bsa/src/archive/extract.rs:131-141
let (decompressed, codec) = if self.version >= BSA_V_SKYRIM_SE {
    let mut decoder = lz4_flex::frame::FrameDecoder::new(&compressed[..]);
    let mut buf = Vec::with_capacity(original_size);
    decoder.read_to_end(&mut buf)?;
    (buf, "LZ4 frame")
} else {
    let mut decoder = ZlibDecoder::new(&compressed[..]);
    let mut buf = Vec::with_capacity(original_size);
    decoder.read_to_end(&mut buf)?;
    (buf, "zlib")
};
```

`read_to_end` has no output limit; it grows `buf` until the decoder reaches end-of-stream. The function *does* notice the mismatch — but only afterwards, and only as a `warn!`:

```rust
// crates/bsa/src/archive/extract.rs:154-165
if decompressed.len() != original_size {
    log::warn!("BSA {} decompression for '{}' produced {} bytes but original_size declared {} …");
}
```

So the archive's own declared ceiling is checked, logged against — and never used to stop the allocation it was checked for.

## Impact

A crafted or corrupt `.bsa` — the ordinary distribution format for Skyrim mods, i.e. the engine's real untrusted-input surface — terminates the process on allocation failure. `entry.size` is masked to 30 bits, so the compressed payload is bounded at 1 GB; LZ4's block ratio tops out near 255:1 and DEFLATE's near 1000:1, so the reachable inflation is hundreds of GB from an archive that looks unremarkable on disk. Unrecoverable: an OOM abort is not an `Err` any caller can handle, and the per-NIF `catch_unwind` in `streaming::pre_parse_cell` cannot intercept it.

## Suggested Fix

Replace both `read_to_end` calls with `Read::take(original_size as u64).read_to_end(&mut buf)` and turn the existing post-hoc length comparison into a hard `Err` when the limit was reached, so an over-ratio payload is diagnosably rejected instead of silently truncated or fatally inflated. Add the two negative tests the BA2 side already has (lying size prefix; over-ratio payload).

## Related

The ESM-side sibling filed in `docs/audits/AUDIT_ESM_2026-08-27.md` (`crates/plugin/src/esm/reader.rs:630-647`, same `Vec::with_capacity` + unbounded `read_to_end` shape, rated HIGH) — that report states verbatim that *"`crates/bsa`'s decompressors are a separate surface"*, so this is the uncovered half, not a duplicate. #2356 (BA2 DX10 per-chunk cap) and #3392 / #3394 (the BA2 LZ4 *block* safe-decoder work) both hardened the **BA2** reader and left the BSA reader as-is. #2585 (LOW) covers only the warn-vs-metric character of the post-hoc size check, not the missing bound.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27b.md` (`/audit-skyrim`).*
