# #1590 — FO4-D1-MED-01: DLC/multi-master precombines resolve the wrong CSG (filename_hash ignored) and a remapped path form-id

**Severity**: MEDIUM · **Dimension**: M49 Precombined Geometry
**Source**: `docs/audits/AUDIT_FO4_2026-06-14.md` (FO4-D1-MED-01)
**Location**: `byroredux/src/cell_loader/precombined.rs:95-99,264-297`; cross-ref `byroredux/src/cell_loader/load_order.rs:41-77`

## Description
Two coupled multi-master assumptions break for DLC cells.

(a) `open_geometry_csg` keys the CSG purely off the loaded plugin's stem (`<esm stem> - Geometry.csg`) and never consults the authoritative `BSPackedGeomObject.filename_hash` (BSCRC32 cross-check is an explicit follow-up in both code and `docs/engine/fo4-csg-format.md:170-172`). The Data dir holds seven distinct CSGs (`Fallout4`, `DLCCoast`, `DLCNukaWorld`, `DLCRobot`, `DLCworkshop01/03`). A DLC cell whose precombined objects reference the base `Fallout4 - Geometry.csg` would be read from the DLC CSG instead.

(b) The `_oc.nif` path is built from `cell.form_id`, which `build_remap_for_plugin` has rewritten to the global load-order index (top byte → `plugin_index`), whereas the on-disk filename uses the cell's plugin-local form id — so the remapped top byte no longer matches the baked filename for DLC cells.

## Evidence
```rust
// precombined.rs — path uses the remapped global form_id
let path = format!("meshes\\precombined\\{:08x}_{:08x}_oc.nif", cell.form_id, hash);
// open_geometry_csg — CSG named for the loaded plugin, hash ignored
let csg_path = dir.join(format!("{stem} - Geometry.csg"));
```
`decode_shared_geom_object` performs no index-range validation (raw u16 → u32), and `ImportedMesh::from_geometry` does not either, so a wrong-blob read yields garbage vertices or out-of-range indices flowing toward mesh/BLAS upload.

## Impact
Base-game (`Fallout4.esm` alone, plugin_index 0, hash 0xddf19a67, unchanged top byte) is correct — the dominant, validated case. DLC / multi-master interior+exterior precombine loads can read the wrong blob (corrupt/dropped geometry) or miss the `_oc.nif` (path mismatch → REFR fallback, which is at least safe). Confined to FO4 DLC content.

## Related
`docs/engine/fo4-csg-format.md:79-81,170-172`; FO4-D1-LOW-01 (#1533, the index-range-check sibling); #1446 (stale CSG doc).

## Suggested Fix
Resolve the CSG by `geom.filename_hash` (reproduce BSCRC32 over candidate `<Plugin>` stems in the load order, cache a `hash → CsgArchive` map) instead of the single loaded-plugin stem; build the `_oc.nif` path from the cell's plugin-local form id (pre-remap); add a `max_index < num_verts` guard in `decode_shared_geom_object` so a wrong-blob read fails closed.

## Completeness Checks
- [ ] **SIBLING**: Same hash-vs-stem assumption checked in any other CSG/precombine resolution site
- [ ] **CANONICAL-BOUNDARY**: Fix stays in the cell-loader / CSG-resolution path; no per-game logic pushed into shaders/renderer
- [ ] **TESTS**: A regression test pins DLC CSG resolution by `filename_hash` and the plugin-local `_oc.nif` path
