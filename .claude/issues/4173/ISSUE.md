# D7-2026-09-11-02: CELL's XEZN (encounter zone) sub-record is never parsed anywhere in the crate

URL: https://github.com/matiaszanolli/ByroRedux/issues/4173
Labels: bug, medium, esm-plugin, doc-rot

---

**Severity**: MEDIUM
**Dimension**: ESM→ECS Handoff
**Record / Sub-record**: `CELL` / `XEZN` (missing); cross-referenced type `ECZN`
**Location**: `crates/plugin/src/esm/cell/walkers.rs` (CELL sub-record match arms — every sibling `XCxx`/`XLxx` field present, `XEZN` absent from both, falls into the `_ => {}` catch-all); doc gap at `docs/engine/lighting-from-cells.md:513-516`; `EcznRecord` at `crates/plugin/src/esm/records/misc/world.rs:1183-1200`
**Status**: NEW

**Description**: `EcznRecord`'s own doc comment states encounter zones govern "spawn scaling / faction ownership on the cells that reference it via `XEZN`", and `ECZN` records are fully parsed, remapped and tested. But the CELL-side half — the `XEZN` sub-record that would populate a FormID field to look that map up by — has no match arm anywhere (`grep -rn 'b"XEZN"' crates/plugin/src/` returns nothing), unlike every other Skyrim-era `XCxx` CELL field (`XCIM`, `XCWT`, `XCAS`, `XCMO`, `XCCM`, `XLCN`, all present and remapped). A separate doc, `docs/engine/lighting-from-cells.md:513-516`, incorrectly claims this metadata lives on `CellData`/`CellOwnership` — neither struct has such a field.

**Evidence**: Confirmed via `grep -n "XEZN\|XCIM\|XCWT\|XCAS\|XCMO\|XCCM\|XLCN" crates/plugin/src/esm/cell/walkers.rs` — every sibling arm present, `XEZN` absent, falling to the catch-all.

**Impact**: Latent — no consumer reads `EsmIndex.encounter_zones` today, so nothing renders wrong now. But the sub-record is silently absorbed with no debug log, no TODO, and no "not yet parsed" doc mention, so a future encounter-zone/leveled-spawn feature will find the zone map populated but silently un-linkable to any cell, with a doc actively pointing at fields that don't exist.

**Related**: none filed; same "guard/doc says covered, code doesn't cover it" class as #4084/ESM-2026-09-11-D4-01's history.

**Suggested Fix**: Add `pub encounter_zone_form: Option<u32>` to `CellData`, add a `b"XEZN" => encounter_zone_form = read_form_id(reader, &sub.data)` arm next to the `XCCM`/`XLCN` arms in both CELL match blocks, and correct the `lighting-from-cells.md` doc reference.

## Completeness Checks
- [ ] **TESTS**: A fixture CELL record with an authored `XEZN` sub-record pins the new field

