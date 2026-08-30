# #3621: OBL-D1-01: NiTexturingProperty Apply Mode has no since=3.3.0.13 lower bound — a sub-3.3.0.13 file reads 4 phantom bytes

**Source**: `docs/audits/AUDIT_OBLIVION_2026-08-30.md` — Dimension 1 (NIF Version Handling)
**Severity**: LOW
**Location**: `crates/nif/src/blocks/properties.rs` — `NiTexturingProperty::parse`, the `apply_mode` read

## Description

`apply_mode` is read for every `version <= STRING_TABLE_THRESHOLD` (20.1.0.1) with **no lower
version bound**. nif.xml declares the field `since="3.3.0.13"`.

## Evidence

Current code (verified 2026-08-30):

```rust
let apply_mode = if stream.version() <= NifVersion::STRING_TABLE_THRESHOLD {
    stream.read_u32_le()?
} else {
    u32::from((flags >> 1) & 0x7)
};
```

nif.xml (line 5233):

```xml
<field name="Apply Mode" type="ApplyMode" default="APPLY_MODULATE"
       since="3.3.0.13" until="20.1.0.1">
```

The `until` half is transcribed correctly; the `since` half is absent.

**Measured exposure on Oblivion: zero.** The lowest version in the vanilla corpus is exactly
3.3.0.13 (1 file, out of 8,032), which nif.xml's inclusive `since` includes. The full
version histogram has no sub-3.3.0.13 entry.

## Impact

Latent. A file below 3.3.0.13 would read 4 phantom bytes and — with no block-size table at
that version — poison the whole downstream stream, i.e. a total-misalignment failure rather
than a graceful one. Becomes real for mod content or other NetImmerse titles.

## Suggested Fix

Add the lower bound: read `apply_mode` only when
`version >= V3_3_0_13 && version <= STRING_TABLE_THRESHOLD`, defaulting to `APPLY_MODULATE`
below it, per nif.xml's stated default.

## Related

OBL-D1-02 (the sibling presence-bool gating divergence in the same parser), #3530
(`apply_mode` is the field it consumes).

## Completeness Checks
- [ ] **SIBLING**: sweep `NiTexturingProperty::parse` for other `until`-only transcriptions missing their `since`
- [ ] **TESTS**: a negative test that a sub-3.3.0.13 stream does not consume the 4 bytes (the #170 `bs_stream_header_not_read_for_off_spec_version` test is the pattern)
