# Investigation — #795 + #796 (BSTriShape tangent decode)

## Convention research (per `feedback_no_guessing.md`)

Authoritative source: nifly `Geometry.cpp` (cloned at `/mnt/data/src/reference/nifly/src/Geometry.cpp`).

### nif.xml BSVertexData layout
```
Vertex          : Vector3 (full prec) or HalfVector3
Bitangent X     : float / hfloat        ← packed at end of position vec4
UV              : HalfTexCoord
Normal          : ByteVector3 (3 normbytes)
Bitangent Y     : normbyte               ← packed after Normal
Tangent         : ByteVector3 (3 normbytes)  ← present iff VF_TANGENTS+VF_NORMALS
Bitangent Z     : normbyte               ← packed after Tangent
Vertex Colors   : ByteColor4 (RGBA u8)
Bone Weights    : 4 × hfloat             ← VF_SKINNED
Bone Indices    : 4 × byte               ← VF_SKINNED
Eye Data        : float                  ← VF_EYE_DATA
```

Bitangent is stored across 3 non-contiguous offsets within each vertex.

### Bethesda's tan_u/tan_v swap
`Geometry.cpp:1014-1034`:
```cpp
Vector3 sdir = ((t2*x1 - t1*x2)*r, ..., ...);  // ∂P/∂U
Vector3 tdir = ((s1*x2 - s2*x1)*r, ..., ...);  // ∂P/∂V
tan1[i] += tdir;  // ∂P/∂V → "tangent"
tan2[i] += sdir;  // ∂P/∂U → "bitangent"
rawTangents[i] = tan1[i];      // Bethesda calls ∂P/∂V the "tangent"
rawBitangents[i] = tan2[i];    // Bethesda calls ∂P/∂U the "bitangent"
```

So:
- BSTriShape's on-disk `tangent[3]` byte field stores **∂P/∂V**
- BSTriShape's on-disk `bitangent{X,Y,Z}` fields store **∂P/∂U**

Our shader contract (`triangle.frag:566` `vertexTangent.xyz`) wants ∂P/∂U. Therefore the importer must put **bitangent into our tangent slot**, mirroring the existing `extract_tangents_from_extra_data` logic at `mesh.rs:97-104`.

### Bitangent sign (`tangent.w`)
Derived as `sign(dot(B, cross(N, T)))` using raw Bethesda values (T = ∂P/∂V on disk, B = ∂P/∂U on disk). Sign is invariant under proper rotation (det=+1), so the Z-up → Y-up axis swap doesn't flip it.

## Plan (4 changes across 2 files)

### `crates/nif/src/blocks/tri_shape.rs`
1. Add `pub tangents: Vec<[f32; 4]>` field to `BsTriShape` struct (raw Z-up tangent xyz = Bethesda's bitangent; w = sign).
2. Inside the `data_size > 0` gate, capture per-vertex bitangent_x (f32/hfloat), bitangent_y (normbyte already read), tangent triplet + bitangent_z (currently `stream.skip(4)`).
3. Reconstruct `[bx, by, bz, sign]` per vertex, push to `tangents`.

### `crates/nif/src/import/mesh.rs`
4. `extract_bs_tri_shape` (line ~792-803):
   - Apply Z-up → Y-up to `shape.tangents.xyz`, preserve `.w`
   - Fall back to `synthesize_tangents` when `shape.tangents` empty AND `VF_TANGENTS` was clear
5. `decode_sse_packed_buffer` (line ~1266-1269):
   - Decode the 4 tangent bytes the same way
   - Reconstruct bitangent triplet (bitangent_x at vertex start, bitangent_y after normal, bitangent_z after tangent)
   - Add `tangents: Vec<[f32; 4]>` to `DecodedPackedBuffer` and `ReconstructedSseGeometry`
6. Wire `sse_tangents` through `extract_bs_tri_shape` so the SSE-reconstructed path overwrites the placeholder.

## Tests

- `BsTriShape::parse` round-trip on a synthetic vertex stream with `VF_TANGENTS` set — assert `tangents.len() == positions.len()` and that values are not all zero.
- `decode_sse_packed_buffer` round-trip — same shape.
- `extract_bs_tri_shape` synthesize-fallback path — when `VF_TANGENTS` clear, importer should fall back to `synthesize_tangents` (mirroring NiTriShape).

## Visual validation

Out of scope per `feedback_speculative_vulkan_fixes.md` — perturbNormal is gated off via #786. The fix routes data; visual outcome can only be validated once #786's RenderDoc diagnosis lands. cargo test covers the round-trip / data-flow correctness.
