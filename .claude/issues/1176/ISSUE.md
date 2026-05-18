# FO4-D2-NEW-01: DX10 mip chunks concatenated without start_mip monotonicity check

**Labels**: bug, import-pipeline, low

**Source**: [`docs/audits/AUDIT_FO4_2026-05-18.md`](docs/audits/AUDIT_FO4_2026-05-18.md)
**Dimension**: BA2 Reader (GNRL + DX10)
**Severity**: LOW

## Observation

`crates/bsa/src/ba2.rs:643-657` (`extract_dx10`):

```rust
let mut pixel_data = Vec::new();
for chunk in chunks {
    reader.seek(SeekFrom::Start(chunk.offset))?;
    if chunk.packed_size == 0 {
        // ... read raw
    } else {
        // ... decompress
    }
}
```

`extract_dx10` iterates `chunks` in file-order and concatenates their decoded bytes into `pixel_data`. The per-chunk `start_mip` / `end_mip` u16s (parsed at lines 533-545) are never consulted by the extract path. There is no assertion or sort step ensuring chunks are authored in ascending mip order (mip 0 = largest first, descending) — which the DDS spec requires for the synthesized header to match the payload.

## Why bug

Vanilla FO4 / FO76 / Starfield archives always author chunks in canonical mip-0-first order — the brute-force test `fo4_meshes_ba2_v8_brute_force_extract_zero_errors` and the cubemap test pass. Failure mode is silent: a hand-crafted or third-party-repacked archive that wrote chunks in non-canonical order (e.g. streaming-mip-tail-first) would produce a DDS whose header declares dimensions matching mip 0 but whose pixel payload starts at a smaller mip. Downstream loaders would read garbage with no diagnostic.

The synthesized header isn't currently rewritten with an effective mip count derived from chunk metadata, so the spec assumption is implicit and unchecked.

## Fix

Add a `debug_assert!` (release-tolerant) in `read_dx10_records` after the chunk loop that verifies `chunks[i].start_mip <= chunks[i+1].start_mip` for all `i`; surface a `log::warn!` on mismatch in release builds. Same pattern as the `num_mips == 0` warning at `ba2.rs:512-519` and the `chunk_hdr_len != 24` debug_assert at `:490-495`. Don't auto-sort — that would mask the archive being malformed.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: confirm the same monotonicity assertion isn't needed elsewhere (GNRL has no mip concept; only DX10)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: synthetic archive with out-of-order mip chunks asserts the warn is emitted in release and the debug_assert fires in debug
