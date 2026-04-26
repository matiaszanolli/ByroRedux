# SK-D2-03: BSA per-file embed-name flag bit 0x80000000 not consulted vs archive flag

## Finding: SK-D2-03

- **Severity**: MEDIUM
- **Source**: `docs/audits/AUDIT_SKYRIM_2026-04-24.md`
- **Game Affected**: Modded BSAs (mixed-mode embed-name files); vanilla unaffected
- **Location**: [crates/bsa/src/archive.rs:188, 315-318, 432-440](crates/bsa/src/archive.rs#L188)

## Description

The archive-level `embed_file_names` flag (bit 8 of archive flags) is XOR'd against per-file `compression_toggle` for compression decisions. There is no analogous XOR for per-file embed-name override.

The per-file `size` field's high bits encode flags: bit 30 (`0x40000000`) is the compression toggle (handled at archive.rs:317 → `compression_toggle = size_raw & 0x40000000 != 0`), but bit 31 (`0x80000000`) is masked off as part of `size & 0x3FFFFFFF` and never re-tested.

Mixed-mode BSAs (mods that flip the flag per file rather than for the whole archive) extract with the wrong path-prefix consumption. Vanilla Skyrim BSAs use a uniform embed-name policy per archive, so this is latent on shipped content.

## Suggested Fix

Mirror the compression-toggle pattern:

```rust
// crates/bsa/src/archive.rs around line 315-318
let size_raw = u32::from_le_bytes(frec[8..12].try_into().unwrap());
let compression_toggle = size_raw & 0x40000000 != 0;
let embed_name_toggle = size_raw & 0x80000000 != 0;
let size = size_raw & 0x3FFFFFFF;

// at extract time
let file_compressed = compressed_by_default != compression_toggle;
let file_embeds_name = embed_file_names != embed_name_toggle;
```

Then gate the path-prefix skip in extract on `file_embeds_name` instead of the archive-level `embed_file_names`.

## Related

- #449 (closed): genhash_file high-word algorithm — adjacent BSA hardening.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Verify the same flag-pair shape doesn't exist elsewhere in the v104/v105 reader.
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Add a synthetic BSA with mixed embed-name files; assert each extracts correctly.

_Filed from audit `docs/audits/AUDIT_SKYRIM_2026-04-24.md`._
