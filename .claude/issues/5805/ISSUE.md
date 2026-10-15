# TD1-2026-10-15-06: `crates/plugin/src/esm/cell/mod.rs` crossed 2000 production LOC (1935 → 2108); it is ~85% record data types

Labels: low,esm-plugin,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD1-2026-10-15-06`).

- **Severity**: LOW
- **Dimension**: 1 - File / Function / Module Complexity
- **Location**: `crates/plugin/src/esm/cell/mod.rs:1-2108`
- **Status**: NEW (watch-listed since 10-05 at 1866)
- **Age**: +173 since 10-12: `3b28dca71` (#5463 XCCM), `e32304ec9` (#5489), `18b3d87a7` (XRGD pose, #5015), `745d64830` (activate parents), `a0045b0cf` (destruction stages)
- **Effort**: medium
- **Description**: after the earlier split (`helpers`, `support`, `walkers`, `wrld`) `mod.rs` is the type hub: lighting (`CellLighting`, `StarfieldLighting`, `decode_starfield_lighting_tail`, L31–197), landscape (L198–254), `CellData` (L255–451, 33 fields), the placed-reference family (`PlacedRef` 230 lines + `RagdollPoseBone`, `LockData`, `TextureSlotSwap`, `TeleportDest`, `PrimitiveBounds`, `LinkedRef`, `ActivateParent`, `PortalLink`, `EnableParent`, L452–927, ~475 lines), base-object data (`LightData`, `DecalData`, `AddonData`, `StaticObject`, `DestructionData/Stage`, `TextureSet`, L928–1323, ~400), worldspace (L1324–1440), and the index with merge logic (`EsmCellIndex` impl 290 lines, `merge_cell_override`, `merge_navmeshes`, `CellLocator`, `CellRef`, L1441–2070, ~630). Functions are all small (largest is `merge_from`, 129 lines).
- **Evidence**: `prod_loc` 2108; item list from `grep -nE '^(pub(\([a-z]+\))? )?(fn|struct|enum|impl|…)'`.
- **Impact**: low per edit but every M47.2 data addition to a placed ref or base object lands here (five commits in three days).
- **Related**: #5309 (closed, walker dedup), `crates/plugin/src/esm/cell/tests/` mirrors the topics already.
- **Suggested Fix**: split by record family, `mod.rs` keeps `pub use` re-exports so no caller changes: `lighting.rs`, `landscape.rs`, `cell_data.rs`, `placed_ref.rs` (the 475-line placed-reference family), `base_objects.rs` (statics/lights/decals/destruction/texture sets), `worldspace.rs`, `index.rs` (`EsmCellIndex` + merge + locator). Hazard: `cell/tests/txst.rs:616` (`include_str!("../mod.rs")`) and the module's own `plugin_loading_doc_pin_tests` (L2111–2216, four `include_str!` hits) read this file's text.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
