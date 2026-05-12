# Issue #975

**URL**: https://github.com/matiaszanolli/ByroRedux/issues/975
**Title**: NIF-D1-NEW-01: hkPackedNiTriStripsData ignores Compressed flag — reads f32 vertices when Compressed != 0
**Labels**: bug, nif-parser, medium
**Audit source**: docs/audits/AUDIT_NIF_2026-05-12.md

---

**Source**: `docs/audits/AUDIT_NIF_2026-05-12.md` (Dim 1)
**Severity**: MEDIUM (dormant on vanilla; live on modded / future content)
**Dimension**: Block Parsing
**Game Affected**: All Bethesda (Oblivion onward — anything carrying `bhkPackedNiTriStripsShape`)
**Location**: `crates/nif/src/blocks/collision.rs:942-953`

## Description

Per `nif.xml` lines 3962-3967, `hkPackedNiTriStripsData` carries a `Compressed: bool` field since 20.2.0.7. When `Compressed == 0` the trailing `Vertices` array is `Vector3[]` (12 B/vertex). When `Compressed != 0` the trailing array is `HalfVector3[]` (6 B/vertex IEEE half-floats).

Our parser reads the byte into `_compressed`, discards it, and always reads f32 triplets:

```rust
let num_vertices = stream.read_u32_le()?;
if version >= crate::version::NifVersion::V20_2_0_7 {
    let _compressed = stream.read_byte_bool()?;       // value discarded
}
let mut vertices: Vec<[f32; 3]> = stream.allocate_vec(num_vertices)?;
for _ in 0..num_vertices {
    let x = stream.read_f32_le()?;                     // WRONG when compressed=1
    let y = stream.read_f32_le()?;
    let z = stream.read_f32_le()?;
    vertices.push([x, y, z]);
}
```

## Impact

When a Bethesda content author flips `compressed=1` (rare on vanilla — Bethesda ships uncompressed packed-tristrip collision), the parser over-reads 6 B/vertex and corrupts subsequent fields. `Num Sub Shapes` u16 at line 957 then lands on garbage bytes, allocating gigabytes of HkSubPartData entries or aborting the block.

Outer block_size recovery covers FO3+ (parse-rate metrics stay green) but the collision mesh's vertex buffer is fully scrambled: every collider for that shape ends up at `(NaN, NaN, NaN)` after `unpack_norm_i16` and Havok rejects it. Oblivion has no block_sizes → cascade.

**Risk window**: (a) modded content that flips the flag, (b) any future Bethesda title that defaults to compressed, (c) Starfield collision (bsver 172) where there's no full-archive collision parse-rate gate today.

## Suggested Fix

```rust
let compressed = if version >= V20_2_0_7 {
    stream.read_byte_bool()?
} else {
    false
};
let mut vertices: Vec<[f32; 3]> = stream.allocate_vec(num_vertices)?;
for _ in 0..num_vertices {
    let (x, y, z) = if compressed {
        (
            half_to_f32(stream.read_u16_le()?),
            half_to_f32(stream.read_u16_le()?),
            half_to_f32(stream.read_u16_le()?),
        )
    } else {
        (stream.read_f32_le()?, stream.read_f32_le()?, stream.read_f32_le()?)
    };
    vertices.push([x, y, z]);
}
```

`half_to_f32` already available via `crate::blocks::tri_shape::half_to_f32`. Add a fixture under `crates/nif/tests/` with a hand-built `compressed=1` block to pin the path.

nif.xml notes some content may use non-IEEE-half packing; if a real compressed sample emerges with out-of-AABB decoded vertices, escalate to a HalfVector3 decoder that handles the Bethesda quirk.

## Completeness Checks

- [ ] **SIBLING**: Search for other "byte read into `_var` then discarded" patterns in `collision.rs` (other Havok blocks with optional compressed paths)
- [ ] **TESTS**: Fixture test pinning both compressed=0 and compressed=1 paths byte-for-byte
- [ ] **DRIFT_HISTOGRAM**: After fix, run `nif_stats --drift-histogram` on a Skyrim Meshes0 sweep; the `hkPackedNiTriStripsData` row should show drift=0 across all entries
- [ ] **DOC**: Comment cites nif.xml lines 3962-3967 as the source of truth so a future audit doesn't re-discover the issue

## Audit reference

`docs/audits/AUDIT_NIF_2026-05-12.md` § Findings → MEDIUM → NIF-D1-NEW-01.

