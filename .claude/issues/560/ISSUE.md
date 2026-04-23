# #560 — SK-D4-02: TriShape variant identity erased at parse time

**Severity:** HIGH
**Labels:** bug, high, nif-parser, import-pipeline
**Source:** AUDIT_SKYRIM_2026-04-22.md
**GitHub:** https://github.com/matiaszanolli/ByroRedux/issues/560

## Location
- `crates/nif/src/blocks/tri_shape.rs:189-216`
- `crates/nif/src/blocks/mod.rs:217-253`

## One-line
`parse_dynamic`, `parse_lod`, `BSSubIndexTriShape` dispatch all return the same `BsTriShape` struct with hardcoded `block_type_name() = "BSTriShape"`. Importer can't distinguish facegen, LOD, dismember, or plain meshes.

## Fix sketch
Add `pub kind: BsTriShapeKind` enum (`Plain | LOD{l0,l1,l2} | MeshLOD | SubIndex | Dynamic`). Mirror `BsRangeKind` pattern at `node.rs:526`.

## Unblocks
- #404 (BSSubIndexTriShape segmentation)
- #565 (BSPackedCombinedGeomDataExtra LOD batches)

## Next
`/fix-issue 560`
