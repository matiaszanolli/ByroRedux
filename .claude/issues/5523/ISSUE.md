# #5523: FO4-2026-10-09-D4-02: SCOL CM path is taken verbatim from `MODL` (114 stale DLCRobot MODLs), and the documented "absent CM → expand" fallback does not exist

**Labels**: bug, esm-plugin, game:fo4, legacy-compat, low

**Source**: `docs/audits/AUDIT_FO4_2026-10-09.md` — finding `FO4-2026-10-09-D4-02` · suite `/audit-suite --preset streaming-deep` (2026-10-09) · HEAD `3bcf6c8e8`

- **Severity**: LOW. Today's reach is 1 + 13 unbaked placements, plus the `pc_spawned == 0` fallback path.
- **Dimension**: 4, SCOL expansion.
- **Location**:
  - `byroredux/src/cell_loader/refr.rs:689-692`: the doc claim.
  - `byroredux/src/cell_loader/refr.rs:720-724`: the `must_expand` gate.
  - The SCOL `MODL` → `statics[base].model_path` mapping (`crates/plugin/src/esm/cell/support.rs:73`).
- **Status**: NEW.
- **Description**:
  - The expansion gate only tests `model_path.is_empty()`.
  - The doc says that "vanilla SCOLs whose CM file is absent under a previsibine-bypass loadout hit the expansion branch".
    They do not. An absent CM file is an `extract_mesh` miss, and the REFR spawns nothing.
  - The authored `MODL` is also not always where the CM ships.
- **Evidence** (probes `scolderive` and `scolcheck`; Python `scol_reach2.py`, `scol_cells.py` and `lair.py`):
  - **Across all seven masters**: 3,509 SCOLs that each master defines itself. Of those, 3,375 `MODL` paths resolve.
    **114 (all DLCRobot)** carry dev-plugin paths, such as `SCOL\Gravato_DLC01SCOL.esp\CM0000F3AD.NIF` and the
    `rwisnewski_DLC01.esp` and `scornett-DLC01.esp` folders. Those paths miss, but
    `meshes\scol\dlcrobot.esm\cm<local id>.nif` resolves for all 114. 19 SCOLs resolve under neither name.
  - **Stale-MODL reach**: 279 DLCRobot REFRs place stale-MODL SCOLs: `DLC01Lair01` 259 and
    `DLC01FortHagenSatelliteArray01` 20. 278 of those are XCRI-baked, which is why the stale path is invisible in the
    shipped game. One `DLC01Lair01` placement is not baked.
  - **Absent-CM reach**: 13 REFRs place absent-CM SCOLs: DLCRobot 6, DLCCoast 1 and DLCNukaWorld 6.
- **Impact**:
  - Today the stale-MODL refs render nothing. Because of FO4-2026-10-09-D1-01 that happens to avoid a double draw.
  - Once D1-01 is fixed, the baked refs are correctly skipped. The 1 unbaked stale-MODL placement and the 13 absent-CM
    placements still render nothing.
  - If a cell's CSG is unavailable (`pc_spawned == 0`), all 279 vanish.
- **Related**: #585, #1182, and FO4-2026-10-09-D1-01.
- **Suggested Fix**:
  - On an extract miss for a SCOL base, try the derived `scol\<owner>\cm<local:08x>.nif`.
  - If that also misses, fall back to `expand_scol_placements`.
  - Correct the `refr.rs:689-692` doc.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other walkers / record decoders / games / call sites)
- [ ] **TESTS**: A regression test pins this specific fix
