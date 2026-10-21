# #6258: OBL-D4-2026-10-21-01: Oblivion's authored cloud colours (`NAM0` groups 2 and 9) are never read; every Oblivion cloud layer gets a constant white tint

**Labels**: medium,bug,terrain-exterior,esm-plugin,game:oblivion,legacy-compat

**Source**: docs/audits/AUDIT_OBLIVION_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: 4 (TES4 WTHR decode → SKYAL cloud tint) · **Location**: `crates/plugin/src/esm/records/weather.rs:1-25` and `:36-55` (module doc and `SKY_UNUSED_2` / `SKY_UNUSED_9`: "Unused" for every game)

## Location
- `crates/plugin/src/esm/records/weather.rs:1-25` and `:36-55` (module doc and `SKY_UNUSED_2` / `SKY_UNUSED_9`: "Unused" for every game)
- `:522-545` (the 160-byte `NAM0` arm fills `sky_colors` only)
- `:696-703` (`PNAM`, the only writer of `cloud_layer_colors` outside the Creation-era path)
- `:408-413` (white default)
- consumers `byroredux/src/env_translate/sky_weather.rs:263-270`, `:407-411`, `byroredux/src/systems/weather.rs:505-514`, `crates/renderer/shaders/include/clouds.glsl:160-183`, `:575-597`

## Status at filing
NEW. #6037 (closed) fixed the `PNAM` stride, and #6041 (closed) fixed the Oblivion `CNAM`/`DNAM` layer order and the `DATA` speeds. Neither touched `NAM0`'s cloud groups. A closed-issue search for WTHR cloud colours finds no Oblivion entry. This is not on `SUITE_FINDINGS.md`.

## Description
xEdit `wbWeatherColors` (`wbDefinitionsCommon.pas:9707-9725`) names `NAM0`'s groups by game:
- **TES4**: group 2 is `Clouds-Lower` and group 9 is `Clouds-Upper` (`IsTES4(...)`).
- **FO3**: group 2 is `Unused` and group 9 is `Clouds (Unused)`.
- **FNV**: group 2 is `Unused` and group 9 is `Effect Lighting`.

The parser's slot constants and module doc follow the FO3/FNV reading for every game, and nothing ever reads groups 2 or 9. Fallout and Creation-era games author cloud colours in `PNAM`. Oblivion authors no `PNAM`, so its `cloud_layer_colors` keep the `[255, 255, 255, 255]` default at every time of day. #6041 put Oblivion's lower deck (`CNAM`) on layer 0 and the upper deck (`DNAM`) on layer 1, so these two groups map directly onto those layers.

## Evidence
(`/tmp/audit/oblivion/py/wthr.py` over `Oblivion.esm`) 37 / 37 WTHR author a 160-byte `NAM0`, and 0 / 37 author `PNAM`. Authored day / night cloud colours that are currently replaced by white:

| Weather | Clouds-Lower (day / night) | Clouds-Upper (day / night) |
|---|---|---|
| `Clear` | (255,255,255) / (4,12,23) | (255,255,255) / (6,15,26) |
| `Cloudy` | (239,237,237) / (14,27,44) | (255,255,255) / (14,22,35) |
| `OblivionStormTamriel` | (185,162,151) | (241,156,99) |
| `OblivionMountainFog` | (235,141,101) | (241,156,99) |
| `SEClear` (lower deck at night) | — / (219,78,222) | |
| `CamoranWeather` | (244,192,170) | (251,177,134) |

In the shader, the tint sets the clouds' chroma (`cloud_authored_colour` → `mood_chroma`, `clouds.glsl:575-587`) and their alpha.

## Impact
Every Oblivion exterior draws neutral-white-chroma clouds. Lost as a result:
- the blue-black night decks of `Clear` and `Cloudy`;
- the orange decks of the Oblivion-gate storms (`OblivionStorm*`, the main-quest Kvatch and Great Gate skies);
- the Shivering Isles mania and dementia tints.

This is a visual fidelity loss on the title's signature skies. Nothing crashes or gets corrupted.

## Related
#6041, #6037, #5351 (the same file's open `OblivionHdrLighting` doc rot), SKYAL.

## Suggested Fix
In `parse_wthr`, under `GameKind::Oblivion`, copy `sky_colors[2][0..4]` into `cloud_layer_colors[0]` and `sky_colors[9][0..4]` into `cloud_layer_colors[1]`. Keep alpha at 255 unless the authored A byte is shown to be opacity. Rename the slot constants and doc per game (TES4 `Clouds-Lower`/`Clouds-Upper`; FNV slot 9 `Effect Lighting`). Pin it with a 160-byte fixture alongside `parse_wthr_oblivion_cloud_layers_and_data_speeds`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
