# ESM-SD-2026-10-19-02: `merge_cell_override` resurrects CELL-record fields the winning override dropped

Labels: medium,esm-plugin,bug,game:skyrim,game:fo4

## Description
A plugin that overrides a CELL carries the whole CELL record (the CK rewrites it); the winning override replaces the record, so a sub-record absent from the override is absent on purpose. `merge_cell_override` (`crates/plugin/src/esm/cell/index.rs`) instead fills every `None` / empty field from the base: `water_type_form`, `regions`, `location_form`, `lighting`, `image_space_form`, `acoustic_space_form`, `music_type_*`, `climate_override`, `precombined_mesh_hashes`, and it **extends** `absorbed_ref_bakes` from the base unconditionally. Only the child-record inheritance (`references`, `landscape`, `navmeshes`; design from #1546) matches how the data is authored. The field set is also incomplete in the other direction: `encounter_zone_form` and `pathgrids` are not inherited at all, so the policy is neither engine-faithful nor consistent, and unlike `EsmCellIndex::merge_from` (#4904) the function has no exhaustive destructure to force a decision when a field is added.

## Evidence
Sub-record sets of each override CELL compared with its base (union over earlier plugins):
- Skyrim SE: 2,982 override CELLs; the override omits `XCWT` in 39 (Update.esm 28: `DefaultMarshWater` x15, `RiverWaterFlowNE` x12, `DefaultCreekWater` x1; Dragonborn 5, Dawnguard 4, HearthFires 2), while 394 other Update.esm overrides keep `XCWT`. Also `XLCN` x3, `XCLR` x3 (HearthFires and Dragonborn on the Tamriel persistent cell `0x0D74`), `XEZN` x1 (Update.esm `0x16BCF`, consumed by `scripting/src/encounter_level.rs:98` and `obscript_more/tail.rs:41`, never inherited).
- Sample: Skyrim.esm `0x91A3` has `DATA EDID LTMP MHDT TVDT XCLC XCLR XCLW XCWT`; Update.esm's override has the same set minus `XCWT`; every other field re-emitted, so the record is whole and the removal authored.
- FO4: DLCNukaWorld.esm override of `Fallout4.esm` `0x1F7A9` drops `XCRI` (the CK does this when a baked reference is edited, to disable the cell's precombine). The merge keeps the base `precombined_mesh_hashes` and extends `absorbed_ref_bakes`, so the base's combined mesh loads and its absorbed refs are skipped, hiding the DLC's edit. Other FO4 drops: `PCMB` 32, `VISI` 23, `RVIS` 22, `XCCM` 1 (previs data, undecoded).
- FO3: `XCET` x3 (undecoded); FNV: none.
- Pinned the other way by `cell/tests/merge.rs::merge_from_partial_exterior_override_preserves_authored_payloads`.
- Disproofs: the base `XCWT` is a different WATR from the worldspace default in 28 Update cells, so the patch is what changed it; the walker genuinely reads the sub-record as absent (`walker_equivalence.rs`); the census (2,982 Skyrim, 752 FO4, 351 FO3, 73 FNV overrides) found no override missing `DATA`/`XCLC`/`XCLL`/`LTMP`/`XCLW`, i.e. no partial CELL records, so the only fields the inheritance ever fills are deliberate removals. Not measured: the engine's fallback for a missing `XCWT`.

## Impact
Wrong water type in at least 39 Skyrim exterior cells (the patch's marsh/river assignment is undone); 3 wrong location refs, 3 wrong region lists, 1 wrong encounter zone; on FO4, a cell whose override breaks its precombine still renders the stale combined mesh (1 vanilla DLC cell, and every modded cell that edits a baked ref, where it hides the edit).

## Related
ESM-SD-2026-10-19-01 (same merge), #1546, #4904, #5484.

## Suggested Fix
Limit the inheritance to the child-record vectors (`references`, `landscape`, `navmeshes`, `pathgrids`, `deleted_refs` handling) and let every CELL-record field come from the winning override; if a deliberate exception is wanted, make it an explicit per-field list behind an exhaustive destructure of `CellData`, with a test per field.

Source: docs/audits/AUDIT_ESM_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers/builders of the same field, other games sharing the code path)
- [ ] **TESTS**: A regression test pins this specific fix
