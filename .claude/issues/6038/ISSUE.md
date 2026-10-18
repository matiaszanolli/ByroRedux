# ESM-2026-10-18-EXT-03: Skyrim `NAM0` and `DALC` are accepted only at their newest size, so older-version vanilla weathers decode to no colours

Labels: medium,esm-plugin,terrain-exterior,bug,game:skyrim
Source: docs/audits/AUDIT_ESM_2026-10-18.md

- **Severity**: MEDIUM. Wrong parse of vanilla records. No live consumer today, because the FX weathers are reachable only through the unconsumed `sky_region` (#5463).
- **Dimension**: Sub-Record Byte Accounting
- **Record / Sub-record**: `WTHR` / `NAM0`, `DALC`
- **Location**: `weather.rs:960` (`NAM0 if len >= 17*4*4`), `:1069` (`DALC if len >= 32`)
- **Status**: NEW
- **Description**: xEdit gates the Skyrim `NAM0` groups on the record form version (`wbFromVersion` 31 for `Cloud LOD Diffuse`/`Cloud LOD Ambient`/`Fog Far`, 35 for `Sky Statics`, 37 for `Water Multiplier`/`Sun Glare`/`Moon Glare`). A record saved at version 35/36 carries 14 groups (224 B), and one at version 31 to 34 carries 13 (208 B). The Skyrim arm drops the whole sub-record unless it is at least 272 B. The Creation-era arm (`creation_nam0_shape`) is exact-size for FO4; the Skyrim arm has no equivalent.
- **Evidence**: `Skyrim.esm` has 11 weathers with `NAM0` = 224 B: `FXWthrInvertWindowsWindhelm`, `…Windhelm2`, `…Whiterun`, `FXWthrInvertLightsSolitude`, `…Whiterun`, `FXWthrSunlight`, `FXWthrSunlightWhite`, `FXWthrInvertDayNighWarm`, `FXWthrCaveBluePaleLight`, `FXWthrCaveBlueSkylight`, `FXWthrInvertDayNight`. `TESTCloudyRain` has `NAM0` = 208 B and `DALC` = 24 B ×4. All of them decode to an all-zero `sky_colors` table.
- **Impact**: zero today (see Severity). It becomes live when `CellData::sky_region` gains a consumer, because Skyrim interior window-light effects take their colours from exactly these weathers.
- **Suggested Fix**: decode `n = len / 16` groups for the Skyrim arm (13 to 17), leaving later groups at default, and accept `DALC` at 24 B (6 axes only, no fresnel). Pin with the 224-byte shape.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other games' arms, other callers)
- [ ] **TESTS**: A regression test pins this specific fix


_Filed from `docs/audits/AUDIT_ESM_2026-10-18.md` (`/audit-suite --area exterior`, HEAD 6f44874ac). Fix with `/fix-issue <number>`._
