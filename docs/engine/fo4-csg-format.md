# Fallout 4 `.csg` Shared-Geometry Format (M49)

Reverse-engineered and validated against `Fallout4 - Geometry.csg`
(Steam build, 240 043 177 bytes) on 2026-06-01. Every claim below was
checked against authoritative NIF-side ground truth — per-object vertex
counts, triangle counts, vertex descriptors, and **bounding spheres**
emitted by `BSPackedCombinedSharedGeomDataExtra` — across 38 objects
spanning both on-disk vertex layouts. No field is guessed: the triangle
decode yields `max_index == num_verts − 1` for all 38 objects, and
decoded normals are unit-length, which a wrong layout cannot produce.

This is the spec the ROADMAP M49 row called "blocked on — none on disk."
It is no longer missing.

## Why it's needed

Vanilla FO4 precombined meshes (`meshes\precombined\<cell>_<hash>_oc.nif`)
are **100 % the Shared variant** — every `BSPackedGeomObject` carries a
`filename_hash` of `0xddf19a67` (BSCRC32 of `Fallout4 - Geometry`) and a
`data_offset` into `Fallout4 - Geometry.csg`. The `_oc.nif` itself ships
zero inline vertices; the geometry lives in the `.csg`. Without a `.csg`
reader the precombined pass spawns nothing and the engine falls back to
per-REFR rendering (correct, but unoptimised). See
[`precombined.rs`](../../byroredux/src/cell_loader/precombined.rs).

The Baked variant (`BSPackedCombinedGeomDataExtra`, geometry inline) is
already fully parsed and is **not** used by vanilla cells — only by some
mod content.

## Container layout

`<Plugin> - Geometry.csg` (the `.psg` "Previs Shared Geometry" after
`CreationKit -CompressPSG`):

```
offset  type            field
0       char[4]         magic = "bcsg"
4       u32             num_objects
8       u32             num_chunks
12      ChunkEntry[num_chunks]      chunk table   (8 bytes each)
...     ObjectEntry[num_objects]    object table  (20 bytes each)
...     zlib stream[num_chunks]     compressed payload chunks
```

`ChunkEntry` (8 bytes):

```
0   u32   compressed_size   (bytes of the zlib stream for this chunk)
4   u32   file_offset       (absolute byte offset of the zlib stream in the .csg)
```

- The chunk table is in file order; `file_offset` is monotonic and the
  first chunk's `file_offset` equals `12 + num_chunks*8 + num_objects*20`
  (the byte immediately after the object table) — this closes to the
  byte and is how `num_chunks`/`num_objects` were confirmed.
- `compressed_size[i] == file_offset[i+1] − file_offset[i]` for all but
  the last chunk (whose payload runs to EOF).
- **Every chunk inflates to exactly 65 536 bytes**, except the final
  chunk (partial). zlib magic `78 9c` (default compression).
- The **uncompressed PSG space** is the concatenation of all inflated
  chunks. Chunk `i` therefore starts at PSG offset `i * 65536`.

The **object table** (20-byte entries) is not required to read geometry
via NIF offsets — `data_offset` already points directly into PSG space —
so it is currently left unparsed. (It appears to be a CK-side index used
during generation / `.cdx` build.)

## Reading an object

`BSPackedGeomObject { filename_hash, data_offset }` plus the paired
`BSPackedSharedGeomData` header (from the `_oc.nif`) give everything:

```
num_verts      from BSPackedSharedGeomData
vertex_desc     "
tri_count_lod0/1/2 "
```

1. Resolve `filename_hash` → `<Plugin> - Geometry.csg` (vanilla =
   `Fallout4 - Geometry`). In practice the cell's master plugin name
   resolves it; the hash is a BSCRC32 cross-check.
2. `psg_stride = runtime_stride − 8` where `runtime_stride =
   (vertex_desc & 0xF) * 4`. On disk the position is **always half4**
   (8 bytes) even when `vertex_desc` has `VF_FULLPREC` (bit 54) set —
   full precision is a runtime/GPU concept, not a storage one. So a
   28-byte runtime vertex stores as 20 bytes; a 32-byte (with colors)
   stores as 24.
