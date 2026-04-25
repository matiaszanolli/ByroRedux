# SK-D1-01: BSTriShape bone indices [u8;4] unchecked — multi-partition skins alias to wrong bones silently

## Finding: SK-D1-01

- **Severity**: HIGH
- **Source**: `docs/audits/AUDIT_SKYRIM_2026-04-24.md`
- **Game Affected**: Skyrim SE (Argonian / Khajiit body + worn armor combinations), FO4 (multi-partition power armor)
- **Locations**:
  - Parser: [crates/nif/src/blocks/tri_shape.rs:240, 689-692](crates/nif/src/blocks/tri_shape.rs#L240) — declared as `Vec<[u8; 4]>`
  - Consumer: [crates/nif/src/import/mesh.rs:496](crates/nif/src/import/mesh.rs#L496) — `extract_skin_bs_tri_shape` reads `bone_indices` directly

## Description

`read_vertex_skin_data` reads 4 × `u8` bone indices per vertex per nif.xml `BSVertexData` line 2124, with no awareness of which `NiSkinPartition` partition the vertex belongs to. The 0..255 range is the on-disk spec ceiling.

When a Skyrim SE skinned mesh has more than 256 distinct bones in `NiSkinInstance.bones` — common for Argonian/Khajiit body meshes when worn armor merges into the partition list — the partition splitter reissues local 0..255 indices per partition. Each partition has its own remap table from local→global bone index, stored in `NiSkinPartition.partitions[i].bones`.

The plain BSTriShape path at `mesh.rs:496` keeps the **partition-local** indices and never references which partition a vertex came from, so the importer hands raw `bone_indices` against the **global** bone array. For meshes with > 1 partition the indices alias to the wrong bones — limbs follow the wrong joints, hair binds to torso bones, etc.

## Evidence

Empirical confirmation requires re-running with verbose import on a vanilla Skyrim SE Argonian body NIF. The structural problem is visible at:

```rust
// crates/nif/src/blocks/tri_shape.rs:689-692
let mut bone_indices = [0u8; 4];
for slot in &mut bone_indices {
    *slot = stream.read_u8()?;
}
```

There is no partition-aware remap site between parser and consumer.

## Suggested Fix

Two options:

1. **Promote to `Vec<[u16; 4]>` and remap during partition unpacking** (preferred): in `extract_skin_bs_tri_shape`, walk the `NiSkinPartition.partitions` array and for each vertex apply `partition.bones[local_idx]` to produce a global index. Store as u16 since global counts can exceed 255.

2. **Defensive logging**: at minimum, emit a parser warning when `inst.bone_refs.len() > 256` so the bug surfaces in test runs against vanilla content. Do this regardless of (1) for safety net coverage.

## Related

- #178 (closed): skinning pipeline — landed before the multi-partition case was exercised.

## Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: NiTriShape skinning path through `NiSkinInstance` has the same shape — verify it remaps correctly (likely already does because it stores partition-local indices in NiSkinData rather than inline on the shape).
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Synthetic NiSkinPartition with 2 partitions, each with a unique remap → assert post-import bone indices match remap output, not raw partition-local values.

_Filed from audit `docs/audits/AUDIT_SKYRIM_2026-04-24.md`._
