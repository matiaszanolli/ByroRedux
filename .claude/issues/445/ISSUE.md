# Issue #445

FO3-6-02: DLC master-file FormID mod-index remapping missing — multi-plugin collisions

---

## Severity: High

**Location**: `crates/plugin/src/esm/cell.rs:320-329`, `crates/plugin/src/esm/records/mod.rs:97`, `crates/plugin/src/esm/reader.rs:175,398-401`

## Problem

`read_file_header` collects `master_files: Vec<String>`, logs the count, then discards the list. Every REFR/CELL/STAT form ID is stored as the raw u32 from the file. In ESM format, the top byte is a **mod-index into the MASTERS array of the containing plugin**.

- Fallout3.esm has 0 masters → mod-index 0x00 == itself. Single-plugin loads work by coincidence.
- DLC plugins (Anchorage.esm, BrokenSteel.esm, PointLookout.esm, ThePitt.esm, Zeta.esm) each list Fallout3.esm as master #0 — their own new forms use mod-index 0x01.
- REFR `0x01000ABC` from Anchorage and REFR `0x01000ABC` from BrokenSteel collide in the same `statics: HashMap<u32, StaticObject>`.

## Impact

- `--esm Anchorage.esm` alone: works (1 master).
- Any multi-master load: silent overwrite on form-ID collision.
- Blocks ALL DLC support (FO3 has 5) and ALL mod support.

`LegacyLoadOrder::resolve` (`crates/plugin/src/legacy/mod.rs:170`) has the mapping machinery but no caller invokes it from the cell loader.

## Fix

Thread `FileHeader.master_files` from `parse_esm_cells` / `parse_esm` down to each record. Store every form ID as `FormIdPair(plugin_id, local_id)` on insert. Use `LegacyLoadOrder` (already exists) to normalize at lookup.

Until this lands: document `--esm` as single-plugin only.

## Completeness Checks

- [ ] **TESTS**: Two-plugin load (Fallout3.esm + Anchorage.esm) with overlapping form IDs — assert no collision
- [ ] **SIBLING**: Same remap needed for Oblivion, Skyrim, FO4, Starfield — this is a cross-game bug, not FO3-specific
- [ ] **DOCS**: Update `modern_plugin_system.md` memory with current state and goal

Audit: `docs/audits/AUDIT_FO3_2026-04-19.md` (FO3-6-02)
