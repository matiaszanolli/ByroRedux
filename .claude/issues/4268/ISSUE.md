# SF-2026-09-11-D2-01: convert_bs_geometry_skin_weights passes .mesh bone indices through unbounded, breaking an invariant render/skinned.rs documents as structural

**Issue**: #4268 — https://github.com/matiaszanolli/ByroRedux/issues/4268
**Labels**: medium,nif-parser,nif,game:starfield,legacy-compat,bug

**Severity**: MEDIUM
**Dimension**: Dimension 2 — BSGeometry Mesh Extraction
**Location**: `crates/nif/src/import/mesh/skin.rs:337-364 (convert_bs_geometry_skin_weights)`
**Status**: NEW

## Description
`convert_bs_geometry_skin_weights` is the per-vertex bone-index producer for Starfield's `.mesh`-sourced `BSGeometry` skinning data. Unlike its sibling BsTriShape/NiTriShape decoders, it writes `idx[slot] = bw.bone_index` directly from the `.mesh` file's raw bone-weight records with no bound check against the shape's actual bone-palette size — the one per-vertex bone-index producer in the codebase that passes indices through unbounded.

## Evidence
`crates/nif/src/import/mesh/skin.rs:356-361`: `for (slot, bw) in sorted.into_iter().take(4).enumerate() { idx[slot] = bw.bone_index; w[slot] = bw.weight as f32 / 65535.0; }` — no comparison of `bw.bone_index` against the bone count/palette size before writing. `render/skinned.rs` documents the bone-palette bound as a structural invariant that every producer is expected to uphold.

## Impact
Not reachable on vanilla retail Starfield content (measured during this audit), but a malformed or hostile `.mesh` file (or future mod content) with an out-of-range bone index would violate the documented invariant and could cause an out-of-bounds palette read on the render side, since nothing upstream clamps or validates it here.

## Related
None filed.

## Suggested Fix
Clamp or reject out-of-range `bw.bone_index` values in `convert_bs_geometry_skin_weights`, mirroring whatever bound-check pattern the other per-vertex bone-index producers (BsTriShape/NiTriShape paths) already use.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed by audit-publish from docs/audits/AUDIT_STARFIELD_2026-09-11.md — findings verified against live code during this publish run.*
