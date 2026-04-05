# Investigation: Issue #71

## Binary Format (Bethesda games, version >= 10.1.0.101)

### NiSkinInstance
- data_ref: BlockRef (→ NiSkinData)
- skin_partition_ref: BlockRef (→ NiSkinPartition)
- skeleton_root_ref: BlockRef (→ NiNode)
- num_bones: u32
- bone_refs: [BlockRef; num_bones] (→ NiNode bone nodes)

### NiSkinData  
- skin_transform: NiTransform (overall skin offset)
- num_bones: u32
- has_vertex_weights: bool (version >= 4.2.1.0, always true for Bethesda)
- per bone:
  - bone_transform: NiTransform (bind-pose bone offset)
  - bounding_sphere: [f32; 4] (center xyz + radius)
  - num_vertices: u16
  - if has_vertex_weights:
    - per vertex: index (u16) + weight (f32)

### NiSkinPartition
- num_partitions: u32
- (SSE has extra vertex data header — skip for now)
- per partition: complex struct (bones, vertex map, weights, strips/triangles)
  → Parse minimally for now, skip partition detail

## Existing References
- NiTriShape.skin_instance_ref — already parsed (line 25)
- BsTriShape.skin_ref — already parsed (line 161)

## Fix
Add parsers for NiSkinInstance + NiSkinData. NiSkinPartition is complex
and the partition detail isn't needed for basic skinning — register it
as a skip-only parser (read num_partitions, skip by block_size).

## Scope
2 files: new skin.rs (parsers), mod.rs (dispatch + imports).
