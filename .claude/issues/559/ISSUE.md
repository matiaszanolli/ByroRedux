# #559 — SK-D5-02: Skyrim skinned actor bodies import 0 meshes

**Severity:** HIGH
**Labels:** bug, high, nif-parser, import-pipeline
**Source:** AUDIT_SKYRIM_2026-04-22.md
**GitHub:** https://github.com/matiaszanolli/ByroRedux/issues/559

## Location
- `crates/nif/src/blocks/skin.rs:190-197`
- `crates/nif/src/import/mesh.rs:248-250`

## One-line
SSE `NiSkinPartition::parse` reads global vertex buffer header then `stream.skip(data_size)` — discards 500+ KB of real vertex data. Dragons, draugr, all humanoids import as 0-mesh entities.

## Fix sketch
Store the SSE global vertex buffer on `NiSkinPartition`. In `extract_bs_tri_shape`, when `vertices.is_empty()` and `skin_ref` is set, reconstruct from `partitions[i].vertex_map` + global buffer + `vertex_desc` bitfield.

## Next
`/fix-issue 559`
