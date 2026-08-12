# #2698: FO4-D1-01: XPRI `absorbed_refs` form IDs are never load-order remapped, so the precombine de-dup gate silently fails on every DLC / multi-master cell

- **Severity**: HIGH
- **Dimension**: 1 — precombines / ESM cell walk
- **Location**: `crates/plugin/src/esm/cell/walkers.rs:306-312`, `crates/plugin/src/esm/cell/wrld.rs:460-466`
- **Status**: NEW
- **Description**: The CELL walker reads XPRI as raw plugin-local `u32`s and inserts them into `absorbed_refs` verbatim. Every other cross-record FormID in the same walker goes through `reader.remap_form_id(...)` (e.g. `crates/plugin/src/esm/cell/walkers.rs:676` for the REFR base), and REFR record form IDs are remapped in `EsmReader::read_record_header` (`crates/plugin/src/esm/reader.rs:536`). The consumer compares the two directly.
- **Evidence**:
  ```rust
  // crates/plugin/src/esm/cell/walkers.rs:306 — raw, unremapped
  b"XPRI" if sub.data.len() % 4 == 0 => {
      for chunk in sub.data.chunks_exact(4) {
          let fid = u32::from_le_bytes(chunk.try_into().unwrap());
          absorbed_refs.insert(fid);          // <- no reader.remap_form_id
      }
  }
  // byroredux/src/cell_loader/references/mod.rs:422 — compared against a REMAPPED id
  if absorbed_refs.contains(&placed_ref.form_id) { ... }
  ```
  Concrete divergence: load order `Fallout4.esm`(0), `DLCRobot.esm`(1), `DLCCoast.esm`(2). `DLCCoast.esm` declares one master, so its self-referencing forms carry mod byte `0x01` on disk and remap to global slot `0x02`. REFR header ids become `0x02xxxxxx`; the XPRI entries stay `0x01xxxxxx`. The sets never intersect.
- **Impact**: In the documented multi-master invocation (`--master Fallout4.esm --esm DLCCoast.esm …`) no XPRI REFR is suppressed while `spawn_precombined_meshes` still emits the baked geometry — exactly the double-geometry condition `absorbed_refs` exists to prevent. Doubled architecture draws, z-fighting, doubled BLAS/TLAS build cost and VRAM for every precombined DLC cell.
- **Related**: #1590 (sibling fix, path half only), #2063 (interior/exterior gate share).
- **Suggested Fix**: Wrap both XPRI insert sites in `reader.remap_form_id(fid)` — `reader` is already in scope in `parse_cell_group` and the WRLD walker. Add a multi-plugin regression test with a non-identity `FormIdRemap` asserting the absorbed set matches the REFR header ids.

---
**Source**: `docs/audits/AUDIT_FO4_2026-08-12.md` (finding `FO4-D1-01`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/mod.rs`, per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix

