# #3222 — OBL-2026-08-20-D5-01: Oblivion's WATR.TNAM is a diffuse texture but env_translate binds it into WaterMaterial::normal_map_index — 15/23 WATR records, 163 vanilla cells shaded off an inverted colour-derived normal

**Issue**: #3222 — https://github.com/matiaszanolli/ByroRedux/issues/3222
**Finding ID**: `OBL-2026-08-20-D5-01`
**Severity**: HIGH
**Dimension**: 5 — NIFAL / WATAL canonical translation
**Audit**: `/audit-oblivion` — `docs/audits/AUDIT_OBLIVION_2026-08-20.md` (HEAD `bb0b92f2`, 2026-08-20 comprehensive suite)
**Labels**: high, legacy-compat, import-pipeline, renderer, bug
**Filed**: 2026-08-20 · `/audit-publish`

---

**Audit**: `/audit-oblivion` — `docs/audits/AUDIT_OBLIVION_2026-08-20.md` (Dim 5 — NIFAL/WATAL canonical translation), HEAD `bb0b92f2`
**Finding ID**: `OBL-2026-08-20-D5-01`

- **Severity**: HIGH
- **Status**: NEW

## Location

- `byroredux/src/env_translate.rs:1030-1035` — **the single translate site**
- Consumed at `byroredux/src/cell_loader/water.rs:463-472` (`normal_map_index`, then `noise_map_indices`)
- Decoded as a tangent-space normal at `crates/renderer/shaders/water.frag:309-310`
- Parsed at `crates/plugin/src/esm/records/misc/water.rs:1312-1313`

## Description

`parse_watr` writes **both** `TNAM` and `NNAM` into the same `WatrRecord::texture_path` field:

```rust
b"TNAM" => out.texture_path = read_zstring(&sub.data),
b"NNAM" => out.texture_path = read_zstring(&sub.data),
```

whose own docstring (`water.rs:65-70`) enumerates only *"FO3 / FNV ship this in `NNAM` … Skyrim+ ships it in `TNAM`"* — **Oblivion is not in the contract at all.**

The translate boundary then does, with **no `GameKind` gate**:

```rust
// TNAM is the diffuse / noise texture — used as the
// bindless normal map for the shader. Empty path =
// procedural fallback.
if !rec.texture_path.is_empty() {
    normal_path = Some(rec.texture_path.clone());
}
```

and the cell loader assigns the resolved handle to `WaterMaterial::normal_map_index`, which the shader samples as a **strict tangent-space normal**:

```glsl
vec3 n = texture(textures[nonuniformEXT(normalMapIndex)], uv).xyz;
n = normalize(n * 2.0 - 1.0);
```

On Oblivion, `TNAM` is the Construction Set's **Texture** field — the water surface's *colour* art. Feeding albedo through `rgb * 2 - 1` does not produce a normal; it produces an arbitrary, usually downward-facing vector. `noise_map_indices` then inherit the same handle for all three wave layers (`cell_loader/water.rs:465-472`), so **every layer samples it**.

## Evidence

Extracted and header/block-decoded straight out of `Oblivion - Textures - Compressed.bsa`:

```
textures\water\oblivionlava06.dds   512x512 DXT1  mean RGB (178, 55, 26)
textures\water\dungeonwater01.dds   512x512 DXT1  mean RGB ( 27, 30, 23)
```

A tangent-space normal map has a mean near (128, 128, 255). **(178, 55, 26)** maps to `normalize((0.396, -0.569, -0.796))` ~ `(0.38, -0.55, -0.75)` — the **Z component is negative**, i.e. the surface normal points *into* the plane. `textures\water\` contains exactly these two files and **no `_n` sibling**, confirming Oblivion ships no separate water normal map.

The authored `TNAM` values are themselves conclusive — they are reused architecture / landscape / dungeon **albedo**:

```
SEBrellachWater              Architecture\city\Dementia\Sewage01.dds
SEPinnacleRockWater          Landscape\Dementia\DementiaMold01.dds
SERuinDungeonWaterNoSwim     Dungeons\RuinsDungeons\RRubblePileA01.dds
SErootDungeonWaterDeepNasty  Dungeons\Rootcaves\Rooms\RootRoomCeiling02.dds
XPBlood                      Dungeons\Misc\BloodPool02.dds
Blood / CamoranLava02        Landscape\Oblivion\TerrainHDOblivionLava01.dds
DungeonWater01 / SewerWater /
  DungeonWaterBrightFog01    Water\DungeonWater01.dds
