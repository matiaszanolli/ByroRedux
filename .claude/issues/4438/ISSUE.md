# #4438: SF-2026-09-16-D5-01: `parse_txst_group` silently drops Starfield TXST TX08 / TX09 / TX17 / TX19 (metal / rough / AO / opacity)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/4438
- **Labels**: low,esm-plugin,game:starfield,legacy-compat,bug
- **Filed**: 2026-09-16 via /audit-publish

**Source**: `docs/audits/AUDIT_STARFIELD_2026-09-16.md` (texture-roles-deep audit suite, HEAD `7996edf61`)

- **Severity**: LOW
- **Dimension**: 5 (this game's data through the shared parser)
- **Location**: `crates/plugin/src/esm/cell/support.rs:572-604`
- **Status**: NEW
- **Description**: The TXST decode matches `TX00`–`TX07` (plus `MNAM`,
  `DNAM`, `DODT`). Starfield TXST records author PBR maps in higher slots
  that fall through the match with no warning and no census entry:
  - TX08 `_metal`
  - TX09 `_rough`
  - TX17 `_ao`
  - TX19 `_opacity`

  The adjacent TX02 comment ("Starfield's xEdit definition has not seen TX02
  in shipped content") is consistent with the census (0 TX02). The
  higher-slot lanes simply are not modelled.
- **Evidence**: The `Starfield.esm` TXST census found 23 records, with sub-record counts
  TX00 19, TX01 19, **TX08 11, TX09 11, TX17 1, TX19 16**, DODT 20, MNAM 2.
  That is 39 sub-records dropped. Sample:
  `TX19 = decals\puddles\decalpuddlemd01_opacity.dds`. The 12 other
  Starfield masters (ShatteredSpace, SFBGS*, Constellation, OldMars,
  BlueprintShips) carry 0 TXST records.
- **Impact**: Small today. `TextureSet` consumers are LTEX terrain and the
  REFR texture overlays (`byroredux/src/cell_loader/refr.rs:401-417`), and
  Starfield exterior terrain is not live. 20 of the 23 records are decals
  (DODT), so the dropped `_opacity` is exactly the coverage map those decals
  depend on. There is also no canonical sink (D3-01), so this is the ESM-side
  twin of that gap.
- **Related**: SF-2026-09-16-D3-01.
- **Suggested Fix**: Capture the four slots once D3-01's roles exist. Until
  then, warn once per unmodelled `TX*` FourCC, as the parser already does for
  era-gated groups, so the drop is visible.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other games' arms)
- [ ] **TESTS**: A regression test pins this specific fix
