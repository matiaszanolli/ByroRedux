# FO4-D2-NEW-01: Unknown BA2 versions silently fall through to v1 layout instead of being rejected

Labels: bug import-pipeline medium legacy-compat 
State: OPEN

**From**: `docs/audits/AUDIT_FO4_2026-05-04_DIM2.md` (FO4 Dim 2 / BA2 reader)
**Severity**: MEDIUM
**Location**: [crates/bsa/src/ba2.rs:173-197](crates/bsa/src/ba2.rs#L173-L197)
**Status**: NEW + CONFIRMED against current code (2026-05-04)

## Description

Version dispatch is structured as two cascading `if`s gated on
`version == 2 || version == 3` and `version == 3`:

```rust
let mut compression = Ba2Compression::Zlib;
if version == 2 || version == 3 {
    let mut extra = [0u8; 8];
    reader.read_exact(&mut extra)?;
}
if version == 3 {
    let mut method_buf = [0u8; 4];
    reader.read_exact(&mut method_buf)?;
    let method = u32::from_le_bytes(method_buf);
    compression = match method {
        0 => Ba2Compression::Zlib,
        3 => Ba2Compression::Lz4Block,
        other => { return Err(...); }
    };
}
```

Any version not in `{2, 3}` — including 0, 4, 5, 6, 9, 10, ..., u32::MAX — falls
through to `read_general_records` / `read_dx10_records` as if it were the
24-byte v1 layout, with no validation that the version is in the supported set
`{1, 2, 3, 7, 8}`.

The module-level docstring (lines 20-26) explicitly commits to the
`{1, 2, 3, 7, 8}` allowlist, so the implementation is out of sync with its
documented contract.

## Why it's wrong

The BA2 lineage shows Bethesda is willing to break header layout between major
releases (v2 added 8 bytes; v3 added another 4). Assuming future versions
revert to v1's 24-byte header is empirically falsifiable within this very
codebase. openMW's reference reader (`ba2gnrlfile.cpp:91-103` /
`ba2dx10file.cpp` equivalent) explicitly enumerates supported versions and
rejects everything else with `fail("Unrecognized")`.

## Repro

Hand-craft a BA2 with `version = 5` and `file_count = 0`. `Ba2Archive::open()`
succeeds with `version() == 5` even though 5 is not in the supported set.
With non-zero file count and a hypothetical future header layout that adds
fields, the reader will eat offset bytes from the v1 layout and either fail
confusingly at extract time or return corrupted bytes.

## Fix sketch

Replace the two cascading `if`s with a `match version` enumerating
{1, 2, 3, 7, 8} and emitting `InvalidData` on the default arm:

```rust
let compression = match version {
    1 | 7 | 8 => Ba2Compression::Zlib,
    2 => {
        let mut extra = [0u8; 8];
        reader.read_exact(&mut extra)?;
        Ba2Compression::Zlib
    }
    3 => {
        let mut extra = [0u8; 8];
        reader.read_exact(&mut extra)?;
        let mut m = [0u8; 4];
        reader.read_exact(&mut m)?;
        match u32::from_le_bytes(m) {
            0 => Ba2Compression::Zlib,
            3 => Ba2Compression::Lz4Block,
            other => return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("BA2 v3: unsupported compression method {other}"),
            )),
        }
    }
    other => return Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("unsupported BA2 version: {other} (expected 1, 2, 3, 7, or 8)"),
    )),
};
```

Add a unit test (`unknown_version_rejected`) mirroring the existing
`v3_unknown_compression_method_rejected` and
`malicious_file_count_u32_max_rejected_before_allocation` regressions.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: BSA reader (`crates/bsa/src/archive.rs`) — same allowlist discipline (v103/v104/v105). Confirm it already rejects unknown versions or apply the same pattern.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic `unknown_version_rejected` test (mirrors existing `v3_unknown_compression_method_rejected`).