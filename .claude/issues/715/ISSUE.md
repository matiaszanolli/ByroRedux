# NIF-D1-02: NiSourceTexture missing Use Internal byte for pre-10.0.1.4 internal-pixel-data path

URL: https://github.com/matiaszanolli/ByroRedux/issues/715
Labels: bug, nif-parser, medium

---

## Severity: MEDIUM

## Game Affected
Pre-Oblivion only (v ≤ 10.0.1.3). Affects Morrowind-era embedded textures and any non-Bethesda Gamebryo content with internal pixel data in the 4.x–10.0.1.3 range.

## Location
- `crates/nif/src/blocks/texture.rs:38-95` (`NiSourceTexture::parse`)

## Description
nif.xml line 5117: `<field name="Use Internal" type="byte" default="1" cond="Use External == 0" until="10.0.1.3" />`. When `use_external == 0` and the file version is ≤ 10.0.1.3 there is an extra single-byte `Use Internal` flag between `Use External` and the optional file-name / pixel-data ref. The current parser never reads it, so the embedded-pixel-data path on pre-10.0.1.4 NIFs is misaligned by one byte.

## Evidence
```rust
// texture.rs:41-64
let use_external = stream.read_u8()? != 0;
// … no `use_internal` read between here and the conditional ref/string …
let (filename, pixel_data_ref) = if use_external { … } else {
    if stream.version() >= crate::version::NifVersion(0x0A010000) {
        if use_string_table { let _unknown = stream.read_string()?; }
        else { let _unknown = stream.read_sized_string()?; }
    }
    let pix_ref = stream.read_block_ref()?;
    (None, pix_ref)
};
```

## Impact
Pre-Oblivion NIFs with embedded NiPixelData under-read by 1 byte. Not in current Bethesda target band, but inherits the format-abstraction policy that says "every version branch should match nif.xml even when the target band doesn't bite."

## Suggested Fix
Add gated read after `use_external`:
```rust
let use_internal = if !use_external && stream.version() <= NifVersion(0x0A000103) {
    stream.read_u8()? != 0
} else {
    true
};
```

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D1-02)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: N/A (single block parser)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Byte-exact regression with pre-10.0.1.4 embedded-pixel NiSourceTexture fixture