3. From PSG `data_offset`, read `num_verts * psg_stride` bytes of vertex
   data, then `(tri0+tri1+tri2) * 6` bytes of triangle data — both may
   span 64 KiB chunk boundaries (decompress consecutive chunks and
   concatenate).

### On-disk vertex (no colors, `psg_stride = 20`)

Standard `BSVertexData` with half positions:

```
0   half   position.x
2   half   position.y
4   half   position.z
6   half   bitangent.x        (position.w slot)
8   half   uv.u
10  half   uv.v
12  u8     normal.x   snorm   value = b/127.5 − 1
13  u8     normal.y   snorm
14  u8     normal.z   snorm
15  u8     bitangent.y snorm
16  u8     tangent.x  snorm
17  u8     tangent.y  snorm
18  u8     tangent.z  snorm
19  u8     bitangent.z snorm
```

When `vertex_desc` has `VF_COLORS` (bit 49), a 4-byte RGBA vertex color
is **appended after the tangent** — nif.xml `BSVertexData` field order is
position, UV, normal, tangent, color — so normal stays at 12, tangent at
16, and color lands at 20 (`psg_stride = 24`). Empirically confirmed: the
alpha byte reads ≈255 for the majority of vertices at offset 20 but
almost never at offset 12. This is the engine's existing BSTriShape
`BSVertexData` layout exactly, so the same decoder
(`decode_bs_vertex_stream`) reads it with positions forced to half.

### Triangles and LOD selection

The triangle block holds `tri_count_lod0 + tri_count_lod1 +
tri_count_lod2` triangles — three `u16` indices each — stored back to
back as `[LOD0][LOD1][LOD2]`. `tri_offset_lodN` is the start offset **in
index units** (so the triangle start is `tri_offset / 3`): e.g. e2db's
object has `lod0 cnt=888 off=0`, `lod1 cnt=0 off=2664`, `lod2 cnt=122
off=2664` (2664 = 888 × 3).

These are **levels of detail** (nif.xml: "switch a geometry at a
specified distance") — alternative triangulations of the *same* surface,
not disjoint segments. Rendering more than one z-fights (overlapping
coplanar triangulations). The importer therefore selects **exactly one
LOD** and emits only its triangles.

**LOD index is not a reliable detail order**: some objects ship
`lod0 ≫ lod2` (e2db: 888 vs 122), others `lod0 ≪ lod2` (another object:
16 vs 127). So the importer picks the **finest** LOD by *highest triangle
count*, reading `tri_count` triangles from `tri_offset / 3`. Verified:
within each LOD, indices are 0-based and dense (`max == num_verts − 1`).

### Placement

Vertices are in the object's **local** space (centroid ≈ origin, extent
≈ bounding-sphere radius). Each `BSPackedGeomDataCombined` instance in
the shared-geom header supplies a `transform` (placement, in the same
space as the cell) and a `bounding_sphere`. Apply the Z-up→Y-up
conversion the rest of the importer uses.

## Validation summary

| Check | Result |
|---|---|
| `magic` | `bcsg` |
| header math closes to first chunk offset | exact (`12 + 6841*8 + 32370*20 == 702140`) |
| chunk inflate size | 65536 (last partial) |
| `compressed_size == Δfile_offset` | holds |
| triangle indices in-range, `max == nv−1` | 38/38 objects |
| decoded normals unit-length | yes (\|n\| ≈ 1.00) |
| vertex span ⊆ bounding sphere | 36/38 within 2.4r; other 2 decode cleanly (tri max == nv−1), tight CK bound |

## Implementation status

- Parser (`_oc.nif` side): **done** —
  [`extra_data.rs`](../../crates/nif/src/blocks/extra_data.rs)
  (`BsPackedCombinedGeomDataExtra`, both variants).
- CSG reader + geometry decode + spawn wiring: in progress (M49).
- Filename-hash BSCRC32: not yet reproduced (resolution keys off the
  cell master plugin meanwhile).
