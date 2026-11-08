# ESM-D3-01: WEAP `ETYP` is an s32 enum on FO3/FNV but is routed through `remap_fid` (inverse shape 4)

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/6786
Labels: low, esm-plugin, bug, game:fnv, game:fo3
Source: docs/audits/AUDIT_ESM_2026-11-08.md

_Filed from `docs/audits/AUDIT_ESM_2026-11-08.md` (audit HEAD 3500d2737)._

- **Severity**: LOW (latent - nothing reads `equip_type` outside `items.rs`)
- **Dimension**: 3
- **Record/Sub-record**: WEAP / ETYP, GameKind::Fallout3NV
- **Location**: `crates/plugin/src/esm/records/items.rs:593-595` (arm), field doc `items.rs:233-236`
- **Status**: NEW (the arm is #3715's Skyrim fix, never game-gated; the field was renamed `skill_form` -> `equip_type` by
  `a9748e54c`, #6350, which corrected the doc but left the remap)
- **Description**: The arm is ungated:
  `b"ETYP" => { equip_type = remap_fid(SubReader::new(&sub.data).u32_or_default(), remap); }`.
  xEdit FNV and FO3 define it as `wbInteger(ETYP, 'Equipment Type', itS32, wbEquipTypeEnum)`
  (`wbDefinitionsFNV.pas:3464`, `wbDefinitionsFO3.pas:2864`). Only Skyrim/FO4/FO76/SF define it as an `EQUP` FormID.
  The field's own doc comment says "on FO3/FNV an enum ... not a form at all", and the test
  `fallout_dnam_decodes_the_weapon_skill` asserts `equip_type == 3` only because it passes `&None`.
- **Evidence**: Scanned the WEAP records of the shipped masters (`etyp.py` in the session scratchpad):
  FalloutNV.esm 261 WEAP, ETYP values `{0:41, 1:49, 2:69, 3:59, 4:19, 5:15, 6:8, 7:1}`;
  Fallout3.esm 160 WEAP, `{0:27, 1:26, 2:44, 3:43, 4:8, 5:7, 6:5}`. Every value is a small enum, none a FormID.
  `FormIdRemap::remap` (`reader.rs:562-611`): a masterless plugin has `mod_index 0 == master_slots.len()`, so
  `3` composes to `plugin_slot << 24 | 3`. A masterless FNV/FO3 plugin loaded at slot N>0 gets `0x0N000003`. A plugin whose first
  master is not at slot 0 gets the same corruption. The `(raw == 0 -> 0)` short-circuit hides enum value 0 only.
  Vanilla masters at slot 0 happen to map identically, so the bug is invisible on a vanilla-only load.
- **Impact**: Wrong `equip_type` for mod-added FO3/FNV weapons in a non-trivial load order. No runtime consumer reads
  it today, so no visible failure yet. It re-opens as soon as the equip/VATS layer reads the field.
- **Suggested fix**: Branch on game. FO3NV stores the raw s32 (preferably in its own field or an enum, not the FormID
  slot). Skyrim+ keeps `remap_fid`. Add a `weap_etyp_enum_stays_raw_under_remap` test modelled on
  `pkin_vnam_version_stays_raw_under_remap`, using a non-identity remap on a FO3NV record.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms
- [ ] **LOCK_ORDER**: n/a unless a RwLock scope changes
- [ ] **TESTS**: A regression test pins this specific fix (real-data pin where an install exists)
