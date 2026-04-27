# NIF-D4-01: FO4+/FO76/Starfield CRC32 shader flag arrays parsed but never consumed by importer

URL: https://github.com/matiaszanolli/ByroRedux/issues/712
Labels: bug, nif-parser, import-pipeline, high

---

## Severity: HIGH

## Game Affected
FO4 (BSVER 130-131 partial), FO76 (BSVER 152-155), Starfield (BSVER 172+)

## Location
- `crates/nif/src/import/material/walker.rs:131-143` (BSLighting decal/two-sided check)
- `crates/nif/src/import/material/walker.rs:200-213` (BSEffect decal/two-sided check)
- `crates/nif/src/blocks/shader.rs:604-635` (CRC32 array parsing)

## Description
For BSVER > 130 the parser stores `shader_flags_1` and `shader_flags_2` as **literal zeros** (parser branch `if bsver <= 130 { read } else { (0, 0) }` at `shader.rs:604-608`) and populates `sf1_crcs` / `sf2_crcs` with CRC32-hashed flag identifiers instead.

The importer's `is_decal_from_modern_shader_flags(shader.shader_flags_1, shader.shader_flags_2)` check (`walker.rs:141, 211`) and the `shader_flags_2 & SF2_DOUBLE_SIDED` two-sided check (`walker.rs:135, 206`) read the **zeroed** fields, so:
- `info.is_decal` is **always false** on FO76/Starfield BSLighting and BSEffect meshes regardless of the authored Decal/Dynamic_Decal flag.
- `info.two_sided` is **always false** on the same content.

## Evidence
```
shader.rs:604:    let (shader_flags_1, shader_flags_2) = if bsver <= 130 {
shader.rs:605:        (stream.read_u32_le()?, stream.read_u32_le()?)
shader.rs:606:    } else {
shader.rs:607:        (0, 0)
shader.rs:608:    };
```
No call site in `crates/nif/src/import/` reads `sf1_crcs` or `sf2_crcs` (verified with `grep -rn "sf1_crcs\|sf2_crcs" import/` — only test-fixture `Vec::new()` references).

## Impact
- FO76/Starfield grass cards, foliage, banners, hair, and decal content (blood splats, posters, ground decals) render with backface culling on and depth-bias off.
- On Starfield this affects every content NIF.
- Fix unblocks a large class of cosmetic regressions on FO76+ once NIF-D5-01 (BSGeometry) lands.

## Suggested Fix
Add a CRC→flag-bit lookup table. The SLSF1/F4SF1/Starfield flag enums in `crates/nif/src/shader_flags.rs` are the reference set; CRC32 over the bit name strings yields the wire values per nif.xml. Then in `extract_material_info_from_refs` test the lookup result for `Decal`, `Dynamic_Decal`, `Double_Sided` on the BSVER >= 132 path.

Could be folded into the existing `is_decal_from_modern_shader_flags` helper as a variant that takes the parsed shader block instead of raw u32s. Bundle with NIF-D4-05 (additive blend ONE/ONE for `Own_Emit` flagged effect meshes — same CRC32 lookup table).

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D4-01)
- Bundle: NIF-D4-05 (additive blend deferred per source comment at walker.rs:256-262)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Both BSLighting (walker.rs:131-143) and BSEffect (walker.rs:200-213) branches updated in lockstep
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Byte-exact CRC32 lookup unit test for canonical flags (Decal, Dynamic_Decal, Double_Sided, Own_Emit, EnvMap_Light_Fade); end-to-end regression test that an FO76 decal NIF imports with `is_decal=true`
- [ ] **CORPUS**: Verify decal/two-sided rates on FO76 + Starfield import are non-zero
