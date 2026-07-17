# SK-D2-01: FO4 Skin Tint alpha (Shader Type 5) parsed then discarded, never reaching MaterialInfo

**Severity**: LOW
**Labels**: low, nif-parser, legacy-compat, bug
**Location**: `crates/nif/src/blocks/shader.rs:1412-1424` (`parse_shader_type_data_fo4`, type 5 arm)
**Source audit**: `docs/audits/AUDIT_SKYRIM_2026-07-16.md` (SK-D2-01)

## Description
For FO4 (BSVER 130-139), nif.xml gives Shader Type 5 (Skin Tint) both a `Color3` and a trailing `Skin Tint Alpha` float. The parser reads the alpha to keep the stream aligned (`let _skin_tint_alpha = stream.read_f32_le()?;`) but binds it to `_` and drops it — no stream drift, pure data loss. `ShaderTypeData::SkinTint` only carries `[f32; 3]`. The FO76 sibling path (`Fo76SkinTint`, Color4) *does* preserve this field, so the two shader-type-data producers are asymmetric.

## Evidence
`shader.rs:1419-1423` reads then discards the value; contrast with `Fo76SkinTint` which surfaces `skin_tint_alpha` as part of a Color4.

## Impact
FO4 NPC/creature skin materials lose their authored skin-tint alpha at import. Small in practice — nif.xml annotates the field as "Overridden by game settings," and no vanilla Skyrim content reaches this arm.

## Suggested Fix
If FO4 fidelity is later wanted, add an optional `skin_tint_alpha` to `ShaderTypeData::SkinTint` (or reuse `Fo76SkinTint`) and populate `MaterialInfo.skin_tint_alpha` from the FO4 arm. Otherwise leave as-is with an explicit `// intentionally dropped: game-setting override` comment.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other per-game parsers)
- [ ] **TESTS**: A regression test pins this specific fix
