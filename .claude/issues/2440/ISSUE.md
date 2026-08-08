# NIFAL-D2-02: ImportedMesh.skin is consumed on the loose-NIF path only — cell-loaded skinned geometry never gets a canonical SkinnedMesh

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2440
**Finding ID**: NIFAL-D2-02 (source: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-07.md`)

**Severity**: MEDIUM
**Dimension**: 2 — NIFAL mapping shape
**Location**: `byroredux/src/cell_loader/spawn.rs:1681` (only cell-path use, as a boolean filter); `byroredux/src/scene/nif_loader.rs:955-1001` (sole `SkinnedMesh` producer)
**Status**: NEW

## Description
`ImportedSkin` is populated identically on both load paths by the shared mesh extractors, but only the loose-NIF path translates it into canonical `SkinnedMesh` (`new_with_global`, one production caller). On the cell-loader path the field is read exactly once — as a negative filter for the architecture-trimesh collider fallback — and never turned into a bone binding. Structurally identical to the acknowledged #2206 class (billboard_mode: correct on loose path, silently absent on cell path), but skinning is not listed in nifal.md §2's passthrough parity table — it's marked flatly "converged".

## Evidence
`grep SkinnedMesh` under `cell_loader/` → 0 hits; `nif_loader.rs:1001` is the sole `SkinnedMesh::new_with_global` call site, confirmed by direct grep.

## Impact
Any cell-placed REFR with skinned geometry (Skyrim/FO4 wind-animated cloth banners, chains, hanging/moveable statics using `NiSkinInstance`) spawns with skin data parsed and per-vertex weights uploaded, but no palette binding — renders frozen in bind pose, never animates. Silent: no `log::warn`, because the translation step doesn't exist at all. NPC actors unaffected (they route through the loose path).

## Related
#2206 (CLOSED, NIFAL-D4-02 — the analogous billboard_mode gap, same "correct on loose path only" shape).

## Suggested Fix
Extend the cell spawn path to build `SkinnedMesh` from `mesh.skin` against the placement's own node map, or — if measurement shows negligible content — record the gap explicitly in nifal.md §2's passthrough table rather than leaving the category marked bare "converged".

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: If the cell path gains a `SkinnedMesh` producer, it uses the same construction path as the loose-NIF producer, not a third parallel one
- [ ] **TESTS**: A regression test spawns a cell-placed skinned static and asserts a `SkinnedMesh` component with resolved bone bindings
