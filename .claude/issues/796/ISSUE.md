# SK-D1-04 / #796 — SSE skin-partition tangent bytes discarded (sibling of #795)

**Severity**: MEDIUM (sibling of #795)
**Domain**: nif-parser × renderer
**Status**: NEW

## Location
`crates/nif/src/import/mesh.rs:1266-1269` (`decode_sse_packed_buffer`)

## One-line summary
Same bug as #795 / SK-D1-03 but on the SSE skin-partition reconstruction path that powers #559 (NPC body / creature / dragon decode). Stale `#351` comment justifies the skip; that issue closed before M-NORMALS made tangent decode load-bearing.

## Fix shape
Mirror the #795 fix on this code path:
1. Read tangent bytes via `byte_to_normal` (don't `off += 4`)
2. Attach to `DecodedPackedBuffer.tangents` (new field)
3. Route through `try_reconstruct_sse_geometry` → `ReconstructedSseGeometry` → `extract_bs_tri_shape` so `ImportedMesh.tangents` is populated identically to the inline-vertex path
4. **Must land in same change as #795** so rigid-clutter and skinned-actor cohorts produce identical tangent shape

## Audit source
`docs/audits/AUDIT_SKYRIM_2026-05-03.md` finding SK-D1-04.