OblivionLavaTest01           Water\OblivionLava06.dds
CamoranLava                  OblivionGate\Lava01.dds
OblivionOil01                Water\OblivionOil01.dds
MS31Water                    Water\water00.dds
```

**15 of 23** vanilla `Oblivion.esm` `WATR` records author a non-empty `TNAM`.

## Impact

**163 vanilla `Oblivion.esm` cells** reference a TNAM-bearing `WATR` — 74 x `DungeonWater01`, 45 x `OblivionLavaTest01`, 15 x `SewerWater`, 12 x `SErootDungeonWaterShallow`, plus the Shivering Isles set, the blood pools and the Camoran lava.

Every one gets an inverted, high-frequency, colour-derived normal field on **all three** wave layers: broken Fresnel, broken reflection-ray direction, broken specular.

Bounded away from CRITICAL because the *default* waters (`DefaultWater`, `DefaultWaterNight`, `DefaultUnderwater`, `SwampWater`, `SEDefault*`, `OblivionCitadelLavaPlane`) all ship an **empty** `TNAM`, so the Tamriel open world correctly falls through to the shader's procedural path (`water.frag:235`, `normalMapIndex == 0xFFFFFFFF`). The bug is interiors, dungeons, sewers, Shivering Isles and lava.

## Related

- **#3145** (`ESM-D5-06` / `LC-D5-01`) and **`OBL-2026-08-20-D3-01`** (`MNAM`) are the other two Oblivion-specific `WATR` defects — same record, adjacent code, one fix commit is reasonable.
- **#3152** (`LC-D2-01`, mesh-bound `blend_normals`) does **not** apply here: `BSWaterShaderProperty` is `#SKY_AND_LATER#` in nif.xml, so the block type cannot appear in an Oblivion NIF, and the 82-type vanilla Oblivion block histogram contains no `BS*ShaderProperty` at all. Oblivion cell water takes the `env_translate` path exclusively.
- **`OBL-2026-08-20-D5-02`** — `watal.md`'s Oblivion `diffuse/normal texture` row states **SENTINEL `u32::MAX` -> procedural**, which is the false premise that made this look correct.
- #1997 (CLOSED) — the procedural fallback this record set *should* be reaching.

## Suggested Fix

Give `WatrRecord` a second field so the diffuse and the noise/normal roles stop sharing one string, and gate the Oblivion `TNAM` arm into the **diffuse** role.

Until a water-diffuse consumer exists, the correct canonical value for Oblivion's `normal_map_index` is the `u32::MAX` procedural sentinel — i.e. **dropping the TNAM is strictly better than binding it**, and is a one-line change at `env_translate.rs:1033`.

Pin it with a real-data-shaped test asserting a `GameKind::Oblivion` `WATR` carrying `Water\OblivionLava06.dds` leaves `normal_map_index` at the sentinel.

## Completeness Checks
- [ ] **SIBLING**: FO3/FNV `NNAM` and Skyrim+ `TNAM` genuinely *are* normal/noise maps — confirm the split does not change either, and check the LOD twin at `cell_loader/water.rs:811-817`
- [ ] **CANONICAL-BOUNDARY**: the per-game role decision lands at the parser -> `WaterMaterial` boundary (`env_translate` / `parse_watr`), never as a `GameKind` branch inside `water.frag`
- [ ] **TESTS**: a `GameKind::Oblivion` `WATR` with an authored `TNAM` asserts `normal_map_index == u32::MAX` and that all three `noise_map_indices` stay at the sentinel
- [ ] **TESTS**: a Skyrim/FNV fixture asserts its normal/noise binding is unchanged by the split
