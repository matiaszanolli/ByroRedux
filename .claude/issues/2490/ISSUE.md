# NIFAL-D6-2026-08-07-04: Raw-material to marker-component block is copy-pasted at both spawn sites instead of living behind the boundary

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2490
**Finding ID**: NIFAL-D6-2026-08-07-04 (source: `docs/audits/AUDIT_RENDERER_2026-08-07.md`)

**Severity**: LOW
**Dimension**: 6 — NIFAL Material
**Location**: `byroredux/src/scene/nif_loader.rs:822-847` and `byroredux/src/cell_loader/spawn.rs:1554-1582`
**Status**: NEW

## Description
Immediately after the `translate_material` call, both sites run a byte-identical ~26-line block that reads **raw `ImportedMaterial`** fields (`is_decal`, `has_alpha`, `alpha_test`, `alpha_threshold`, `src_blend_mode`, `dst_blend_mode`, `two_sided`) and derives the `AlphaBlend` / `IsDecalMesh` / `TwoSided` components, including the implicit-decal-blend fallback and the hard-coded `(6, 7)` alpha-over pair. This is the same duplicated-construction shape `translate_material`'s own module doc describes as "itself a translation leak: a field added to one site and not the other silently diverged the two load paths" — just for the marker-component subset rather than the `Material` struct.

## Evidence
Both blocks are currently identical (same `decal_uses_implicit_alpha_blend` helper, same `(6, 7)` fallback, same three conditional inserts), so there is **no live divergence today** — this is a structural/latent finding, not an active bug. The shared decision predicate was already factored out (`decal_uses_implicit_alpha_blend`); the surrounding blend-mode selection and the three inserts were not.

## Impact
Latent. A future blend/decal rule added to one site silently diverges loose-NIF loads from cell-placed REFRs — the failure mode is "the same NIF renders differently depending on how it was loaded", which is hard to spot and has no test coverage at the two-site level.

## Related
`docs/engine/nifal.md` §3 "De-duplication"; #2300 (the same consolidation already performed for the particle slice's `texture_path`/`src_blend`/`dst_blend` overrides, identical copy-paste-at-both-sites shape).

## Suggested Fix
Follow the #2300 precedent — add a `attach_blend_and_facing_markers(world, entity, &mesh.material)` helper next to `translate_material` and call it from both sites, so the marker derivation has the single declared boundary the `Material` derivation already has.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: New helper lives beside `translate_material`, single call site pattern from both spawn paths
- [ ] **TESTS**: Existing marker-component tests at both call sites still pass after consolidation
