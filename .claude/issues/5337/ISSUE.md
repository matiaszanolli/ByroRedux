# #5337: EXT-D5-2026-10-05-03: `build_distant_water_mesh` re-derives the effective cell water height inline, defaulting to `NAM4` where the full-detail cell uses `DNAM` / `default_water_for_worldspace`

**Labels**: low,terrain-exterior,water,game:fo3,bug
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/5337

**Source**: `docs/audits/AUDIT_EXTERIOR_2026-10-05.md` — `EXT-D5-2026-10-05-03` (HEAD `a2c24b16e`)

- **Severity**: LOW (latent in vanilla; a single-boundary dent)
- **Dimension**: Water translation (WATAL)
- **Location**:
  - `byroredux/src/cell_loader/water.rs:935-939` (inline `if cell.explicit { cell.water_height } else { default_height }`); `:1069` / `:1216` (`Some(lod_height)` / `Some(default_height)` = `translate_lod_water`'s `NAM4`).
  - The authoritative rule is `byroredux/src/cell_loader/exterior.rs:71-81` (`resolved_exterior_water_height`), called with `wctx.default_water_height` at `:1955-1959`.
- **Status**: NEW
- **Tier Violated**: single-boundary (two derivations of one quantity, from two different default sources)
- **Game Affected**: FO3 (MegatonWorld); any mod or child worldspace with inherit cells whose `NAM4` ≠ inherited DNAM water
- **Description**:
  - A cell with no XCLW is drawn near at the worldspace default from `DNAM` (Oblivion: 0 via NAM2), and far at `NAM4`.
  - #5243's doc calls `NAM4` "the worldspace default". exal.md §5.4 says the two are "genuinely distinct … on real content (NAM4 ≠ DNAM water on 22/30 Skyrim.esm)". Whenever they differ, an inherit cell's water steps in height at the streaming boundary.
- **Evidence** (census):
  - The main worldspaces agree: Wasteland 10500/10500, WastelandNV −2300/−2300, Tamriel −14000/−14000, Commonwealth 450/450.
  - Skyrim.esm, FalloutNV.esm and Fallout4.esm have **zero** inherit cells in any worldspace.
  - In Fallout3.esm only Wasteland (equal) and MegatonWorld have inherit cells: 6 of them. MegatonWorld inherits Wasteland's DNAM 10500 through PNAM 0x01 but owns `NAM4` = 0.
- **Impact**: None visible in the vanilla main worlds. It is a trap for mods and for the D5-02 Oblivion fallback, which must not copy the inline rule.
- **Suggested Fix**:
  - Have the builder call `resolved_exterior_water_height` with the same `default_water_for_worldspace` default the near cells use.
  - Keep `NAM3`/`NAM4` for what is genuinely LOD-only (the LOD water form, and a sheet for cells absent from the table), or document why `NAM4` is the right inherit height.

## Publisher note

Cross-referenced by `AUDIT_FO3_2026-10-05.md` (MegatonWorld inherits Wasteland's DNAM 10500 but owns `NAM4` = 0).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, sibling call sites)
- [ ] **TESTS**: A regression test pins this specific fix
