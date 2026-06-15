**Severity**: HIGH · **Dimension**: NPC Equip + FaceGen (M41)
**Location**: `crates/nif/src/import/mesh/sse_recon.rs:279-285, 306-309, 337-342` (direct indexing inside the per-vertex decode loop)
**Source**: AUDIT_SKYRIM_2026-06-14 (SK-D3-01)

## Description
`decode_sse_packed_buffer` slices each vertex into a fixed-length sub-slice `bytes[base..base+vertex_size]`, then walks it with an `off` cursor. Position/UV/skin-weight reads use the bounds-checked `read_f32_le`/`read_u16_le` (which return `None` via `?`), but the **normal, tangent, vertex-color, and skin-index** reads use direct slice indexing (`bytes[off]`, `bytes[off+1]`, … `bytes[off+11]`), which panics on OOB. `vertex_size` (`skin.rs:224`) and `vertex_desc` (`skin.rs:225`) are independent raw file fields with no cross-validation; the only structural check is `is_multiple_of(vertex_size)`, which guarantees the stride *divides* the buffer but not that it's *large enough* for the declared attribute mask. The function's own `if off > vertex_size { return None; }` guard fires only at end-of-loop, after the indexing has already panicked.

## Evidence
Confirmed in live code — `bytes[off]`/`bytes[off+1]`/`bytes[off+2]`/`bytes[off+3]` at the normal block (279-285), `bytes[off..off+3]` at vertex-colors (306-309), and `bytes[off+8..off+11]` at skin-indices (337-342) are all unguarded, while the interleaved `read_u16_le(bytes, off)?` weight reads are guarded. Contrast: the inline decoder (`bs_tri_shape.rs`) reads through `NifStream`'s bounds-checked methods, so it yields `Err` (never panics) on bad input.

## Impact
This is the SSE NPC-body / FaceGeom reconstruction path that M41 drives for every Skyrim+ NPC body. A truncated or corrupt partition buffer crashes the cell loader (hard process panic) instead of skipping the shape. Not reachable on consistent vanilla data, so day-to-day risk is low, but the failure mode is a hard crash, and modded / LE→SE-converted content is the realistic trigger.

## Related
SK-D1-AUDIT-01 (a *different* SSE-recon defect — strip-authored partition drop). Distinct sites, distinct symptoms.

## Suggested Fix
Replace the raw byte reads with `bytes.get(off)…?` (or add an `off + needed > vertex_size` check at the top of each attribute block) so malformed geometry returns `None` and the shape is skipped, matching the inline path's fail-soft behaviour.

## Completeness Checks
- [ ] **SIBLING**: All four unguarded blocks (normal, tangent, vertex-color, skin-index) converted to bounds-checked reads
- [ ] **TESTS**: A regression test feeds a truncated partition buffer and asserts `None` (no panic)
