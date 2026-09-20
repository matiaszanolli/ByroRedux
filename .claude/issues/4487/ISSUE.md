# EXT-D5-2026-09-19-03: WRLD DNAM/NAM4 water heights parsed with no finite gate — NaN propagates into spawn

- **ID**: EXT-D5-2026-09-19-03
- **Labels**: medium,water,esm-plugin,bug
- **Filed from**: docs/audits/AUDIT_EXTERIOR_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4487

**Severity**: MEDIUM · **Dimension**: WATAL · **Tier Violated**: no-fabrication (a non-finite value must not become a canonical height) · **Game Affected**: FO3 / FNV / Skyrim / FO4 / FO76 / Starfield (`DNAM`/`NAM4` eras; Oblivion unaffected)
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D5-2026-09-19-03)

**Location**: `crates/plugin/src/esm/cell/wrld.rs:149-153` (`DNAM` → `default_water_height`), `:173-178` (`NAM4` → `lod_water_height`); consumers `byroredux/src/cell_loader/exterior.rs:72-81`, `cell_loader/water.rs:888-921`

**Description**
`xclw_water_height` rigorously gates the cell-level height (finite + |h| < 1e9 near-sentinel rejection, tests at `helpers.rs:120-141`), but the worldspace-default height and LOD height are raw `f32::from_le_bytes` with no gate. A corrupt/hostile `DNAM`/`NAM4` yields `Some(NaN)` (or `Some(±huge)`), which flows through `resolved_exterior_water_height` into every water-less cell of the worldspace: `spawn_water_plane` inserts a NaN `Transform` and NaN-min/max `WaterVolume` (NaN `<=` compares cull every triangle → the plane is silently suppressed → dry ocean), and `spawn_lod_water_plane` uploads NaN vertex Y positions to the GPU.

**Impact**
Hostile/corrupt plugin data only; no panic. Whole-worldspace dead water or garbage LOD geometry — while the identical hazard is fully gated one record over (XCLW).

**Suggested Fix**
Route both reads through the same gate as `xclw_water_height` (finite, |h| < 1e9, else `None`); add a parse test mirroring `xclw_short_or_nonfinite_is_none`.

## Completeness Checks
- [ ] **SIBLING**: Grep the WRLD walker for other raw `f32::from_le_bytes` record fields feeding transforms
- [ ] **TESTS**: Parse tests for NaN/Inf/huge `DNAM`/`NAM4`
