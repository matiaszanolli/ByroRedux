# #6256: FO4-2026-10-21-01: `ATXT` layers naming LTEX form 0 resolve no diffuse and are skipped

**Labels**: medium,bug,terrain-exterior,game:fo4,game:skyrim,game:fo3,legacy-compat

**Source**: docs/audits/AUDIT_FO4_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: FO4 data through the shared terrain splat path (mechanism owner `/audit-exterior`; filed here because that audit has already reported) · **Location**: `byroredux/src/cell_loader/terrain.rs:209-218`: `Some(ltex)` is passed for every ATXT layer, including 0.

## Location
- `byroredux/src/cell_loader/terrain.rs:209-218`: `Some(ltex)` is passed for every ATXT layer, including 0.
- `terrain.rs:299-326`: `resolve_cell_splat_layer`. `texture_set(0)` returns `None`, the default-land fallback is gated on `ltex.is_none()`, and the result is `diffuse_index = 0` and "skipping layer".
- `terrain.rs:226-231`: `normalize_base_ltex` maps form 0 to the default, but only for BTXT bases.
- `crates/renderer/shaders/include/bindings.glsl:686`: `if (layerIdx == 0u) continue;`

## Status at filing
NEW. Not in `issues.json`, not in `SUITE_FINDINGS.md`, and the closed-issue search `ATXT form 0` matches only #4078 (an unrelated VTXT-less ATXT) and #3314 (remap). e62e258bd's commit message names this residual ("the 7 left are ATXT layers referencing form 0, a separate question"), but nothing tracks it.

## Description
LAND alpha layers (`ATXT` + `VTXT`) may name LTEX form 0. xEdit's shared `wbLANDTextureToStr` (`wbDefinitionsCommon.pas:4793-4812`) is attached to **both** the BTXT and the ATXT `Texture` field (`:8997`, `:9011`). It renders form 0 as the game's default land texture: FO4 `LCWDefault01Grass01`, Skyrim `LDirt02`, FO3/FNV `LDirtWasteland01`, Oblivion `TerrainHDDirt01dds`. The engine already applies this rule to BTXT, through `normalize_base_ltex` and `DefaultLandTexture::for_game`. The FO4 entry in `for_game` is `ground\commonwealthdefault01_d/_n/_s`, sourced from the exe strings.

The ATXT path never maps 0 to the default. Three things follow:
- The layer keeps its alpha but gets texture index 0, so the shader skips the lane. The painted default-land area shows whatever lies under it (the base or lower layers) instead of the default texture.
- The layer still consumes one of the 8 splat lanes in `cap_splat_layers_to_shader_budget`, which can push out a real layer.
- Its ground-cover affinity key is `""` (`cover_affinity_key(Some(0), …)`). For FO4 this matters most, because the default LTEX is a *grass* layer.

## Evidence
raw LAND walk, `/tmp/audit/fo4/py/atxt0.py` / `atxt0g.py`. "opacity > 0.05" means the layer actually paints.

| Master | LAND | ATXT layers | ATXT form 0 | ATXT form 0 with opacity > 0.05 | BTXT form 0 |
|---|---|---|---|---|---|
| Fallout4.esm | 37,020 | 56,859 | 3,335 (5.9%) | 3,335 | **0** |
| DLCCoast.esm | 2,432 | 21,761 | 1,567 (7.2%) | 1,567 | 0 |
| DLCNukaWorld.esm | 4,326 | 21,084 | 2,148 (10.2%) | 2,145 | 0 |
| Skyrim.esm | 15,564 | 102,968 | 10,914 (10.6%) | 10,851 | 0 |
| Fallout3.esm | 40,256 | 63,456 | 6,791 (10.7%) | 6,771 | 0 |

Examples: `Fallout4.esm` LAND `0x0000F540` paints form 0 over 129 vertices at opacity 1.0, and `DLCCoast` `0x0000F830` over 240. Note that **no vanilla master in this table authors BTXT form 0**. The default-land rule the engine implements fires on no vanilla FO4 / Skyrim / FO3 data, while the ATXT case it misses fires thousands of times. (FalloutNV.esm was not measured: the Python walker's zlib inflate failed on one of its compressed records.)

Code path: `FormIdRemap::remap` keeps 0 as 0 (`reader.rs:567`), so the parser hands `ltex_form_id: 0` to the terrain code unchanged.

## Impact
About 6–11% of authored terrain paint layers on FO4, Skyrim and FO3 render as the underlying texture instead of the default land texture, and lose their grass affinity. In FO4 that removes the default grass from the Commonwealth's painted patches, and it wastes a splat lane on every affected tile. Full-res terrain only. The LOD path is a separate tier.

## Related
#6199 / e62e258bd (the same resolver; residual named in its message); EXT-D6-2026-10-21-02 (the terrain-LOD sibling of #6199); `DefaultLandTexture` (the BTXT half of the same rule).

## Suggested Fix
Route ATXT form 0 through the same default-land resolution as BTXT. For example, normalize an ATXT `Some(0)` to the default in `resolve_cell_splat_layer` / `LandscapeLayerTextures::texture_set`. Key `cover_affinity_key` on the default LTEX's editor ID. Add a unit pin that an ATXT-0 layer gets a non-zero diffuse index whenever `DefaultLandTexture::for_game` is `Some`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
