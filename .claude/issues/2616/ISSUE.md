# SF-D6-01: BSLightingShaderProperty misaligned by one 4-byte word on 100% of Starfield full-body blocks

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2616
**Finding ID**: SF-D6-01

**Severity**: HIGH
**Dimension**: 6 (NIF Shader Blocks, BSVER 155+)
**Location**: `crates/nif/src/blocks/shader.rs:1142-1161` (`BSLightingShaderProperty::parse_fo76_plus`)
**Status**: NEW — not covered by #1510, #1606, #1721, #1881, or #2353

## Description
`parse_fo76_plus` makes two 4-byte compensating errors for
`bsver >= STARFIELD`: it skips the `shader_type` u32 that Starfield *does*
carry, and it reads `root_material_path` unconditionally, which Starfield
does *not* carry. Total block consumption stays right — no drift reported —
which is precisely why this survived #1510, #1606, and two prior Starfield
audits. Every field between the two errors is read one word early:
`num_sf1`, `num_sf2`, both CRC arrays, `uv_offset`, `uv_scale`,
`texture_set_ref`, `emissive_color`, `emissive_multiple`. Fields from
`texture_clamp_mode` onward re-converge and are unaffected.

## Evidence
```rust
// crates/nif/src/blocks/shader.rs:1142-1146
let shader_type = if bsver < crate::version::bsver::STARFIELD {
    stream.read_u32_le()?
} else {
    0   // <-- WRONG: Starfield DOES carry shader_type
};
...
let root_material_path = stream.read_string()?;  // <-- WRONG: read unconditionally,
                                                   //     Starfield does NOT carry this field
```
Real-block dump, `Starfield - LODMeshes.ba2`, `shiplandingmarker_lod_3.nif`
block 6 (bsver 173, block_size 166): the shipped parser reads `sf2[0]` = CRC
`0` (not a valid `BSShaderCRC32` value — the real `num_sf2` word),
`uv_scale.y`/`texture_set_ref` from the same word pair
(`texture_set_ref = 1065353216`, unresolvable), `emissive_color.r` = **NaN**
(from the `0xFFFFFFFF` word that is really `texture_set_ref`'s NULL
sentinel). Corpus-wide corrected-alignment scoring (CRC membership in the
32-value set, resolvable texture-set ref, finite non-negative emissive)
across LODMeshes/Meshes01/MeshesPatch: **0/2,538 valid under the shipped
alignment, 2,538/2,538 valid under the corrected one.** Under the corrected
alignment the previously-bimodal tail length `{38: 1868, 42: 11}` collapses
to a uniform `{38: 1879}` — the 11 outliers were an artifact of the
misalignment itself. Downstream, `dedicated_shader.rs` copies all of
`emissive_color`, `uv_offset`/`uv_scale`, `texture_set_ref`,
`root_material_path`, and `sf1_crcs` (decal/two-sided/PBR/vertex-colour
classification) directly from these fields.

## Impact
All 2,538 inline-authored Starfield `BSLightingShaderProperty` meshes
receive a **NaN** emissive colour propagated through `translate_material`
into the ECS `Material` and `GpuMaterial` SSBO (poisoning any lighting term
it multiplies into); a `texture_set_ref` that can never resolve (texture
slots silently empty); a UV transform with a **zero U-scale**; and a
shader-flag CRC set invalid on 1,446/2,538 blocks (57%), making
decal/two-sided/PBR/vertex-colour classification arbitrary. Per the
severity table, "Wrong/divergent Material out of the NIFAL boundary" is HIGH
minimum.

## Suggested Fix
Read `shader_type` unconditionally for `bsver >= FO76` (revert the
`< STARFIELD` gate); gate `root_material_path` on `bsver < STARFIELD`
instead. Net byte count is unchanged. Add a real-data-derived fixture rather
than editing the synthetic builder to match (see the tautological-fixture
finding SF-D6-03).

## Related
#1510 (`c2778fc5`, introduced the `bsver < STARFIELD` shader-type gate),
#1606 (`497700e7`, built the opaque tail on top of the misalignment),
SF-D6-02, SF-D6-03, SF-D6-04.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Fix is parser-side (block decode), upstream of the `translate_material` boundary — no per-game branch needed there once this lands
- [ ] **TESTS**: Add a real-data-derived fixture (shiplandingmarker_lod_3.nif block 6) asserting semantic invariants (finite emissive, resolvable texture_set_ref, valid CRC membership) — see SF-D6-03
