# #5463: SKY-D4-2026-10-08-01: TES5/FO4 CELL `XCCM` is "Sky/Weather from Region" (a REGN reference) but is decoded and consumed as a CLMT climate override

**Labels**: low,esm-plugin,terrain-exterior,bug,game:skyrim,game:fo4,legacy-compat
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5463

**Source**: `docs/audits/AUDIT_SKYRIM_2026-10-08.md` — `SKY-D4-2026-10-08-01` (HEAD `00f580e09`)

**Publish note**: Validated at HEAD: `XCCM` still decodes game-blind into `climate_override` (`crates/plugin/src/esm/cell/helpers.rs:187-191`) under the "Skyrim climate override" doc; consumer `resolve_cell_climate` is at `byroredux/src/env_translate.rs:462`.

- **Severity**: LOW
- **Dimension**: 4 — TES5 cell data through the shared CELL walker
- **Location**:
  - `crates/plugin/src/esm/cell/helpers.rs:44-46` and `:187-191`. The doc says "XCCM Skyrim climate override (per-cell CLMT FormID, exterior cells in vanilla — boss arenas…)".
  - Consumer: `byroredux/src/env_translate.rs:442-479` (`resolve_cell_climate`), reached from `scene/world_setup.rs:472` (`apply_cell_climate_override`, exterior cells only).
- **Status**: NEW.
  - #693 (CLOSED) added the decode on the "Skyrim climate" premise, and #2451 (CLOSED) wired the exterior consumer.
  - EXT-D1-2026-10-08-03 covers the WTHS stand-in on the same path, a different defect.
  - ESM D2-02 (2026-10-08) covers Oblivion REGN `CNAM`, also different.
- **Description**:
  - **The definitions differ by game.** xEdit defines `XCCM` as `wbFormIDCk(XCCM, 'Sky/Weather from Region', [REGN])` on TES5 (`wbDefinitionsTES5.pas:4293`) and FO4 (`wbDefinitionsFO4.pas:6078`). Only TES4, FO3 and FNV define it as `'Climate', [CLMT]`.
  - **The code ignores the difference.** It decodes the field game-blind into `climate_override`. Its doc asserts the CLMT/exterior semantics for Skyrim specifically.
- **Evidence**: `xccm.py` byte walk.
  - `Skyrim.esm`: 214 CELLs author `XCCM`. All 214 are interior, and all 214 target a REGN (for example `NightingaleHall01` → `0xC5857`, `ThalmorEmbassy05` → `0xC5853`).
  - `Dawnguard.esm` / `Dragonborn.esm`: 35 / 19, all interior.
- **Impact**:
  - **Vanilla: none.** The only consumer is exterior-keyed, and every vanilla occurrence is interior.
  - **The interior feature is lost.** Interiors that show the sky take their sky and weather from a region. That is what `XCCM` means on TES5, and it is unconsumed.
  - **Modded exteriors.** A modded Skyrim/FO4 exterior that authors `XCCM` would have a REGN id looked up in the CLMT map, log a "not among parsed CLMT" warning, and fall back.
- **Suggested Fix**: Split the decode by game. Keep `climate_override` (CLMT) for TES4 / FO3 / FNV, and add a `sky_region` (REGN) field for TES5 / FO4. Correct both docs. Leave the interior sky-from-region consumer as forward scope, owned by `/audit-exterior`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (WRLD / other CELL sub-records whose target type differs TES4/FO3/FNV vs TES5/FO4)
- [ ] **TESTS**: A regression test pins this specific fix (TES5 XCCM decodes as REGN, not CLMT)
