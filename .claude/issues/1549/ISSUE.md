**Severity**: MEDIUM · **Dimension**: BSTriShape Packed Geometry + SSE Reconstruction
**Location**: `crates/nif/src/blocks/skin.rs:289-300` (`NiSkinPartition::parse`), consumed at `crates/nif/src/import/mesh/sse_recon.rs:105-138` (`try_reconstruct_sse_geometry`)
**Source**: AUDIT_SKYRIM_2026-06-14 (SK-D1-AUDIT-01)

## Description
`NiSkinPartition::parse` only fills `SkinPartitionEntry.triangles` when `num_strips == 0`; strip-authored partitions are `stream.skip`-ed, leaving `triangles` empty. `try_reconstruct_sse_geometry` builds indices solely from `part.triangles`, so a fully strip-authored skinned shape produces `indices.is_empty()` → `return None` → the whole NPC/creature body fails to reconstruct, with no diagnostic.

## Evidence
`skin.rs:289-300` skips the strip arrays without de-stripping (`for &len in &strip_lengths { stream.skip(len as u64 * 2)?; }`); `sse_recon.rs:105-138` has no strip fallback. Vanilla SSE ships indexed triangles (`num_strips == 0`), so vanilla content is unaffected — but LE→SE-converted and modded meshes that retain strips drop geometry wholesale.

## Impact
A strip-authored skinned body (creature or NPC) renders as nothing, with no WARN to point at the cause. Realistic on modded / converted content; not on vanilla SSE.

## Suggested Fix
De-strip partition strips into triangles during parse (standard triangle-strip → triangle-list expansion), or at minimum emit a WARN when `num_triangles > 0 && part.triangles.is_empty()` so the silent drop becomes diagnosable.

## Completeness Checks
- [ ] **SIBLING**: Check whether other skin-partition consumers assume populated `triangles`
- [ ] **TESTS**: A regression test pins de-strip (or the WARN) on a strip-authored partition
