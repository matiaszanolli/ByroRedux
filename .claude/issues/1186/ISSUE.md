# SF-D2-NEW-03: BA2 v2/v3 trailing "2×u32 unknown" bytes read and dropped without sanity log

**Labels**: bug, import-pipeline, low

**Source**: [`docs/audits/AUDIT_STARFIELD_2026-05-18.md`](docs/audits/AUDIT_STARFIELD_2026-05-18.md)
**Dimension**: BA2 v2/v3 LZ4 Block Decompression
**Severity**: LOW (defense-in-depth)

## Observation

`crates/bsa/src/ba2.rs:220-227`:

```rust
BA2_V_STARFIELD_V2 => {
    let mut extra = [0u8; 8];
    reader.read_exact(&mut extra)?;
    Ba2Compression::Zlib
}
BA2_V_STARFIELD_V3 => {
    let mut extra = [0u8; 8];
    reader.read_exact(&mut extra)?;
    let mut method_buf = [0u8; 4];
    reader.read_exact(&mut method_buf)?;
    // ...
}
```

The eight trailing bytes (community-reverse-engineered as "compressed name-table size" u32 + a second reserved u32) are read and immediately discarded. v3 does the same before the `compression_method` u32.

## Why bug

When a malformed v2 archive starts the file-record table mid-frame (e.g., the first u32 disagrees with where the parser thinks `name_table_offset` should be), the failure surfaces 50+ records deep inside `read_general_records` / `read_dx10_records` with a confusing `failed to fill whole buffer` instead of "trailer u32 disagreed with name_table_offset by N bytes."

Same philosophy as the `padding != 0xBAADF00D` debug-log at `ba2.rs:442` — cheap sanity check at the header boundary saves 50 records of stack walk during diagnosis.

## Fix

One-line `log::trace!("BA2 v{} extra: {:02x?}", version, extra)` per arm + warn if `u32::from_le_bytes(extra[0..4])` plus the current stream position exceeds `name_table_offset`. Two minutes of code, zero behavioral change for well-formed archives.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: the `padding != 0xBAADF00D` debug-log at `ba2.rs:442` is the model — make sure both arms (v2 and v3) get the symmetric treatment
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: synthetic v2 archive with a deliberately-wrong first trailer u32 asserts the warn surfaces; well-formed archive asserts the trace-level log is silent (or use a `log::Level` test harness)

## Related

- #708 / Session 7 — BA2 v3 LZ4 work
- `ba2.rs:442` — the `0xBAADF00D` padding sanity check is the model
