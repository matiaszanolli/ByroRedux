# NIF-D3-01: BSSkyShaderProperty + BSWaterShaderProperty Skyrim+ shaders aliased to FO3 PPLighting parser

URL: https://github.com/matiaszanolli/ByroRedux/issues/713
Labels: bug, nif-parser, high

---

## Severity: HIGH

## Game Affected
Skyrim SE, FO4

## Location
- `crates/nif/src/blocks/mod.rs:293-300` (alias arm at the catch-all PPLighting branch)

## Description
`BSSkyShaderProperty` (nif.xml line 6708) and `BSWaterShaderProperty` (line 6695) are `versions="#SKY_AND_LATER#"` blocks that **inherit `BSShaderProperty` directly** — no `texture_clamp_mode`, no `texture_set_ref`, no PP refraction/parallax trailer. Their on-disk layout is the Skyrim shader layout: Shader Flags 1/2 u32 pair (or BSVER>=132 CRC32 arrays), then UV Offset/Scale, then per-type tail.

The current dispatch routes both names to `BSShaderPPLightingProperty::parse`, which calls `BSShaderPropertyData::parse_fo3` (reads `texture_clamp_mode`) and then reads the FO3 PP trailer (`texture_set_ref` + `refraction*` + `parallax*`). On a Skyrim+ `BSSkyShaderProperty`/`BSWaterShaderProperty` this **over-consumes 12-28 bytes** that should be carrying `Shader Flags 1/2 + UV Offset + UV Scale + sky-/water-specific tail`.

Block-size recovery masks the drift; the per-game shader struct never reaches the importer.

This is the Skyrim-side equivalent of the FO3 `SkyShaderProperty` issue closed by #550.

## Evidence
- nif.xml `BSSkyShaderProperty`:
  ```
  inherit="BSShaderProperty" versions="#SKY_AND_LATER#"
  Shader Flags 1: SkyrimShaderPropertyFlags1   (u32, BSVER < 132)
  Shader Flags 2: SkyrimShaderPropertyFlags2   (u32)
  Num SF1: uint                                (BSVER >= 132)
  SF1: BSShaderCRC32 × Num SF1                 (BSVER >= 132)
  ... (similar for SF2 at BSVER >= 152)
  UV Offset: TexCoord (2 × f32)
  UV Scale: TexCoord (2 × f32)
  Source Texture: SizedString
  Sky Object Type: SkyObjectType (u32)
  ```
- Code at `blocks/mod.rs:293-300`:
  ```rust
  "BSShaderPPLightingProperty"
  | "Lighting30ShaderProperty"
  | ...
  | "BSSkyShaderProperty"
  | "BSWaterShaderProperty" => Ok(Box::new(BSShaderPPLightingProperty::parse(stream)?)),
  ```
- Corpus: 2 NiUnknown `BSSkyShaderProperty` on `Skyrim - Meshes1.bsa`; 3 + 1 on FO4 Meshes.

## Impact
Sky dome / sun glare / water surface NIFs lose their shader struct (Shader Flags, UV transform, sky type). Renders with default cloud scroll, default water flow direction, and a texture pulled from whatever the PP-Lighting parser landed on (likely a missing-texture fallback).

## Suggested Fix
Add dedicated `BSSkyShaderProperty::parse` and `BSWaterShaderProperty::parse` parsers that share a Skyrim shader base:
- Branch on `BSVER >= 132`: Shader Flags pair vs CRC32 arrays
- Then `UV Offset + UV Scale`
- Then per-type tail (Source Texture + Sky Object Type for sky; water has its own fields)

Move the names out of the PPLighting alias arm into dedicated dispatch arms next to the existing `SkyShaderProperty` line. Add corpus regressions on the 2 Meshes1 sky NIFs.

## Related
- Audit: docs/audits/AUDIT_NIF_2026-04-26.md (NIF-D3-01)
- Sibling: #550 (closed FO3 `SkyShaderProperty` — same root cause, only fixed the FO3-prefixed variant)
- Adjacent: NIF-D3-02 (4 more shader types in same alias arm)

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: NIF-D3-02 (HairShaderProperty / VolumetricFog / DistantLOD / BSDistantTree) also in same alias arm — should be fixed in same pass
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Byte-exact dispatch test for both blocks; corpus regression — zero drift on `Skyrim - Meshes1.bsa` sky NIFs
