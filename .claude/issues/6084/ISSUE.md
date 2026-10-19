# ESM-SD-2026-10-19-01: A REFR an override plugin moves into another cell keeps its base copy, so both spawn

Labels: medium,esm-plugin,bug,game:skyrim,game:fo3,game:fo4

## Description
A REFR is globally unique by FormID. When a later plugin overrides a placement and authors it under a different CELL (the CK regroups a moved object under its new cell), the engine has one object at its new position. `EsmCellIndex::merge_from` (`crates/plugin/src/esm/cell/index.rs`) keys placements per cell: the override lands in the new cell's list, and the base copy stays in the old cell's list. The only cross-cell removal is the tombstone sweep (`tombstones` built from `deleted_refs`). Nothing compares an override's placement FormIDs against the other cells' lists, and `cell_loader` has no spawn-time dedup by FormID.

Related, not the same: #3362 (cross-cell **tombstone**, closed; the sweep fixed only deletes) and #1546 (partial override keeps base REFRs).

## Evidence
Independent Python census (no Rust parser), walking GRUP type 6 labels, skipping Deleted records, FormIDs normalised to (originating master, low 24 bits):
- Skyrim SE (Skyrim + Update + Dawnguard + HearthFires + Dragonborn): 22 of 872,414 placements are non-deleted in two cells. E.g. `ACHR 03C57D` Skyrim.esm cell `0x9749` and Dragonborn.esm cell `0x0D74`; `ACHR 084625` Skyrim.esm `0x9291` and HearthFires.esm `0x9292`; REFR `0B60B3`/`0B60B8`/`0B60B9`/`0DDD35` Skyrim.esm `0x7158` to Update.esm `0x7157`.
- FO3 + 5 DLCs: 16 (6 `ACRE` moved from `0x0E13` by PointLookout.esm into its persistent-children cell `0x2DB4`; 10 REFR, e.g. `0A6028` into BrokenSteel's persistent cell).
- FO4 + 6 DLCs: 7 (e.g. `065543` Fallout4.esm `0xE2A9` to DLCworkshop03.esm `0x18AA2`; DLCCoast moves).
- FNV + 4 DLCs: 0 of 427,387. Oblivion.esm alone: 0 (DLC `.esp` not censused).

Code path: `merge_from` interior loop and exterior loop each call `merge_cell_override(base, &mut over)` only against the same key; no path outside `tombstones` removes a `PlacedRef` from a cell that `other` did not author. Disproof attempted: per-plugin check shows every case is one cell per plugin, base vs override; the FormID is identical in both and Deleted-flag records are excluded; the walker derives the cell from GRUP nesting as the census does; no loader-side dedup found. Not verified at runtime (both entities rendering needs an engine run).

## Impact
The stale base copy renders at the old position next to the moved one. For the 2 Skyrim ACHRs and 6 FO3 ACREs this is a duplicated actor/creature: the persistent-cell actor stubs (`prepare_logical_actor_stubs`) and `SceneAliasCandidate` key by FormID, so two live entities share one FormID and `resolve_entity_by_global_form_id` picks one arbitrarily. For a moved door, `locate_refr` scans three `HashMap`s in iteration order, so the destination cell of an XTEL can differ between runs. Mod load orders (and unofficial patches, which move objects routinely) hit this at far higher volume than the 45 vanilla cases.

## Related
#3362, #1546, ESM-SD-2026-10-19-02 (same merge function).

## Suggested Fix
In `merge_from`, before the per-cell loops, collect the placement FormIDs `other` authors per destination cell and `retain` away any copy of those FormIDs in `self`'s other cells (same sweep shape as the tombstone one, destination cell exempt). Pin it with a `merge.rs` test that moves a REFR from cell A to cell B in a child plugin and asserts one copy.

Source: docs/audits/AUDIT_ESM_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers/builders of the same field, other games sharing the code path)
- [ ] **TESTS**: A regression test pins this specific fix
