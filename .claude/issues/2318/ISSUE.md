# SK-D1-01: Every Skyrim SE BSDynamicTriShape imports to zero meshes — all NPC FaceGen head/eye/brow/mouth geometry is dropped

**Source audit**: `docs/audits/AUDIT_SKYRIM_2026-08-03.md` (Dimension 1)
**GitHub issue**: #2318

**Severity**: HIGH
**Location**: `crates/nif/src/import/mesh/bs_tri_shape.rs:26-37` (reconstruction gate, `extract_bs_tri_shape`), `crates/nif/src/import/mesh/sse_recon.rs:206-208` (`decode_sse_packed_buffer`'s `VF_VERTEX` bail), `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:515-566` (`parse_dynamic`)

## Description

`BSDynamicTriShape` ships all Skyrim SE head geometry. On this block type,
`VF_VERTEX` (bit 0) is **clear** in `vertex_desc` — positions live in a
trailing `Vector4[]` dynamic array instead of the packed vertex buffer, and
the triangle list lives only on the sister `NiSkinPartition` (`num_triangles
== 0` on the block body). Three sites interact to drop the whole shape:

1. `extract_bs_tri_shape` only attempts SSE reconstruction when **both**
   `shape.vertices` and `shape.triangles` are empty; `parse_dynamic` has
   already filled `shape.vertices` from the `Vector4[]` array, so the
   reconstruction path is never reached, and the shape is then dropped for
   having no triangles.
2. Even if reached, `decode_sse_packed_buffer` bails outright when
   `VF_VERTEX` is clear — exactly the `BSDynamicTriShape` case — which also
   kills the #638 skin-payload fallback.
3. `parse_dynamic` discards the `Vector4`'s `w` lane (`let _w = ...; //
   bitangent-x or unused`), which per `nif.xml` is the **only** source of
   `bitangent_x` once `VF_VERTEX` is clear — so even a fixed (1)+(2) can't
   reassemble the tangent basis without this.

## Evidence

Confirmed against real vanilla data (`Skyrim - Meshes0.bsa`): `femalehead.nif`
— `BSDynamicTriShape verts=996 tris=0 desc=0x0046200021000045` → **imported
meshes: 0**. `facegendata\facegeom\skyrim.esm\00096559.nif` (real NPC head, 6
shapes, 2797 verts/4234 tris) → **imported meshes: 0**. Directly re-confirmed
against current HEAD (1ae86f62) during this audit's publish pass (grep of
all three sites).

## Impact

Skyrim SE NPCs render **headless** — no face, eyes, brows, mouth, or
hair-base geometry on any actor, on the renderer's own control-bench cell (6
named NPCs, WhiterunBanneredMare). Nothing is missing from the parser —
everything needed is already in memory — only the import-boundary plumbing
needs to route it. Undetected through at least three prior audit passes
because the M41 equip smoke test checks entity/component/texture counts, not
head-mesh presence.

## Related

#559, #157, #341, #571, #621, #946, #638, #1225 (this is its root cause),
M41.0 (closed only for the FNV kf-era `NiTriShape` path).

## Suggested Fix

1. Keep the `Vector4` `w` lane instead of discarding it.
2. Make `decode_sse_packed_buffer` handle "positions supplied externally"
   (consume 0 bytes for the position quad when `VF_VERTEX` is clear) instead
   of bailing.
3. Widen `extract_bs_tri_shape`'s reconstruction gate to fire on
   `shape.triangles.is_empty()` alone, keeping the already-parsed
   `shape.vertices` as the position source.

Add a real-data regression test pinning `femalehead.nif → 1 mesh, 996
positions, 5118 indices`.

## Completeness Checks
- [ ] **SIBLING**: Confirm no other `BsTriShapeKind` variant (or FO4/FO76/Starfield equivalents) has the same "positions already filled → reconstruction never reached" gap
- [ ] **TESTS**: A regression test pins this specific fix (`femalehead.nif → 1 mesh, 996 positions, 5118 indices`)
