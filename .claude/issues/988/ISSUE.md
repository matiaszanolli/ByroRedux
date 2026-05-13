# #988 — SK-D5-NEW-09: BSLODTriShape geometry silently dropped by import walker

**Source**: `docs/audits/AUDIT_SKYRIM_2026-05-12.md` § Dim 5
**Severity**: MEDIUM
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/988

## Locations

- `crates/nif/src/import/walk.rs:328-407` — `walk_node_local` (only downcasts `NiTriShape` / `BsTriShape` / `BSGeometry`)
- `crates/nif/src/import/walk.rs:649-713` — `walk_node` (same three arms)
- `crates/nif/src/blocks/mod.rs:306` — parser dispatch (correct, lands `NiLodTriShape`)
- `crates/nif/src/blocks/tri_shape.rs:217-255` — `NiLodTriShape` definition with `.base: NiTriShape`

## Summary

#838 corrected the parser to route `BSLODTriShape` through a dedicated `NiLodTriShape::parse` (distinct from `BsTriShape::parse_lod`). The import walker still only downcasts `NiTriShape` / `BsTriShape` / `BSGeometry`, so the 23 `BSLODTriShape` blocks in Meshes0 parse cleanly but never produce an `ImportedMesh`.

## Fix

Add an `NiLodTriShape` arm in both walkers operating on `&lod.base` (which IS an `NiTriShape`). ~6 LOC + regression test that loads a synthetic `BSLODTriShape` scene and asserts non-empty `ImportedMesh` output.

## Audit-guard reminder

`NiLodTriShape` MUST remain distinct from `BsTriShape` (#838). The fix is to *consume* it, not fold it back.
