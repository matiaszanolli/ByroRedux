**Severity**: MEDIUM · **Dimension**: 2 — Memory Corruption / UB / NIF parse
**Location**: `crates/nif/src/import/precombine.rs:109-146` (`decode_shared_geom_object`)
**Source**: `docs/audits/AUDIT_SAFETY_2026-06-14.md` (SAFE-D2-NEW-03; carryover of unpublished 2026-06-11 SAFE-D6-NEW-02)

## Description
The M49 precombine path reads raw u16 triples from the PSG blob (`stream.read_u16_triple_array(tri_count)?`, `precombine.rs:140`) and converts them straight to `u32` indices in a plain push loop (`:141-145`) **without validating any index `< num_verts`**. `num_verts` is in scope and unused for validation. Unlike inline NIF geometry, the PSG slice is located by a `(filename_hash, data_offset)` pointer into a separate `.csg` blob — a hash collision or stale/mispointed offset silently decodes arbitrary bytes as indices (values up to 65535) against an arbitrary vertex count. Unchanged since June 11.

## Evidence
`precombine.rs:140-145` — read then push, no `< num_verts` check; the result flows into `ImportedMesh` → `accumulate_global_geometry`, whose only guard is the log-only diagnostic of SAFE-D2-NEW-02.

## Impact
Producer-side half of SAFE-D2-NEW-02 — a corrupt CSG read becomes OOB draw/BLAS input instead of a rejected object. MEDIUM per the "translatable block / parse mismatch" class; the escalating GPU consequences are owned by SAFE-D2-NEW-02.

## Related
SAFE-D2-NEW-02; `docs/engine/fo4-csg-format.md` (reverse-engineered format, M49); 2026-06-11 SAFE-D6-NEW-02 (never published).

## Suggested Fix
After the read loop, `if indices.iter().any(|&i| i as usize >= num_verts) { return Err(io::Error::new(InvalidData, ...)) }` — one pass, decode-time rejection with the object's hash in the message.

## Completeness Checks
- [ ] **SIBLING**: Same index-bounds check applied to any other raw-index decode path (inline NIF tri readers)
- [ ] **TESTS**: A regression test pins this fix (an OOB index in a synthetic PSG slice is rejected at decode time)
