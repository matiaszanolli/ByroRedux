# ESM-2026-10-18-EXT-02: WTHR `PNAM` cloud colours are read with a fixed 4-slot layer stride; FNV (6 slots) and FO4/FO76/Starfield (8 slots) read the wrong bytes for layers 1 to 3

Labels: medium,esm-plugin,terrain-exterior,bug,game:fnv,game:fo4,game:fo76,game:starfield
Source: docs/audits/AUDIT_ESM_2026-10-18.md

- **Severity**: MEDIUM. The tint is multiplied into the cloud dome, so a wrong table is a visible error on the reference title.
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: `WTHR` / `PNAM`
- **Location**: `crates/plugin/src/esm/records/weather.rs:844-859` (`parse_cloud_colors`); callers at `:667` (non-Skyrim arm) and `:1031` (Skyrim arm)
- **Status**: NEW
- **Description**: xEdit's `wbWeatherCloudColors` is an array of `wbWeatherTimeOfDay('Layer')` structs. `wbWeatherTimeOfDay` (`wbDefinitionsCommon.pas`) has 6 colours per layer on FNV (4 when the sub-record size is 64 or 160, via `wbWeatherTimeOfDayDecider`), 8 on FO4/FO76/Starfield, and 4 elsewhere. The code always uses `offset = (layer * 4 + slot) * 4`, a 16-byte layer stride. That is only right for Skyrim, FO3 and short FNV records. FNV `NAM0` already honours the 240/160 split (`:505-528`), but `PNAM` does not.
- **Evidence** (real data, `FalloutNV.esm`):
  - `PNAM` is 96 B on 55 of 63 weathers (6 slots × 4 layers) and 64 B on 8.
  - FO4 `PNAM` is 1,024 B on 67 of 71 records (32 layers × 8 slots), and FO76 is 1,024 B on 123 of 123.
  - `NVWastelandClearEast` `PNAM`, 24 RGBA words. Layer 3 (words 18 to 23) reads as a coherent sunrise/day/sunset/night/noon/midnight ramp: `(230,190,182) (241,238,228) (253,203,189) (24,28,35) (241,238,228) (37,44,56)`.
  - The code's layer 1 reads words 4 to 7: `(255,0,128) (0,0,0) (255,255,255) (255,255,255)`. That means a Day tint of black and a Sunrise tint of magenta. The code's layer 2 (words 8 to 11) also has a black Day.
  - `parse_weather_dynamics_keeps_timing_cloud_motion_and_lightning` (`weather.rs:1915`) builds a 64-byte `PNAM` and never exercises the 96-byte form.
- **Impact**: 55 of 63 FNV weathers and about 65 of 71 FO4 weathers feed wrong cloud tints for layers 1 to 3 into `cloud_tints` (`env_translate/sky_weather.rs:267`, `systems/weather.rs:505`). Layer 0 is correct.
- **Related**: EXT-04 (the same non-Skyrim branch never reads FO4 cloud textures).
- **Suggested Fix**: derive slots-per-layer from `(game, data.len())`. FNV: 4 when the length is 64, else 6. FO4/FO76/Starfield: 8 when the length is 1,024. Skyrim: 4. Pin it with the real 96-byte FNV words above.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, other callers)
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_ESM_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
