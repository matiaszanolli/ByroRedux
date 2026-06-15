**Severity**: HIGH · **Dimension**: Multi-Master Load Order + TES5 Cell-Load
**Location**: `crates/plugin/src/esm/cell/mod.rs:944-952` (`EsmCellIndex::merge_from`)
**Source**: AUDIT_SKYRIM_2026-06-14 (SK-D4-01)

## Description
`merge_from` folds each plugin's cell index into the running index with `self.cells.extend(other.cells)` — a whole-value `HashMap<String, CellData>` overwrite keyed by editor-id. The doc comment states the intent explicitly: "DLC redefining a base cell wins the entire CellData (REFRs, lighting, water level)." But a Bethesda DLC override cell is a **partial** record: the engine is supposed to merge *per-REFR by FormID*, keeping the base game's references and applying only the DLC's added/changed/deleted ones. Replacing the whole `references` vec drops every base REFR the DLC didn't re-emit. The exterior per-grid path has the same defect at `mod.rs:947-952`.

## Evidence
- `mod.rs:945` — `self.cells.extend(other.cells);` (whole-value overwrite) — confirmed in live code.
- Reproduced (temporary probe against real `Skyrim.esm` + `Dawnguard.esm`, since reverted): 57 interior cells overlap by editor-id, and in all 57 the DLC re-emits far fewer refs — `riftenraggedflagon` base=826 → dg=5; `chillwinddepths01` 3153 → 28; `kagrenzel01` 1017 → **0**.
- Runtime spawn (`cell_loader/load.rs:225,237`) consumes the stomped `cell.references` directly — no per-REFR re-resolution downstream to mask it.

## Impact
`--master Skyrim.esm --esm Dawnguard.esm --cell <overridden>` renders near-empty / empty cells for the 57 DLC-overridden interiors (and the exterior equivalent). Multi-master DLC is M46.0's headline use case. Single-plugin cells (Whiterun control bench) are unaffected, which is why prior audits — which validated the FormID *remap* as green — never caught the *merge* stomp.

## Related
M46.0 / #561 (the remap this merge sits beside). Remap math is correct (verified, 41 tests); the defect is purely the merge granularity.

## Suggested Fix
Merge interior cells per-REFR by FormID instead of whole-value — start from the base cell's `references`, then apply the override's adds/changes/deletes keyed on REFR FormID (last-write-wins per REFR, not per cell). Apply the same to the exterior per-grid table.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the exterior per-grid path (`mod.rs:947-952`)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins per-REFR merge (base REFRs survive a partial DLC override)
