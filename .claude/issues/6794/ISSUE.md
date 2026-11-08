# ESM-5-02: CELL `XCCM` on FO76 and Starfield is a REGN ("Sky/Weather from Region"), decoded as a CLMT climate override

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6794
Labels: low, esm-plugin, bug, game:fo76, game:starfield
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

**Related**: Extends closed #5463 (TES5/FO4 `XCCM` = REGN) to FO76 and Starfield.

- **Severity**: LOW
- **Dimension**: CELL walkers (checklist: XCCM per game)
- **Record / Sub-record**: `CELL XCCM`
- **Location**:
  - `crates/plugin/src/esm/cell/helpers.rs:213-224` (the comment still claims "FO76 / Starfield have no verified definition")
  - `crates/plugin/src/esm/cell/helpers.rs:337-339` (`xccm_targets_region`: `Skyrim | Fallout4` only)
- **Status**: NEW (the original #5463 fix, closed, left these two games out deliberately)
- **Description**: xEdit defines the sub-record as a region on both games. FO76 (current `FO76.pas:10724`, older `wbDefinitionsFO76.pas:8059`): `wbFormIDCk(XCCM, 'Sky/Weather from Region', [REGN])`. Starfield (older `wbDefinitionsSF1.pas:10685`): `wbFormIDCk(XCCM, 'Cell Sky Region', [REGN])`. The current `SF1.pas` copy has no CELL XCCM, which is why the verification was missing. The code puts both games on the pre-Skyrim CLMT branch, so `climate_override` receives a REGN FormID.
- **Evidence** (raw master scan, target record type resolved through the FormID to type map):
  - Starfield.esm: 36 CELLs author XCCM, 36/36 target `REGN`.
  - NW.esm (FO76): 41 CELLs, 41/41 `REGN`.
  - Fallout4.esm 79/79 and Skyrim.esm 214/214 REGN, already handled.
- **Impact**: The regression class named in the skill ("a REGN id reaching `climate_override`"). `climate_override` is read in `byroredux/src/scene/world_setup.rs:754,895` and `env_translate/region_weather.rs`. The lookup against `index.climates` misses, so the best case is a wasted lookup plus a once-per-grid-cell warn, and the region never reaches `sky_region`. No wrong weather results today; it is a decode that is wrong at the source.
- **Suggested fix**: Make `xccm_targets_region` return true for `Skyrim | Fallout4 | Fallout76 | Starfield` (only TES4 and FO3/FNV are CLMT). Add the FO76 and Starfield cases to `parse_cell_tes5_fo4_xccm_is_a_region_not_a_climate`. Update the `helpers.rs` comment and the audit-esm skill text ("no verified xEdit definition").

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
