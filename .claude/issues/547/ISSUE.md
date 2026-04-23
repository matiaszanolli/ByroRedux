# NIF-02: NiAdditionalGeometryData missing from dispatch (4,039 FO3+FNV blocks)

**Severity**: CRITICAL
**Dimension**: Coverage Gaps × Import Pipeline
**Game Affected**: FO3, FNV
**Audit**: docs/audits/AUDIT_NIF_2026-04-22.md § NIF-02

## Summary

`NiAdditionalGeometryData` carries per-vertex auxiliary channels — nif.xml defines tangents, bitangents, blend weights, optional skin-bone IDs. These are the channels the renderer needs for normal-mapped specular on FO3/FNV architecture. 4,039 blocks in vanilla FO3+FNV archives fall into `NiUnknown` because there is no dispatch arm; the geometry side of the pipeline sees zero aux data.

## Evidence

- `/tmp/audit/nif/fnv_unk.out`: 2,308 blocks
- `/tmp/audit/nif/fo3_unk.out`: 1,731 blocks
- Zero on Skyrim (BSTriShape replaced it) and Oblivion (predates it)

## Location

- `crates/nif/src/blocks/mod.rs` — no dispatch arm
- `crates/nif/src/blocks/tri_shape.rs` — no parser

## Suggested fix

Add `NiAdditionalGeometryData` parser per nif.xml line 4721. Structure: `{ num_vertices, num_block_infos, block_infos[], blocks[] }`. Wire the tangent/bitangent channels into the same `MeshHandle` vertex-attribute slot used by BSTriShape today so the shader sees a unified tangent source. ~80 LOC parser + dispatch arm.

## Completeness Checks
- [ ] **SIBLING**: Verify `NiTriShape::extract_mesh` picks up the new channels via the existing tangent/bitangent slot
- [ ] **TESTS**: Synthetic `NiAdditionalGeometryData` round-trip + integration with `NiTriShape` geometry
- [ ] **REAL-DATA**: Both unknown sweeps drop `NiAdditionalGeometryData` to 0

Fix with: /fix-issue <number>
