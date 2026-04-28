# #755: SF-DIM2-02: BA2 v3 unknown `compression_method` silently warn-falls-back to zlib

URL: https://github.com/matiaszanolli/ByroRedux/issues/755
Labels: bug, medium

---

**From**: `docs/audits/AUDIT_STARFIELD_2026-04-27.md` (Dim 2, SF-DIM2-02)
**Severity**: MEDIUM (defense-in-depth gap; no observed trigger today)
**Status**: NEW

## Description

`crates/bsa/src/ba2.rs:176-186` falls back to zlib on any unknown v3 `compression_method`. An unknown method (e.g. 1, 2, 4 if Bethesda adds a future codec like `lz4_frame` or `zstd`) emits one warn line and proceeds to deflate-decode garbage bytes.

```rust
compression = match method {
    0 => Ba2Compression::Zlib,
    3 => Ba2Compression::Lz4Block,
    other => {
        log::warn!(
            "BA2 v3: unknown compression method {}, assuming zlib",
            other
        );
        Ba2Compression::Zlib
    }
};
```

zlib's deflate header is just two bytes (`78 ??`); the decoder will most likely fail with `Error::Io(InvalidInput)` ~10 bytes in — but not before allocating the full `unpacked_size` Vec. Worse, on a hostile archive that crafts a payload starting with valid-looking zlib bytes, the decoder might emit nonsense for several KB before hitting a CRC mismatch.

## Impact

Today: zero (every observed v3 archive uses method 0 or 3). Future: if Starfield CK or a DLC adds a codec method, the warn drowns in the log and operators see "decompression failed" 100k times per archive instead of one "unsupported BA2 v3 codec" up front.

## Suggested Fix

Convert fallback to hard `Err`:

```rust
other => {
    return Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "BA2 v3: unsupported compression method {} (expected 0=zlib or 3=lz4_block)",
            other
        ),
    ));
}
```

Surfaces once at archive-open time instead of per-extract.

## Completeness Checks

- [ ] **TESTS**: Add a synthetic test with a forged v3 header that has `compression_method = 2`; assert `Ba2Archive::open` returns `InvalidData`.
- [ ] **SIBLING**: n/a — the fallback is unique to this site.
- [ ] **DROP / LOCK_ORDER / FFI**: n/a.

## Related

- Audit-finding-hygiene class as the FO4 docstring drift in #596.
