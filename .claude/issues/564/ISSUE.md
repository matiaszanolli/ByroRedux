# #564 — SK-D4-04: BSPackedCombinedGeomDataExtra body skipped

**Severity:** MEDIUM
**Labels:** bug, medium, nif-parser, import-pipeline
**Source:** AUDIT_SKYRIM_2026-04-22.md
**GitHub:** https://github.com/matiaszanolli/ByroRedux/issues/564

## Location
- `crates/nif/src/blocks/mod.rs:364-379`
- `crates/nif/src/blocks/extra_data.rs`

## One-line
Dispatcher parses the fixed header then `stream.skip(block_size)` over the variable-size per-object + vertex/index pool data. Distant LOD batches on `BSMultiBoundNode` roots produce zero meshes.

## Fix sketch
Option A: parse per-object array + vertex/index pools; import walker generates instanced draws.
Option B: document as M35 gap, skip the host subtree cleanly.

## Depends on
- #560 (SK-D4-02) for `BsTriShapeKind::LOD` discrimination

## Next
`/fix-issue 564`
