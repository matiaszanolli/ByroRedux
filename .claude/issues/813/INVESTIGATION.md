# Investigation — #813 + #814

**Domain**: esm (FO4/SE TXST parser)
**Bundle**: #813 (DODT) + #814 (DNAM) per "land together" instruction in both issues.

## Code path

`parse_txst_group` (`crates/plugin/src/esm/cell/support.rs:212-273`) iterates sub-records. Has arms for TX00..TX07 + MNAM. DODT and DNAM fall to `_ => {}`.

`TextureSet` (`crates/plugin/src/esm/cell/mod.rs:530-546`) carries 8 slots + `material_path`. No decal-data, no flags.

Existing precedent: `LightData` and `AddonData` in same module (`mod.rs:435-476`). DODT layout sub-record fits the same shape.

## DODT layout (FO4, 36 bytes)

Per UESP / xEdit `wbDefinitionsFO4`:

| Offset | Size | Field            |
|--------|------|------------------|
| 0      | 4    | MinWidth (f32)   |
| 4      | 4    | MaxWidth (f32)   |
| 8      | 4    | MinHeight (f32)  |
| 12     | 4    | MaxHeight (f32)  |
| 16     | 4    | Depth (f32)      |
| 20     | 4    | Shininess (f32)  |
| 24     | 4    | ParallaxScale (f32) |
| 28     | 1    | ParallaxPasses (u8) |
| 29     | 1    | Flags (u8) — bit 0 Parallax, bit 1 Alpha-Blending, bit 2 Alpha-Testing, bit 3 No Subtextures |
| 30     | 2    | Unknown (u16)    |
| 32     | 4    | Color (RGBA u8s) |

Skyrim DODT is the same 36-byte layout per UESP. No game-gating needed on size.

## DNAM layout

- FO4: u16 — bits 0x01 NoSpecular, 0x02 FaceGenTinting, 0x04 HasModelSpaceNormals
- Skyrim: u8 — bits 0x01 NoSpecular, 0x02 FaceGenTinting

Capture as `u16`, accept payload >= 1 byte (Skyrim path produces low-byte-only).

## Approach

1. Add `DecalData` struct to `mod.rs` mirroring `LightData` style.
2. Add `flags: u16` and `decal_data: Option<DecalData>` to `TextureSet`.
3. Add `b"DODT"` arm (length-gated to ≥36) and `b"DNAM"` arm (length-gated to ≥1) to the match in `support.rs`. Defensive: drop on length mismatch (matches LightData pattern).
4. Round-trip tests: helper `build_txst_record_raw` that takes raw byte payloads; one test per arm + one combined.

## Scope

- 1 source file edit: `crates/plugin/src/esm/cell/mod.rs` (new struct + 2 fields)
- 1 source file edit: `crates/plugin/src/esm/cell/support.rs` (2 match arms)
- 1 test file edit: `crates/plugin/src/esm/cell/tests.rs` (helper + 3 tests)

3 files. Within the no-pause threshold.
