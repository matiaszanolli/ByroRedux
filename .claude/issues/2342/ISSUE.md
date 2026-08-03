# FO4-M49-D1-01: Stale exterior-absorption comment in wrld.rs

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2342
**Labels**: low, import-pipeline, documentation
**Source audit**: docs/audits/AUDIT_FO4_2026-08-03.md (Dimension 1 — M49 Precombined Geometry)

**Severity**: LOW
**Dimension**: 1 — M49 Precombined Geometry
**Location**: `crates/plugin/src/esm/cell/wrld.rs:493-503`

## Description

A `CellData` construction comment claims exterior precombine absorption is still dormant pending `#1221`. That landed in `1ed8dc0b`; a later doc-correction pass (`0ace5caf`, "docs(cell-loader): correct stale pre-M49 precombine comments") fixed the equivalent comments in `byroredux/src/cell_loader/{exterior,load,precombined}.rs` but missed this plugin-crate copy.

## Evidence

Current text at `crates/plugin/src/esm/cell/wrld.rs:493-503`:

```rust
// #1220 / D3-NEW-01 — FO4+ PreCombined Mesh
// refs on exterior cells. The cell loader's
// conditional-absorption gate ties XPRI
// honour-vs-ignore to the precombined-spawn
// count; exterior call-site wiring lands
// separately under #1221. Until then these
// fields are populated but the exterior
// loader doesn't yet invoke the
// precombined-spawn pass, so the absorbed
// set is effectively dormant (REFRs render
// as before).
precombined_mesh_hashes,
absorbed_refs,
```

`git log -p -- crates/plugin/src/esm/cell/wrld.rs` shows the comment unchanged since `#1220`. `git show 0ace5caf --stat` touches only `byroredux/src/cell_loader/*`, confirming this file was missed by that correction pass.

## Impact

No runtime effect (comment-only). Risk is future-audit / future-dev misdiagnosis — a reader trusting this comment would wrongly conclude exterior FO4 precombine absorption is unimplemented, when `#1221`/`#1222` landed and exterior cells do invoke the precombine spawn pass with the conditional-absorption gate.

## Related

`#1220`, `#1221`, `#1222`, commit `0ace5caf`

## Suggested Fix

Update the comment to reflect that `#1221`/`#1222` landed — exterior cells invoke the precombine spawn pass and honor the conditional-absorption gate identically to interior cells.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (`byroredux/src/cell_loader/{exterior,load,precombined}.rs` — already corrected by `0ace5caf`; confirm no other plugin-crate copies of this comment exist)
- [ ] **TESTS**: N/A — comment-only fix, no behavior to pin
