# 2002: NIF-D5-01: BSLightingShaderProperty::parse_fo4 reads pre-Name Shader Type unconditionally across BSVER 130-154

https://github.com/matiaszanolli/ByroRedux/issues/2002

Labels: medium, nif-parser, nif, bug

**Severity**: MEDIUM · **Dimension**: Collision & Shader Block Parsing
**Location**: `crates/nif/src/blocks/shader.rs:976-978` (`parse_fo4`)
**Status**: NEW
**Audit**: docs/audits/AUDIT_NIF_2026-07-16.md (NIF-D5-01)

## Description
nif.xml gates the pre-`Name` `Shader Type` field with `vercond="#BS_GTE_SKY# #AND# #NI_BS_LTE_FO4#"` (`bsver` in `[83, 139]`). `parse_fo4` handles the wider BSVER 130-154 band but reads `shader_type` unconditionally as the very first field, before `NiObjectNETData::parse`. The sibling function already has the correct `bsver < FO4_DLC_UPPER` gate for its own trailing field (fixed under #1552), but that fix was never applied here.

## Evidence
```rust
fn parse_fo4(stream: &mut NifStream, bsver: u32) -> io::Result<Self> {
    let shader_type = stream.read_u32_le()?;   // unconditional across the whole 130-154 band
    let net = NiObjectNETData::parse(stream)?;
```
vs. the already-fixed sibling one call deeper (`shader.rs:1392-1398`, #1552):
```rust
let env_map_scale = if bsver < crate::version::bsver::FO4_DLC_UPPER {
    stream.read_f32_le()?
} else {
    1.0
};
```

## Impact
For any block at `bsver` in `[140, 154]`, `Name` and every subsequent field are corrupted. FO3+'s `block_sizes` table means this does not cascade to later blocks, but the affected block is silently wrong. Dead band per #1552's own comment, so vanilla-content impact is unlikely; residual risk on dev-stream/modded content is non-zero.

## Related
#1552 / SK-D2-01 (same gate, sibling field, already fixed there).

## Suggested Fix
Gate `shader_type` the same way #1552 gated `env_map_scale`: read only if `bsver < FO4_DLC_UPPER`, default 0 otherwise; add a `bsver=145`-style test fixture.

## Completeness Checks
- [ ] SIBLING: Same pattern checked in related files (other `bsver`-gated fields in the same `parse_fo4` function, and Skyrim-side siblings)
- [ ] TESTS: A regression test pins this specific fix (`bsver=145` fixture)
