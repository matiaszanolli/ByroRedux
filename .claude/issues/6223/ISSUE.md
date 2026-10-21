# #6223: TD2-2026-10-21-01: WTHR decoders duplicate NAM0 / FNAM / cloud-control decode across `parse_wthr` and `parse_wthr_skyrim`, with divergent fix history

**Labels**: medium,esm-plugin,terrain-exterior,tech-debt,bug

**Source**: docs/audits/AUDIT_TECH_DEBT_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: 2 (also Dim 1: `parse_wthr` crossed 300 lines) · **Location**: `crates/plugin/src/esm/records/weather.rs:455-765` (`parse_wthr`), `:1131-1306` (`parse_wthr_skyrim`); NAM0 loops at `:502-521` (FO4/FO76), `:522-547` (classic), `:1160-1195` (Skyrim); fog heads `:570-623` and `:1197-1215`, legacy HNAM fog `:663-674`; Creation cloud controls `:697-716` vs …

- **Severity**: MEDIUM (promotion: duplicated logic with divergent bug-fix history)
- **Dimension**: 2 (also Dim 1: `parse_wthr` crossed 300 lines)
- **Location**: `crates/plugin/src/esm/records/weather.rs:455-765` (`parse_wthr`), `:1131-1306` (`parse_wthr_skyrim`); NAM0 loops at `:502-521` (FO4/FO76), `:522-547` (classic), `:1160-1195` (Skyrim); fog heads `:570-623` and `:1197-1215`, legacy HNAM fog `:663-674`; Creation cloud controls `:697-716` vs `:1224-1240`; IMSP `:480` vs `:1251`
- **Status**: NEW
- **Age**: `parse_wthr` 251→306 lines, `weather.rs` 1131→1338 prod LOC in 7 commits since 12677d718 (`7f4c25e3e` #6037, `67433ee3a` #6038, `3ab1b5557` #6039, `f16741886` #6040–2, `1dd217412` #6045, …)
- **Effort**: medium
- **Description**: FO4, FO76 and Starfield author Skyrim's Creation-era WTHR layout: the 32-layer `00TX`..`O0TX` cloud list, IMSP, JNAM/RNAM/QNAM/PNAM and the FNAM fog extension. They are still routed through the FO3/FNV-shaped `parse_wthr`, which bolts the Creation fields on behind a `creation_era` flag and 10 `matches!(game, …)` guards. Each Creation feature therefore exists twice, and the two copies have already drifted:
  - **#6039**: FO4/FO76/SF cloud textures were never decoded because only the Skyrim arm handled `xxTX`. The fix added a third call site through `set_creation_cloud_texture`.
  - **#6038**: Skyrim NAM0 accepted only the version-37 272-byte form and gained `skyrim_nam0_groups`. The FO4 arm has its own `creation_nam0_shape`.
  - **OBL-D4-2026-10-21-01** (today): Oblivion's NAM0 groups 2/9 are cloud colours, so a third per-game NAM0 rule is about to land in the same function.

  The `SkyColor { r: data[o], g: data[o+1], … }` unpack and the HIGH_NOON/MIDNIGHT synthesis are written three times. The four-float fog head (`fog_day_near`, `fog_day_far`, `fog_night_near`, `fog_night_far` with the 10000.0 defaults) is written three times.
- **Evidence**: `grep -n 'TOD_HIGH_NOON\] = ' weather.rs` returns 518, 540 and 1187. #6039's commit message: "Only Skyrim was routed to the xTX cloud-list decode; the generic WTHR arm matched FO3/FNV's DNAM/CNAM/ANAM/BNAM only."
- **Impact**: Every WTHR fix has to be applied once per arm and per game guard. The last three fixes each found a game the other arm had left behind. The weather table feeds SKYAL sky, clouds and fog in every exterior.
- **Related**: OBL-D4-2026-10-21-01, ESM audit's WTHR "verified clean" (byte layout is correct; this is shape), #5796 (the WATR equivalent, closed), Existing #6188 (two new hand-indexed reads in the same arms).
- **Suggested Fix**: Make `parse_wthr_skyrim` the Creation decoder for Skyrim, FO4, FO76 and Starfield, with a per-version NAM0/FNAM shape table, and leave `parse_wthr` for Oblivion/FO3/FNV. Extract `read_sky_color_table` and `read_fog_head` helpers. Keep the per-game WTHR tests as the pin.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
