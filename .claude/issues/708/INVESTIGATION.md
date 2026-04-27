# Investigation — BSGeometry (Starfield)

## Where the loss happens
[crates/nif/src/blocks/mod.rs:824](crates/nif/src/blocks/mod.rs#L824)
falls every unrecognised block to `NiUnknown` with a debug skip.
`BSGeometry` lands here. Per the checked-in baseline TSV
[starfield.tsv:7](crates/nif/tests/data/per_block_baselines/starfield.tsv#L7):
`BSGeometry  0  190549`.

## Schema is NOT in nif.xml
nif.xml mentions `BSGeometry` in a *comment* inside `NiGeometry`
(line 3855) but does **not** define a `<niobject name="BSGeometry">`
block — the upstream niftools spec has no Starfield wire layout.

## Authoritative source: nifly (Starfield-aware niftools fork)
`/mnt/data/src/reference/nifly/src/Geometry.cpp:1769`
`/mnt/data/src/reference/nifly/include/Geometry.hpp:627`

### BSGeometry::Sync wire layout
```
NiAVObject prefix (net + flags(u32) + transform + collision_ref)
  // BSGeometry has NO properties list (Skyrim+ shape, like BSTriShape)
bounds:           NiBound      (Vector3 center + f32 radius = 16 bytes)
boundMinMax:      f32[6]       (24 bytes)
skinInstanceRef:  BlockRef     (i32, → NiBoneContainer)
shaderPropertyRef: BlockRef    (i32, → BSShaderProperty / BSLightingShader)
alphaPropertyRef: BlockRef     (i32, → NiAlphaProperty)
// up to 4 BSGeometryMesh slots, each gated by a u8 boolean prefix
for i in 0..4:
  testByte: u8  // 1 = mesh present, 0 = absent
  if testByte != 0:
    BSGeometryMesh::Sync()
```

### BSGeometryMesh::Sync (per slot)
```
triSize:  u32
numVerts: u32
flags:    u32
if (BSGeometry.av.flags & 0x200) != 0:
  // Inline mesh data — RARE in vanilla SF (per nifly: "no mesh/morph files yet")
  meshData: BSGeometryMeshData (UDEC3 packed normals/tangents, meshlets,
                                cull data, skin weights — substantial decode)
else:
  meshName: NiString  // length-prefixed (4-byte length), holds 41-char sha1
                     // hex name or human-readable .mesh path
```

The `flags & 0x200` gate lives on the parent `BSGeometry`'s NiAVObject
flags — that's what nifly's `HasInternalGeomData()` queries.

## Sibling parsers reviewed
* [tri_shape.rs](crates/nif/src/blocks/tri_shape.rs) — `BsTriShape::parse`
  is the closest analogue. Uses
  [`NiAVObjectData::parse_no_properties`](crates/nif/src/blocks/base.rs#L138)
  (Skyrim+ shape — no property list, dedicated shader/alpha refs).
  Same pattern fits `BSGeometry`.
* [`NiAVObjectData`](crates/nif/src/blocks/base.rs#L61) — base fields
  (`net`, `flags: u32`, `transform`, `properties`, `collision_ref`).
  `flags` is the field the `0x200` internal-mesh gate reads.

## Available stream helpers
* `read_block_ref()` — i32 → `BlockRef` (mod.rs:429)
* `read_sized_string()` — u32 length + bytes (mod.rs:403)  ← matches
  nifly `meshName.Sync(stream, 4)` (4-byte length prefix)
* `read_ni_point3()` for Vector3 components of NiBound
* `read_f32_le()` × N for the boundMinMax[6] tail

No new helpers needed.

## Files this fix touches
1. New file `crates/nif/src/blocks/bs_geometry.rs` — struct + parser.
2. `crates/nif/src/blocks/mod.rs` — declare module + dispatch arm.
3. `crates/nif/src/blocks/dispatch_tests.rs` — synthetic-bytes regression.
4. `crates/nif/tests/data/per_block_baselines/starfield.tsv` — regen
   (BSGeometry flips from `0/190549` to `190549/0`).

Total: 4 files (within the 5-file scope cap).

## Scope decision for this session

Two scopes are visible:

* **Scoped (recommended):** Parse the external-mesh path (the 99%
  Starfield reality — flag 0x200 unset). Captures bounds, all three
  refs, segment metadata pointer, and the external `.mesh` filename
  per slot. Inline mesh data (`BSGeometryMeshData`) is consumed as
  raw bytes via `block_size` skip when present, so the parser
  doesn't desync the rest of the file.

* **Bundle-with-D5-02-and-D5-08 (multi-session):** the issue's
  recommendation. Adds full `BSGeometryMeshData` (UDEC3 packed
  normals/tangents, meshlets, cull data, skin weights) + paired
  `SkinAttach` + paired `BoneTranslations`. Substantial work; nifly
  itself still flags Starfield support as "initial, no mesh/morph
  files" — so even nifly is in the scoped state.

Recommendation: **scoped**. Lifts the ~190K NiUnknown drops, captures
everything the renderer can use today, and leaves the inline-mesh
decode for when there's a renderer consumer for it.
