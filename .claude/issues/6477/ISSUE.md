# OBL-D3-2026-10-27-01: The race body skin is painted onto Oblivion's underwear meshes

Labels: medium, game:oblivion, legacy-compat, nifal, import-pipeline, bug
Source: docs/audits/AUDIT_OBLIVION_2026-10-27.md

**Audit report**: `docs/audits/AUDIT_OBLIVION_2026-10-27.md` -- finding `OBL-D3-2026-10-27-01` (MEDIUM)
**Location (re-mapped to current code)**: `byroredux/src/npc_spawn/loot_appearance.rs` (`set_body_base_color`, `skin_texture_symbols_from`); callers `byroredux/src/npc_spawn/resumable/runtime.rs:735` (spawn) and `loot_appearance.rs:917` (restore after looting, #6128). Premise re-read at HEAD: the filter is still `starts_with("textures\\characters\\")` over the interned pool, no per-part stem check.
**Status at filing**: NEW (pre-existing since #5487; #6090 added the `textures\characters\` filter but only separates FO3/FNV underwear, which lives under `textures\armor\underwear\`)

## Description
`set_body_base_color` treats every mesh whose authored base colour is under `textures\characters\` as a "skin" mesh and overrides it with the race skin. Its doc claims "Oblivion's single-mesh bodies", but Oblivion's body NIFs are not single-mesh: the underwear geometry is a second sub-mesh authored under that same folder, so it passes `skins.contains(&sym)` and is repainted with the race's torso/leg skin.

## Evidence
`material_dump` over `Oblivion - Meshes.bsa` (per the audit):
- `femaleupperbody.nif`: `UpperBody:0` = `...\imperial\female\upperbodyfemale.dds`, `UpperBody:1` = **`textures\characters\imperial\female\underwear.dds`**, then `Arms`.
- `femalelowerbody.nif`: `LowerBody:0` = `legfemale.dds`, `LowerBody:1` = **`underwear.dds`**.
- `lowerbody.nif`: `LowerBody:0` = `legmale.dds`, `LowerBody:1` = **`textures\characters\imperial\male\groinmale.dds`**.
- `Oblivion - Textures - Compressed.bsa` ships one `underwear.dds` (Imperial female); `groinmale.dds` exists only for Imperial and Dremora.
- The RACE body-section ICON runs (INDX 0..4) have no underwear entry, so vanilla keeps the Imperial underwear for every race.
- Every one of those meshes passes the `skins.contains(&sym)` test and so receives the race's `UpperBodyFemale` / `LegMale` skin.

## Impact
Every unclothed Oblivion body part draws its underwear geometry in torso/leg skin, mapped through the underwear UVs. This covers spawn with no clothing in that slot and any part restored after the player loots a corpse's clothing. Imperials are affected too because the override always applies. Visual only; Oblivion only.

## Related
#6090, #5487, #6128, NIFAL-D8-2026-10-27-01 (a different fragility in the same filter).

## Suggested Fix
Restrict the override to meshes whose authored texture stem is the part's own skin stem (`upperbody*`, `leg*`, `hand*`, `foot*`, `tail*`), or exclude `underwear` and `groin*` on Oblivion. Pin it with an `--ignored` test over the five vanilla body NIFs. Update the `set_body_base_color` doc, which wrongly says Oblivion bodies are single-mesh.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser->`Material` boundary -- never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

