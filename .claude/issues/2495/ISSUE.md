# REN-D9-NEW-03: Stale palette-bound number in the skin_vertices.comp clamp rationale

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2495
**Finding ID**: REN-D9-NEW-03 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 9 — Skinning
**Location**: `crates/renderer/shaders/skin_vertices.comp:141`
**Status**: NEW

## Description
The #651 / SH-6 clamp comment says an unclamped index "would read past `bone_offset + 127` into the adjacent mesh's palette". `MAX_BONES_PER_MESH` was raised to 144 (#1135), so the real boundary is `bone_offset + 143`. The code itself is correct (`min(boneIdx, uvec4(MAX_BONES_PER_MESH - 1u))`); only the prose is stale.

## Evidence
`crates/renderer/src/shader_constants_data.rs:64` → `pub const MAX_BONES_PER_MESH: u32 = 144;`, matching `crates/core/src/ecs/components/skinned_mesh.rs:52`.

## Impact
Documentation only. Flagged because this is a stride/bound comment on a safety clamp, and the M29 failure modes in this dimension are all "two sites drifted and nothing observed it" — a wrong number here is exactly the kind of thing a future reader would trust.

## Related
#651 / SH-6, #1135.

## Suggested Fix
Change `127` to `MAX_BONES_PER_MESH - 1` (avoid re-baking a literal).

## Completeness Checks
- [ ] **TESTS**: N/A (comment-only change)
